//! Deterministic SCC framing, member addressing, and graph edge encoding.

use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

use super::*;

/// A target edge before SCC member addresses exist.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::codebase::graph) enum CandidateTarget {
    /// Another checked object in this candidate set.
    Object(ObjectKey),
    /// A trusted compiler or host registry identity.
    TrustedExternal { identity: String, registry: String },
}

/// One reference occurrence owned by a candidate object or part.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::codebase::graph) struct CandidateReference {
    /// Owner-relative source part path.
    pub part: Vec<String>,
    /// Closed semantic edge kind.
    pub kind: EdgeKind,
    /// Resolved object or trusted registry identity.
    pub target: CandidateTarget,
    /// Checked call facts used to pin eligible implementation objects.
    pub dispatch: Option<ContractDispatch>,
}

/// An object before SCC member addressing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::codebase::graph) struct Candidate {
    /// Nominal or implementation key.
    pub key: ObjectKey,
    /// Checked object form kind.
    pub kind: ObjectKind,
    /// Canonical `@checked-definition.v1` frame.
    pub definition: Vec<u8>,
    /// Resolved semantic references owned by this object.
    pub references: Vec<CandidateReference>,
    /// Source provenance excluded from object identity.
    pub origins: Vec<ObjectOrigin>,
    /// Additional nested declaration/test/implementation-member mappings.
    pub entities: Vec<(EntityKey, Vec<String>)>,
}

impl Candidate {
    /// Creates a candidate with its owner entity mapping already installed.
    pub(in crate::codebase::graph) fn new(
        key: ObjectKey,
        kind: ObjectKind,
        form: &DefinitionNode,
    ) -> Result<Self, GraphEncodingError> {
        let owner_entity = match &key {
            ObjectKey::Declaration {
                identity,
                entity_kind,
            } => EntityKey::Declaration {
                identity: identity.clone(),
                entity_kind: entity_kind.clone(),
            },
            ObjectKey::Test { module, name } => EntityKey::Test {
                module: module.clone(),
                name: name.clone(),
            },
            ObjectKey::Implementation {
                receiver,
                interface,
            } => EntityKey::Implementation {
                receiver: receiver.clone(),
                interface: interface.clone(),
            },
        };
        Ok(Self {
            key,
            kind,
            definition: encode_definition(kind, form)?,
            references: Vec::new(),
            origins: Vec::new(),
            entities: vec![(owner_entity, Vec::new())],
        })
    }
}

/// The addressed objects, components, mappings, and edges from SCC encoding.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::codebase::graph) struct EncodedMembers {
    /// SCC frames in deterministic dependency order.
    pub components: Vec<EncodedComponent>,
    /// Addressed objects in canonical object-key order.
    pub objects: Vec<EncodedObject>,
    /// Revision entity mappings in canonical entity-key order.
    pub entities: Vec<EntityMapping>,
    /// Resolved edges in canonical owner/part/kind/target order.
    pub edges: Vec<EncodedEdge>,
}

/// Encodes all candidates after validating their identity and reference closure.
// These indices come from enumerate(), the candidate-key map, and the SCC
// partition produced from that same candidate vector; all target identities
// are checked before the address loop uses them.
#[allow(clippy::indexing_slicing)]
pub(in crate::codebase::graph) fn encode_candidates(
    mut candidates: Vec<Candidate>,
) -> Result<EncodedMembers, GraphEncodingError> {
    candidates.sort_by(|left, right| {
        left.key
            .canonical()
            .as_bytes()
            .cmp(right.key.canonical().as_bytes())
    });
    let mut by_key = BTreeMap::<ObjectKey, usize>::new();
    for (index, candidate) in candidates.iter().enumerate() {
        if by_key.insert(candidate.key.clone(), index).is_some() {
            return Err(GraphEncodingError::InconsistentCheckedFacts(format!(
                "duplicate checked object key {}",
                candidate.key.canonical()
            )));
        }
    }

    let adjacency = candidates
        .iter()
        .map(|candidate| {
            let mut targets = candidate
                .references
                .iter()
                .filter_map(|reference| match &reference.target {
                    CandidateTarget::Object(target) => by_key.get(target).copied(),
                    CandidateTarget::TrustedExternal { .. } => None,
                })
                .collect::<Vec<_>>();
            targets.sort_unstable();
            targets.dedup();
            targets
        })
        .collect::<Vec<_>>();
    for candidate in &candidates {
        for reference in &candidate.references {
            if let CandidateTarget::Object(target) = &reference.target
                && !by_key.contains_key(target)
            {
                return Err(GraphEncodingError::MissingResolution(target.canonical()));
            }
        }
    }
    let (components, member_component) =
        strongly_connected_components(&candidates, &adjacency);
    let mut dependencies = vec![BTreeSet::<usize>::new(); components.len()];
    let mut dependents = vec![BTreeSet::<usize>::new(); components.len()];
    for (owner, targets) in adjacency.iter().enumerate() {
        let owner_component = member_component[owner];
        for target in targets {
            let target_component = member_component[*target];
            if owner_component != target_component
                && dependencies[owner_component].insert(target_component)
            {
                dependents[target_component].insert(owner_component);
            }
        }
    }

    let mut pending = dependencies.iter().map(BTreeSet::len).collect::<Vec<_>>();
    let mut ready = BTreeSet::<(Vec<Vec<u8>>, usize)>::new();
    for component_index in 0..components.len() {
        if pending[component_index] == 0 {
            ready.insert((
                component_order_key(&components[component_index], &candidates),
                component_index,
            ));
        }
    }
    let mut addresses = vec![None::<ObjectAddress>; candidates.len()];
    let mut component_addresses = vec![None::<ComponentAddress>; components.len()];
    let mut encoded_components = Vec::with_capacity(components.len());
    let mut encoded_count = 0usize;
    while let Some((_, component_index)) = ready.pop_first() {
        let members = &components[component_index];
        let mut member_index = BTreeMap::<usize, u64>::new();
        for (ordinal, candidate_index) in members.iter().enumerate() {
            let ordinal = u64::try_from(ordinal).map_err(|_| {
                GraphEncodingError::MalformedInput(
                    "component member count exceeds u64".to_owned(),
                )
            })?;
            member_index.insert(*candidate_index, ordinal);
        }
        let mut member_records = Vec::with_capacity(members.len());
        for candidate_index in members {
            let candidate = &candidates[*candidate_index];
            let definition =
                std::str::from_utf8(&candidate.definition).map_err(|error| {
                    GraphEncodingError::MalformedInput(error.to_string())
                })?;
            let definition = definition.strip_suffix('\n').ok_or_else(|| {
                GraphEncodingError::MalformedInput(
                    "checked definition frame has no final LF".to_owned(),
                )
            })?;
            let references = canonical_references(
                *candidate_index,
                candidate,
                component_index,
                &member_component,
                &member_index,
                &by_key,
                &addresses,
            )?;
            member_records.push(format!(
                "(record key: {} kind: @{} definition: {} references: (array{}{}))",
                candidate.key.canonical(),
                candidate.kind.as_str(),
                definition,
                if references.is_empty() { "" } else { " " },
                references.join(" ")
            ));
        }
        let frame = format!(
            "(record format: @checked-component.v1 members: (array {}))",
            member_records.join(" ")
        );
        let payload = canonical_frame(&frame)?;
        let digest = hash_domain("vibra-codebase-component-v1\0", &payload);
        let component_address = ComponentAddress(address_from_digest(&digest));
        let mut member_addresses = Vec::with_capacity(members.len());
        for (ordinal, candidate_index) in members.iter().enumerate() {
            let ordinal = u64::try_from(ordinal).map_err(|_| {
                GraphEncodingError::MalformedInput(
                    "component member count exceeds u64".to_owned(),
                )
            })?;
            let mut address_hash = Sha256::new();
            address_hash.update(b"vibra-codebase-object-v1\0");
            address_hash.update(digest);
            address_hash.update(ordinal.to_be_bytes());
            let address =
                ObjectAddress(address_from_digest(&address_hash.finalize().into()));
            addresses[*candidate_index] = Some(address.clone());
            member_addresses.push(address);
        }
        component_addresses[component_index] = Some(component_address.clone());
        encoded_components.push(EncodedComponent {
            address: component_address,
            digest,
            payload,
            members: member_addresses,
        });
        encoded_count += 1;
        for dependent in &dependents[component_index] {
            pending[*dependent] = pending[*dependent].saturating_sub(1);
            if pending[*dependent] == 0 {
                ready.insert((
                    component_order_key(&components[*dependent], &candidates),
                    *dependent,
                ));
            }
        }
    }
    if encoded_count != components.len() {
        return Err(GraphEncodingError::MalformedInput(
            "SCC condensation graph is cyclic".to_owned(),
        ));
    }

    let mut objects = Vec::with_capacity(candidates.len());
    let mut entities = Vec::new();
    let mut edge_records =
        Vec::<(Vec<u8>, Vec<Vec<u8>>, String, Vec<u8>, EncodedEdge)>::new();
    for (candidate_index, candidate) in candidates.iter().enumerate() {
        let address = addresses[candidate_index].clone().ok_or_else(|| {
            GraphEncodingError::MalformedInput(
                "object address was not assigned after SCC encoding".to_owned(),
            )
        })?;
        let component_index = member_component[candidate_index];
        let component_address = component_addresses[component_index]
            .clone()
            .ok_or_else(|| {
                GraphEncodingError::MalformedInput(
                    "component address was not assigned".to_owned(),
                )
            })?;
        let ordinal = *components[component_index]
            .iter()
            .position(|candidate| *candidate == candidate_index)
            .and_then(|ordinal| u64::try_from(ordinal).ok())
            .as_ref()
            .ok_or_else(|| {
                GraphEncodingError::MalformedInput(
                    "component member ordinal is invalid".to_owned(),
                )
            })?;
        let mut origins = candidate.origins.clone();
        origins.sort_by(|left, right| {
            left.source_id
                .as_bytes()
                .cmp(right.source_id.as_bytes())
                .then_with(|| left.span.start().cmp(&right.span.start()))
                .then_with(|| left.span.end().cmp(&right.span.end()))
                .then_with(|| left.part.cmp(&right.part))
        });
        origins.dedup();
        objects.push(EncodedObject {
            key: candidate.key.clone(),
            kind: candidate.kind,
            address: address.clone(),
            component_address,
            member_index: ordinal,
            origins,
        });
        for (key, part) in &candidate.entities {
            entities.push(EntityMapping {
                key: key.clone(),
                object: address.clone(),
                part: part.clone(),
            });
        }
        for reference in &candidate.references {
            let target = match &reference.target {
                CandidateTarget::Object(key) => {
                    let target_index = *by_key.get(key).ok_or_else(|| {
                        GraphEncodingError::MissingResolution(key.canonical())
                    })?;
                    EdgeTarget::Object(addresses[target_index].clone().ok_or_else(
                        || {
                            GraphEncodingError::MalformedInput(
                                "referenced object address was not assigned".to_owned(),
                            )
                        },
                    )?)
                }
                CandidateTarget::TrustedExternal { identity, registry } => {
                    EdgeTarget::TrustedExternal {
                        identity: identity.clone(),
                        registry: registry.clone(),
                    }
                }
            };
            let target_sort = match &reference.target {
                CandidateTarget::Object(key) => key.canonical(),
                CandidateTarget::TrustedExternal { identity, registry } => {
                    format!("{identity}\0{registry}")
                }
            };
            edge_records.push((
                candidate.key.canonical().into_bytes(),
                reference
                    .part
                    .iter()
                    .map(|part| part.as_bytes().to_vec())
                    .collect(),
                reference.kind.as_str().to_owned(),
                target_sort.into_bytes(),
                EncodedEdge {
                    owner: address.clone(),
                    part: reference.part.clone(),
                    kind: reference.kind,
                    target,
                },
            ));
        }
    }
    objects.sort_by(|left, right| {
        left.key
            .canonical()
            .as_bytes()
            .cmp(right.key.canonical().as_bytes())
    });
    entities.sort_by(|left, right| {
        left.key
            .canonical()
            .as_bytes()
            .cmp(right.key.canonical().as_bytes())
    });
    for pair in entities.windows(2) {
        let (Some(left), Some(right)) = (pair.first(), pair.get(1)) else {
            continue;
        };
        if left.key == right.key
            && (left.object != right.object || left.part != right.part)
        {
            return Err(GraphEncodingError::InconsistentCheckedFacts(format!(
                "entity key {} has conflicting object mappings",
                left.key.canonical()
            )));
        }
    }
    entities.dedup_by(|left, right| {
        left.key == right.key && left.object == right.object && left.part == right.part
    });
    edge_records.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.as_bytes().cmp(right.2.as_bytes()))
            .then_with(|| left.3.cmp(&right.3))
    });
    edge_records.dedup_by(|left, right| {
        left.0 == right.0 && left.1 == right.1 && left.2 == right.2 && left.3 == right.3
    });
    let edges = edge_records.into_iter().map(|edge| edge.4).collect();
    Ok(EncodedMembers {
        components: encoded_components,
        objects,
        entities,
        edges,
    })
}

// Candidate and member indices are validated once in encode_candidates and
// refer to vectors built from that exact candidate set.
#[allow(clippy::indexing_slicing)]
fn canonical_references(
    candidate_index: usize,
    candidate: &Candidate,
    component_index: usize,
    member_component: &[usize],
    member_index: &BTreeMap<usize, u64>,
    by_key: &BTreeMap<ObjectKey, usize>,
    addresses: &[Option<ObjectAddress>],
) -> Result<Vec<String>, GraphEncodingError> {
    let mut references = candidate.references.iter().collect::<Vec<_>>();
    references.sort_by(|left, right| {
        left.part
            .iter()
            .map(String::as_bytes)
            .cmp(right.part.iter().map(String::as_bytes))
            .then_with(|| {
                left.kind
                    .as_str()
                    .as_bytes()
                    .cmp(right.kind.as_str().as_bytes())
            })
            .then_with(|| {
                target_sort_key(&left.target).cmp(&target_sort_key(&right.target))
            })
    });
    references.dedup_by(|left, right| {
        left.part == right.part
            && left.kind == right.kind
            && left.target == right.target
    });
    references
        .into_iter()
        .map(|reference| {
            let target = match &reference.target {
                CandidateTarget::Object(key) => {
                    let target_index = *by_key.get(key).ok_or_else(|| {
                        GraphEncodingError::MissingResolution(key.canonical())
                    })?;
                    if member_component[target_index] == component_index {
                        let index =
                            *member_index.get(&target_index).ok_or_else(|| {
                                GraphEncodingError::MalformedInput(
                                    "internal component member has no ordinal"
                                        .to_owned(),
                                )
                            })?;
                        format!("(record kind: @component-member index: {index}u64)")
                    } else {
                        let address = addresses[target_index].as_ref().ok_or_else(|| {
                            GraphEncodingError::MalformedInput(format!(
                                "dependency {} was not encoded before its dependent",
                                key.canonical()
                            ))
                        })?;
                        format!(
                            "(record kind: @object address: {})",
                            vibra_syntax::canonical_string_spelling(address.as_str())
                        )
                    }
                }
                CandidateTarget::TrustedExternal { identity, registry } => format!(
                    "(record kind: @trusted-external identity: {} registry: {})",
                    vibra_syntax::canonical_string_spelling(identity),
                    vibra_syntax::canonical_string_spelling(registry)
                ),
            };
            let part = format!(
                "(array{})",
                reference
                    .part
                    .iter()
                    .map(|piece| format!(
                        " {}",
                        vibra_syntax::canonical_string_spelling(piece)
                    ))
                    .collect::<String>()
            );
            let _ = candidate_index;
            Ok(format!(
                "(record part: {part} kind: @{} target: {target})",
                reference.kind.as_str()
            ))
        })
        .collect()
}

fn target_sort_key(target: &CandidateTarget) -> String {
    match target {
        CandidateTarget::Object(key) => format!("object:{}", key.canonical()),
        CandidateTarget::TrustedExternal { identity, registry } => {
            format!("trusted:{identity}\0{registry}")
        }
    }
}

// Adjacency rows and edge targets are generated from the enumerated candidate
// vector in encode_candidates, so every graph node index is within these arrays.
#[allow(clippy::indexing_slicing)]
fn strongly_connected_components(
    candidates: &[Candidate],
    adjacency: &[Vec<usize>],
) -> (Vec<Vec<usize>>, Vec<usize>) {
    let mut visited = vec![false; candidates.len()];
    let mut finish = Vec::with_capacity(candidates.len());
    for root in 0..candidates.len() {
        if visited[root] {
            continue;
        }
        visited[root] = true;
        let mut stack = vec![(root, 0usize)];
        while let Some((node, next)) = stack.last().copied() {
            if let Some(target) = adjacency
                .get(node)
                .and_then(|neighbors| neighbors.get(next))
            {
                if let Some(last) = stack.last_mut() {
                    last.1 = next.saturating_add(1);
                }
                if !visited[*target] {
                    visited[*target] = true;
                    stack.push((*target, 0));
                }
            } else {
                stack.pop();
                finish.push(node);
            }
        }
    }
    let mut reverse = vec![Vec::new(); candidates.len()];
    for (owner, targets) in adjacency.iter().enumerate() {
        for target in targets {
            if let Some(neighbors) = reverse.get_mut(*target) {
                neighbors.push(owner);
            }
        }
    }
    for neighbors in &mut reverse {
        neighbors.sort_unstable();
        neighbors.dedup();
    }
    visited.fill(false);
    let mut components = Vec::new();
    let mut member_component = vec![usize::MAX; candidates.len()];
    while let Some(root) = finish.pop() {
        if visited[root] {
            continue;
        }
        let component_index = components.len();
        let mut members = Vec::new();
        let mut stack = vec![root];
        visited[root] = true;
        while let Some(node) = stack.pop() {
            members.push(node);
            member_component[node] = component_index;
            if let Some(neighbors) = reverse.get(node) {
                for target in neighbors.iter().rev() {
                    if !visited[*target] {
                        visited[*target] = true;
                        stack.push(*target);
                    }
                }
            }
        }
        members.sort_by(|left, right| {
            candidates[*left]
                .key
                .canonical()
                .as_bytes()
                .cmp(candidates[*right].key.canonical().as_bytes())
        });
        components.push(members);
    }
    (components, member_component)
}

// Member indices come from strongly_connected_components over candidates.
#[allow(clippy::indexing_slicing)]
fn component_order_key(members: &[usize], candidates: &[Candidate]) -> Vec<Vec<u8>> {
    members
        .iter()
        .map(|index| candidates[*index].key.canonical().into_bytes())
        .collect()
}

pub(in crate::codebase::graph) fn canonical_frame(
    record: &str,
) -> Result<Vec<u8>, GraphEncodingError> {
    let document = vibra_syntax::parse_data("checked-frame.vibon", record)
        .map_err(|error| GraphEncodingError::MalformedInput(error.to_string()))?;
    if !document.accepted() || document.recovered() {
        return Err(GraphEncodingError::MalformedInput(
            "generated graph record is not valid VIBON".to_owned(),
        ));
    }
    let data = document.data().ok_or_else(|| {
        GraphEncodingError::MalformedInput(
            "generated graph record has no VIBON value".to_owned(),
        )
    })?;
    let mut payload = vibra_syntax::canonical_data(data).into_bytes();
    payload.push(b'\n');
    Ok(payload)
}

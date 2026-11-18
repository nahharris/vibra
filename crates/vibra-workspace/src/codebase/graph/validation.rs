//! Pure validation of canonical graph payloads and derived graph rows.

use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

use super::*;

/// Parses one authoritative VIBON payload and rejects noncanonical bytes.
pub fn parse_canonical_frame(
    payload: &[u8],
) -> Result<vibra_syntax::DataNode, GraphEncodingError> {
    let text = std::str::from_utf8(payload)
        .map_err(|error| GraphEncodingError::MalformedInput(error.to_string()))?;
    let document = vibra_syntax::parse_data("codebase-record.vibon", text)
        .map_err(|error| GraphEncodingError::MalformedInput(error.to_string()))?;
    if !document.accepted() || document.recovered() {
        return Err(GraphEncodingError::MalformedInput(
            "record is not accepted canonical VIBON".to_owned(),
        ));
    }
    let data = document.into_data().ok_or_else(|| {
        GraphEncodingError::MalformedInput("record has no VIBON value".to_owned())
    })?;
    let mut canonical = vibra_syntax::canonical_data(&data).into_bytes();
    canonical.push(b'\n');
    if canonical != payload {
        return Err(GraphEncodingError::MalformedInput(
            "record bytes differ from canonical VIBON".to_owned(),
        ));
    }
    Ok(data)
}

/// Recomputes and checks a stored revision's domain-separated address.
pub fn validate_revision_payload(
    address: &str,
    payload: &[u8],
) -> Result<vibra_syntax::DataNode, GraphEncodingError> {
    let frame = parse_canonical_frame(payload)?;
    validate_address("vibra-codebase-revision-v1\0", address, payload)?;
    Ok(frame)
}

/// Recomputes and checks a stored SCC address and its raw component digest.
pub fn validate_component_payload(
    address: &str,
    expected_digest: &[u8],
    payload: &[u8],
) -> Result<vibra_syntax::DataNode, GraphEncodingError> {
    let frame = parse_canonical_frame(payload)?;
    let digest = hash_domain("vibra-codebase-component-v1\0", payload);
    if expected_digest != digest {
        return Err(GraphEncodingError::MalformedInput(
            "component digest does not match its frame".to_owned(),
        ));
    }
    if address_from_digest(&digest) != address {
        return Err(GraphEncodingError::MalformedInput(
            "component address does not match its frame".to_owned(),
        ));
    }
    Ok(frame)
}

fn validate_address(
    domain: &str,
    address: &str,
    payload: &[u8],
) -> Result<(), GraphEncodingError> {
    let digest = hash_domain(domain, payload);
    if address_from_digest(&digest) != address {
        return Err(GraphEncodingError::MalformedInput(
            "record address does not match its canonical frame".to_owned(),
        ));
    }
    Ok(())
}

/// Validates every canonical payload, address, membership, and derived graph
/// row before publication or after a store read.
pub fn validate_encoded_graph(graph: &EncodedGraph) -> Result<(), GraphEncodingError> {
    validate_revision_payload(
        graph.revision.address.as_str(),
        &graph.revision.payload,
    )?;

    let mut source_ids = BTreeSet::new();
    let mut previous_source: Option<&[u8]> = None;
    for source in &graph.sources {
        if previous_source
            .is_some_and(|previous| previous >= source.source_id.as_bytes())
        {
            return Err(GraphEncodingError::MalformedInput(
                "source attachments are not uniquely sorted".to_owned(),
            ));
        }
        previous_source = Some(source.source_id.as_bytes());
        if !source_ids.insert(source.source_id.as_str()) {
            return Err(GraphEncodingError::MalformedInput(
                "duplicate source attachment identity".to_owned(),
            ));
        }
        let text = std::str::from_utf8(&source.text)
            .map_err(|error| GraphEncodingError::MalformedInput(error.to_string()))?;
        let expected =
            source_attachment(&source.source_id, &source.origin, &source.text, text)?;
        if expected != *source {
            return Err(GraphEncodingError::MalformedInput(
                "source attachment metadata does not match its bytes".to_owned(),
            ));
        }
    }

    let mut component_by_address = BTreeMap::<String, &EncodedComponent>::new();
    let mut component_frames = BTreeMap::<String, vibra_syntax::DataNode>::new();
    let mut parsed_edges = Vec::<EncodedEdge>::new();
    for component in &graph.components {
        if component.members.is_empty() {
            return Err(GraphEncodingError::MalformedInput(
                "an SCC component has no members".to_owned(),
            ));
        }
        if component_by_address
            .insert(component.address.as_str().to_owned(), component)
            .is_some()
        {
            return Err(GraphEncodingError::MalformedInput(
                "duplicate SCC component address".to_owned(),
            ));
        }
        let frame = validate_component_payload(
            component.address.as_str(),
            &component.digest,
            &component.payload,
        )?;
        let members = data_field(&frame, "members")
            .and_then(data_array)
            .ok_or_else(|| {
                GraphEncodingError::MalformedInput(
                    "component frame has no member array".to_owned(),
                )
            })?;
        if members.len() != component.members.len() {
            return Err(GraphEncodingError::MalformedInput(
                "component frame/member index count differs".to_owned(),
            ));
        }
        for (index, member) in members.iter().enumerate() {
            let ordinal = u64::try_from(index).map_err(|_| {
                GraphEncodingError::MalformedInput(
                    "member count exceeds u64".to_owned(),
                )
            })?;
            let member_address = derive_object_address(&component.digest, ordinal);
            if component
                .members
                .get(index)
                .is_none_or(|address| address != &member_address)
            {
                return Err(GraphEncodingError::MalformedInput(
                    "component member address does not match its ordinal".to_owned(),
                ));
            }
            let key = data_field(member, "key").ok_or_else(|| {
                GraphEncodingError::MalformedInput(
                    "component member has no key".to_owned(),
                )
            })?;
            let kind = data_atom(data_field(member, "kind").ok_or_else(|| {
                GraphEncodingError::MalformedInput(
                    "component member has no kind".to_owned(),
                )
            })?)
            .ok_or_else(|| {
                GraphEncodingError::MalformedInput(
                    "component member kind is not an atom".to_owned(),
                )
            })?;
            let references = data_field(member, "references")
                .and_then(data_array)
                .ok_or_else(|| {
                    GraphEncodingError::MalformedInput(
                        "component member has no reference array".to_owned(),
                    )
                })?;
            for reference in references {
                parsed_edges.push(parse_component_edge(
                    reference,
                    &member_address,
                    component,
                )?);
            }
            let _ = (key, kind);
        }
        component_frames.insert(component.address.as_str().to_owned(), frame);
    }

    let mut object_by_address = BTreeMap::<String, &EncodedObject>::new();
    let mut object_keys = BTreeSet::new();
    for object in &graph.objects {
        if !object_keys.insert(object.key.clone()) {
            return Err(GraphEncodingError::MalformedInput(
                "duplicate object key".to_owned(),
            ));
        }
        if object_by_address
            .insert(object.address.as_str().to_owned(), object)
            .is_some()
        {
            return Err(GraphEncodingError::MalformedInput(
                "duplicate object address".to_owned(),
            ));
        }
        let component = component_by_address
            .get(object.component_address.as_str())
            .ok_or_else(|| {
                GraphEncodingError::MalformedInput(
                    "object points at a missing component".to_owned(),
                )
            })?;
        let index = usize::try_from(object.member_index).map_err(|_| {
            GraphEncodingError::MalformedInput("member index exceeds usize".to_owned())
        })?;
        if component.members.get(index) != Some(&object.address) {
            return Err(GraphEncodingError::MalformedInput(
                "object membership ordinal does not match component".to_owned(),
            ));
        }
        if object.address
            != derive_object_address(&component.digest, object.member_index)
        {
            return Err(GraphEncodingError::MalformedInput(
                "object address does not match component digest and ordinal".to_owned(),
            ));
        }
        let component_frame = component_frames
            .get(component.address.as_str())
            .ok_or_else(|| {
                GraphEncodingError::MalformedInput(
                    "verified component frame was not cached".to_owned(),
                )
            })?;
        let member =
            data_array(data_field(component_frame, "members").ok_or_else(|| {
                GraphEncodingError::MalformedInput(
                    "component frame has no member array".to_owned(),
                )
            })?)
            .and_then(|members| members.get(index))
            .ok_or_else(|| {
                GraphEncodingError::MalformedInput(
                    "component member ordinal is out of bounds".to_owned(),
                )
            })?;
        let key = data_field(member, "key").ok_or_else(|| {
            GraphEncodingError::MalformedInput("component member has no key".to_owned())
        })?;
        let kind = data_atom(data_field(member, "kind").ok_or_else(|| {
            GraphEncodingError::MalformedInput(
                "component member has no kind".to_owned(),
            )
        })?)
        .ok_or_else(|| {
            GraphEncodingError::MalformedInput(
                "component member kind is not an atom".to_owned(),
            )
        })?;
        if vibra_syntax::canonical_data(key) != object.key.canonical()
            || kind != object.kind.as_str()
        {
            return Err(GraphEncodingError::MalformedInput(
                "object key or kind disagrees with its component member".to_owned(),
            ));
        }
    }

    let mut mapping_keys = BTreeSet::new();
    for mapping in &graph.entities {
        if !mapping_keys.insert(mapping.key.clone()) {
            return Err(GraphEncodingError::MalformedInput(
                "duplicate entity mapping key".to_owned(),
            ));
        }
        if !object_by_address.contains_key(mapping.object.as_str()) {
            return Err(GraphEncodingError::MalformedInput(
                "entity mapping points at a missing object".to_owned(),
            ));
        }
    }

    let actual_edges = normalize_edge_rows(&graph.edges);
    let payload_edges = normalize_edge_rows(&parsed_edges);
    if actual_edges != payload_edges {
        return Err(GraphEncodingError::MalformedInput(
            "edge rows disagree with checked component references".to_owned(),
        ));
    }
    for edge in &graph.edges {
        if !object_by_address.contains_key(edge.owner.as_str()) {
            return Err(GraphEncodingError::MalformedInput(
                "edge owner is missing".to_owned(),
            ));
        }
        match &edge.target {
            EdgeTarget::Object(address)
                if !object_by_address.contains_key(address.as_str()) =>
            {
                return Err(GraphEncodingError::MalformedInput(
                    "edge target object is missing".to_owned(),
                ));
            }
            EdgeTarget::TrustedExternal { identity, registry } => {
                if !(identity.starts_with("@compiler:")
                    || identity.starts_with("@host:"))
                    || !graph
                        .revision
                        .payload
                        .windows(registry.len())
                        .any(|window| window == registry.as_bytes())
                {
                    return Err(GraphEncodingError::MalformedInput(
                        "trusted edge target is not pinned by this revision".to_owned(),
                    ));
                }
            }
            EdgeTarget::Object(_) => {}
        }
    }

    if graph.objects.windows(2).any(|pair| {
        let (Some(left), Some(right)) = (pair.first(), pair.get(1)) else {
            return false;
        };
        left.key.canonical().as_bytes() >= right.key.canonical().as_bytes()
    }) {
        return Err(GraphEncodingError::MalformedInput(
            "objects are not uniquely sorted by canonical key".to_owned(),
        ));
    }
    if graph.entities.windows(2).any(|pair| {
        let (Some(left), Some(right)) = (pair.first(), pair.get(1)) else {
            return false;
        };
        left.key.canonical().as_bytes() >= right.key.canonical().as_bytes()
    }) {
        return Err(GraphEncodingError::MalformedInput(
            "entity mappings are not uniquely sorted".to_owned(),
        ));
    }

    let revision = parse_canonical_frame(&graph.revision.payload)?;
    let package = data_field(&revision, "package").ok_or_else(|| {
        GraphEncodingError::MalformedInput("revision package is absent".to_owned())
    })?;
    let name = data_string(data_field(package, "name").ok_or_else(|| {
        GraphEncodingError::MalformedInput("revision package name is absent".to_owned())
    })?)
    .ok_or_else(|| {
        GraphEncodingError::MalformedInput(
            "revision package name is not a string".to_owned(),
        )
    })?;
    let version = data_string(data_field(package, "version").ok_or_else(|| {
        GraphEncodingError::MalformedInput(
            "revision package version is absent".to_owned(),
        )
    })?)
    .ok_or_else(|| {
        GraphEncodingError::MalformedInput(
            "revision package version is not a string".to_owned(),
        )
    })?;
    let trusted = data_field(&revision, "trusted")
        .and_then(data_array)
        .ok_or_else(|| {
            GraphEncodingError::MalformedInput(
                "revision trusted array is absent".to_owned(),
            )
        })?
        .iter()
        .map(vibra_syntax::canonical_data)
        .collect::<Vec<_>>();
    let expected_revision =
        revision_frame(name, version, &trusted, &graph.entities, &graph.sources)?;
    if expected_revision != graph.revision.payload {
        return Err(GraphEncodingError::MalformedInput(
            "revision manifest does not match graph membership/source rows".to_owned(),
        ));
    }
    Ok(())
}

fn derive_object_address(digest: &[u8; 32], ordinal: u64) -> ObjectAddress {
    let mut hash = Sha256::new();
    hash.update(b"vibra-codebase-object-v1\0");
    hash.update(digest);
    hash.update(ordinal.to_be_bytes());
    ObjectAddress(address_from_digest(&hash.finalize().into()))
}

fn data_field<'a>(
    node: &'a vibra_syntax::DataNode,
    name: &str,
) -> Option<&'a vibra_syntax::DataNode> {
    let vibra_syntax::DataValue::Record(fields) = node.value() else {
        return None;
    };
    fields
        .iter()
        .find(|field| field.label().value() == name)
        .map(|field| field.value())
}

fn data_array(node: &vibra_syntax::DataNode) -> Option<&[vibra_syntax::DataNode]> {
    let vibra_syntax::DataValue::Array(values) = node.value() else {
        return None;
    };
    Some(values)
}

fn data_atom(node: &vibra_syntax::DataNode) -> Option<&str> {
    let vibra_syntax::DataValue::Atom(value) = node.value() else {
        return None;
    };
    Some(value.value())
}

fn data_string(node: &vibra_syntax::DataNode) -> Option<&str> {
    let vibra_syntax::DataValue::Literal(vibra_syntax::Literal::String(value)) =
        node.value()
    else {
        return None;
    };
    Some(value.value())
}

fn data_u64(node: &vibra_syntax::DataNode) -> Option<u64> {
    let vibra_syntax::DataValue::Literal(vibra_syntax::Literal::Integer(value)) =
        node.value()
    else {
        return None;
    };
    if value.is_negative() {
        return None;
    }
    value.digits().parse().ok()
}

fn parse_component_edge(
    node: &vibra_syntax::DataNode,
    owner: &ObjectAddress,
    component: &EncodedComponent,
) -> Result<EncodedEdge, GraphEncodingError> {
    let part = data_field(node, "part")
        .and_then(data_array)
        .ok_or_else(|| {
            GraphEncodingError::MalformedInput(
                "component edge part is absent".to_owned(),
            )
        })?
        .iter()
        .map(|value| {
            data_string(value).map(str::to_owned).ok_or_else(|| {
                GraphEncodingError::MalformedInput(
                    "component edge part item is not a string".to_owned(),
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let kind = data_atom(data_field(node, "kind").ok_or_else(|| {
        GraphEncodingError::MalformedInput("component edge kind is absent".to_owned())
    })?)
    .ok_or_else(|| {
        GraphEncodingError::MalformedInput(
            "component edge kind is not an atom".to_owned(),
        )
    })?;
    let kind = [
        EdgeKind::ValueReference,
        EdgeKind::TypeReference,
        EdgeKind::Contract,
        EdgeKind::Implementation,
        EdgeKind::EffectOwner,
        EdgeKind::Initializer,
        EdgeKind::Call,
    ]
    .into_iter()
    .find(|candidate| candidate.as_str() == kind)
    .ok_or_else(|| {
        GraphEncodingError::MalformedInput("component edge kind is unknown".to_owned())
    })?;
    let target = data_field(node, "target").ok_or_else(|| {
        GraphEncodingError::MalformedInput("component edge target is absent".to_owned())
    })?;
    let target_kind = data_atom(data_field(target, "kind").ok_or_else(|| {
        GraphEncodingError::MalformedInput("component target kind is absent".to_owned())
    })?)
    .ok_or_else(|| {
        GraphEncodingError::MalformedInput(
            "component target kind is not an atom".to_owned(),
        )
    })?;
    let target = match target_kind {
        "component-member" => {
            let index = data_u64(data_field(target, "index").ok_or_else(|| {
                GraphEncodingError::MalformedInput(
                    "internal target index is absent".to_owned(),
                )
            })?)
            .ok_or_else(|| {
                GraphEncodingError::MalformedInput(
                    "internal target index is invalid".to_owned(),
                )
            })?;
            let index = usize::try_from(index).map_err(|_| {
                GraphEncodingError::MalformedInput(
                    "internal target index exceeds usize".to_owned(),
                )
            })?;
            EdgeTarget::Object(component.members.get(index).cloned().ok_or_else(
                || {
                    GraphEncodingError::MalformedInput(
                        "internal target member is out of bounds".to_owned(),
                    )
                },
            )?)
        }
        "object" => EdgeTarget::Object(ObjectAddress(
            data_string(data_field(target, "address").ok_or_else(|| {
                GraphEncodingError::MalformedInput(
                    "external object target address is absent".to_owned(),
                )
            })?)
            .ok_or_else(|| {
                GraphEncodingError::MalformedInput(
                    "external object target address is not a string".to_owned(),
                )
            })?
            .to_owned(),
        )),
        "trusted-external" => EdgeTarget::TrustedExternal {
            identity: data_string(data_field(target, "identity").ok_or_else(|| {
                GraphEncodingError::MalformedInput(
                    "trusted target identity is absent".to_owned(),
                )
            })?)
            .ok_or_else(|| {
                GraphEncodingError::MalformedInput(
                    "trusted target identity is not a string".to_owned(),
                )
            })?
            .to_owned(),
            registry: data_string(data_field(target, "registry").ok_or_else(|| {
                GraphEncodingError::MalformedInput(
                    "trusted target registry is absent".to_owned(),
                )
            })?)
            .ok_or_else(|| {
                GraphEncodingError::MalformedInput(
                    "trusted target registry is not a string".to_owned(),
                )
            })?
            .to_owned(),
        },
        _ => {
            return Err(GraphEncodingError::MalformedInput(
                "component target kind is unknown".to_owned(),
            ));
        }
    };
    let _ = owner;
    Ok(EncodedEdge {
        owner: owner.clone(),
        part,
        kind,
        target,
    })
}

fn normalize_edge_rows(
    edges: &[EncodedEdge],
) -> Vec<(String, Vec<String>, String, String)> {
    let mut rows = edges
        .iter()
        .map(|edge| {
            let target = match &edge.target {
                EdgeTarget::Object(address) => format!("object:{}", address.as_str()),
                EdgeTarget::TrustedExternal { identity, registry } => {
                    format!("trusted:{identity}\0{registry}")
                }
            };
            (
                edge.owner.as_str().to_owned(),
                edge.part.clone(),
                edge.kind.as_str().to_owned(),
                target,
            )
        })
        .collect::<Vec<_>>();
    rows.sort();
    rows
}

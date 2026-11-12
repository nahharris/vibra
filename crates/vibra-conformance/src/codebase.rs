//! Independent, non-executing codebase lifecycle scenarios.

use crate::{
    Case, CaseObservation, ConformanceOperation, HandlerError, ProfileHandler,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};
use vibra_syntax::{DataNode, DataValue, Literal, decode_data_root, parse_data};
use vibra_workspace::codebase::graph::{
    EdgeTarget, EncodedGraph, EncodedObject, ObjectKey,
};
use vibra_workspace::codebase::import::{PreparedImport, prepare_import};
use vibra_workspace::codebase::store::{
    CodebaseStore, HeadExpectation, ReadTarget, StoreAccess,
};

const STORE: &str = ".vibra/codebase.sqlite";

/// Exercises frozen lifecycle scenarios through the production workspace service.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ToolingV1CodebaseHandler;

impl ProfileHandler for ToolingV1CodebaseHandler {
    fn can_run(&self, case: &Case) -> bool {
        case.manifest().operation == ConformanceOperation::Codebase
    }

    fn run(&self, case: &Case) -> Result<CaseObservation, HandlerError> {
        let scenario_path = case
            .manifest()
            .inputs
            .data
            .first()
            .ok_or_else(|| handler_error("missing scenario"))?;
        let scenario =
            decode_scenario(&case.read_file(scenario_path).map_err(handler_error)?)
                .map_err(handler_error)?;
        let tree = TemporaryTree::copy(case)?;
        let first = prepare_import(&tree.project).map_err(handler_error)?;
        let store = CodebaseStore::init(&tree.project, STORE).map_err(handler_error)?;
        let first_receipt = store
            .publish("main", HeadExpectation::Absent, &first.graph, &first.index)
            .map_err(handler_error)?;
        let revision = first_receipt.revision;
        let mut facts = BTreeMap::new();
        match scenario.name.as_str() {
            "import" => {
                drop(store);
                let reopened =
                    CodebaseStore::open(&tree.project, STORE, StoreAccess::ReadOnly)
                        .map_err(handler_error)?;
                facts.insert(
                    "reopened",
                    reopened.head(None).map_err(handler_error)? == revision,
                );
                facts.insert(
                    "source-preserved",
                    sources_equal(&reopened, &revision, &first.graph)?,
                );
                facts.insert(
                    "definitions-readable",
                    definitions_readable(&reopened, &revision, &first.graph)
                        && ["/app.main.main", "/math.parity.even", "/math.parity.odd"]
                            .iter()
                            .all(|suffix| {
                                object_address(&first.graph, suffix).is_some()
                            })
                        && first
                            .graph
                            .objects
                            .iter()
                            .any(|object| matches!(object.key, ObjectKey::Test { .. })),
                );
                facts.insert(
                    "edges-pinned",
                    edges_pinned(&reopened, &revision, &first.graph),
                );
            }
            "lifecycle" => {
                scenario.apply_edits(&tree.project, &first)?;
                let second = prepare_import(&tree.project).map_err(handler_error)?;
                let published = store
                    .publish(
                        "main",
                        HeadExpectation::Revision(revision.clone()),
                        &second.graph,
                        &second.index,
                    )
                    .map_err(handler_error)?;
                facts.insert(
                    "old-readable",
                    definitions_readable(&store, &revision, &first.graph)
                        && edges_pinned(&store, &revision, &first.graph),
                );
                facts.insert(
                    "new-readable",
                    definitions_readable(&store, &published.revision, &second.graph)
                        && edges_pinned(&store, &published.revision, &second.graph),
                );
                facts.insert(
                    "dependent-changed",
                    object_address(&first.graph, "/app.main.main")
                        != object_address(&second.graph, "/app.main.main"),
                );
                let old_unrelated =
                    object_address(&first.graph, "/math.other.constant");
                facts.insert(
                    "unrelated-deduplicated",
                    old_unrelated.is_some()
                        && old_unrelated
                            == object_address(&second.graph, "/math.other.constant"),
                );
                facts.insert(
                    "source-changed",
                    first.graph.sources != second.graph.sources
                        && revision != published.revision,
                );
            }
            "stale" => {
                scenario.apply_edits(&tree.project, &first)?;
                let second = prepare_import(&tree.project).map_err(handler_error)?;
                let winner = store
                    .publish(
                        "main",
                        HeadExpectation::Revision(revision.clone()),
                        &second.graph,
                        &second.index,
                    )
                    .map_err(handler_error)?;
                let loser = store.publish(
                    "main",
                    HeadExpectation::Revision(revision),
                    &first.graph,
                    &first.index,
                );
                let rejected = loser.as_ref().is_err_and(|error| {
                    error.code() == vibra_diagnostics::DiagnosticCode::CodebaseStaleHead
                });
                facts.insert("one-winner", winner.published && rejected);
                facts.insert("stale-rejected", rejected);
                facts.insert(
                    "only-complete-head",
                    store.head(None).map_err(handler_error)? == winner.revision
                        && store.verify().is_ok()
                        && definitions_readable(
                            &store,
                            &winner.revision,
                            &second.graph,
                        ),
                );
            }
            "backup" => {
                store.backup("backup.sqlite").map_err(handler_error)?;
                CodebaseStore::restore(
                    &tree.project,
                    "backup.sqlite",
                    ".vibra/restored.sqlite",
                )
                .map_err(handler_error)?;
                let restored = CodebaseStore::open(
                    &tree.project,
                    ".vibra/restored.sqlite",
                    StoreAccess::ReadOnly,
                )
                .map_err(handler_error)?;
                facts.insert(
                    "head-preserved",
                    restored.head(None).map_err(handler_error)? == revision,
                );
                facts.insert(
                    "addresses-preserved",
                    definitions_readable(&restored, &revision, &first.graph),
                );
                facts.insert(
                    "sources-preserved",
                    sources_equal(&restored, &revision, &first.graph)?,
                );
                facts.insert(
                    "relations-preserved",
                    edges_pinned(&restored, &revision, &first.graph),
                );
            }
            "roundtrip" => {
                return run_roundtrip(&tree.project, &store, &revision, &first.graph);
            }
            "integrity" => {
                return run_integrity(&tree.project, &store, &revision, &first);
            }
            "declarations" => {
                return run_declarations(&store, &revision, &first.graph);
            }
            _ => return Err(handler_error("unknown codebase scenario")),
        }
        observation(&scenario.name, &facts)
    }
}

fn run_declarations(
    store: &CodebaseStore,
    revision: &str,
    graph: &EncodedGraph,
) -> Result<CaseObservation, HandlerError> {
    use vibra_workspace::codebase::graph::{EntityKey, ObjectKind};
    let local = |suffix: &str| {
        graph.objects.iter().find(|object| {
        matches!(&object.key, ObjectKey::Declaration { identity, .. } if identity == &format!("@codebase-kinds@0.1.0/math.model.{suffix}"))
    })
    };
    let mut facts = BTreeMap::new();
    facts.insert("source-preserved", sources_equal(store, revision, graph)?);
    facts.insert(
        "nominal-distinct",
        local("first")
            .zip(local("second"))
            .is_some_and(|(first, second)| {
                first.kind == ObjectKind::Deftype
                    && second.kind == ObjectKind::Deftype
                    && first.address != second.address
            }),
    );
    facts.insert(
        "generics-checked",
        local("identity").is_some_and(|object| {
            store
                .read(revision, object.address.as_str())
                .is_ok_and(|read| {
                    read.payload.contains("binder-kind: @type")
                        && read.payload.contains("binder-kind: @value")
                })
        }),
    );
    facts.insert(
        "implementation-addressed",
        graph.objects.iter().any(|object| {
            object.kind == ObjectKind::Implementation
                && object
                    .origins
                    .iter()
                    .any(|origin| origin.source_id == "src/math/model.vib")
                && graph.entities.iter().any(|entity| {
                    matches!(entity.key, EntityKey::ImplementationMember { .. })
                        && entity.object == object.address
                        && !entity.part.is_empty()
                })
                && store.read(revision, object.address.as_str()).is_ok()
        }),
    );
    facts.insert("effect-identified", local("audit").is_some_and(|object| {
        object.kind == ObjectKind::Deffect && graph.entities.iter().any(|entity| {
            matches!(&entity.key, EntityKey::Declaration { identity, .. } if identity == "@codebase-kinds@0.1.0/math.model.audit.record")
                && entity.object == object.address && !entity.part.is_empty()
        })
    }));
    facts.insert(
        "trusted-references-pinned",
        local("use-host").is_some_and(|caller| {
            reaches_trusted_host(graph, caller.address.as_str(), "@host:io.stdout-text")
        }) && edges_pinned(store, revision, graph),
    );
    observation("declarations", &facts)
}

fn reaches_trusted_host(graph: &EncodedGraph, start: &str, expected: &str) -> bool {
    let mut pending = vec![start];
    let mut visited = BTreeSet::new();
    while let Some(owner) = pending.pop() {
        if !visited.insert(owner) {
            continue;
        }
        for edge in graph
            .edges
            .iter()
            .filter(|edge| edge.owner.as_str() == owner)
        {
            match &edge.target {
                EdgeTarget::Object(address) => pending.push(address.as_str()),
                EdgeTarget::TrustedExternal { identity, registry }
                    if identity == expected
                        && registry.starts_with("sha256:")
                        && registry.len() == 71 =>
                {
                    return true;
                }
                EdgeTarget::TrustedExternal { .. } => {}
            }
        }
    }
    false
}

fn handler_error(error: impl std::fmt::Display) -> HandlerError {
    HandlerError::new(error.to_string())
}

fn observation(
    scenario: &str,
    facts: &BTreeMap<&str, bool>,
) -> Result<CaseObservation, HandlerError> {
    let fields = facts
        .iter()
        .map(|(name, value)| format!("@{name} {value}"))
        .collect::<Vec<_>>()
        .join(" ");
    let text = format!(
        "(record format: @codebase-observation.v1 scenario: @{scenario} facts: (dict {fields}))\n"
    );
    let document = parse_data("observation.vibon", &text).map_err(handler_error)?;
    let value = document
        .data()
        .ok_or_else(|| handler_error("invalid scenario observation"))?;
    Ok(CaseObservation {
        accepted: true,
        codebase: Some(format!("{}\n", vibra_syntax::canonical_data(value))),
        ..CaseObservation::default()
    })
}

fn sources_equal(
    store: &CodebaseStore,
    revision: &str,
    graph: &EncodedGraph,
) -> Result<bool, HandlerError> {
    let stored = store.revision_sources(revision).map_err(handler_error)?;
    Ok(stored.len() == graph.sources.len()
        && graph.sources.iter().all(|expected| {
            stored.iter().any(|actual| {
                actual.source_id == expected.source_id
                    && actual.origin == expected.origin
                    && actual.text == expected.text
                    && actual.raw_hash == expected.raw_hash.as_str()
                    && actual.address == expected.address.as_str()
            })
        }))
}

// Scenarios exercise every declaration authored in their captured project.
// Embedded stdlib closure and payload integrity are checked by the store;
// reading every stdlib object separately would repeat a complete revision
// validation for each unrelated library declaration.
fn local_objects(graph: &EncodedGraph) -> impl Iterator<Item = &EncodedObject> {
    graph.objects.iter().filter(|object| {
        object.origins.iter().any(|origin| {
            graph
                .sources
                .iter()
                .any(|source| source.source_id == origin.source_id)
        })
    })
}

fn definitions_readable(
    store: &CodebaseStore,
    revision: &str,
    graph: &EncodedGraph,
) -> bool {
    let objects = local_objects(graph).collect::<Vec<_>>();
    !objects.is_empty()
        && objects
            .iter()
            .all(|object| store.read(revision, object.address.as_str()).is_ok())
        && graph
            .entities
            .iter()
            .filter(|mapping| {
                objects
                    .iter()
                    .any(|object| object.address == mapping.object)
            })
            .all(|mapping| {
                store
                    .lookup(revision, &mapping.key.canonical())
                    .is_ok_and(|actual| {
                        actual.object == mapping.object.as_str()
                            && actual.part == mapping.part
                    })
            })
}

fn edges_pinned(store: &CodebaseStore, revision: &str, graph: &EncodedGraph) -> bool {
    local_objects(graph).all(|object| {
        store
            .read(revision, object.address.as_str())
            .is_ok_and(|read| {
                let expected = graph
                    .edges
                    .iter()
                    .filter(|edge| edge.owner == object.address)
                    .map(|edge| {
                        let target = match &edge.target {
                            EdgeTarget::Object(address) => format!("object:{address}"),
                            EdgeTarget::TrustedExternal { identity, registry } => {
                                format!("external:{identity}:{registry}")
                            }
                        };
                        (edge.part.clone(), edge.kind.as_str().to_owned(), target)
                    })
                    .collect::<BTreeSet<_>>();
                let actual = read
                    .references
                    .iter()
                    .map(|edge| {
                        let target = match &edge.target {
                            ReadTarget::Object(address) => format!("object:{address}"),
                            ReadTarget::InternalMember(index) => graph
                                .components
                                .iter()
                                .find(|component| {
                                    component.address == object.component_address
                                })
                                .and_then(|component| {
                                    usize::try_from(*index)
                                        .ok()
                                        .and_then(|index| component.members.get(index))
                                })
                                .map_or_else(
                                    || "invalid-internal-member".to_owned(),
                                    |address| format!("object:{address}"),
                                ),
                            ReadTarget::TrustedExternal { identity, registry } => {
                                format!("external:{identity}:{registry}")
                            }
                        };
                        (edge.part.clone(), edge.kind.clone(), target)
                    })
                    .collect::<BTreeSet<_>>();
                expected == actual
            })
    })
}

fn object_address<'a>(graph: &'a EncodedGraph, suffix: &str) -> Option<&'a str> {
    graph.objects.iter().find_map(|object| match &object.key {
        ObjectKey::Declaration { identity, .. } if identity.ends_with(suffix) => {
            Some(object.address.as_str())
        }
        _ => None,
    })
}

fn run_roundtrip(
    root: &Path,
    store: &CodebaseStore,
    revision: &str,
    graph: &EncodedGraph,
) -> Result<CaseObservation, HandlerError> {
    let plan = store
        .preview_export(revision, "exported")
        .map_err(handler_error)?;
    store
        .apply_export(revision, "exported", &plan.plan)
        .map_err(handler_error)?;
    let exported = prepare_import(root.join("exported")).map_err(handler_error)?;
    let mut facts = BTreeMap::new();
    let exact = graph.sources.iter().all(|source| {
        std::fs::read(root.join("exported").join(&source.origin))
            .is_ok_and(|bytes| bytes == source.text)
    });
    facts.insert("source-preserved", exact);
    let original = graph
        .objects
        .iter()
        .map(|object| (object.key.canonical(), object.address.as_str()))
        .collect::<BTreeMap<_, _>>();
    let roundtrip = exported
        .graph
        .objects
        .iter()
        .map(|object| (object.key.canonical(), object.address.as_str()))
        .collect::<BTreeMap<_, _>>();
    facts.insert(
        "addresses-preserved",
        original == roundtrip
            && graph.revision.address == exported.graph.revision.address,
    );
    let raced = store
        .preview_export(revision, "raced")
        .map_err(handler_error)?;
    std::fs::create_dir(root.join("raced")).map_err(handler_error)?;
    let result = store.apply_export(revision, "raced", &raced.plan);
    facts.insert(
        "stale-destination-rejected",
        result.is_err_and(|error| {
            error.code() == vibra_diagnostics::DiagnosticCode::CodebaseStalePlan
        }) && std::fs::read_dir(root.join("raced"))
            .is_ok_and(|mut entries| entries.next().is_none()),
    );
    observation("roundtrip", &facts)
}

fn run_integrity(
    root: &Path,
    store: &CodebaseStore,
    revision: &str,
    prepared: &PreparedImport,
) -> Result<CaseObservation, HandlerError> {
    use vibra_diagnostics::DiagnosticCode;
    store.backup("future.sqlite").map_err(handler_error)?;
    let future = rusqlite::Connection::open(root.join("future.sqlite"))
        .map_err(handler_error)?;
    future
        .pragma_update(None, "user_version", i32::MAX)
        .map_err(handler_error)?;
    drop(future);
    let version = CodebaseStore::open(root, "future.sqlite", StoreAccess::ReadOnly);
    std::fs::write(root.join("corrupt.sqlite"), b"not a SQLite database")
        .map_err(handler_error)?;
    let corrupt = CodebaseStore::open(root, "corrupt.sqlite", StoreAccess::ReadOnly);
    let connection =
        rusqlite::Connection::open(root.join(STORE)).map_err(handler_error)?;
    // Closed fixture mutations, never SQL supplied by a scenario document.
    connection.execute_batch("DELETE FROM index_documents; DELETE FROM revision_edges; DELETE FROM revision_origins; DELETE FROM revision_entities; DELETE FROM revision_sources; DELETE FROM revision_objects;").map_err(handler_error)?;
    drop(connection);
    store.reindex().map_err(handler_error)?;
    let connection =
        rusqlite::Connection::open(root.join(STORE)).map_err(handler_error)?;
    let rebuilt: Vec<u8> = connection
        .query_row(
            "SELECT payload_vibon FROM index_documents WHERE revision_address=?1",
            [revision],
            |row| row.get(0),
        )
        .map_err(handler_error)?;
    drop(connection);
    let rebuilt = std::str::from_utf8(&rebuilt).map_err(handler_error)?;
    let mut facts = BTreeMap::new();
    facts.insert(
        "corrupt-rejected",
        corrupt.is_err_and(|error| error.code() == DiagnosticCode::StoreCorrupt),
    );
    facts.insert(
        "version-rejected",
        version.is_err_and(|error| {
            error.code() == DiagnosticCode::StoreUnsupportedVersion
        }),
    );
    facts.insert(
        "indexes-rebuilt",
        index_facts(rebuilt)? == index_facts(&prepared.index.canonical_vibon())?
            && definitions_readable(store, revision, &prepared.graph)
            && edges_pinned(store, revision, &prepared.graph)
            && sources_equal(store, revision, &prepared.graph)?,
    );
    observation("integrity", &facts)
}

fn index_facts(text: &str) -> Result<BTreeMap<String, String>, HandlerError> {
    let document = parse_data("index.vibon", text).map_err(handler_error)?;
    let value = document
        .data()
        .ok_or_else(|| handler_error("missing index data"))?;
    let DataValue::Record(fields) = value.value() else {
        return Err(handler_error("index is not a record"));
    };
    Ok(fields
        .iter()
        .filter(|field| field.label().value() != "revision")
        .map(|field| {
            (
                field.label().value().to_owned(),
                vibra_syntax::canonical_data(field.value()),
            )
        })
        .collect())
}

struct TemporaryTree {
    root: std::path::PathBuf,
    project: std::path::PathBuf,
}

impl TemporaryTree {
    fn copy(case: &Case) -> Result<Self, HandlerError> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let temporary_root =
            std::env::temp_dir().canonicalize().map_err(handler_error)?;
        let root = loop {
            let path = temporary_root.join(format!(
                "vibra-codebase-corpus-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => break path,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    continue;
                }
                Err(error) => return Err(handler_error(error)),
            }
        };
        let tree = Self {
            project: root.join("project"),
            root,
        };
        std::fs::create_dir(&tree.project).map_err(handler_error)?;
        let prefix = case
            .manifest()
            .inputs
            .tree
            .as_deref()
            .ok_or_else(|| handler_error("missing tree"))?;
        for file in case.tree_files().map_err(handler_error)? {
            let relative = Path::new(&file.source_id)
                .strip_prefix(prefix)
                .map_err(handler_error)?;
            let destination = tree.project.join(relative);
            std::fs::create_dir_all(
                destination
                    .parent()
                    .ok_or_else(|| handler_error("source has no parent"))?,
            )
            .map_err(handler_error)?;
            std::fs::write(destination, file.bytes).map_err(handler_error)?;
        }
        Ok(tree)
    }
}

impl Drop for TemporaryTree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Scenario {
    name: String,
    edits: Vec<(String, String)>,
}

impl Scenario {
    fn apply_edits(
        &self,
        root: &Path,
        prepared: &PreparedImport,
    ) -> Result<(), HandlerError> {
        for (path, text) in &self.edits {
            if !prepared
                .graph
                .sources
                .iter()
                .any(|source| source.origin == *path)
            {
                return Err(handler_error(
                    "scenario edit does not replace a captured source",
                ));
            }
            std::fs::write(root.join(path), text).map_err(handler_error)?;
        }
        Ok(())
    }
}

fn decode_scenario(text: &str) -> Result<Scenario, String> {
    let document =
        parse_data("scenario.vibon", text).map_err(|error| error.to_string())?;
    if !document.diagnostics().is_empty() {
        return Err("scenario has reader diagnostics".to_owned());
    }
    let decoded = decode_data_root(document.root());
    let root = decoded
        .value()
        .ok_or("scenario is not a literal data value")?;
    let fields = record(root, &["format", "scenario", "edits"])?;
    if atom(field(&fields, "format")?)? != "codebase-case.v1" {
        return Err("unsupported codebase scenario format".to_owned());
    }
    let name = atom(field(&fields, "scenario")?)?;
    if ![
        "import",
        "lifecycle",
        "roundtrip",
        "stale",
        "integrity",
        "backup",
        "declarations",
    ]
    .contains(&name)
    {
        return Err("unknown codebase scenario".to_owned());
    }
    let DataValue::Array(values) = field(&fields, "edits")?.value() else {
        return Err("scenario edits must be an array".to_owned());
    };
    let mut edits = Vec::new();
    let mut paths = BTreeSet::new();
    for value in values {
        let fields = record(value, &["path", "text"])?;
        let path = string(field(&fields, "path")?)?;
        if path.is_empty()
            || path.contains('\\')
            || path.contains(':')
            || !path.ends_with(".vib")
            || Path::new(path)
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
            || !paths.insert(path.to_owned())
        {
            return Err(
                "scenario edits require unique confined source paths".to_owned()
            );
        }
        edits.push((path.to_owned(), string(field(&fields, "text")?)?.to_owned()));
    }
    Ok(Scenario {
        name: name.to_owned(),
        edits,
    })
}

fn record<'a>(
    node: &'a DataNode,
    expected: &[&str],
) -> Result<BTreeMap<String, &'a DataNode>, String> {
    let DataValue::Record(fields) = node.value() else {
        return Err("scenario requires a record".to_owned());
    };
    let fields = fields
        .iter()
        .map(|field| (field.label().value().to_owned(), field.value()))
        .collect::<BTreeMap<_, _>>();
    if fields.len() != expected.len()
        || expected.iter().any(|name| !fields.contains_key(*name))
    {
        return Err("scenario record has missing or unknown fields".to_owned());
    }
    Ok(fields)
}

fn field<'a>(
    fields: &BTreeMap<String, &'a DataNode>,
    name: &str,
) -> Result<&'a DataNode, String> {
    fields
        .get(name)
        .copied()
        .ok_or_else(|| format!("scenario is missing {name}"))
}

fn atom(node: &DataNode) -> Result<&str, String> {
    if let DataValue::Atom(name) = node.value() {
        Ok(name.value())
    } else {
        Err("scenario requires an atom".to_owned())
    }
}

fn string(node: &DataNode) -> Result<&str, String> {
    if let DataValue::Literal(Literal::String(value)) = node.value() {
        Ok(value.value())
    } else {
        Err("scenario requires a string".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::decode_scenario;

    #[test]
    fn declaration_coverage_is_an_explicit_closed_scenario() {
        assert!(decode_scenario("(record format: @codebase-case.v1 scenario: @declarations edits: (array))").is_ok());
    }

    #[test]
    fn scenarios_are_closed_data_and_edits_are_confined_source_replacements()
    -> Result<(), String> {
        let source = r#"(record format: @codebase-case.v1 scenario: @lifecycle edits: (array (record path: "src/app/main.vib" text: "(defn main () void void)")))"#;
        let scenario = decode_scenario(source)?;
        assert_eq!(scenario.name, "lifecycle");
        assert_eq!(
            scenario.edits,
            [(
                "src/app/main.vib".to_owned(),
                "(defn main () void void)".to_owned()
            )]
        );
        for invalid in [
            source.replace("@lifecycle", "@shell"),
            source.replace("src/app/main.vib", "../outside.vib"),
            source.replace("src/app/main.vib", "C:/outside.vib"),
            source.replace("src/app/main.vib", ".vibra/codebase.sqlite"),
            source.replace("text:", "shell:"),
            source.replace("edits:", "extra: void edits:"),
            source.replace("@codebase-case.v1", "@codebase-case.v2"),
        ] {
            assert!(decode_scenario(&invalid).is_err(), "accepted {invalid}");
        }
        Ok(())
    }
}

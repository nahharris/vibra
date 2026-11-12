use super::*;

pub(in crate::codebase) fn parse_canonical_record(
    payload: &[u8],
) -> Result<(), StoreError> {
    let mut frame = payload.to_vec();
    if !frame.ends_with(b"\n") {
        frame.push(b'\n');
    }
    crate::codebase::graph::parse_canonical_frame(&frame)
        .map(|_| ())
        .map_err(|error| {
            StoreError::Corrupt(format!("invalid canonical record: {error:?}"))
        })
}

pub(in crate::codebase) fn parse_path(
    payload: &[u8],
) -> Result<Vec<String>, StoreError> {
    let mut frame = payload.to_vec();
    if !frame.ends_with(b"\n") {
        frame.push(b'\n');
    }
    let node =
        crate::codebase::graph::parse_canonical_frame(&frame).map_err(|error| {
            StoreError::Corrupt(format!("invalid stored part path: {error:?}"))
        })?;
    let vibra_syntax::DataValue::Array(values) = node.value() else {
        return Err(StoreError::Corrupt(
            "stored part path is not an array".to_owned(),
        ));
    };
    values
        .iter()
        .map(|value| match value.value() {
            vibra_syntax::DataValue::Literal(vibra_syntax::Literal::String(value)) => {
                Ok(value.value().to_owned())
            }
            _ => Err(StoreError::Corrupt(
                "stored part path contains a non-string".to_owned(),
            )),
        })
        .collect()
}

pub(in crate::codebase) fn load_verified_revision(
    connection: &Connection,
    revision: &str,
    verify_indexes: bool,
) -> Result<vibra_syntax::DataNode, StoreError> {
    let payload = connection
        .query_row(
            "SELECT manifest_vibon FROM revisions WHERE address=?1",
            [revision],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()
        .map_err(map_sqlite_error)?
        .ok_or_else(|| StoreError::UnknownRevision(revision.to_owned()))?;
    let frame = crate::codebase::graph::validate_revision_payload(revision, &payload)
        .map_err(|error| {
        StoreError::Corrupt(format!("invalid revision {revision}: {error:?}"))
    })?;
    validate_revision_shape(&frame)?;
    source_files_from_manifest(connection, &frame)?;
    verify_revision_graph_closure(connection, revision, &frame)?;
    if verify_indexes {
        compare_revision_indexes(connection, revision, &frame)?;
    }
    Ok(frame)
}

pub(in crate::codebase) fn capture_generation(
    connection: &Connection,
) -> Result<StoreGeneration, StoreError> {
    let mut revisions = BTreeMap::new();
    let mut statement = connection
        .prepare("SELECT address,manifest_vibon FROM revisions ORDER BY address")
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
        })
        .map_err(map_sqlite_error)?;
    for row in rows {
        let (address, payload) = row.map_err(map_sqlite_error)?;
        revisions.insert(address, payload);
    }
    drop(statement);

    let mut heads = BTreeMap::new();
    let mut statement = connection
        .prepare("SELECT name,revision_address FROM heads ORDER BY name")
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(map_sqlite_error)?;
    for row in rows {
        let (name, revision) = row.map_err(map_sqlite_error)?;
        heads.insert(name, revision);
    }
    Ok(StoreGeneration { revisions, heads })
}

pub(in crate::codebase) fn verify_heads(
    connection: &Connection,
) -> Result<(), StoreError> {
    let mut statement = connection
        .prepare("SELECT name,revision_address FROM heads ORDER BY name")
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(map_sqlite_error)?;
    for row in rows {
        let (name, revision) = row.map_err(map_sqlite_error)?;
        validate_head_name(&name).map_err(|error| {
            StoreError::Corrupt(format!("stored head name is invalid: {error}"))
        })?;
        let exists = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM revisions WHERE address=?1)",
                [&revision],
                |row| row.get::<_, bool>(0),
            )
            .map_err(map_sqlite_error)?;
        if !exists {
            return Err(StoreError::Corrupt(format!(
                "head {name:?} points to missing revision {revision}"
            )));
        }
    }
    Ok(())
}

pub(in crate::codebase) fn verify_foreign_keys(
    connection: &Connection,
) -> Result<(), StoreError> {
    let mut statement = connection
        .prepare("PRAGMA foreign_key_check")
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })
        .map_err(map_sqlite_error)?;
    if let Some(row) = rows.into_iter().next() {
        let (table, rowid, parent, foreign_key) = row.map_err(map_sqlite_error)?;
        return Err(StoreError::Corrupt(format!(
            "foreign key violation in {table} row {rowid:?} referencing {parent} (constraint {foreign_key})"
        )));
    }
    Ok(())
}

pub(in crate::codebase) fn verify_authoritative(
    connection: &Connection,
    verify_indexes: bool,
) -> Result<VerifyReport, StoreError> {
    let integrity = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
        .map_err(map_sqlite_error)?;
    if integrity != "ok" {
        return Err(StoreError::Corrupt(format!(
            "SQLite integrity check failed: {integrity}"
        )));
    }
    let mut component_statement = connection
        .prepare("SELECT address,digest,payload_vibon FROM components ORDER BY address")
        .map_err(map_sqlite_error)?;
    let component_rows = component_statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, Vec<u8>>(2)?,
            ))
        })
        .map_err(map_sqlite_error)?;
    let mut component_members =
        BTreeMap::<String, (Vec<u8>, Vec<(Vec<u8>, String)>)>::new();
    for row in component_rows {
        let (address, digest, payload) = row.map_err(map_sqlite_error)?;
        let frame = crate::codebase::graph::validate_component_payload(
            &address, &digest, &payload,
        )
        .map_err(|error| {
            StoreError::Corrupt(format!("invalid component {address}: {error:?}"))
        })?;
        validate_component_shape(&frame)?;
        let members = array_field(&frame, "members")?
            .iter()
            .map(|member| {
                Ok((
                    required_field(member, "key")?.raw().as_bytes().to_vec(),
                    data_atom(required_field(member, "kind")?)?,
                ))
            })
            .collect::<Result<Vec<_>, StoreError>>()?;
        component_members.insert(address, (digest, members));
    }
    drop(component_statement);
    let mut object_statement = connection
        .prepare("SELECT address,component_address,member_index,kind,key_vibon FROM objects ORDER BY address")
        .map_err(map_sqlite_error)?;
    let object_rows = object_statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Vec<u8>>(4)?,
            ))
        })
        .map_err(map_sqlite_error)?;
    for row in object_rows {
        let (address, component, index, kind, key) = row.map_err(map_sqlite_error)?;
        let (digest, members) = component_members.get(&component).ok_or_else(|| {
            StoreError::Corrupt(format!(
                "object {address} refers to missing component {component}"
            ))
        })?;
        let index = u64::try_from(index).map_err(|_| {
            StoreError::Corrupt(format!("object {address} has negative member index"))
        })?;
        if object_address(
            digest.as_slice().try_into().map_err(|_| {
                StoreError::Corrupt(format!(
                    "component {component} digest has invalid length"
                ))
            })?,
            index,
        ) != address
        {
            return Err(StoreError::Corrupt(format!(
                "object address mismatch at {address}"
            )));
        }
        let member = members
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .ok_or_else(|| {
                StoreError::Corrupt(format!(
                    "object {address} index is outside its frame"
                ))
            })?;
        if member.0.as_slice() != key.as_slice() || member.1 != kind {
            return Err(StoreError::Corrupt(format!(
                "object row disagrees with component member {address}"
            )));
        }
    }
    drop(object_statement);
    let mut source_statement = connection
        .prepare("SELECT address,raw_hash,payload_vibon FROM source_attachments ORDER BY address")
        .map_err(map_sqlite_error)?;
    let source_rows = source_statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Vec<u8>>(2)?,
            ))
        })
        .map_err(map_sqlite_error)?;
    for row in source_rows {
        let (address, raw_hash, payload) = row.map_err(map_sqlite_error)?;
        let text = decode_source_wrapper(&payload)?;
        validate_source_record(&address, &raw_hash, &payload, text.as_bytes())?;
    }
    drop(source_statement);
    let mut revision_statement = connection
        .prepare("SELECT address,manifest_vibon FROM revisions ORDER BY address")
        .map_err(map_sqlite_error)?;
    let revision_rows = revision_statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
        })
        .map_err(map_sqlite_error)?;
    for row in revision_rows {
        let (address, payload) = row.map_err(map_sqlite_error)?;
        let frame =
            crate::codebase::graph::validate_revision_payload(&address, &payload)
                .map_err(|error| {
                    StoreError::Corrupt(format!(
                        "invalid revision {address}: {error:?}"
                    ))
                })?;
        validate_revision_shape(&frame)?;
        let sources = source_files_from_manifest(connection, &frame)?;
        if sources.is_empty() {
            return Err(StoreError::Corrupt(format!(
                "revision {address} has no source attachments"
            )));
        }
        verify_revision_graph_closure(connection, &address, &frame)?;
        if verify_indexes {
            compare_revision_indexes(connection, &address, &frame)?;
        }
    }
    drop(revision_statement);
    verify_heads(connection)?;
    if verify_indexes {
        verify_foreign_keys(connection)?;
    }

    let dangling_edges = connection
        .query_row(
            "SELECT count(*) FROM revision_edges e LEFT JOIN revision_objects t ON t.revision_address=e.revision_address AND t.object_address=e.target_value WHERE e.target_kind='object' AND t.object_address IS NULL",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(map_sqlite_error)?;
    if verify_indexes && dangling_edges != 0 {
        return Err(StoreError::Corrupt(format!(
            "{dangling_edges} revision edges have no member target"
        )));
    }
    // `revision_origins` is derived. Strict verification compares every row
    // with a graph re-encoded from the exact project attachments and the
    // revision's hash-pinned embedded standard-library modules. Trusted
    // stdlib source IDs intentionally have no project-source attachment and
    // are not materialized during export.
    let count = |table: &str| -> Result<u64, StoreError> {
        let value = connection
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get::<_, i64>(0)
            })
            .map_err(map_sqlite_error)?;
        u64::try_from(value)
            .map_err(|_| StoreError::Corrupt("negative SQLite row count".to_owned()))
    };
    let report = VerifyReport {
        revision_count: count("revisions")?,
        object_count: count("objects")?,
        source_count: count("source_attachments")?,
    };
    Ok(report)
}

pub(in crate::codebase) fn source_files_from_manifest(
    connection: &Connection,
    frame: &vibra_syntax::DataNode,
) -> Result<Vec<SourceFile>, StoreError> {
    let mut sources = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for entry in array_field(frame, "sources")? {
        let source_id = data_string(required_field(entry, "source-id")?)?;
        let address = data_string(required_field(entry, "attachment")?)?;
        let raw_hash = data_string(required_field(entry, "source-hash")?)?;
        let origin = data_string(required_field(entry, "origin")?)?;
        if !seen.insert(source_id.clone()) {
            return Err(StoreError::Corrupt(format!(
                "revision repeats source ID {source_id}"
            )));
        }
        let (stored_hash, wrapper): (String, Vec<u8>) = connection
            .query_row(
                "SELECT raw_hash,payload_vibon FROM source_attachments WHERE address=?1",
                [&address],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(map_sqlite_error)?
            .ok_or_else(|| StoreError::Corrupt(format!("revision refers to missing source attachment {address}")))?;
        if stored_hash != raw_hash {
            return Err(StoreError::Corrupt(format!(
                "source hash disagrees with manifest for {source_id}"
            )));
        }
        let text = decode_source_wrapper(&wrapper)?;
        validate_source_record(&address, &raw_hash, &wrapper, text.as_bytes())?;
        sources.push(SourceFile {
            source_id,
            origin,
            text: text.into_bytes(),
            raw_hash,
            address,
        });
    }
    Ok(sources)
}

pub(in crate::codebase) fn manifest_object_addresses(
    frame: &vibra_syntax::DataNode,
) -> Result<std::collections::BTreeSet<String>, StoreError> {
    let mut entities = std::collections::BTreeSet::new();
    let mut objects = std::collections::BTreeSet::new();
    for entry in array_field(frame, "objects")? {
        let key = required_field(entry, "key")?.raw().to_owned();
        let address = data_string(required_field(entry, "object")?)?;
        if !entities.insert(key) {
            return Err(StoreError::Corrupt(
                "revision repeats an entity key".to_owned(),
            ));
        }
        objects.insert(address);
    }
    if objects.is_empty() {
        return Err(StoreError::Corrupt(
            "revision has no checked objects".to_owned(),
        ));
    }
    Ok(objects)
}

pub(in crate::codebase) fn verify_revision_graph_closure(
    connection: &Connection,
    revision: &str,
    frame: &vibra_syntax::DataNode,
) -> Result<(), StoreError> {
    let expected_objects = manifest_object_addresses(frame)?;
    let mut seen_components = std::collections::BTreeSet::new();
    let mut components = BTreeMap::<String, vibra_syntax::DataNode>::new();
    for address in &expected_objects {
        let row = connection
            .query_row(
                "SELECT o.component_address,o.member_index,o.key_vibon,c.digest,c.payload_vibon FROM objects o JOIN components c ON c.address=o.component_address WHERE o.address=?1",
                [address],
                |row| Ok((
                    row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Vec<u8>>(3)?, row.get::<_, Vec<u8>>(4)?,
                )),
            )
            .optional()
            .map_err(map_sqlite_error)?
            .ok_or_else(|| StoreError::Corrupt(format!(
                "revision {revision} refers to missing object {address}"
            )))?;
        let (component_address, member_index, object_key, digest, payload) = row;
        let member_index = u64::try_from(member_index).map_err(|_| {
            StoreError::Corrupt(format!("object {address} has a negative member index"))
        })?;
        let digest_array: [u8; 32] = digest.as_slice().try_into().map_err(|_| {
            StoreError::Corrupt(format!(
                "component {component_address} has an invalid digest length"
            ))
        })?;
        if object_address(&digest_array, member_index) != *address {
            return Err(StoreError::Corrupt(format!(
                "object address mismatch at {address}"
            )));
        }
        if !components.contains_key(&component_address) {
            let parsed = crate::codebase::graph::validate_component_payload(
                &component_address,
                &digest,
                &payload,
            )
            .map_err(|error| {
                StoreError::Corrupt(format!(
                    "invalid component {component_address}: {error:?}"
                ))
            })?;
            validate_component_shape(&parsed)?;
            components.insert(component_address.clone(), parsed);
        }
        let component_frame = components.get(&component_address).ok_or_else(|| {
            StoreError::Corrupt(format!(
                "component {component_address} was not cached after validation"
            ))
        })?;
        let members = array_field(component_frame, "members")?;
        let selected_member = members
            .get(usize::try_from(member_index).unwrap_or(usize::MAX))
            .ok_or_else(|| {
                StoreError::Corrupt(format!(
                    "object {address} index is outside its component"
                ))
            })?;
        if required_field(selected_member, "key")?.raw().as_bytes()
            != object_key.as_slice()
        {
            return Err(StoreError::Corrupt(format!(
                "object row key disagrees with component member {address}"
            )));
        }
        if !seen_components.insert(component_address.clone()) {
            continue;
        }
        for index in 0..members.len() {
            let ordinal = u64::try_from(index).map_err(|_| {
                StoreError::Corrupt("component member count exceeds u64".to_owned())
            })?;
            let member_address = object_address(&digest_array, ordinal);
            if !expected_objects.contains(&member_address) {
                return Err(StoreError::Corrupt(format!(
                    "component {component_address} is only partially present in revision {revision}"
                )));
            }
            let present = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM objects WHERE address=?1 AND component_address=?2 AND member_index=?3)",
                    params![member_address, component_address, i64::try_from(ordinal).unwrap_or(i64::MAX)],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(map_sqlite_error)?;
            if !present {
                return Err(StoreError::Corrupt(format!(
                    "component {component_address} is missing member {member_address}"
                )));
            }
            let _references = read_member_references(
                connection,
                component_frame,
                &component_address,
                &digest_array,
                ordinal,
                &expected_objects,
                frame,
            )?;
        }
    }
    Ok(())
}

pub(in crate::codebase) fn verify_rebuilt_indexes(
    connection: &Connection,
) -> Result<(), StoreError> {
    let generation = capture_generation(connection)?;
    for (revision, manifest) in generation.revisions {
        let frame =
            crate::codebase::graph::validate_revision_payload(&revision, &manifest)
                .map_err(|error| {
                    StoreError::Corrupt(format!(
                        "invalid revision {revision}: {error:?}"
                    ))
                })?;
        validate_revision_shape(&frame)?;
        let sources = source_files_from_manifest(connection, &frame)?;
        let workspace = ReindexWorkspace::materialize(&sources)?;
        let prepared = crate::codebase::import::prepare_import(workspace.path())
            .map_err(|error| {
                StoreError::Corrupt(format!(
                    "stored revision {revision} no longer checks: {error}"
                ))
            })?;
        if prepared.graph.revision.address.as_str() != revision
            || prepared.graph.revision.payload != manifest
        {
            return Err(StoreError::Corrupt(format!(
                "rebuilding from exact source attachments produced a different revision than {revision}"
            )));
        }
        validate_graph(&prepared.graph)?;
        compare_derived_graph(connection, &prepared.graph, &prepared.index)?;
    }
    Ok(())
}

fn compare_derived_graph(
    connection: &Connection,
    graph: &EncodedGraph,
    index: &IndexDocument,
) -> Result<(), StoreError> {
    let revision = graph.revision.address.as_str();
    let mut expected_entities = BTreeMap::new();
    for entity in &graph.entities {
        expected_entities.insert(
            entity.key.canonical(),
            (
                entity.object.as_str().to_owned(),
                canonical_path(&entity.part).into_bytes(),
            ),
        );
    }
    let mut actual_entities = BTreeMap::new();
    let mut statement = connection
        .prepare("SELECT entity_key,object_address,part_path_vibon FROM revision_entities WHERE revision_address=?1")
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([revision], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Vec<u8>>(2)?,
            ))
        })
        .map_err(map_sqlite_error)?;
    for row in rows {
        let (key, object, part) = row.map_err(map_sqlite_error)?;
        actual_entities.insert(key, (object, part));
    }
    drop(statement);
    if actual_entities != expected_entities {
        return Err(StoreError::Corrupt(format!(
            "rebuilt entity index differs for revision {revision}"
        )));
    }

    let expected_objects = graph
        .objects
        .iter()
        .map(|object| object.address.as_str().to_owned())
        .collect::<std::collections::BTreeSet<_>>();
    let actual_objects = query_string_set(
        connection,
        "SELECT object_address FROM revision_objects WHERE revision_address=?1",
        revision,
    )?;
    if actual_objects != expected_objects {
        return Err(StoreError::Corrupt(format!(
            "rebuilt object index differs for revision {revision}"
        )));
    }

    let mut expected_sources = BTreeMap::new();
    for source in &graph.sources {
        expected_sources.insert(
            source.source_id.clone(),
            (
                source.address.as_str().to_owned(),
                source.raw_hash.as_str().to_owned(),
                source.origin.clone(),
            ),
        );
    }
    let mut actual_sources = BTreeMap::new();
    let mut statement = connection
        .prepare("SELECT source_id,source_address,source_hash,origin FROM revision_sources WHERE revision_address=?1")
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([revision], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(map_sqlite_error)?;
    for row in rows {
        let (source_id, address, hash, origin) = row.map_err(map_sqlite_error)?;
        actual_sources.insert(source_id, (address, hash, origin));
    }
    drop(statement);
    if actual_sources != expected_sources {
        return Err(StoreError::Corrupt(format!(
            "rebuilt source index differs for revision {revision}"
        )));
    }

    let expected_edges = graph
        .edges
        .iter()
        .map(|edge| {
            let (target_kind, target_value, target_registry) = match &edge.target {
                EdgeTarget::Object(address) => {
                    ("object".to_owned(), address.as_str().to_owned(), None)
                }
                EdgeTarget::TrustedExternal { identity, registry } => (
                    "trusted-external".to_owned(),
                    identity.clone(),
                    Some(registry.clone()),
                ),
            };
            (
                edge.owner.as_str().to_owned(),
                canonical_path(&edge.part).into_bytes(),
                edge.kind.as_str().to_owned(),
                target_kind,
                target_value,
                target_registry,
            )
        })
        .collect::<Vec<_>>();
    let mut actual_edges = Vec::new();
    let mut statement = connection
        .prepare("SELECT owner_address,part_path_vibon,kind,target_kind,target_value,target_registry FROM revision_edges WHERE revision_address=?1 ORDER BY edge_index")
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([revision], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })
        .map_err(map_sqlite_error)?;
    for row in rows {
        actual_edges.push(row.map_err(map_sqlite_error)?);
    }
    drop(statement);
    if actual_edges != expected_edges {
        return Err(StoreError::Corrupt(format!(
            "rebuilt edge index differs for revision {revision}"
        )));
    }

    let mut expected_origins = Vec::new();
    for object in &graph.objects {
        for origin in &object.origins {
            expected_origins.push((
                object.address.as_str().to_owned(),
                origin.source_id.clone(),
                i64::try_from(origin.span.start()).map_err(|_| {
                    StoreError::Corrupt("origin start exceeds SQLite range".to_owned())
                })?,
                i64::try_from(origin.span.end()).map_err(|_| {
                    StoreError::Corrupt("origin end exceeds SQLite range".to_owned())
                })?,
                canonical_path(&origin.part).into_bytes(),
            ));
        }
    }
    let mut actual_origins = Vec::new();
    let mut statement = connection
        .prepare("SELECT object_address,source_id,start_byte,end_byte,part_path_vibon FROM revision_origins WHERE revision_address=?1 ORDER BY origin_index")
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([revision], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Vec<u8>>(4)?,
            ))
        })
        .map_err(map_sqlite_error)?;
    for row in rows {
        actual_origins.push(row.map_err(map_sqlite_error)?);
    }
    drop(statement);
    if actual_origins != expected_origins {
        return Err(StoreError::Corrupt(format!(
            "rebuilt source-origin index differs for revision {revision}"
        )));
    }

    let stored_index: Vec<u8> = connection
        .query_row(
            "SELECT payload_vibon FROM index_documents WHERE revision_address=?1",
            [revision],
            |row| row.get(0),
        )
        .optional()
        .map_err(map_sqlite_error)?
        .ok_or_else(|| {
            StoreError::Corrupt(format!(
                "revision {revision} is missing its derived index document"
            ))
        })?;
    if stored_index != index.canonical_vibon().as_bytes() {
        return Err(StoreError::Corrupt(format!(
            "rebuilt index document differs for revision {revision}"
        )));
    }
    Ok(())
}

pub(in crate::codebase) struct ReindexWorkspace {
    path: PathBuf,
}

impl ReindexWorkspace {
    pub(in crate::codebase) fn materialize(
        sources: &[SourceFile],
    ) -> Result<Self, StoreError> {
        let temp_root = std::env::temp_dir().canonicalize().map_err(|error| {
            map_path_error(
                &std::env::temp_dir(),
                error,
                "cannot resolve temporary directory",
            )
        })?;
        let mut created_path = None;
        for _ in 0..16 {
            let serial = NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed);
            let path = temp_root
                .join(format!(".vibra-reindex-{}-{serial}", std::process::id()));
            match std::fs::create_dir(&path) {
                Ok(()) => {
                    created_path = Some(path);
                    break;
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(map_path_error(
                        &path,
                        error,
                        "cannot create private reindex workspace",
                    ));
                }
            }
        }
        let path = created_path.ok_or_else(|| {
            StoreError::Io("could not allocate a private reindex workspace".to_owned())
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
                .map_err(|error| {
                    map_path_error(&path, error, "cannot restrict reindex workspace")
                })?;
        }
        let root = ConfinedDir::open(&path).map_err(|error| {
            map_path_error(&path, error, "cannot open private reindex workspace")
        })?;
        let mut origins = std::collections::BTreeSet::new();
        for source in sources {
            let normalized = normalize_source_origin(&source.origin)?;
            if !origins.insert(normalized.clone()) {
                return Err(StoreError::Corrupt(format!(
                    "revision has duplicate source origin {normalized}"
                )));
            }
            let file = Path::new(&normalized);
            let file_name = file.file_name().ok_or_else(|| {
                StoreError::Corrupt(format!(
                    "source origin has no filename: {normalized}"
                ))
            })?;
            let parent_relative = file.parent().unwrap_or_else(|| Path::new(""));
            let parent = create_confined_parent(&root, parent_relative)?;
            parent
                .write_new_file(file_name, &source.text, None)
                .map_err(|error| {
                    map_path_error(
                        file,
                        error,
                        "cannot materialize exact source attachment",
                    )
                })?;
        }
        if !origins.contains("project.vibon") {
            return Err(StoreError::Corrupt(
                "revision omits the project.vibon attachment".to_owned(),
            ));
        }
        Ok(Self { path })
    }

    pub(in crate::codebase) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ReindexWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn normalize_source_origin(origin: &str) -> Result<String, StoreError> {
    if origin.is_empty() || origin.contains('\\') {
        return Err(StoreError::Corrupt(format!(
            "unsafe source origin {origin:?}"
        )));
    }
    let path = Path::new(origin);
    if path.is_absolute()
        || origin
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(StoreError::Corrupt(format!(
            "unsafe source origin {origin:?}"
        )));
    }
    Ok(origin.to_owned())
}

pub(in crate::codebase) fn insert_derived_rows(
    transaction: &rusqlite::Transaction<'_>,
    graph: &EncodedGraph,
    index: &IndexDocument,
) -> Result<(), StoreError> {
    let revision = graph.revision.address.as_str();
    for object in &graph.objects {
        transaction.execute(
            "INSERT INTO revision_objects(revision_address,object_address) VALUES (?1,?2)",
            params![revision, object.address.as_str()],
        ).map_err(map_sqlite_error)?;
    }
    for entity in &graph.entities {
        transaction.execute(
            "INSERT INTO revision_entities(revision_address,entity_key,object_address,part_path_vibon) VALUES (?1,?2,?3,?4)",
            params![revision, entity.key.canonical(), entity.object.as_str(), canonical_path(&entity.part).as_bytes()],
        ).map_err(map_sqlite_error)?;
    }
    for (edge_index, edge) in graph.edges.iter().enumerate() {
        let (target_kind, target_value, target_registry) = match &edge.target {
            EdgeTarget::Object(address) => ("object", address.as_str(), None),
            EdgeTarget::TrustedExternal { identity, registry } => (
                "trusted-external",
                identity.as_str(),
                Some(registry.as_str()),
            ),
        };
        transaction.execute(
            "INSERT INTO revision_edges(revision_address,edge_index,owner_address,part_path_vibon,kind,target_kind,target_value,target_registry) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![revision, i64::try_from(edge_index).unwrap_or(i64::MAX), edge.owner.as_str(), canonical_path(&edge.part).as_bytes(), edge.kind.as_str(), target_kind, target_value, target_registry],
        ).map_err(map_sqlite_error)?;
    }
    let mut origin_index = 0_i64;
    for object in &graph.objects {
        for origin in &object.origins {
            transaction.execute(
                "INSERT INTO revision_origins(revision_address,origin_index,object_address,source_id,start_byte,end_byte,part_path_vibon) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![revision, origin_index, object.address.as_str(), origin.source_id, i64::try_from(origin.span.start()).unwrap_or(i64::MAX), i64::try_from(origin.span.end()).unwrap_or(i64::MAX), canonical_path(&origin.part).as_bytes()],
            ).map_err(map_sqlite_error)?;
            origin_index = origin_index.checked_add(1).ok_or_else(|| {
                StoreError::Corrupt(
                    "origin count exceeds SQLite integer range".to_owned(),
                )
            })?;
        }
    }
    for source in &graph.sources {
        transaction.execute(
            "INSERT INTO revision_sources(revision_address,source_id,source_address,source_hash,origin) VALUES (?1,?2,?3,?4,?5)",
            params![revision, source.source_id, source.address.as_str(), source.raw_hash.as_str(), source.origin],
        ).map_err(map_sqlite_error)?;
    }
    transaction.execute(
        "INSERT INTO index_documents(revision_address,payload_vibon) VALUES (?1,?2)",
        params![revision, index.canonical_vibon().as_bytes()],
    ).map_err(map_sqlite_error)?;
    Ok(())
}

fn compare_revision_indexes(
    connection: &Connection,
    revision: &str,
    frame: &vibra_syntax::DataNode,
) -> Result<(), StoreError> {
    let mut expected_entities = BTreeMap::<String, (String, Vec<u8>)>::new();
    let mut expected_objects = std::collections::BTreeSet::<String>::new();
    for entry in array_field(frame, "objects")? {
        let key = required_field(entry, "key")?.raw().to_owned();
        let address = data_string(required_field(entry, "object")?)?;
        let part = required_field(entry, "part")?.raw().as_bytes().to_vec();
        expected_objects.insert(address.clone());
        if expected_entities.insert(key, (address, part)).is_some() {
            return Err(StoreError::Corrupt(
                "duplicate entity key in revision manifest".to_owned(),
            ));
        }
    }
    let mut actual_entities = BTreeMap::new();
    let mut statement = connection
        .prepare("SELECT entity_key,object_address,part_path_vibon FROM revision_entities WHERE revision_address=?1")
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([revision], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Vec<u8>>(2)?,
            ))
        })
        .map_err(map_sqlite_error)?;
    for row in rows {
        let (key, object, part) = row.map_err(map_sqlite_error)?;
        actual_entities.insert(key, (object, part));
    }
    drop(statement);
    if actual_entities != expected_entities {
        return Err(StoreError::Corrupt(format!(
            "revision entity index differs from manifest {revision}"
        )));
    }
    let actual_objects = query_string_set(
        connection,
        "SELECT object_address FROM revision_objects WHERE revision_address=?1",
        revision,
    )?;
    if actual_objects != expected_objects {
        return Err(StoreError::Corrupt(format!(
            "revision object membership differs from manifest {revision}"
        )));
    }
    let expected_sources = array_field(frame, "sources")?
        .iter()
        .map(|entry| {
            Ok((
                data_string(required_field(entry, "source-id")?)?,
                (
                    data_string(required_field(entry, "attachment")?)?,
                    data_string(required_field(entry, "source-hash")?)?,
                    data_string(required_field(entry, "origin")?)?,
                ),
            ))
        })
        .collect::<Result<BTreeMap<_, _>, StoreError>>()?;
    let mut actual_sources = BTreeMap::new();
    let mut statement = connection
        .prepare("SELECT source_id,source_address,source_hash,origin FROM revision_sources WHERE revision_address=?1")
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([revision], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(map_sqlite_error)?;
    for row in rows {
        let (source_id, address, hash, origin) = row.map_err(map_sqlite_error)?;
        actual_sources.insert(source_id, (address, hash, origin));
    }
    drop(statement);
    if actual_sources != expected_sources {
        return Err(StoreError::Corrupt(format!(
            "revision source index differs from manifest {revision}"
        )));
    }
    Ok(())
}

fn query_string_set(
    connection: &Connection,
    sql: &str,
    revision: &str,
) -> Result<std::collections::BTreeSet<String>, StoreError> {
    let mut statement = connection.prepare(sql).map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([revision], |row| row.get::<_, String>(0))
        .map_err(map_sqlite_error)?;
    rows.map(|row| row.map_err(map_sqlite_error)).collect()
}

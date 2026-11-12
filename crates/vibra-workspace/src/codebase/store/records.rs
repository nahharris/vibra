use super::*;

pub(in crate::codebase) fn validate_graph(
    graph: &EncodedGraph,
) -> Result<(), StoreError> {
    crate::codebase::graph::validate_encoded_graph(graph).map_err(|error| {
        StoreError::Corrupt(format!("invalid encoded graph: {error:?}"))
    })?;
    let revision = graph.revision.address.as_str();
    let revision_frame = crate::codebase::graph::validate_revision_payload(
        revision,
        &graph.revision.payload,
    )
    .map_err(|error| {
        StoreError::Corrupt(format!("invalid revision frame: {error:?}"))
    })?;
    validate_revision_shape(&revision_frame)?;

    let mut component_members = BTreeMap::<String, Vec<&str>>::new();
    for component in &graph.components {
        let component_frame = crate::codebase::graph::validate_component_payload(
            component.address.as_str(),
            &component.digest,
            &component.payload,
        )
        .map_err(|error| {
            StoreError::Corrupt(format!("invalid component frame: {error:?}"))
        })?;
        validate_component_shape(&component_frame)?;
        component_members.insert(
            component.address.as_str().to_owned(),
            component
                .members
                .iter()
                .map(|member| member.as_str())
                .collect(),
        );
    }
    if component_members.len() != graph.components.len() {
        return Err(StoreError::Corrupt(
            "duplicate component address in candidate".to_owned(),
        ));
    }
    let mut object_addresses =
        BTreeMap::<String, &crate::codebase::graph::EncodedObject>::new();
    for object in &graph.objects {
        validate_address_text(object.address.as_str())?;
        let digest = parse_address_digest(object.component_address.as_str())?;
        let expected_object = object_address(&digest, object.member_index);
        if expected_object != object.address.as_str() {
            return Err(StoreError::Corrupt(format!(
                "object address does not match component member index: {}",
                object.address
            )));
        }
        parse_canonical_record(object.key.canonical().as_bytes())?;
        if !component_members
            .get(object.component_address.as_str())
            .is_some_and(|members| {
                members.get(object.member_index as usize)
                    == Some(&object.address.as_str())
            })
        {
            return Err(StoreError::Corrupt(format!(
                "object {} is absent from its component membership",
                object.address
            )));
        }
        if object_addresses
            .insert(object.address.as_str().to_owned(), object)
            .is_some()
        {
            return Err(StoreError::Corrupt(
                "duplicate object address in candidate".to_owned(),
            ));
        }
    }
    for (component_address, members) in &component_members {
        let objects = graph
            .objects
            .iter()
            .filter(|object| object.component_address.as_str() == component_address)
            .collect::<Vec<_>>();
        if objects.len() != members.len()
            || objects.iter().enumerate().any(|(index, object)| {
                usize::try_from(object.member_index).ok() != Some(index)
                    || members.get(index) != Some(&object.address.as_str())
            })
        {
            return Err(StoreError::Corrupt(format!(
                "component membership is incomplete or out of order: {component_address}"
            )));
        }
    }
    for source in &graph.sources {
        let text = std::str::from_utf8(&source.text).map_err(|error| {
            StoreError::Corrupt(format!("source attachment is not UTF-8: {error}"))
        })?;
        let expected_payload = format!(
            "(record format: @source.v1 text: {})\n",
            canonical_string(text)
        );
        if source.payload != expected_payload.as_bytes() {
            return Err(StoreError::Corrupt(format!(
                "source wrapper is not canonical for {}",
                source.source_id
            )));
        }
        let source_text = decode_source_wrapper(&source.payload)?;
        validate_source_record(
            source.address.as_str(),
            source.raw_hash.as_str(),
            &source.payload,
            source_text.as_bytes(),
        )?;
        let raw_hash = format_address(&Sha256::digest(&source.text));
        let attachment_hash =
            hash_with_domain(b"vibra-codebase-source-v1\0", &source.payload);
        if source.raw_hash.as_str() != raw_hash
            || source.address.as_str() != format_address(&attachment_hash)
        {
            return Err(StoreError::Corrupt(format!(
                "source attachment hash mismatch for {}",
                source.source_id
            )));
        }
    }
    let source_ids = graph
        .sources
        .iter()
        .map(|source| source.source_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    if source_ids.len() != graph.sources.len() {
        return Err(StoreError::Corrupt(
            "duplicate source ID in candidate".to_owned(),
        ));
    }
    let stdlib = embedded_stdlib()?;
    let stdlib_source_ids = stdlib
        .modules()
        .iter()
        .map(|module| module.source_id())
        .collect::<std::collections::BTreeSet<_>>();
    if source_ids
        .iter()
        .any(|source_id| stdlib_source_ids.contains(source_id))
    {
        return Err(StoreError::Corrupt(
            "project source ID collides with an embedded standard-library source ID"
                .to_owned(),
        ));
    }
    for object in &graph.objects {
        for origin in &object.origins {
            let bytes = if let Some(source) = graph
                .sources
                .iter()
                .find(|source| source.source_id == origin.source_id)
            {
                source.text.as_slice()
            } else if let Some(module) = stdlib
                .modules()
                .iter()
                .find(|module| module.source_id() == origin.source_id)
            {
                // `validate_revision_shape` above verified that this module's
                // exact raw hash appears in the revision's @stdlib record.
                module.bytes()
            } else {
                return Err(StoreError::Corrupt(format!(
                    "object origin names an unattached or untrusted source {}",
                    origin.source_id
                )));
            };
            let start = origin.span.start();
            let end = origin.span.end();
            if std::str::from_utf8(bytes)
                .ok()
                .and_then(|text| text.get(start..end))
                .is_none()
            {
                return Err(StoreError::Corrupt(format!(
                    "object origin span {}..{} is outside UTF-8 source {}",
                    start, end, origin.source_id
                )));
            }
        }
    }
    let mut entities = std::collections::BTreeSet::new();
    for entity in &graph.entities {
        if !object_addresses.contains_key(entity.object.as_str()) {
            return Err(StoreError::Corrupt(format!(
                "entity maps to missing object {}",
                entity.object
            )));
        }
        let key = entity.key.canonical();
        parse_canonical_record(key.as_bytes())?;
        if !entities.insert(key) {
            return Err(StoreError::Corrupt(
                "duplicate entity key in candidate".to_owned(),
            ));
        }
    }
    for edge in &graph.edges {
        if !object_addresses.contains_key(edge.owner.as_str()) {
            return Err(StoreError::Corrupt(format!(
                "edge has missing owner {}",
                edge.owner
            )));
        }
        if let EdgeTarget::Object(target) = &edge.target
            && !object_addresses.contains_key(target.as_str())
        {
            return Err(StoreError::Corrupt(format!(
                "edge points outside candidate revision: {}",
                target
            )));
        }
        if let EdgeTarget::TrustedExternal { identity, registry } = &edge.target
            && (!(identity.starts_with("@compiler:") || identity.starts_with("@host:"))
                || parse_address_digest(registry).is_err())
        {
            return Err(StoreError::Corrupt(
                "invalid trusted external edge target".to_owned(),
            ));
        }
        if let EdgeTarget::TrustedExternal { identity, registry } = &edge.target {
            validate_trusted_edge_identity(identity, registry, &revision_frame)?;
        }
    }
    Ok(())
}

pub(in crate::codebase) fn validate_trusted_edge_identity(
    identity: &str,
    fingerprint: &str,
    revision_frame: &vibra_syntax::DataNode,
) -> Result<(), StoreError> {
    let (provider, symbol) = if let Some(symbol) = identity.strip_prefix("@compiler:") {
        ("compiler", symbol)
    } else if let Some(symbol) = identity.strip_prefix("@host:") {
        ("host", symbol)
    } else {
        return Err(StoreError::Corrupt(format!(
            "invalid trusted external identity {identity}"
        )));
    };
    if symbol.is_empty() {
        return Err(StoreError::Corrupt(format!(
            "trusted external identity {identity} has no symbol"
        )));
    }
    validate_address_text(fingerprint)?;

    let registry = array_field(revision_frame, "trusted")?
        .iter()
        .find(|entry| {
            required_field(entry, "kind")
                .and_then(data_atom)
                .is_ok_and(|kind| kind == "registry")
                && required_field(entry, "fingerprint")
                    .and_then(data_string)
                    .is_ok_and(|value| value == fingerprint)
        })
        .ok_or_else(|| {
            StoreError::Corrupt(format!(
                "trusted reference {identity} uses a registry absent from its revision"
            ))
        })?;
    let registry_frame = required_field(registry, "registry")?;
    let actual_provider = data_atom(required_field(registry_frame, "provider")?)?;
    if actual_provider != provider {
        return Err(StoreError::Corrupt(format!(
            "trusted reference {identity} is pinned to the {actual_provider} registry"
        )));
    }

    let known_symbol = match provider {
        "compiler" => array_field(registry_frame, "rows")?.iter().any(|row| {
            required_field(row, "symbol")
                .and_then(data_string)
                .is_ok_and(|candidate| candidate == symbol)
        }),
        "host" => vibra_ir::external::host::host_row(symbol).is_some(),
        _ => false,
    };
    if !known_symbol {
        return Err(StoreError::Corrupt(format!(
            "trusted reference {identity} names no symbol in the {provider} registry"
        )));
    }
    Ok(())
}

pub(in crate::codebase) fn verify_immutable_revision(
    transaction: &rusqlite::Transaction<'_>,
    graph: &EncodedGraph,
) -> Result<(), StoreError> {
    let revision = graph.revision.address.as_str();
    let stored: Vec<u8> = transaction
        .query_row(
            "SELECT manifest_vibon FROM revisions WHERE address=?1",
            [revision],
            |row| row.get(0),
        )
        .optional()
        .map_err(map_sqlite_error)?
        .ok_or_else(|| {
            StoreError::Corrupt("head points to a missing revision".to_owned())
        })?;
    if stored != graph.revision.payload {
        return Err(StoreError::Corrupt("revision address collision".to_owned()));
    }
    Ok(())
}

pub(in crate::codebase) fn validate_address_text(
    address: &str,
) -> Result<(), StoreError> {
    parse_address_digest(address).map(|_| ())
}

pub(in crate::codebase) fn parse_address_digest(
    address: &str,
) -> Result<[u8; 32], StoreError> {
    let Some(hex) = address.strip_prefix("sha256:") else {
        return Err(StoreError::InvalidInput(format!(
            "invalid SHA-256 address: {address:?}"
        )));
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(StoreError::InvalidInput(format!(
            "invalid SHA-256 address: {address:?}"
        )));
    }
    let mut digest = [0_u8; 32];
    for (slot, pair) in digest.iter_mut().zip(hex.as_bytes().chunks_exact(2)) {
        let [high_byte, low_byte] = pair else {
            return Err(StoreError::InvalidInput(address.to_owned()));
        };
        let high = hex_nibble(*high_byte)
            .ok_or_else(|| StoreError::InvalidInput(address.to_owned()))?;
        let low = hex_nibble(*low_byte)
            .ok_or_else(|| StoreError::InvalidInput(address.to_owned()))?;
        *slot = high << 4 | low;
    }
    Ok(digest)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

pub(in crate::codebase) fn format_address(digest: &[u8]) -> String {
    let mut address = String::from("sha256:");
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(address, "{byte:02x}");
    }
    address
}

pub(in crate::codebase) fn hash_with_domain(domain: &[u8], bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(bytes);
    hasher.finalize().into()
}

pub(in crate::codebase) fn object_address(
    component_digest: &[u8; 32],
    member_index: u64,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"vibra-codebase-object-v1\0");
    hasher.update(component_digest);
    hasher.update(member_index.to_be_bytes());
    format_address(&hasher.finalize())
}

pub(in crate::codebase) fn canonical_path(path: &[String]) -> String {
    let values = path
        .iter()
        .map(|value| canonical_string(value))
        .collect::<Vec<_>>();
    canonical_array(&values)
}

pub(in crate::codebase) fn validate_component_shape(
    frame: &vibra_syntax::DataNode,
) -> Result<(), StoreError> {
    let format = data_atom(required_field(frame, "format")?)?;
    ensure_supported_record_format(
        &format,
        "checked-component.v1",
        "checked component",
    )?;
    validate_record_fields(frame, &["format", "members"])?;
    let members = array_field(frame, "members")?;
    if members.is_empty() {
        return Err(StoreError::Corrupt(
            "component frame must contain members".to_owned(),
        ));
    }
    for member in members {
        validate_record_fields(member, &["key", "kind", "definition", "references"])?;
        validate_key_record(required_field(member, "key")?)?;
        let kind = required_field(member, "kind")?;
        if !matches!(
            data_atom(kind)?.as_str(),
            "def"
                | "defn"
                | "deftype"
                | "defint"
                | "deffect"
                | "implementation"
                | "test"
        ) {
            return Err(StoreError::Corrupt(
                "component member has an unknown object kind".to_owned(),
            ));
        }
        validate_definition(required_field(member, "definition")?)?;
        for reference in array_field(member, "references")? {
            validate_record_fields(reference, &["part", "kind", "target"])?;
            validate_string_array(required_field(reference, "part")?)?;
            if !matches!(
                data_atom(required_field(reference, "kind")?)?.as_str(),
                "value-reference"
                    | "type-reference"
                    | "contract"
                    | "implementation"
                    | "effect-owner"
                    | "initializer"
                    | "call"
            ) {
                return Err(StoreError::Corrupt(
                    "component reference has an unknown kind".to_owned(),
                ));
            }
            validate_target_record(required_field(reference, "target")?)?;
        }
    }
    Ok(())
}

pub(in crate::codebase) fn validate_definition(
    node: &vibra_syntax::DataNode,
) -> Result<(), StoreError> {
    let format = data_atom(required_field(node, "format")?)?;
    ensure_supported_record_format(
        &format,
        "checked-definition.v1",
        "checked definition",
    )?;
    validate_record_fields(node, &["format", "kind", "form"])?;
    validate_definition_node(required_field(node, "form")?)
}

pub(in crate::codebase) fn validate_definition_node(
    node: &vibra_syntax::DataNode,
) -> Result<(), StoreError> {
    let mut pending = vec![node];
    while let Some(node) = pending.pop() {
        let kind = data_atom(required_field(node, "kind")?)?;
        let fields: &[&str] = match kind.as_str() {
            "list" => &["kind", "items"],
            "atom" | "atom-value" | "bytes" => &["kind", "value"],
            "label" => &["kind", "name"],
            "string" | "character" => &["kind", "value"],
            "integer" | "float" => &["kind", "type", "value"],
            "void" => &["kind"],
            "reference" => &["kind", "identity"],
            "binder" => &["kind", "binder-kind", "index"],
            _ => {
                return Err(StoreError::Corrupt(format!(
                    "unknown checked definition node kind {kind}"
                )));
            }
        };
        validate_record_fields(node, fields)?;
        match kind.as_str() {
            "list" => pending.extend(array_field(node, "items")?.iter().rev()),
            "atom" | "label" | "string" => {
                let label = if kind == "label" { "name" } else { "value" };
                data_string(required_field(node, label)?)?;
            }
            "character" => {
                let value = data_string(required_field(node, "value")?)?;
                if value.chars().count() != 1 {
                    return Err(StoreError::Corrupt(
                        "checked character must contain one Unicode scalar".to_owned(),
                    ));
                }
            }
            "atom-value" => {
                let value = data_string(required_field(node, "value")?)?;
                if value.starts_with('@') {
                    return Err(StoreError::Corrupt(
                        "checked atom-value must not include its source @ prefix"
                            .to_owned(),
                    ));
                }
            }
            "bytes" => {
                let value = data_string(required_field(node, "value")?)?;
                if value.len() % 2 != 0
                    || !value.bytes().all(|byte| {
                        byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
                    })
                {
                    return Err(StoreError::Corrupt(
                        "checked bytes must use lowercase hexadecimal pairs".to_owned(),
                    ));
                }
            }
            "integer" => {
                let type_name = data_atom(required_field(node, "type")?)?;
                let value = data_string(required_field(node, "value")?)?;
                validate_definition_integer(&type_name, &value)?;
            }
            "float" => {
                let type_name = data_atom(required_field(node, "type")?)?;
                let value = data_string(required_field(node, "value")?)?;
                validate_definition_float(&type_name, &value)?;
            }
            "reference" => {
                let identity = data_string(required_field(node, "identity")?)?;
                let trusted_symbol = identity
                    .strip_prefix("@compiler:")
                    .or_else(|| identity.strip_prefix("@host:"));
                if let Some(symbol) = trusted_symbol {
                    if !matches!(
                        vibra_syntax::classify_name(symbol),
                        vibra_syntax::NameClassification::Name(name)
                            if name.kind() == vibra_syntax::NameKind::Symbol
                    ) {
                        return Err(StoreError::Corrupt(
                            "checked trusted reference has an invalid symbol"
                                .to_owned(),
                        ));
                    }
                } else {
                    let key = format!(
                        "(record kind: @declaration identity: {} entity-kind: @function)",
                        canonical_string(&identity)
                    );
                    if !crate::codebase::is_valid_entity_key(&key) {
                        return Err(StoreError::Corrupt(
                            "checked reference is not a canonical declaration identity"
                                .to_owned(),
                        ));
                    }
                }
            }
            "binder" => {
                let binder_kind = data_atom(required_field(node, "binder-kind")?)?;
                if !matches!(binder_kind.as_str(), "type" | "value") {
                    return Err(StoreError::Corrupt(
                        "checked binder kind must be type or value".to_owned(),
                    ));
                }
                data_u64(required_field(node, "index")?)?;
            }
            "void" => {}
            _ => unreachable!("definition kind was closed above"),
        }
    }
    Ok(())
}

fn validate_definition_integer(type_name: &str, value: &str) -> Result<(), StoreError> {
    if vibra_syntax::primitive_class(type_name)
        != Some(vibra_syntax::PrimitiveClass::Integer)
    {
        return Err(StoreError::Corrupt(
            "checked integer has a non-integer type atom".to_owned(),
        ));
    }
    let token = format!("{value}{type_name}");
    let suffix = match vibra_syntax::classify_literal(&token) {
        vibra_syntax::LiteralClassification::Literal(
            vibra_syntax::Literal::Integer(literal),
        ) => literal.suffix().map(vibra_syntax::IntegerSuffix::as_str),
        _ => None,
    };
    if suffix != Some(type_name) {
        return Err(StoreError::Corrupt(
            "checked integer value is not canonical base-10 text".to_owned(),
        ));
    }
    let canonical = match type_name {
        "i8" => value.parse::<i8>().map(|value| value.to_string()),
        "i16" => value.parse::<i16>().map(|value| value.to_string()),
        "i32" => value.parse::<i32>().map(|value| value.to_string()),
        "i64" => value.parse::<i64>().map(|value| value.to_string()),
        "u8" => value.parse::<u8>().map(|value| value.to_string()),
        "u16" => value.parse::<u16>().map(|value| value.to_string()),
        "u32" => value.parse::<u32>().map(|value| value.to_string()),
        "u64" => value.parse::<u64>().map(|value| value.to_string()),
        _ => unreachable!("primitive integer class was checked above"),
    }
    .map_err(|_| {
        StoreError::Corrupt(
            "checked integer value is outside its type range".to_owned(),
        )
    })?;
    if canonical != value {
        return Err(StoreError::Corrupt(
            "checked integer value is not canonical base-10 text".to_owned(),
        ));
    }
    Ok(())
}

fn validate_definition_float(type_name: &str, value: &str) -> Result<(), StoreError> {
    if vibra_syntax::primitive_class(type_name)
        != Some(vibra_syntax::PrimitiveClass::Float)
    {
        return Err(StoreError::Corrupt(
            "checked float has a non-float type atom".to_owned(),
        ));
    }
    if !matches!(value, "nan" | "inf" | "-inf")
        && !matches!(
            vibra_syntax::classify_literal(value),
            vibra_syntax::LiteralClassification::Literal(
                vibra_syntax::Literal::Float(literal)
            ) if literal.suffix().is_none()
        )
    {
        return Err(StoreError::Corrupt(
            "checked float value is not a canonical float literal".to_owned(),
        ));
    }
    let canonical = match (type_name, value) {
        ("f32", "nan") => vibra_ir::canonical_f32_text(f32::NAN),
        ("f32", "inf") => vibra_ir::canonical_f32_text(f32::INFINITY),
        ("f32", "-inf") => vibra_ir::canonical_f32_text(f32::NEG_INFINITY),
        ("f32", value) => {
            let parsed = value.parse::<f32>().map_err(|_| {
                StoreError::Corrupt("checked float value is invalid for f32".to_owned())
            })?;
            vibra_ir::canonical_f32_text(parsed)
        }
        ("f64", "nan") => vibra_ir::canonical_f64_text(f64::NAN),
        ("f64", "inf") => vibra_ir::canonical_f64_text(f64::INFINITY),
        ("f64", "-inf") => vibra_ir::canonical_f64_text(f64::NEG_INFINITY),
        ("f64", value) => {
            let parsed = value.parse::<f64>().map_err(|_| {
                StoreError::Corrupt("checked float value is invalid for f64".to_owned())
            })?;
            vibra_ir::canonical_f64_text(parsed)
        }
        _ => unreachable!("primitive float class was checked above"),
    };
    if canonical != value {
        return Err(StoreError::Corrupt(
            "checked float value is not canonical round-tripping text".to_owned(),
        ));
    }
    Ok(())
}

pub(in crate::codebase) fn validate_key_record(
    node: &vibra_syntax::DataNode,
) -> Result<(), StoreError> {
    let kind = data_atom(required_field(node, "kind")?)?;
    let fields: &[&str] = match kind.as_str() {
        "declaration" => &["kind", "identity", "entity-kind"],
        "test" => &["kind", "module", "name"],
        "implementation" => &["kind", "receiver", "interface"],
        _ => {
            return Err(StoreError::Corrupt(format!(
                "unknown checked object key kind {kind}"
            )));
        }
    };
    validate_record_fields(node, fields)
}

pub(in crate::codebase) fn validate_entity_key_record(
    node: &vibra_syntax::DataNode,
) -> Result<(), StoreError> {
    let kind = data_atom(required_field(node, "kind")?)?;
    let fields: &[&str] = match kind.as_str() {
        "declaration" => &["kind", "identity", "entity-kind"],
        "test" => &["kind", "module", "name"],
        "implementation" => &["kind", "receiver", "interface"],
        "implementation-member" => &["kind", "contract", "receiver", "interface"],
        _ => {
            return Err(StoreError::Corrupt(format!(
                "unknown revision entity key kind {kind}"
            )));
        }
    };
    validate_record_fields(node, fields)?;
    if matches!(kind.as_str(), "implementation" | "implementation-member") {
        for label in ["receiver", "interface"] {
            let text = data_string(required_field(node, label)?)?;
            validate_normalized_type_version(&text)?;
        }
    }
    let canonical = vibra_syntax::canonical_data(node);
    if !crate::codebase::is_valid_entity_key(&canonical) {
        return Err(StoreError::Corrupt(
            "revision contains an invalid canonical entity key".to_owned(),
        ));
    }
    Ok(())
}

pub(in crate::codebase) fn validate_normalized_type_version(
    text: &str,
) -> Result<(), StoreError> {
    let mut frame = text.as_bytes().to_vec();
    frame.push(b'\n');
    if let Ok(parsed) = crate::codebase::graph::parse_canonical_frame(&frame)
        && let Ok(format) = required_field(&parsed, "format").and_then(data_atom)
    {
        ensure_supported_record_format(
            &format,
            "normalized-type.v1",
            "normalized type",
        )?;
    }
    Ok(())
}

pub(in crate::codebase) fn validate_target_record(
    node: &vibra_syntax::DataNode,
) -> Result<(), StoreError> {
    let kind = data_atom(required_field(node, "kind")?)?;
    match kind.as_str() {
        "component-member" => {
            validate_record_fields(node, &["kind", "index"])?;
            data_u64(required_field(node, "index")?)?;
        }
        "object" => {
            validate_record_fields(node, &["kind", "address"])?;
            validate_address_text(
                data_string(required_field(node, "address")?)?.as_str(),
            )?;
        }
        "trusted-external" => {
            validate_record_fields(node, &["kind", "identity", "registry"])?;
            let identity = data_string(required_field(node, "identity")?)?;
            if !(identity.starts_with("@compiler:") || identity.starts_with("@host:")) {
                return Err(StoreError::Corrupt(
                    "invalid trusted external identity".to_owned(),
                ));
            }
            validate_address_text(
                data_string(required_field(node, "registry")?)?.as_str(),
            )?;
        }
        _ => {
            return Err(StoreError::Corrupt(format!(
                "unknown reference target kind {kind}"
            )));
        }
    }
    Ok(())
}

pub(in crate::codebase) fn validate_revision_shape(
    frame: &vibra_syntax::DataNode,
) -> Result<(), StoreError> {
    let format = data_atom(required_field(frame, "format")?)?;
    ensure_supported_record_format(
        &format,
        "codebase-revision.v1",
        "codebase revision",
    )?;
    validate_record_fields(
        frame,
        &["format", "package", "trusted", "objects", "sources"],
    )?;
    validate_record_fields(required_field(frame, "package")?, &["name", "version"])?;
    validate_trusted_records(array_field(frame, "trusted")?)?;
    for entry in array_field(frame, "objects")? {
        validate_record_fields(entry, &["key", "object", "part"])?;
        validate_entity_key_record(required_field(entry, "key")?)?;
        validate_address_text(data_string(required_field(entry, "object")?)?.as_str())?;
        validate_string_array(required_field(entry, "part")?)?;
    }
    for entry in array_field(frame, "sources")? {
        validate_record_fields(
            entry,
            &["source-id", "attachment", "source-hash", "origin"],
        )?;
        data_string(required_field(entry, "source-id")?)?;
        validate_address_text(
            data_string(required_field(entry, "attachment")?)?.as_str(),
        )?;
        validate_address_text(
            data_string(required_field(entry, "source-hash")?)?.as_str(),
        )?;
        data_string(required_field(entry, "origin")?)?;
    }
    Ok(())
}

pub(in crate::codebase) fn validate_trusted_records(
    records: &[vibra_syntax::DataNode],
) -> Result<(), StoreError> {
    let stdlib = embedded_stdlib()?;
    let mut expected = Vec::<(String, String)>::new();
    let mut module_records = Vec::new();
    for module in stdlib.modules() {
        let identity = format!(
            "@{}@{}/{}",
            stdlib.package().name(),
            stdlib.package().version(),
            module.atom()
        );
        module_records.push((
            identity.clone(),
            format!(
                "(record module: {} source-hash: {})",
                canonical_string(&identity),
                canonical_string(&format_address(&Sha256::digest(module.bytes())))
            ),
        ));
    }
    module_records.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    let modules = canonical_array(
        &module_records
            .iter()
            .map(|(_, record)| record.clone())
            .collect::<Vec<_>>(),
    );
    expected.push((
        format!("1:@stdlib:{}@{}", stdlib.package().name(), stdlib.package().version()),
        format!(
            "(record kind: @stdlib package-name: {} package-version: {} manifest: {} modules: {})",
            canonical_string(stdlib.package().name()),
            canonical_string(stdlib.package().version()),
            canonical_string(stdlib.manifest_hash()),
            modules
        ),
    ));

    let host_snapshot = vibra_ir::external::host::registry_snapshot();
    let minor = host_snapshot
        .split_whitespace()
        .nth(2)
        .and_then(|version| version.strip_prefix("1."))
        .and_then(|minor| minor.parse::<u32>().ok())
        .ok_or_else(|| {
            StoreError::Corrupt(
                "embedded host registry version is malformed".to_owned(),
            )
        })?;
    let host_payload = canonical_vibon_frame(&format!(
        "(record format: @codebase-registry.v1 provider: @host version: {} minor: {}u32 snapshot: {})\n",
        canonical_string("vibra_v1"),
        minor,
        canonical_string(&host_snapshot)
    ))?;
    let host_fingerprint = format_address(&hash_with_domain(
        b"vibra-codebase-registry-v1\0",
        host_payload.as_bytes(),
    ));
    expected.push((
        "0:@host".to_owned(),
        format!(
            "(record kind: @registry fingerprint: {} registry: {})",
            canonical_string(&host_fingerprint),
            host_payload.trim_end()
        ),
    ));

    let roles = vibra_ir::external::RoleTypes::new(None);
    let mut compiler_rows = vibra_ir::external::CompilerIntrinsic::all()
        .into_iter()
        .map(|intrinsic| {
            let type_parameters = canonical_array(
                &intrinsic
                    .type_parameters()
                    .iter()
                    .map(|parameter| canonical_string(parameter))
                    .collect::<Vec<_>>(),
            );
            let signature = vibra_ir::canonical_type(&vibra_ir::Type::Function(
                Box::new(intrinsic.signature(&roles)),
            ));
            (
                intrinsic.symbol().to_owned(),
                format!(
                    "(record symbol: {} type-parameters: {} signature: {} native: {})",
                    canonical_string(intrinsic.symbol()),
                    type_parameters,
                    canonical_string(&signature),
                    intrinsic.is_native()
                ),
            )
        })
        .collect::<Vec<_>>();
    compiler_rows.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    let rows = canonical_array(
        &compiler_rows
            .iter()
            .map(|(_, row)| row.clone())
            .collect::<Vec<_>>(),
    );
    let compiler_payload = canonical_vibon_frame(&format!(
        "(record format: @codebase-registry.v1 provider: @compiler version: {} rows: {})\n",
        canonical_string("compiler-v1"),
        rows
    ))?;
    let compiler_fingerprint = format_address(&hash_with_domain(
        b"vibra-codebase-registry-v1\0",
        compiler_payload.as_bytes(),
    ));
    expected.push((
        "0:@compiler".to_owned(),
        format!(
            "(record kind: @registry fingerprint: {} registry: {})",
            canonical_string(&compiler_fingerprint),
            compiler_payload.trim_end()
        ),
    ));
    for (_, record) in &mut expected {
        let frame = canonical_vibon_frame(&format!("{record}\n"))?;
        *record = frame
            .strip_suffix('\n')
            .ok_or_else(|| {
                StoreError::Corrupt(
                    "canonical trust record lacks a final newline".to_owned(),
                )
            })?
            .to_owned();
    }
    expected.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));

    let actual = records
        .iter()
        .map(|record| {
            let kind = data_atom(required_field(record, "kind")?)?;
            let sort_key = match kind.as_str() {
                "stdlib" => format!(
                    "1:@stdlib:{}@{}",
                    data_string(required_field(record, "package-name")?)?,
                    data_string(required_field(record, "package-version")?)?
                ),
                "registry" => {
                    validate_record_fields(
                        record,
                        &["kind", "fingerprint", "registry"],
                    )?;
                    let payload = required_field(record, "registry")?;
                    let format = data_atom(required_field(payload, "format")?)?;
                    ensure_supported_record_format(
                        &format,
                        "codebase-registry.v1",
                        "codebase registry",
                    )?;
                    let provider = data_atom(required_field(payload, "provider")?)?;
                    let expected_fields = if provider == "host" {
                        &["format", "provider", "version", "minor", "snapshot"][..]
                    } else if provider == "compiler" {
                        &["format", "provider", "version", "rows"][..]
                    } else {
                        return Err(StoreError::Corrupt(format!(
                            "unknown trusted registry provider {provider}"
                        )));
                    };
                    validate_record_fields(payload, expected_fields)?;
                    let canonical = vibra_syntax::canonical_data(payload);
                    let mut bytes = canonical.into_bytes();
                    bytes.push(b'\n');
                    let fingerprint = format_address(&hash_with_domain(
                        b"vibra-codebase-registry-v1\0",
                        &bytes,
                    ));
                    if data_string(required_field(record, "fingerprint")?)?
                        != fingerprint
                    {
                        return Err(StoreError::Corrupt(
                            "trusted registry fingerprint mismatch".to_owned(),
                        ));
                    }
                    let expected_provider_version = if provider == "host" {
                        "vibra_v1"
                    } else {
                        "compiler-v1"
                    };
                    if data_string(required_field(payload, "version")?)?
                        != expected_provider_version
                    {
                        return Err(StoreError::Corrupt(
                            "trusted registry version mismatch".to_owned(),
                        ));
                    }
                    format!("0:@{provider}")
                }
                _ => {
                    return Err(StoreError::Corrupt(format!(
                        "unknown trusted input kind {kind}"
                    )));
                }
            };
            Ok((sort_key, vibra_syntax::canonical_data(record)))
        })
        .collect::<Result<Vec<_>, StoreError>>()?;
    if actual != expected {
        return Err(StoreError::Corrupt(
            "trusted standard library or registry records differ from this toolchain"
                .to_owned(),
        ));
    }
    Ok(())
}

pub(in crate::codebase) fn canonical_vibon_frame(
    text: &str,
) -> Result<String, StoreError> {
    let document =
        vibra_syntax::parse_data("trusted-record.vibon", text).map_err(|error| {
            StoreError::Corrupt(format!("cannot parse embedded trust record: {error}"))
        })?;
    if !document.accepted() || document.recovered() {
        return Err(StoreError::Corrupt(
            "embedded trust record is not accepted canonical VIBON".to_owned(),
        ));
    }
    let node = document.into_data().ok_or_else(|| {
        StoreError::Corrupt(
            "embedded trust record has no decoded VIBON value".to_owned(),
        )
    })?;
    let mut frame = vibra_syntax::canonical_data(&node);
    frame.push('\n');
    Ok(frame)
}

pub(in crate::codebase) fn embedded_stdlib()
-> Result<&'static vibra_types::Stdlib, StoreError> {
    static STDLIB: OnceLock<Result<vibra_types::Stdlib, String>> = OnceLock::new();
    match STDLIB
        .get_or_init(|| vibra_types::load_stdlib().map_err(|error| error.to_string()))
    {
        Ok(stdlib) => Ok(stdlib),
        Err(message) => Err(StoreError::Corrupt(format!(
            "cannot verify embedded standard library: {message}"
        ))),
    }
}

pub(in crate::codebase) fn decode_source_wrapper(
    payload: &[u8],
) -> Result<String, StoreError> {
    let frame =
        crate::codebase::graph::parse_canonical_frame(payload).map_err(|error| {
            StoreError::Corrupt(format!("invalid source wrapper: {error:?}"))
        })?;
    let format = data_atom(required_field(&frame, "format")?)?;
    ensure_supported_record_format(&format, "source.v1", "source attachment")?;
    validate_record_fields(&frame, &["format", "text"])?;
    data_string(required_field(&frame, "text")?)
}

pub(in crate::codebase) fn validate_source_record(
    address: &str,
    raw_hash: &str,
    wrapper: &[u8],
    text: &[u8],
) -> Result<(), StoreError> {
    let text = std::str::from_utf8(text).map_err(|error| {
        StoreError::Corrupt(format!("source attachment text is not UTF-8: {error}"))
    })?;
    let expected_wrapper = format!(
        "(record format: @source.v1 text: {})",
        canonical_string(text)
    );
    if wrapper != expected_wrapper.as_bytes()
        && wrapper != format!("{expected_wrapper}\n").as_bytes()
    {
        return Err(StoreError::Corrupt(
            "source wrapper does not match exact attached text".to_owned(),
        ));
    }
    if format_address(&Sha256::digest(text.as_bytes())) != raw_hash {
        return Err(StoreError::Corrupt(
            "raw source hash does not match attached bytes".to_owned(),
        ));
    }
    if format_address(&hash_with_domain(b"vibra-codebase-source-v1\0", wrapper))
        != address
    {
        return Err(StoreError::Corrupt(
            "source attachment address does not match wrapper".to_owned(),
        ));
    }
    Ok(())
}

pub(in crate::codebase) fn validate_record_fields(
    node: &vibra_syntax::DataNode,
    expected: &[&str],
) -> Result<(), StoreError> {
    let vibra_syntax::DataValue::Record(fields) = node.value() else {
        return Err(StoreError::Corrupt("expected VIBON record".to_owned()));
    };
    let actual = fields
        .iter()
        .map(|field| field.label().value())
        .collect::<Vec<_>>();
    if actual != expected {
        return Err(StoreError::Corrupt(format!(
            "record fields are invalid: expected {expected:?}, found {actual:?}"
        )));
    }
    Ok(())
}

pub(in crate::codebase) fn required_field<'a>(
    node: &'a vibra_syntax::DataNode,
    label: &str,
) -> Result<&'a vibra_syntax::DataNode, StoreError> {
    let vibra_syntax::DataValue::Record(fields) = node.value() else {
        return Err(StoreError::Corrupt("expected VIBON record".to_owned()));
    };
    fields
        .iter()
        .find(|field| field.label().value() == label)
        .map(|field| field.value())
        .ok_or_else(|| StoreError::Corrupt(format!("record is missing field {label}")))
}

pub(in crate::codebase) fn array_field<'a>(
    node: &'a vibra_syntax::DataNode,
    label: &str,
) -> Result<&'a [vibra_syntax::DataNode], StoreError> {
    match required_field(node, label)?.value() {
        vibra_syntax::DataValue::Array(values) => Ok(values),
        _ => Err(StoreError::Corrupt(format!(
            "field {label} is not an array"
        ))),
    }
}

pub(in crate::codebase) fn validate_string_array(
    node: &vibra_syntax::DataNode,
) -> Result<(), StoreError> {
    let vibra_syntax::DataValue::Array(values) = node.value() else {
        return Err(StoreError::Corrupt("expected string array".to_owned()));
    };
    for value in values {
        data_string(value)?;
    }
    Ok(())
}

pub(in crate::codebase) fn data_atom(
    node: &vibra_syntax::DataNode,
) -> Result<String, StoreError> {
    match node.value() {
        vibra_syntax::DataValue::Atom(atom) => Ok(atom.value().to_owned()),
        _ => Err(StoreError::Corrupt("expected VIBON atom".to_owned())),
    }
}

pub(in crate::codebase) fn ensure_supported_record_format(
    found: &str,
    expected: &str,
    description: &str,
) -> Result<(), StoreError> {
    if found == expected {
        return Ok(());
    }
    let (prefix, supported_text) = expected.rsplit_once(".v").ok_or_else(|| {
        StoreError::Corrupt(format!("invalid internal format identifier {expected}"))
    })?;
    let supported = supported_text.parse::<i32>().map_err(|error| {
        StoreError::Corrupt(format!(
            "invalid internal format version {expected}: {error}"
        ))
    })?;
    let version_prefix = format!("{prefix}.v");
    if let Some(version_text) = found.strip_prefix(&version_prefix)
        && let Ok(found_version) = version_text.parse::<i32>()
        && found_version > supported
    {
        return Err(StoreError::UnsupportedVersion {
            found: found_version,
            supported,
        });
    }
    Err(StoreError::Corrupt(format!(
        "{description} has unsupported format {found:?}"
    )))
}

pub(in crate::codebase) fn data_string(
    node: &vibra_syntax::DataNode,
) -> Result<String, StoreError> {
    match node.value() {
        vibra_syntax::DataValue::Literal(vibra_syntax::Literal::String(value)) => {
            Ok(value.value().to_owned())
        }
        _ => Err(StoreError::Corrupt("expected VIBON string".to_owned())),
    }
}

pub(in crate::codebase) fn data_u64(
    node: &vibra_syntax::DataNode,
) -> Result<u64, StoreError> {
    match node.value() {
        vibra_syntax::DataValue::Literal(vibra_syntax::Literal::Integer(value))
            if !value.is_negative()
                && value.suffix() == Some(vibra_syntax::IntegerSuffix::U64) =>
        {
            value.digits().parse::<u64>().map_err(|error| {
                StoreError::Corrupt(format!("invalid u64 record field: {error}"))
            })
        }
        _ => Err(StoreError::Corrupt(
            "expected nonnegative u64 field".to_owned(),
        )),
    }
}

pub(in crate::codebase) fn read_member_references(
    connection: &Connection,
    component: &vibra_syntax::DataNode,
    component_address: &str,
    component_digest: &[u8; 32],
    member_index: u64,
    revision_objects: &std::collections::BTreeSet<String>,
    revision_frame: &vibra_syntax::DataNode,
) -> Result<Vec<ReadReference>, StoreError> {
    let members = array_field(component, "members")?;
    let member = members
        .get(usize::try_from(member_index).unwrap_or(usize::MAX))
        .ok_or_else(|| {
            StoreError::Corrupt(
                "selected object index is outside its component".to_owned(),
            )
        })?;
    let mut references = Vec::new();
    for reference in array_field(member, "references")? {
        let part = data_string_array(required_field(reference, "part")?)?;
        let kind = data_atom(required_field(reference, "kind")?)?;
        let target_node = required_field(reference, "target")?;
        let target_kind = data_atom(required_field(target_node, "kind")?)?;
        let target = match target_kind.as_str() {
            "component-member" => {
                let index = data_u64(required_field(target_node, "index")?)?;
                let target_address = object_address(component_digest, index);
                let stored_member = connection
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM objects WHERE address=?1 AND component_address=?2 AND member_index=?3)",
                        params![target_address, component_address, i64::try_from(index).unwrap_or(i64::MAX)],
                        |row| row.get::<_, bool>(0),
                    )
                    .map_err(map_sqlite_error)?;
                if !stored_member || !revision_objects.contains(&target_address) {
                    return Err(StoreError::Corrupt(format!(
                        "component reference points to a member outside the revision: {target_address}"
                    )));
                }
                ReadTarget::InternalMember(index)
            }
            "object" => {
                let address = data_string(required_field(target_node, "address")?)?;
                let row = connection
                    .query_row(
                        "SELECT component_address,member_index FROM objects WHERE address=?1",
                        [address.as_str()],
                        |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
                    )
                    .optional()
                    .map_err(map_sqlite_error)?
                    .ok_or_else(|| StoreError::Corrupt(format!(
                        "component reference points to missing object {address}"
                    )))?;
                if !revision_objects.contains(&address) {
                    return Err(StoreError::Corrupt(format!(
                        "component reference points outside its revision: {address}"
                    )));
                }
                if row.0 == component_address {
                    ReadTarget::InternalMember(u64::try_from(row.1).map_err(|_| {
                        StoreError::Corrupt("negative internal member index".to_owned())
                    })?)
                } else {
                    ReadTarget::Object(address)
                }
            }
            "trusted-external" => {
                let identity = data_string(required_field(target_node, "identity")?)?;
                let registry = data_string(required_field(target_node, "registry")?)?;
                validate_trusted_edge_identity(&identity, &registry, revision_frame)?;
                ReadTarget::TrustedExternal { identity, registry }
            }
            other => {
                return Err(StoreError::Corrupt(format!(
                    "unknown target record kind {other}"
                )));
            }
        };
        references.push(ReadReference { part, kind, target });
    }
    Ok(references)
}

pub(in crate::codebase) fn data_string_array(
    node: &vibra_syntax::DataNode,
) -> Result<Vec<String>, StoreError> {
    match node.value() {
        vibra_syntax::DataValue::Array(values) => {
            values.iter().map(data_string).collect()
        }
        _ => Err(StoreError::Corrupt("expected string array".to_owned())),
    }
}

//! Closed semantic validation shared by CLI selectors and store reads.

use vibra_syntax::{DataNode, DataValue, Literal, canonical_data, parse_data};

/// Whether text is one canonical entity-key record, including its inner
/// resolver identities and normalized implementation type records.
#[must_use]
pub fn is_valid_entity_key(text: &str) -> bool {
    let Ok(document) = parse_data("entity-key.vibon", text) else {
        return false;
    };
    document.accepted()
        && document.data().is_some_and(|node| {
            canonical_data(node) == text && check_key(node).is_some()
        })
}

fn check_key(node: &DataNode) -> Option<()> {
    match atom(field(node, "kind")?)? {
        "@declaration" => {
            shape(node, &["kind", "identity", "entity-kind"])?;
            let kind = atom(field(node, "entity-kind")?)?;
            if !matches!(
                kind,
                "@type"
                    | "@interface"
                    | "@effect"
                    | "@value"
                    | "@function"
                    | "@field"
                    | "@variant"
                    | "@operation"
            ) {
                return None;
            }
            identity(string(field(node, "identity")?)?, true)?;
        }
        "@test" => {
            shape(node, &["kind", "module", "name"])?;
            identity(string(field(node, "module")?)?, false)?;
            string(field(node, "name")?)?;
        }
        "@implementation" | "@implementation-member" => {
            if atom(field(node, "kind")?)? == "@implementation-member" {
                shape(node, &["kind", "contract", "receiver", "interface"])?;
                identity(string(field(node, "contract")?)?, true)?;
            } else {
                shape(node, &["kind", "receiver", "interface"])?;
            }
            normalized_type(string(field(node, "receiver")?)?, false)?;
            normalized_type(string(field(node, "interface")?)?, true)?;
        }
        _ => return None,
    }
    Some(())
}

fn identity(text: &str, declaration: bool) -> Option<()> {
    let (package, path) = text.strip_prefix('@')?.split_once('/')?;
    let (name, version) = package.split_once('@')?;
    if !crate::project::is_kebab_name(name) || !crate::project::is_semver(version) {
        return None;
    }
    let segments = path.split('.').collect::<Vec<_>>();
    if segments.len() < if declaration { 2 } else { 1 }
        || !segments
            .first()
            .is_some_and(|segment| crate::project::is_kebab_name(segment))
        || !segments.iter().skip(1).all(|segment| symbol(segment))
    {
        return None;
    }
    Some(())
}

fn normalized_type(text: &str, interface: bool) -> Option<()> {
    let document = parse_data("normalized-type.vibon", text).ok()?;
    if !document.accepted() {
        return None;
    }
    let root = document.data()?;
    if canonical_data(root) != text {
        return None;
    }
    shape(root, &["format", "form"])?;
    if atom(field(root, "format")?)? != "@normalized-type.v1" {
        return None;
    }
    let form = field(root, "form")?;
    if interface && atom(field(form, "kind")?)? != "@interface" {
        return None;
    }
    let mut pending = vec![form];
    while let Some(node) = pending.pop() {
        let kind = atom(field(node, "kind")?)?;
        match kind {
            "@primitive" => {
                shape(node, &["kind", "name"])?;
                if !matches!(
                    atom(field(node, "name")?)?,
                    "@bool"
                        | "@void"
                        | "@never"
                        | "@char"
                        | "@str"
                        | "@bytes"
                        | "@atom"
                        | "@i8"
                        | "@i16"
                        | "@i32"
                        | "@i64"
                        | "@u8"
                        | "@u16"
                        | "@u32"
                        | "@u64"
                        | "@f32"
                        | "@f64"
                        | "@any"
                ) {
                    return None;
                }
            }
            "@atom-singleton" => {
                shape(node, &["kind", "name"])?;
                let name = string(field(node, "name")?)?;
                if !vibra_syntax::classify_name(&format!("@{name}"))
                    .as_name()
                    .is_some_and(|name| {
                        matches!(
                            name.kind(),
                            vibra_syntax::NameKind::Atom
                                | vibra_syntax::NameKind::Discard
                        )
                    })
                {
                    return None;
                }
            }
            "@declared" | "@interface" => {
                shape(node, &["kind", "identity", "arguments"])?;
                identity(string(field(node, "identity")?)?, true)?;
                pending.extend(array(field(node, "arguments")?)?);
            }
            "@parameter" => {
                shape(node, &["kind", "binder-kind", "index"])?;
                if atom(field(node, "binder-kind")?)? != "@type" {
                    return None;
                }
                let DataValue::Literal(Literal::Integer(value)) =
                    field(node, "index")?.value()
                else {
                    return None;
                };
                value.raw().strip_suffix("u64")?.parse::<u64>().ok()?;
            }
            "@array" => {
                shape(node, &["kind", "element"])?;
                pending.push(field(node, "element")?);
            }
            "@dict" => {
                shape(node, &["kind", "key", "value"])?;
                pending.extend([field(node, "key")?, field(node, "value")?]);
            }
            "@tuple" | "@union" => {
                let label = if kind == "@tuple" { "items" } else { "members" };
                shape(node, &["kind", label])?;
                let values = array(field(node, label)?)?;
                if kind == "@union" {
                    if values.len() < 2 {
                        return None;
                    }
                    let mut previous = None;
                    for value in values {
                        let member_kind = atom(field(value, "kind")?)?;
                        if matches!(member_kind, "@union" | "@interface" | "@parameter")
                            || (member_kind == "@primitive"
                                && atom(field(value, "name")?)? == "@any")
                        {
                            return None;
                        }
                        let key = union_order_key(value)?;
                        if previous.as_ref().is_some_and(|last| last >= &key) {
                            return None;
                        }
                        previous = Some(key);
                    }
                }
                pending.extend(values);
            }
            "@record" | "@enum" => {
                let label = if kind == "@record" {
                    "fields"
                } else {
                    "variants"
                };
                shape(node, &["kind", label])?;
                named_types(field(node, label)?, &mut pending)?;
            }
            "@function" => {
                shape(
                    node,
                    &[
                        "kind",
                        "positional",
                        "result",
                        "labelled",
                        "variadic",
                        "effects",
                    ],
                )?;
                pending.extend(array(field(node, "positional")?)?);
                pending.push(field(node, "result")?);
                named_types(field(node, "labelled")?, &mut pending)?;
                let variadic = field(node, "variadic")?;
                match atom(field(variadic, "kind")?)? {
                    "@absent" => shape(variadic, &["kind"])?,
                    "@present" => {
                        shape(variadic, &["kind", "type"])?;
                        pending.push(field(variadic, "type")?);
                    }
                    _ => return None,
                }
                let effects = field(node, "effects")?;
                shape(effects, &["roots", "tail"])?;
                if atom(field(effects, "tail")?)? != "@none" {
                    return None;
                }
                let mut previous = None;
                for root in array(field(effects, "roots")?)? {
                    shape(root, &["kind", "identity"])?;
                    if atom(field(root, "kind")?)? != "@reference" {
                        return None;
                    }
                    let current = string(field(root, "identity")?)?;
                    identity(current, true)?;
                    if previous.is_some_and(|last: &str| last >= current) {
                        return None;
                    }
                    previous = Some(current);
                }
            }
            _ => return None,
        }
    }
    Some(())
}

// Reproduce the canonical_type ordering key with an explicit work stack,
// avoiding recursive traversal of a caller-controlled selector. Anonymous
// unions cannot contain generic parameters anywhere (type-system chapter).
fn union_order_key(root: &DataNode) -> Option<String> {
    enum Piece<'a> {
        Node(&'a DataNode),
        Text(String),
    }
    let text = |value: &str| Piece::Text(value.to_owned());
    let mut pending = vec![Piece::Node(root)];
    let mut output = String::new();
    while let Some(piece) = pending.pop() {
        let Piece::Node(node) = piece else {
            if let Piece::Text(value) = piece {
                output.push_str(&value);
            }
            continue;
        };
        let kind = atom(field(node, "kind")?)?;
        let mut pieces = Vec::new();
        match kind {
            "@primitive" => {
                pieces.push(Piece::Text(atom(field(node, "name")?)?.to_owned()))
            }
            "@declared" | "@interface" => {
                let (_, path) = string(field(node, "identity")?)?.split_once('/')?;
                let arguments = array(field(node, "arguments")?)?;
                if arguments.is_empty() {
                    pieces.push(Piece::Text(format!("@{path}")));
                } else {
                    pieces.push(Piece::Text(format!(
                        "(record type: @{path} arguments: (array"
                    )));
                    for argument in arguments {
                        pieces.extend([text(" "), Piece::Node(argument)]);
                    }
                    pieces.push(text("))"));
                }
            }
            "@parameter" => return None,
            "@atom-singleton" => pieces.push(Piece::Text(format!(
                "(record type: @atom-singleton atom: @{})",
                string(field(node, "name")?)?
            ))),
            "@array" | "@dict" | "@tuple" => {
                pieces.push(Piece::Text(format!(
                    "(record type: {kind} arguments: (array"
                )));
                let children = match kind {
                    "@array" => vec![field(node, "element")?],
                    "@dict" => vec![field(node, "key")?, field(node, "value")?],
                    _ => array(field(node, "items")?)?.iter().collect(),
                };
                for child in children {
                    pieces.extend([text(" "), Piece::Node(child)]);
                }
                pieces.push(text("))"));
            }
            "@record" | "@enum" => {
                let label = if kind == "@record" {
                    "fields"
                } else {
                    "variants"
                };
                pieces.push(Piece::Text(format!(
                    "(record type: {kind} {label}: (record"
                )));
                for member in array(field(node, label)?)? {
                    pieces.extend([
                        Piece::Text(format!(" {}: ", string(field(member, "name")?)?)),
                        Piece::Node(field(member, "type")?),
                    ]);
                }
                pieces.push(text("))"));
            }
            "@union" => {
                pieces.push(text("(record type: @union members: (array"));
                for member in array(field(node, "members")?)? {
                    pieces.extend([text(" "), Piece::Node(member)]);
                }
                pieces.push(text("))"));
            }
            "@function" => {
                pieces.push(text("(record type: @fn parameters: (array"));
                for parameter in array(field(node, "positional")?)? {
                    pieces.extend([text(" "), Piece::Node(parameter)]);
                }
                pieces.push(text(") labelled: (record"));
                let mut labelled = array(field(node, "labelled")?)?
                    .iter()
                    .map(|parameter| {
                        Some((
                            string(field(parameter, "name")?)?,
                            field(parameter, "type")?,
                        ))
                    })
                    .collect::<Option<Vec<_>>>()?;
                labelled.sort_by(|left, right| left.0.cmp(right.0));
                for (name, ty) in labelled {
                    pieces.extend([Piece::Text(format!(" {name}: ")), Piece::Node(ty)]);
                }
                pieces.push(text(")"));
                let variadic = field(node, "variadic")?;
                if atom(field(variadic, "kind")?)? == "@present" {
                    pieces.extend([
                        text(" variadic: "),
                        Piece::Node(field(variadic, "type")?),
                    ]);
                }
                pieces.extend([
                    text(" result: "),
                    Piece::Node(field(node, "result")?),
                    text(" effects: (record roots: (array"),
                ]);
                let effects = field(node, "effects")?;
                let mut roots = array(field(effects, "roots")?)?
                    .iter()
                    .map(|root| {
                        Some(string(field(root, "identity")?)?.split_once('/')?.1)
                    })
                    .collect::<Option<Vec<_>>>()?;
                roots.sort_unstable();
                for root in roots {
                    pieces.push(Piece::Text(format!(" @{root}")));
                }
                pieces.push(text(")))"));
            }
            _ => return None,
        }
        pending.extend(pieces.into_iter().rev());
    }
    Some(output)
}

fn named_types<'a>(node: &'a DataNode, pending: &mut Vec<&'a DataNode>) -> Option<()> {
    let mut previous = None;
    for item in array(node)? {
        shape(item, &["name", "type"])?;
        let name = string(field(item, "name")?)?;
        if !symbol(name) || previous.is_some_and(|last: &str| last >= name) {
            return None;
        }
        previous = Some(name);
        pending.push(field(item, "type")?);
    }
    Some(())
}

fn shape(node: &DataNode, names: &[&str]) -> Option<()> {
    let DataValue::Record(fields) = node.value() else {
        return None;
    };
    (fields.len() == names.len()
        && fields
            .iter()
            .zip(names)
            .all(|(field, name)| field.label().value() == *name))
    .then_some(())
}
fn symbol(text: &str) -> bool {
    vibra_syntax::classify_name(text)
        .as_name()
        .is_some_and(|name| name.kind() == vibra_syntax::NameKind::Symbol)
}
fn field<'a>(node: &'a DataNode, name: &str) -> Option<&'a DataNode> {
    let DataValue::Record(fields) = node.value() else {
        return None;
    };
    fields
        .iter()
        .find(|field| field.label().value() == name)
        .map(vibra_syntax::DataField::value)
}
fn atom(node: &DataNode) -> Option<&str> {
    if let DataValue::Atom(value) = node.value() {
        Some(value.raw())
    } else {
        None
    }
}
fn string(node: &DataNode) -> Option<&str> {
    if let DataValue::Literal(Literal::String(value)) = node.value() {
        Some(value.value())
    } else {
        None
    }
}
fn array(node: &DataNode) -> Option<&[DataNode]> {
    if let DataValue::Array(values) = node.value() {
        Some(values)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_noncanonical_inner_key_values() {
        assert!(is_valid_entity_key(
            "(record kind: @declaration identity: \"@demo@0.1.0/app.main.run\" entity-kind: @function)"
        ));
        assert!(is_valid_entity_key(
            "(record kind: @declaration identity: \"@demo@0.1.0/app.main.run--\" entity-kind: @function)"
        ));
        for invalid in [
            "(record kind: @declaration identity: \"not-an-identity\" entity-kind: @function)",
            "(record kind: @declaration identity: \"@demo@01.1.0/app.main.run\" entity-kind: @function)",
            "(record kind: @implementation receiver: \"garbage\" interface: \"garbage\")",
            "(record kind: @test module: \"../outside\" name: \"test\")",
            "(record kind: @declaration identity: \"@demo@0.1.0/tests.main.test\" entity-kind: @test)",
        ] {
            assert!(!is_valid_entity_key(invalid), "{invalid}");
        }
    }

    #[test]
    fn normalized_anonymous_union_order_matches_canonical_type() {
        let ordered = "(record format: @normalized-type.v1 form: (record kind: @union members: (array (record kind: @primitive name: @i32) (record kind: @primitive name: @str))))";
        assert!(normalized_type(ordered, false).is_some());
        let reversed = ordered
            .replace("@i32", "@temporary")
            .replace("@str", "@i32")
            .replace("@temporary", "@str");
        assert!(normalized_type(&reversed, false).is_none());
        for members in [
            "",
            "(record kind: @primitive name: @i32)",
            "(record kind: @union members: (array (record kind: @primitive name: @i32) (record kind: @primitive name: @str))) (record kind: @primitive name: @u32)",
        ] {
            assert!(normalized_type(&format!("(record format: @normalized-type.v1 form: (record kind: @union members: (array {members})))"), false).is_none());
        }
        let array =
            "(record kind: @array element: (record kind: @primitive name: @i32))";
        let parsed = parse_data("type.vibon", array).expect("type node");
        assert_eq!(
            union_order_key(parsed.data().expect("data")),
            Some(vibra_ir::canonical_type(&vibra_ir::Type::Array(Box::new(
                vibra_ir::Type::I32
            ))))
        );
    }
}

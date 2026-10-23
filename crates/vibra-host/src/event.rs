//! The audit event and its codecs.
//!
//! `docs/spec/06-runtime.md`, "The audit trace". The trace is the VIBON record
//! `(record format: @audit-trace.v1 events: (array event...))` and an **event**
//! is the record
//!
//! ```text
//! (record kind: @host op: "<registry symbol>"
//!         args: (array <canonical encodings>) result: <canonical encoding>)
//! ```
//!
//! One event is built for each completed host call, by [`AuditEvent::completed`],
//! from the call and its answer: both backends use this one function, so a trace
//! is part of the observable behavior they reproduce. A consumer preserves an
//! event of a kind it does not know as opaque data and never fails on it; an
//! unknown field in a known kind is a violation of the format.
//!
//! The same event is a JSON object `{ "kind", "op", "args", "result" }` whose
//! values are the VIBON text of the fields, with `kind` in its atom spelling.

use std::path::Path;

use serde_json::{Map, Value};
use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_syntax::{DataNode, DataValue, Literal, canonical_data, parse_data};

use crate::operation::{HostCall, HostOk, HostReply};

/// The format atom of an audit trace, without `@`.
pub const TRACE_FORMAT: &str = "audit-trace.v1";

/// The event of a completed host call: the only kind the format defines in v1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostEvent {
    /// The registry symbol of the operation.
    pub op: String,
    /// The canonical VIBON encoding of each argument, in parameter order.
    pub args: Vec<String>,
    /// The canonical VIBON encoding of the `(result t e)` value.
    pub result: String,
}

/// An event of a kind this consumer does not know, kept as text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpaqueEvent {
    /// The kind atom without `@`.
    pub kind: String,
    /// The other fields in order, each with the VIBON text of its value.
    pub fields: Vec<(String, String)>,
}

/// One event of an audit trace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuditEvent {
    /// A `@host` event.
    Host(HostEvent),
    /// An event of a kind added by a later minor of the format.
    Opaque(OpaqueEvent),
}

impl AuditEvent {
    /// The event of a call that completed with `reply`.
    #[must_use]
    pub fn completed(call: &HostCall, reply: &HostReply) -> Self {
        Self::Host(HostEvent {
            op: call.symbol().to_owned(),
            args: call_arguments(call),
            result: encode_reply(reply),
        })
    }

    /// The kind atom without `@`.
    #[must_use]
    pub fn kind(&self) -> &str {
        match self {
            Self::Host(_) => "host",
            Self::Opaque(event) => &event.kind,
        }
    }

    /// The canonical VIBON text of the event on one line.
    #[must_use]
    pub fn to_vibon(&self) -> String {
        match self {
            Self::Host(event) => format!(
                "(record kind: @host op: {} args: {} result: {})",
                quote(&event.op),
                array(&event.args),
                event.result
            ),
            Self::Opaque(event) => {
                let mut text = format!("(record kind: @{}", event.kind);
                for (name, value) in &event.fields {
                    text.push_str(&format!(" {name}: {value}"));
                }
                text.push(')');
                text
            }
        }
    }

    /// The event as the JSON object of the CLI.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut object = Map::new();
        object.insert(
            "kind".to_owned(),
            Value::String(format!("@{}", self.kind())),
        );
        match self {
            Self::Host(event) => {
                object.insert("op".to_owned(), Value::String(event.op.clone()));
                object.insert(
                    "args".to_owned(),
                    Value::Array(
                        event.args.iter().cloned().map(Value::String).collect(),
                    ),
                );
                object.insert("result".to_owned(), Value::String(event.result.clone()));
            }
            Self::Opaque(event) => {
                for (name, value) in &event.fields {
                    object.insert(name.clone(), Value::String(value.clone()));
                }
            }
        }
        Value::Object(object)
    }

    /// Reads the JSON object of the CLI. An object of an unknown kind keeps its
    /// field names and values as text.
    pub fn from_json(value: &Value) -> Result<Self, String> {
        let object = value.as_object().ok_or("an event must be an object")?;
        let kind = object
            .get("kind")
            .and_then(Value::as_str)
            .and_then(|kind| kind.strip_prefix('@'))
            .ok_or("an event needs a `kind` atom")?;
        let text = |name: &str, value: &Value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("`{name}` must be a string"))
        };
        if kind == "host" {
            let mut op = None;
            let mut args = None;
            let mut result = None;
            for (name, value) in object {
                match name.as_str() {
                    "kind" => {}
                    "op" => op = Some(text(name, value)?),
                    "args" => {
                        let items =
                            value.as_array().ok_or("`args` must be an array")?;
                        args = Some(
                            items
                                .iter()
                                .map(|item| text("args", item))
                                .collect::<Result<Vec<_>, _>>()?,
                        );
                    }
                    "result" => result = Some(text(name, value)?),
                    other => {
                        return Err(format!("a host event has no field `{other}`"));
                    }
                }
            }
            return match (op, args, result) {
                (Some(op), Some(args), Some(result)) => {
                    Ok(Self::Host(HostEvent { op, args, result }))
                }
                _ => Err("a host event needs `op`, `args`, and `result`".to_owned()),
            };
        }
        let mut fields = Vec::new();
        for (name, value) in object {
            if name != "kind" {
                fields.push((name.clone(), text(name, value)?));
            }
        }
        Ok(Self::Opaque(OpaqueEvent {
            kind: kind.to_owned(),
            fields,
        }))
    }

    /// Decodes one event from the text of a VIBON document.
    pub fn decode(source_id: &str, text: &str) -> Result<Self, Vec<Diagnostic>> {
        let root = read_document(source_id, text)?;
        decode_event(source_id, &root).map_err(|diagnostic| vec![*diagnostic])
    }

    /// Decodes an event from a decoded VIBON value, reporting a violation at it.
    pub fn from_data(
        source_id: &str,
        node: &DataNode,
    ) -> Result<Self, Box<Diagnostic>> {
        decode_event(source_id, node)
    }
}

/// The text of the audit trace record of `events`.
#[must_use]
pub fn trace_to_vibon(events: &[AuditEvent]) -> String {
    let events = events.iter().map(AuditEvent::to_vibon).collect::<Vec<_>>();
    format!(
        "(record format: @{TRACE_FORMAT} events: {})",
        array(&events)
    )
}

/// Decodes the text of an audit trace record into its events.
pub fn decode_trace(
    source_id: &str,
    text: &str,
) -> Result<Vec<AuditEvent>, Vec<Diagnostic>> {
    let root = read_document(source_id, text)?;
    let shape = |node: &DataNode, message: &str| {
        vec![
            Diagnostic::new(DiagnosticCode::DataInvalidShape, node.span(), message)
                .with_source_id(source_id),
        ]
    };
    let DataValue::Record(fields) = root.value() else {
        return Err(shape(&root, "an audit trace is a record"));
    };
    let mut format = None;
    let mut events = None;
    for field in fields {
        match field.label().value() {
            "format" => format = Some(field.value()),
            "events" => events = Some(field.value()),
            _ => return Err(shape(field.value(), "an audit trace has no such field")),
        }
    }
    match format.map(DataNode::value) {
        Some(DataValue::Atom(atom)) if atom.value() == TRACE_FORMAT => {}
        _ => return Err(shape(&root, "the format must be @audit-trace.v1")),
    }
    let Some(events) = events else {
        return Err(shape(&root, "an audit trace needs its `events`"));
    };
    let DataValue::Array(items) = events.value() else {
        return Err(shape(events, "`events` must be an array"));
    };
    items
        .iter()
        .map(|item| {
            decode_event(source_id, item).map_err(|diagnostic| vec![*diagnostic])
        })
        .collect()
}

fn read_document(source_id: &str, text: &str) -> Result<DataNode, Vec<Diagnostic>> {
    let document = parse_data(Path::new(source_id), text).map_err(|error| {
        vec![
            Diagnostic::new(
                DiagnosticCode::DataInvalidExtension,
                ByteSpan::new(0, 0),
                error.to_string(),
            )
            .with_source_id(source_id),
        ]
    })?;
    if !document.accepted() || document.recovered() {
        return Err(document
            .diagnostics()
            .iter()
            .cloned()
            .map(|diagnostic| diagnostic.with_source_id(source_id))
            .collect());
    }
    document.data().cloned().ok_or_else(|| {
        vec![
            Diagnostic::new(
                DiagnosticCode::DataInvalidShape,
                ByteSpan::new(0, text.len()),
                "the document has no data value",
            )
            .with_source_id(source_id),
        ]
    })
}

fn decode_event(
    source_id: &str,
    node: &DataNode,
) -> Result<AuditEvent, Box<Diagnostic>> {
    let shape = |at: &DataNode, message: &str| {
        Box::new(
            Diagnostic::new(DiagnosticCode::DataInvalidShape, at.span(), message)
                .with_source_id(source_id),
        )
    };
    let DataValue::Record(fields) = node.value() else {
        return Err(shape(node, "an event is a record"));
    };
    let kind = match fields.first() {
        Some(field) if field.label().value() == "kind" => match field.value().value() {
            DataValue::Atom(atom) => atom.value().to_owned(),
            _ => return Err(shape(field.value(), "an event `kind` is an atom")),
        },
        _ => return Err(shape(node, "an event's first field is `kind`")),
    };
    let rest = fields.get(1..).unwrap_or_default();
    if kind != "host" {
        return Ok(AuditEvent::Opaque(OpaqueEvent {
            kind,
            fields: rest
                .iter()
                .map(|field| {
                    (
                        field.label().value().to_owned(),
                        canonical_data(field.value()),
                    )
                })
                .collect(),
        }));
    }
    let mut op = None;
    let mut args = None;
    let mut result = None;
    for field in rest {
        let value = field.value();
        match field.label().value() {
            "op" if op.is_none() => match value.value() {
                DataValue::Literal(Literal::String(literal)) => {
                    op = Some(literal.value().to_owned());
                }
                _ => return Err(shape(value, "`op` is a string")),
            },
            "args" if args.is_none() => match value.value() {
                DataValue::Array(items) => {
                    args = Some(items.iter().map(canonical_data).collect::<Vec<_>>());
                }
                _ => return Err(shape(value, "`args` is an array")),
            },
            "result" if result.is_none() => result = Some(canonical_data(value)),
            name => {
                return Err(shape(
                    value,
                    &format!("a host event has no field `{name}`, or repeats it"),
                ));
            }
        }
    }
    match (op, args, result) {
        (Some(op), Some(args), Some(result)) => {
            Ok(AuditEvent::Host(HostEvent { op, args, result }))
        }
        _ => Err(shape(node, "a host event needs `op`, `args`, and `result`")),
    }
}

// --- canonical encodings of the values of the registry -----------------------

fn quote(text: &str) -> String {
    let mut output = String::with_capacity(text.len().saturating_add(2));
    output.push('"');
    for character in text.chars() {
        match character {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            control if control <= '\u{1f}' || control == '\u{7f}' => {
                output.push_str(&format!("\\u{{{:x}}}", u32::from(control)));
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

fn array(items: &[String]) -> String {
    if items.is_empty() {
        "(array)".to_owned()
    } else {
        format!("(array {})", items.join(" "))
    }
}

fn bytes(bytes: &[u8]) -> String {
    let values = bytes
        .iter()
        .map(|byte| format!("{byte}u8"))
        .collect::<Vec<_>>();
    format!("(record kind: @bytes values: {})", array(&values))
}

fn path(text: &str) -> String {
    format!(
        "(record kind: @wrapper type: @std.fs.path value: {})",
        quote(text)
    )
}

fn enum_value(
    module: &str,
    name: &str,
    variant: &str,
    payload: Option<&str>,
) -> String {
    let mut text =
        format!("(record kind: @enum type: @{module}.{name} variant: @{variant}");
    if let Some(payload) = payload {
        text.push_str(&format!(" payload: {payload}"));
    }
    text.push(')');
    text
}

fn strings(items: &[String]) -> String {
    let values = items.iter().map(|item| quote(item)).collect::<Vec<_>>();
    format!("(record kind: @array values: {})", array(&values))
}

/// The canonical encoding of each argument of `call`, in parameter order.
fn call_arguments(call: &HostCall) -> Vec<String> {
    match call {
        HostCall::StdinText
        | HostCall::StdinBytes
        | HostCall::EnvArgs
        | HostCall::NowWall
        | HostCall::NowMonotonic => Vec::new(),
        HostCall::StdoutText(value)
        | HostCall::StderrText(value)
        | HostCall::EnvVar(value) => vec![quote(value)],
        HostCall::StdoutBytes(value) | HostCall::StderrBytes(value) => {
            vec![bytes(value)]
        }
        HostCall::ReadFile(at)
        | HostCall::ReadText(at)
        | HostCall::ReadEntries(at)
        | HostCall::Stat(at) => vec![path(at)],
        HostCall::WriteFile(at, contents) => vec![path(at), bytes(contents)],
        HostCall::WriteText(at, contents) => vec![path(at), quote(contents)],
        HostCall::RandomBytes(count) => vec![format!("{count}u64")],
    }
}

/// The canonical encoding of the `(result t e)` value of a completed call.
fn encode_reply(reply: &HostReply) -> String {
    match reply {
        Ok(ok) => {
            let payload = match ok {
                HostOk::Void => None,
                HostOk::Text(text) => Some(quote(text)),
                HostOk::Bytes(value) => Some(bytes(value)),
                HostOk::Entries(names) => Some(strings(names)),
                HostOk::Metadata(metadata) => Some(format!(
                    "(record kind: @record type: @std.fs.fs-metadata fields: (record kind: {} size: {}u64))",
                    enum_value("std.fs", "fs-kind", metadata.kind.name(), None),
                    metadata.size
                )),
                HostOk::Integer(value) => Some(format!("{value}i64")),
                HostOk::Count(value) => Some(format!("{value}u64")),
            };
            enum_value("std.result", "result", "ok", payload.as_deref())
        }
        Err(error) => {
            let ty = error.error_type();
            let payload =
                enum_value(ty.module(), ty.name(), error.variant_name(), None);
            enum_value("std.result", "result", "err", Some(&payload))
        }
    }
}

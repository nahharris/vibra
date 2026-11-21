//! The audit event and its codecs.
//!
//! `docs/spec/06-runtime.md`, "Audit trace". The trace is the data document
//! `{format: @auditTrace.v1 events: [event ...]}` and an **event** is the
//! record
//!
//! ```text
//! {kind: @host op: "<registry symbol>" args: [<encodings>] result: <encoding>}
//! ```
//!
//! over the major-1 host rows. One event is built for each completed host
//! call, by [`AuditEvent::completed`], from the call and its answer, so every
//! backend that records a trace records it by this one function. A consumer
//! preserves an event of a kind it does not know as opaque data and never
//! fails on it; an unknown field in a known kind is a violation of the format.
//!
//! Every encoding an event holds is canonical data under the opaque rules of
//! `docs/spec/04-programs-and-packages.md`, "Decoded values and canonical
//! bytes": record fields in the byte order of their names, numbers with their
//! minimal digits and their suffix. A decoded event holds the canonical text
//! the schema reader of `vibra-syntax` writes, so two spellings of one event
//! decode to equal events.
//!
//! The same event is a JSON object `{ "kind", "op", "args", "result" }` whose
//! values are the data text of the fields, with `kind` in its atom spelling.

use serde_json::{Map, Value};
use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_syntax::{
    DataNode, DataValue, FieldSchema, Schema, Slot, TypedNode, TypedValue,
    canonical_string, decode, parse_data,
};

use crate::operation::{HostCall, HostOk, HostReply};

/// The format atom of an audit trace, with its `@`.
pub const TRACE_FORMAT: &str = "@auditTrace.v1";

/// The event of a completed host call: the only kind the format defines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostEvent {
    /// The registry symbol of the operation.
    pub op: String,
    /// The canonical data of each argument, in parameter order.
    pub args: Vec<String>,
    /// The canonical data of the `Result` value.
    pub result: String,
}

/// An event of a kind this consumer does not know, kept as text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpaqueEvent {
    /// The kind atom without `@`.
    pub kind: String,
    /// The other fields in the byte order of their names, each with the
    /// canonical data of its value.
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

    /// The data text of the event on one line, its `kind` first.
    #[must_use]
    pub fn to_vibon(&self) -> String {
        match self {
            Self::Host(event) => format!(
                "{{kind: @host op: {} args: {} result: {}}}",
                canonical_string(&event.op),
                list(&event.args),
                event.result
            ),
            Self::Opaque(event) => {
                let mut text = format!("{{kind: @{}", event.kind);
                for (name, value) in &event.fields {
                    text.push(' ');
                    text.push_str(name);
                    text.push_str(": ");
                    text.push_str(value);
                }
                text.push('}');
                text
            }
        }
    }

    /// The event as the JSON object of the command line.
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

    /// Reads the JSON object of the command line. An object of an unknown
    /// kind keeps its field names and values as text.
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

    /// Decodes one event from the text of a data document.
    pub fn decode(source_id: &str, text: &str) -> Result<Self, Vec<Diagnostic>> {
        let whole = ByteSpan::new(0, text.len());
        let canonical = canonical_event(source_id, text)?;
        event_of(source_id, &canonical, whole).map_err(|diagnostic| vec![*diagnostic])
    }
}

/// The text of the audit trace document of `events`.
#[must_use]
pub fn trace_to_vibon(events: &[AuditEvent]) -> String {
    let events = events.iter().map(AuditEvent::to_vibon).collect::<Vec<_>>();
    format!("{{format: {TRACE_FORMAT} events: {}}}", list(&events))
}

/// Decodes the text of an audit trace document into its events.
pub fn decode_trace(
    source_id: &str,
    text: &str,
) -> Result<Vec<AuditEvent>, Vec<Diagnostic>> {
    let (document, diagnostics) =
        decode(&trace_schema(), Some(source_id), text.as_bytes()).into_parts();
    let Some(document) = document else {
        return Err(diagnostics);
    };
    let events = document
        .root()
        .get("events")
        .and_then(TypedNode::as_list)
        .unwrap_or_default();
    events
        .iter()
        .map(|event| match event.value() {
            TypedValue::Opaque(canonical) => {
                event_of(source_id, canonical, event.span())
                    .map_err(|diagnostic| vec![*diagnostic])
            }
            _ => Err(vec![shape(source_id, event.span(), "an event is data")]),
        })
        .collect()
}

/// The schema of the trace: its events are data no schema describes, which
/// the reader writes as canonical text.
fn trace_schema() -> Schema {
    Schema::new(
        TRACE_FORMAT,
        vec![FieldSchema::required("events", Slot::list(Slot::Opaque))],
    )
}

fn shape(source_id: &str, span: ByteSpan, message: &str) -> Diagnostic {
    Diagnostic::new(DiagnosticCode::DataInvalidShape, span, message)
        .with_source_id(source_id)
}

/// The canonical text of the one data value `text` holds: the text is read
/// on its own first, so its faults are reported at its own bytes, and then as
/// the one event of a trace, which writes it as canonical opaque data.
fn canonical_event(source_id: &str, text: &str) -> Result<String, Vec<Diagnostic>> {
    let whole = ByteSpan::new(0, text.len());
    let document = parse_data(Some(source_id), text.as_bytes());
    if !document.diagnostics().is_empty() {
        return Err(document.diagnostics().to_vec());
    }
    if document.value().is_none() {
        return Err(vec![shape(
            source_id,
            whole,
            "the document has no data value",
        )]);
    }
    // The text is one well-formed value, so the trace around it is too; the
    // line breaks end any comment it carries.
    let trace = format!("{{format: {TRACE_FORMAT} events: [\n{text}\n]}}");
    let (document, _) = decode(&trace_schema(), None, trace.as_bytes()).into_parts();
    let canonical = document.as_ref().and_then(|document| {
        match document.root().get("events")?.as_list()? {
            [event] => match event.value() {
                TypedValue::Opaque(canonical) => Some(canonical.clone()),
                _ => None,
            },
            _ => None,
        }
    });
    canonical.ok_or_else(|| vec![shape(source_id, whole, "an event is one data value")])
}

/// The event whose canonical text is `canonical`; a fault is reported at
/// `span`, where the event was read.
fn event_of(
    source_id: &str,
    canonical: &str,
    span: ByteSpan,
) -> Result<AuditEvent, Box<Diagnostic>> {
    let fault = |message: &str| Box::new(shape(source_id, span, message));
    let document = parse_data(None, canonical.as_bytes());
    let Some(node) = document.value() else {
        return Err(fault("an event is one data value"));
    };
    let DataValue::Record(fields) = node.value() else {
        return Err(fault("an event is a record"));
    };
    // A sub-value of canonical text is canonical text itself.
    let text = |value: &DataNode| {
        canonical
            .get(value.span().start()..value.span().end())
            .unwrap_or_default()
            .to_owned()
    };
    let kind = match fields.iter().find(|field| field.name() == "kind") {
        Some(field) => match field.value().value() {
            DataValue::Atom(atom) => atom.strip_prefix('@').unwrap_or(atom).to_owned(),
            _ => return Err(fault("an event `kind` is an atom")),
        },
        None => return Err(fault("an event has a `kind` field")),
    };
    let rest = fields.iter().filter(|field| field.name() != "kind");
    if kind != "host" {
        return Ok(AuditEvent::Opaque(OpaqueEvent {
            kind,
            fields: rest
                .map(|field| (field.name().to_owned(), text(field.value())))
                .collect(),
        }));
    }
    let mut op = None;
    let mut args = None;
    let mut result = None;
    for field in rest {
        let value = field.value();
        match field.name() {
            "op" => match value.value() {
                DataValue::String(symbol) => op = Some(symbol.clone()),
                _ => return Err(fault("`op` is a string")),
            },
            "args" => match value.value() {
                DataValue::List(items) => {
                    args = Some(items.iter().map(text).collect::<Vec<_>>());
                }
                _ => return Err(fault("`args` is a list")),
            },
            "result" => result = Some(text(value)),
            name => {
                return Err(fault(&format!("a host event has no field `{name}`")));
            }
        }
    }
    match (op, args, result) {
        (Some(op), Some(args), Some(result)) => {
            Ok(AuditEvent::Host(HostEvent { op, args, result }))
        }
        _ => Err(fault("a host event needs `op`, `args`, and `result`")),
    }
}

// --- canonical data of the values of the registry ----------------------------
//
// Each writer emits record fields in the byte order of their names, so the
// text is canonical as written.

fn list(items: &[String]) -> String {
    format!("[{}]", items.join(" "))
}

/// A name of the major-1 registry, such as `fs-metadata`, as an atom part:
/// `fsMetadata`.
fn atom_part(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut upper = false;
    for character in name.chars() {
        if character == '-' {
            upper = true;
        } else if upper {
            out.extend(character.to_uppercase());
            upper = false;
        } else {
            out.push(character);
        }
    }
    out
}

fn bytes(bytes: &[u8]) -> String {
    let values = bytes
        .iter()
        .map(|byte| format!("{byte}u8"))
        .collect::<Vec<_>>();
    format!("{{kind: @bytes values: {}}}", list(&values))
}

fn path(text: &str) -> String {
    format!(
        "{{kind: @wrapper typeInfo: @std.fs.path value: {}}}",
        canonical_string(text)
    )
}

fn variant(module: &str, name: &str, case: &str, payload: Option<&str>) -> String {
    let payload =
        payload.map_or_else(String::new, |payload| format!(" payload: {payload}"));
    format!(
        "{{kind: @enum{payload} typeInfo: @{module}.{} variant: @{}}}",
        atom_part(name),
        atom_part(case)
    )
}

fn strings(items: &[String]) -> String {
    let values = items
        .iter()
        .map(|item| canonical_string(item))
        .collect::<Vec<_>>();
    format!("{{kind: @list values: {}}}", list(&values))
}

/// The canonical data of each argument of `call`, in parameter order.
fn call_arguments(call: &HostCall) -> Vec<String> {
    match call {
        HostCall::StdinText
        | HostCall::StdinBytes
        | HostCall::EnvArgs
        | HostCall::NowWall
        | HostCall::NowMonotonic => Vec::new(),
        HostCall::StdoutText(value)
        | HostCall::StderrText(value)
        | HostCall::EnvVar(value) => vec![canonical_string(value)],
        HostCall::StdoutBytes(value) | HostCall::StderrBytes(value) => {
            vec![bytes(value)]
        }
        HostCall::ReadFile(at)
        | HostCall::ReadText(at)
        | HostCall::ReadEntries(at)
        | HostCall::Stat(at) => vec![path(at)],
        HostCall::WriteFile(at, contents) => vec![path(at), bytes(contents)],
        HostCall::WriteText(at, contents) => vec![path(at), canonical_string(contents)],
        HostCall::RandomBytes(count) => vec![format!("{count}u64")],
    }
}

/// The canonical data of the `Result` value of a completed call.
fn encode_reply(reply: &HostReply) -> String {
    match reply {
        Ok(ok) => {
            let payload = match ok {
                HostOk::Void => None,
                HostOk::Text(text) => Some(canonical_string(text)),
                HostOk::Bytes(value) => Some(bytes(value)),
                HostOk::Entries(names) => Some(strings(names)),
                HostOk::Metadata(metadata) => Some(format!(
                    "{{fields: {{kind: {} size: {}u64}} kind: @record typeInfo: @std.fs.fsMetadata}}",
                    variant("std.fs", "fs-kind", metadata.kind.name(), None),
                    metadata.size
                )),
                HostOk::Integer(value) => Some(format!("{value}i64")),
                HostOk::Count(value) => Some(format!("{value}u64")),
            };
            variant("std.result", "result", "ok", payload.as_deref())
        }
        Err(error) => {
            let ty = error.error_type();
            let payload = variant(ty.module(), ty.name(), error.variant_name(), None);
            variant("std.result", "result", "err", Some(&payload))
        }
    }
}

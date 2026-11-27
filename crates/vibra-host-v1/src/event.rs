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

use vibra_ir_v1::{canonical_array, canonical_string, vocabulary};

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
                canonical_string(&event.op),
                canonical_array(&event.args),
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
}

/// The text of the audit trace record of `events`.
#[must_use]
pub fn trace_to_vibon(events: &[AuditEvent]) -> String {
    let events = events.iter().map(AuditEvent::to_vibon).collect::<Vec<_>>();
    format!(
        "(record format: @{TRACE_FORMAT} events: {})",
        canonical_array(&events)
    )
}

// --- canonical encodings of the values of the registry -----------------------

fn bytes(bytes: &[u8]) -> String {
    let values = bytes
        .iter()
        .map(|byte| format!("{byte}u8"))
        .collect::<Vec<_>>();
    format!("(record kind: @bytes values: {})", canonical_array(&values))
}

fn path(text: &str) -> String {
    format!(
        "(record kind: @wrapper type: @std.fs.path value: {})",
        canonical_string(text)
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
    let values = items
        .iter()
        .map(|item| canonical_string(item))
        .collect::<Vec<_>>();
    format!("(record kind: @array values: {})", canonical_array(&values))
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

/// The canonical encoding of the `(result t e)` value of a completed call.
fn encode_reply(reply: &HostReply) -> String {
    match reply {
        Ok(ok) => {
            let payload = match ok {
                HostOk::Void => None,
                HostOk::Text(text) => Some(canonical_string(text)),
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
            enum_value("std.result", "result", vocabulary::OK, payload.as_deref())
        }
        Err(error) => {
            let ty = error.error_type();
            let payload =
                enum_value(ty.module(), ty.name(), error.variant_name(), None);
            enum_value("std.result", "result", vocabulary::ERR, Some(&payload))
        }
    }
}

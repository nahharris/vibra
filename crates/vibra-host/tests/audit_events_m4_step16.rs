//! The audit event, its codecs, and the value-access trait (Milestone 4, Step
//! 16a; ledger D18, D30.2).
//!
//! `docs/spec/06-runtime.md`, "The audit trace". The JSON cases are the ones of
//! the command-result schema test of Step 13
//! (`the_audit_trace_is_an_array_of_event_records_and_keeps_an_unknown_kind`).

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use serde_json::json;
use vibra_diagnostics::DiagnosticCode;
use vibra_host::{
    AuditEvent, EnvError, FsError, FsKind, HostCall, HostEnum, HostEvent, HostOk,
    HostRecord, HostReply, HostValues, HostWorld, IoError, Metadata, OpaqueEvent,
    OperationError, RandomError, ReadError, TimeError, WorldProvider, build_reply,
    decode_trace, perform, trace_to_vibon,
};
use vibra_ir::external::host::{HOST_ROWS, HostErrorType, HostType};

/// The reference implementation of the value-access trait: a plain tree.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Tree {
    Void,
    Text(String),
    Path(String),
    Bytes(Vec<u8>),
    I64(i64),
    U64(u64),
    Array(Vec<Tree>),
    Enum(HostEnum, u32, Option<Box<Tree>>),
    Record(HostRecord, Vec<Tree>),
}

#[derive(Default)]
struct Plain;

impl HostValues for Plain {
    type Value = Tree;

    fn read_text(&mut self, value: &Tree) -> Result<String, ReadError> {
        match value {
            Tree::Text(text) => Ok(text.clone()),
            _ => Err(ReadError::Value),
        }
    }
    fn read_bytes(&mut self, value: &Tree) -> Result<Vec<u8>, ReadError> {
        match value {
            Tree::Bytes(bytes) => Ok(bytes.clone()),
            _ => Err(ReadError::Value),
        }
    }
    fn read_path(&mut self, value: &Tree) -> Result<String, ReadError> {
        match value {
            Tree::Path(text) => Ok(text.clone()),
            _ => Err(ReadError::Value),
        }
    }
    fn read_u64(&mut self, value: &Tree) -> Result<u64, ReadError> {
        match value {
            Tree::U64(number) => Ok(*number),
            _ => Err(ReadError::Value),
        }
    }
    fn build_void(&mut self) -> Tree {
        Tree::Void
    }
    fn build_text(&mut self, text: &str) -> Tree {
        Tree::Text(text.to_owned())
    }
    fn build_bytes(&mut self, bytes: &[u8]) -> Tree {
        Tree::Bytes(bytes.to_vec())
    }
    fn build_i64(&mut self, value: i64) -> Tree {
        Tree::I64(value)
    }
    fn build_u64(&mut self, value: u64) -> Tree {
        Tree::U64(value)
    }
    fn build_array(&mut self, elements: Vec<Tree>) -> Tree {
        Tree::Array(elements)
    }
    fn build_enum(
        &mut self,
        ty: HostEnum,
        variant: u32,
        payload: Option<Tree>,
    ) -> Tree {
        Tree::Enum(ty, variant, payload.map(Box::new))
    }
    fn build_record(&mut self, ty: HostRecord, fields: Vec<Tree>) -> Tree {
        Tree::Record(ty, fields)
    }
}

fn host(op: &str, args: &[&str], result: &str) -> AuditEvent {
    AuditEvent::Host(HostEvent {
        op: op.to_owned(),
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        result: result.to_owned(),
    })
}

fn ok(payload: Option<&str>) -> String {
    match payload {
        Some(payload) => {
            format!(
                "(record kind: @enum type: @std.result.result variant: @ok payload: {payload})"
            )
        }
        None => "(record kind: @enum type: @std.result.result variant: @ok)".to_owned(),
    }
}

fn err(module: &str, name: &str, variant: &str) -> String {
    format!(
        "(record kind: @enum type: @std.result.result variant: @err payload: (record kind: @enum type: @{module}.{name} variant: @{variant}))"
    )
}

// --- the event built from a completed call -----------------------------------

#[test]
fn the_event_of_the_chapter_example_is_built_from_the_call() {
    let call = HostCall::ReadFile("/in.txt".to_owned());
    let reply = Ok(HostOk::Bytes(b"hi".to_vec()));
    let event = AuditEvent::completed(&call, &reply);
    assert_eq!(
        event.to_vibon(),
        "(record kind: @host op: \"fs.read-file\" args: (array (record kind: @wrapper type: @std.fs.path value: \"/in.txt\")) result: (record kind: @enum type: @std.result.result variant: @ok payload: (record kind: @bytes values: (array 104u8 105u8))))"
    );
}

#[test]
fn events_of_every_argument_and_result_shape() {
    let cases: Vec<(HostCall, HostReply, AuditEvent)> = vec![
        (
            HostCall::StdinText,
            Ok(HostOk::Text("a\"b\n\u{e9}".to_owned())),
            host("io.stdin-text", &[], &ok(Some("\"a\\\"b\\n\u{e9}\""))),
        ),
        (
            HostCall::StdinBytes,
            Ok(HostOk::Bytes(Vec::new())),
            host(
                "io.stdin-bytes",
                &[],
                &ok(Some("(record kind: @bytes values: (array))")),
            ),
        ),
        (
            HostCall::StdoutText("caf\u{e9}\u{1}".to_owned()),
            Ok(HostOk::Void),
            host("io.stdout-text", &["\"caf\u{e9}\\u{1}\""], &ok(None)),
        ),
        (
            HostCall::StderrBytes(vec![0, 255]),
            Err(OperationError::Io(IoError::Closed)),
            host(
                "io.stderr-bytes",
                &["(record kind: @bytes values: (array 0u8 255u8))"],
                &err("std.io", "io-error", "closed"),
            ),
        ),
        (
            HostCall::WriteText("/w".to_owned(), "x".to_owned()),
            Err(OperationError::Fs(FsError::NotFound)),
            host(
                "fs.write-text",
                &[
                    "(record kind: @wrapper type: @std.fs.path value: \"/w\")",
                    "\"x\"",
                ],
                &err("std.fs", "fs-error", "not-found"),
            ),
        ),
        (
            HostCall::ReadEntries("/".to_owned()),
            Ok(HostOk::Entries(vec!["a".to_owned(), "b".to_owned()])),
            host(
                "fs.read-entries",
                &["(record kind: @wrapper type: @std.fs.path value: \"/\")"],
                &ok(Some("(record kind: @array values: (array \"a\" \"b\"))")),
            ),
        ),
        (
            HostCall::Stat("/d".to_owned()),
            Ok(HostOk::Metadata(Metadata {
                kind: FsKind::File,
                size: 3,
            })),
            host(
                "fs.metadata-stat",
                &["(record kind: @wrapper type: @std.fs.path value: \"/d\")"],
                &ok(Some(
                    "(record kind: @record type: @std.fs.fs-metadata fields: (record kind: (record kind: @enum type: @std.fs.fs-kind variant: @file) size: 3u64))",
                )),
            ),
        ),
        (
            HostCall::EnvVar("HOME".to_owned()),
            Err(OperationError::Env(EnvError::NotPresent)),
            host(
                "env.read-var",
                &["\"HOME\""],
                &err("std.env", "env-error", "not-present"),
            ),
        ),
        (
            HostCall::EnvArgs,
            Ok(HostOk::Entries(Vec::new())),
            host(
                "env.read-args",
                &[],
                &ok(Some("(record kind: @array values: (array))")),
            ),
        ),
        (
            HostCall::NowWall,
            Ok(HostOk::Integer(-5)),
            host("time.now-wall", &[], &ok(Some("-5i64"))),
        ),
        (
            HostCall::NowMonotonic,
            Err(OperationError::Time(TimeError::Unavailable)),
            host(
                "time.now-monotonic",
                &[],
                &err("std.time", "time-error", "unavailable"),
            ),
        ),
        (
            HostCall::RandomBytes(2),
            Err(OperationError::Random(RandomError::Unavailable)),
            host(
                "random.generate-bytes",
                &["2u64"],
                &err("std.random", "random-error", "unavailable"),
            ),
        ),
    ];
    for (call, reply, expected) in cases {
        let event = AuditEvent::completed(&call, &reply);
        assert_eq!(event, expected, "{call:?}");
        // Every event round-trips through the canonical VIBON text it prints.
        let text = event.to_vibon();
        assert_eq!(
            AuditEvent::decode("event.vibon", &text).expect("decodes"),
            event,
            "{text}"
        );
    }
}

// --- VIBON and JSON codecs ----------------------------------------------------

fn unknown_event() -> AuditEvent {
    AuditEvent::Opaque(OpaqueEvent {
        kind: "deliver".to_owned(),
        fields: vec![("channel".to_owned(), "(record id: 1u64)".to_owned())],
    })
}

#[test]
fn an_event_of_an_unknown_kind_is_preserved_as_opaque_data() {
    let text = "(record kind: @deliver channel: (record id: 1u64) note: \"x\")";
    let event = AuditEvent::decode("event.vibon", text).expect("not a failure");
    assert_eq!(
        event,
        AuditEvent::Opaque(OpaqueEvent {
            kind: "deliver".to_owned(),
            fields: vec![
                ("channel".to_owned(), "(record id: 1u64)".to_owned()),
                ("note".to_owned(), "\"x\"".to_owned()),
            ],
        })
    );
    assert_eq!(event.to_vibon(), text);
    assert_eq!(event.kind(), "deliver");
}

#[test]
fn json_events_match_the_command_result_schema_cases() {
    let host_json = json!({
        "kind": "@host",
        "op": "fs.read-file",
        "args": ["(record kind: @wrapper type: @std.fs.path value: \"/in.txt\")"],
        "result": "(record kind: @enum type: @std.result.result variant: @ok payload: (record kind: @bytes values: (array 104u8 105u8)))"
    });
    let event = AuditEvent::completed(
        &HostCall::ReadFile("/in.txt".to_owned()),
        &Ok(HostOk::Bytes(b"hi".to_vec())),
    );
    assert_eq!(event.to_json(), host_json);
    assert_eq!(AuditEvent::from_json(&host_json).expect("reads"), event);

    let future = json!({ "kind": "@deliver", "channel": "(record id: 1u64)" });
    assert_eq!(unknown_event().to_json(), future);
    assert_eq!(
        AuditEvent::from_json(&future).expect("reads"),
        unknown_event()
    );

    let test_item = json!({
        "kind": "@host", "op": "io.stdout-text",
        "args": ["\"hi\\n\""],
        "result": "(record kind: @enum type: @std.result.result variant: @ok)"
    });
    let event = AuditEvent::completed(
        &HostCall::StdoutText("hi\n".to_owned()),
        &Ok(HostOk::Void),
    );
    assert_eq!(event.to_json(), test_item);
}

#[test]
fn malformed_json_events_are_rejected() {
    for value in [
        json!("fs.read-file"),
        json!({ "op": "x", "args": [], "result": "y" }),
        json!({ "kind": "host", "op": "x", "args": [], "result": "y" }),
        json!({ "kind": "@host", "op": "x", "args": [] }),
        json!({ "kind": "@host", "op": "x", "args": [], "result": "y", "note": "z" }),
        json!({ "kind": "@host", "op": "x", "args": [1], "result": "y" }),
        json!({ "kind": "@deliver", "channel": 1 }),
    ] {
        assert!(AuditEvent::from_json(&value).is_err(), "{value}");
    }
}

#[test]
fn an_unknown_field_in_a_known_kind_is_rejected() {
    let text = "(record kind: @host op: \"io.stdin-text\" args: (array) result: 1u8 note: \"x\")";
    let diagnostics = AuditEvent::decode("event.vibon", text).expect_err("rejected");
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code(), DiagnosticCode::DataInvalidShape);
    assert_eq!(diagnostics[0].source_id(), Some("event.vibon"));
}

#[test]
fn malformed_events_are_rejected_with_data_diagnostics() {
    for text in [
        "(array)",
        "(record op: \"x\")",
        "(record kind: \"host\")",
        "(record kind: @host op: 1u8 args: (array) result: 1u8)",
        "(record kind: @host op: \"x\" args: 1u8 result: 1u8)",
        "(record kind: @host op: \"x\" args: (array))",
        "(record kind: @host op: \"x\" op: \"y\" args: (array) result: 1u8)",
        "(record kind: @host op: \"x\" args: (array) result: 1u8",
    ] {
        let diagnostics = AuditEvent::decode("event.vibon", text).expect_err(text);
        assert!(!diagnostics.is_empty(), "{text}");
    }
}

#[test]
fn a_trace_is_the_record_of_its_events_and_keeps_unknown_kinds() {
    let events = vec![
        AuditEvent::completed(&HostCall::StdoutText("a".to_owned()), &Ok(HostOk::Void)),
        unknown_event(),
        AuditEvent::completed(&HostCall::NowWall, &Ok(HostOk::Integer(0))),
    ];
    let text = trace_to_vibon(&events);
    assert!(text.starts_with("(record format: @audit-trace.v1 events: (array "));
    assert_eq!(decode_trace("trace.vibon", &text).expect("decodes"), events);

    assert_eq!(
        trace_to_vibon(&[]),
        "(record format: @audit-trace.v1 events: (array))"
    );
    assert_eq!(
        decode_trace(
            "trace.vibon",
            "(record format: @audit-trace.v1 events: (array))"
        )
        .expect("decodes"),
        Vec::new()
    );
    for bad in [
        "(record format: @audit-trace.v2 events: (array))",
        "(record events: (array))",
        "(record format: @audit-trace.v1)",
        "(record format: @audit-trace.v1 events: (array 1u8))",
        "(record format: @audit-trace.v1 events: (array) extra: 1u8)",
    ] {
        assert!(decode_trace("trace.vibon", bad).is_err(), "{bad}");
    }
}

// --- the value-access trait ---------------------------------------------------

fn arguments_for(symbol: &str) -> Vec<Tree> {
    let row = HOST_ROWS
        .iter()
        .find(|row| row.symbol == symbol)
        .expect("row");
    row.parameters
        .iter()
        .map(|parameter| match parameter.ty {
            HostType::Str => Tree::Text("v".to_owned()),
            HostType::Bytes => Tree::Bytes(vec![1]),
            HostType::Path => Tree::Path("/p".to_owned()),
            HostType::U64 => Tree::U64(4),
            other => panic!("no argument of type {other:?}"),
        })
        .collect()
}

#[test]
fn every_row_reads_into_the_call_of_its_symbol() {
    for row in &HOST_ROWS {
        let arguments = arguments_for(row.symbol);
        let call =
            HostCall::read(row.symbol, &arguments, &mut Plain).expect(row.symbol);
        assert_eq!(call.symbol(), row.symbol);
        assert_eq!(call.row(), Some(row));
        // The audit event has one encoding for each parameter.
        let event = AuditEvent::completed(&call, &Ok(HostOk::Void));
        match event {
            AuditEvent::Host(event) => {
                assert_eq!(event.args.len(), row.parameters.len())
            }
            AuditEvent::Opaque(_) => panic!("a call completes as a host event"),
        }
    }
}

#[test]
fn reading_a_call_rejects_an_unknown_symbol_arity_and_a_wrong_value() {
    assert_eq!(
        HostCall::read("io.stdin-line", &[], &mut Plain),
        Err(ReadError::UnknownSymbol("io.stdin-line".to_owned()))
    );
    assert_eq!(
        HostCall::read("io.stdout-text", &[], &mut Plain),
        Err(ReadError::Arity {
            expected: 1,
            found: 0
        })
    );
    assert_eq!(
        HostCall::read("io.stdin-text", &[Tree::Void], &mut Plain),
        Err(ReadError::Arity {
            expected: 0,
            found: 1
        })
    );
    assert_eq!(
        HostCall::read("io.stdout-text", &[Tree::Bytes(vec![])], &mut Plain),
        Err(ReadError::Value)
    );
    assert_eq!(
        HostCall::read(
            "fs.write-text",
            &[Tree::Path("/a".into()), Tree::U64(1)],
            &mut Plain
        ),
        Err(ReadError::Value)
    );
}

#[test]
fn a_reply_is_built_children_first_in_the_backends_own_values() {
    let result =
        |payload: Option<Tree>| Tree::Enum(HostEnum::Result, 0, payload.map(Box::new));
    assert_eq!(build_reply(&Ok(HostOk::Void), &mut Plain), result(None));
    assert_eq!(
        build_reply(&Ok(HostOk::Text("t".into())), &mut Plain),
        result(Some(Tree::Text("t".into())))
    );
    assert_eq!(
        build_reply(
            &Ok(HostOk::Entries(vec!["a".into(), "b".into()])),
            &mut Plain
        ),
        result(Some(Tree::Array(vec![
            Tree::Text("a".into()),
            Tree::Text("b".into())
        ])))
    );
    assert_eq!(
        build_reply(
            &Ok(HostOk::Metadata(Metadata {
                kind: FsKind::Directory,
                size: 0
            })),
            &mut Plain
        ),
        result(Some(Tree::Record(
            HostRecord::FsMetadata,
            vec![Tree::Enum(HostEnum::FsKind, 1, None), Tree::U64(0)]
        )))
    );
    assert_eq!(
        build_reply(&Ok(HostOk::Integer(-1)), &mut Plain),
        result(Some(Tree::I64(-1)))
    );
    assert_eq!(
        build_reply(&Err(OperationError::Fs(FsError::InvalidText)), &mut Plain),
        Tree::Enum(
            HostEnum::Result,
            1,
            Some(Box::new(Tree::Enum(HostEnum::FsError, 5, None)))
        )
    );
}

#[test]
fn every_provider_error_is_a_variant_of_the_registry_enum_in_order() {
    fn names<T: Copy>(
        all: &[T],
        name: impl Fn(T) -> &'static str,
    ) -> Vec<&'static str> {
        all.iter().map(|variant| name(*variant)).collect()
    }
    assert_eq!(
        names(IoError::ALL, IoError::name),
        HostErrorType::Io.variants()
    );
    assert_eq!(
        names(FsError::ALL, FsError::name),
        HostErrorType::Fs.variants()
    );
    assert_eq!(
        names(EnvError::ALL, EnvError::name),
        HostErrorType::Env.variants()
    );
    assert_eq!(
        names(TimeError::ALL, TimeError::name),
        HostErrorType::Time.variants()
    );
    assert_eq!(
        names(RandomError::ALL, RandomError::name),
        HostErrorType::Random.variants()
    );
    assert_eq!(
        names(FsKind::ALL, FsKind::name),
        vibra_ir::external::host::FS_KIND_VARIANTS
    );
    for (index, variant) in FsError::ALL.iter().enumerate() {
        assert_eq!(variant.index() as usize, index);
    }
}

#[test]
fn a_call_performed_on_a_world_gives_the_event_and_the_value_from_one_reply() {
    let world = HostWorld::decode(
        "world.vibon",
        "(record format: @host-world.v1 files: (dict \"/in.txt\" \"hi\"))",
    )
    .expect("world");
    let mut provider = WorldProvider::new(&world);
    let call =
        HostCall::read("fs.read-text", &[Tree::Path("/in.txt".into())], &mut Plain)
            .expect("call");
    let reply = perform(&call, &mut provider).expect("completes");
    assert_eq!(reply, Ok(HostOk::Text("hi".to_owned())));
    assert_eq!(
        build_reply(&reply, &mut Plain),
        Tree::Enum(HostEnum::Result, 0, Some(Box::new(Tree::Text("hi".into()))))
    );
    let event = AuditEvent::completed(&call, &reply);
    assert_eq!(event.to_json()["op"], json!("fs.read-text"));

    // A failed call is a completed call with an `err` result, and has an event.
    let missing = HostCall::ReadText("/missing".to_owned());
    let reply = perform(&missing, &mut provider).expect("completes");
    assert_eq!(reply, Err(OperationError::Fs(FsError::NotFound)));
    let AuditEvent::Host(event) = AuditEvent::completed(&missing, &reply) else {
        panic!("a host event");
    };
    assert_eq!(event.result, err("std.fs", "fs-error", "not-found"));
}

#[test]
fn every_row_performs_on_the_default_world() {
    let mut provider = WorldProvider::new(&HostWorld::default());
    for row in &HOST_ROWS {
        let arguments = arguments_for(row.symbol);
        let call =
            HostCall::read(row.symbol, &arguments, &mut Plain).expect(row.symbol);
        let reply = perform(&call, &mut provider).expect("completes");
        // The answer is of the row's result type.
        match (&reply, row.ok) {
            (Err(error), _) => {
                assert_eq!(error.error_type(), row.error, "{}", row.symbol)
            }
            (Ok(HostOk::Void), HostType::Void)
            | (Ok(HostOk::Text(_)), HostType::Str)
            | (Ok(HostOk::Bytes(_)), HostType::Bytes)
            | (Ok(HostOk::Entries(_)), HostType::Array(_))
            | (Ok(HostOk::Metadata(_)), HostType::FsMetadata)
            | (Ok(HostOk::Integer(_)), HostType::I64)
            | (Ok(HostOk::Count(_)), HostType::U64) => {}
            (other, expected) => {
                panic!("{}: {other:?} is not {expected:?}", row.symbol)
            }
        }
    }
}

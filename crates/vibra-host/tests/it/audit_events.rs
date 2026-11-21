//! The audit event, its codecs, and the value-access trait.
//!
//! `docs/spec/06-runtime.md`, "Audit trace", over the major-1 host rows: the
//! `@auditTrace.v1` document in the data grammar. Every encoding an event holds
//! is canonical data, record fields in the byte order of their names.

use serde_json::json;
use vibra_diagnostics::DiagnosticCode;
use vibra_host::{
    AuditEvent, EnvError, FsError, FsKind, HostCall, HostEnum, HostEvent, HostOk,
    HostRecord, HostReply, HostWorld, IoError, Metadata, OpaqueEvent, OperationError,
    RandomError, ReadError, TimeError, WorldProvider, build_reply, decode_trace,
    perform, trace_to_vibon,
};
use vibra_ir::external::host::{HOST_ROWS, HostErrorType, HostType};

use crate::support::{Plain, Tree};

fn host(op: &str, args: &[&str], result: &str) -> AuditEvent {
    AuditEvent::Host(HostEvent {
        op: op.to_owned(),
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        result: result.to_owned(),
    })
}

fn ok(payload: Option<&str>) -> String {
    match payload {
        Some(payload) => format!(
            "{{kind: @enum payload: {payload} typeInfo: @std.result.result variant: @ok}}"
        ),
        None => "{kind: @enum typeInfo: @std.result.result variant: @ok}".to_owned(),
    }
}

fn err(module: &str, name: &str, variant: &str) -> String {
    format!(
        "{{kind: @enum payload: {{kind: @enum typeInfo: @{module}.{name} variant: @{variant}}} typeInfo: @std.result.result variant: @err}}"
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
        "{kind: @host op: \"fs.read-file\" args: [{kind: @wrapper typeInfo: @std.fs.path value: \"/in.txt\"}] result: {kind: @enum payload: {kind: @bytes values: [104u8 105u8]} typeInfo: @std.result.result variant: @ok}}"
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
                &ok(Some("{kind: @bytes values: []}")),
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
                &["{kind: @bytes values: [0u8 255u8]}"],
                &err("std.io", "ioError", "closed"),
            ),
        ),
        (
            HostCall::WriteText("/w".to_owned(), "x".to_owned()),
            Err(OperationError::Fs(FsError::NotFound)),
            host(
                "fs.write-text",
                &[
                    "{kind: @wrapper typeInfo: @std.fs.path value: \"/w\"}",
                    "\"x\"",
                ],
                &err("std.fs", "fsError", "notFound"),
            ),
        ),
        (
            HostCall::ReadEntries("/".to_owned()),
            Ok(HostOk::Entries(vec!["a".to_owned(), "b".to_owned()])),
            host(
                "fs.read-entries",
                &["{kind: @wrapper typeInfo: @std.fs.path value: \"/\"}"],
                &ok(Some("{kind: @list values: [\"a\" \"b\"]}")),
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
                &["{kind: @wrapper typeInfo: @std.fs.path value: \"/d\"}"],
                &ok(Some(
                    "{fields: {kind: {kind: @enum typeInfo: @std.fs.fsKind variant: @file} size: 3u64} kind: @record typeInfo: @std.fs.fsMetadata}",
                )),
            ),
        ),
        (
            HostCall::EnvVar("HOME".to_owned()),
            Err(OperationError::Env(EnvError::NotPresent)),
            host(
                "env.read-var",
                &["\"HOME\""],
                &err("std.env", "envError", "notPresent"),
            ),
        ),
        (
            HostCall::EnvArgs,
            Ok(HostOk::Entries(Vec::new())),
            host("env.read-args", &[], &ok(Some("{kind: @list values: []}"))),
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
                &err("std.time", "timeError", "unavailable"),
            ),
        ),
        (
            HostCall::RandomBytes(2),
            Err(OperationError::Random(RandomError::Unavailable)),
            host(
                "random.generate-bytes",
                &["2u64"],
                &err("std.random", "randomError", "unavailable"),
            ),
        ),
    ];
    for (call, reply, expected) in cases {
        let event = AuditEvent::completed(&call, &reply);
        assert_eq!(event, expected, "{call:?}");
        // Every event round-trips through the canonical text it prints.
        let text = event.to_vibon();
        assert_eq!(
            AuditEvent::decode("event.vibon", &text).expect("decodes"),
            event,
            "{text}"
        );
    }
}

// --- data and JSON codecs -----------------------------------------------------

fn unknown_event() -> AuditEvent {
    AuditEvent::Opaque(OpaqueEvent {
        kind: "deliver".to_owned(),
        fields: vec![("channel".to_owned(), "{id: 1u64}".to_owned())],
    })
}

#[test]
fn an_event_of_an_unknown_kind_is_preserved_as_opaque_data() {
    let text = "{kind: @deliver channel: {id: 1u64} note: \"x\"}";
    let event = AuditEvent::decode("event.vibon", text).expect("not a failure");
    assert_eq!(
        event,
        AuditEvent::Opaque(OpaqueEvent {
            kind: "deliver".to_owned(),
            fields: vec![
                ("channel".to_owned(), "{id: 1u64}".to_owned()),
                ("note".to_owned(), "\"x\"".to_owned()),
            ],
        })
    );
    assert_eq!(event.to_vibon(), text);
    assert_eq!(event.kind(), "deliver");
}

#[test]
fn two_spellings_of_one_event_decode_to_equal_events() {
    let written = "{\n  result: {variant: @ok typeInfo: @std.result.result kind: @enum}\n  args: []\n  op: \"io.stdin-text\" // a comment\n  kind: @host\n}";
    let canonical = AuditEvent::completed(&HostCall::StdinText, &Ok(HostOk::Void));
    assert_ne!(
        AuditEvent::completed(&HostCall::StdinText, &Ok(HostOk::Text(String::new()))),
        canonical
    );
    let AuditEvent::Host(expected) = &canonical else {
        panic!("a host event");
    };
    let mut expected = expected.clone();
    expected.result = ok(None);
    assert_eq!(
        AuditEvent::decode("event.vibon", written).expect("decodes"),
        AuditEvent::Host(expected)
    );
}

#[test]
fn json_events_match_the_command_result_schema_cases() {
    let host_json = json!({
        "kind": "@host",
        "op": "fs.read-file",
        "args": ["{kind: @wrapper typeInfo: @std.fs.path value: \"/in.txt\"}"],
        "result": "{kind: @enum payload: {kind: @bytes values: [104u8 105u8]} typeInfo: @std.result.result variant: @ok}"
    });
    let event = AuditEvent::completed(
        &HostCall::ReadFile("/in.txt".to_owned()),
        &Ok(HostOk::Bytes(b"hi".to_vec())),
    );
    assert_eq!(event.to_json(), host_json);
    assert_eq!(AuditEvent::from_json(&host_json).expect("reads"), event);

    let future = json!({ "kind": "@deliver", "channel": "{id: 1u64}" });
    assert_eq!(unknown_event().to_json(), future);
    assert_eq!(
        AuditEvent::from_json(&future).expect("reads"),
        unknown_event()
    );

    let test_item = json!({
        "kind": "@host", "op": "io.stdout-text",
        "args": ["\"hi\\n\""],
        "result": "{kind: @enum typeInfo: @std.result.result variant: @ok}"
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
    let text = "{kind: @host op: \"io.stdin-text\" args: [] result: 1u8 note: \"x\"}";
    let diagnostics = AuditEvent::decode("event.vibon", text).expect_err("rejected");
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code(), DiagnosticCode::DataInvalidShape);
    assert_eq!(diagnostics[0].source_id(), Some("event.vibon"));
}

#[test]
fn malformed_events_are_rejected_with_data_diagnostics() {
    for text in [
        "[]",
        "{op: \"x\"}",
        "{kind: \"host\"}",
        "{kind: @host op: 1u8 args: [] result: 1u8}",
        "{kind: @host op: \"x\" args: 1u8 result: 1u8}",
        "{kind: @host op: \"x\" args: []}",
        "{kind: @host op: \"x\" op: \"y\" args: [] result: 1u8}",
        "{kind: @host op: \"x\" args: [] result: 1u8",
    ] {
        let diagnostics = AuditEvent::decode("event.vibon", text).expect_err(text);
        assert!(!diagnostics.is_empty(), "{text}");
        for diagnostic in &diagnostics {
            assert_eq!(diagnostic.source_id(), Some("event.vibon"), "{text}");
        }
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
    assert!(text.starts_with("{format: @auditTrace.v1 events: ["));
    assert_eq!(decode_trace("trace.vibon", &text).expect("decodes"), events);

    assert_eq!(trace_to_vibon(&[]), "{format: @auditTrace.v1 events: []}");
    assert_eq!(
        decode_trace("trace.vibon", "{format: @auditTrace.v1 events: []}")
            .expect("decodes"),
        Vec::new()
    );
    for bad in [
        "{events: []}",
        "{format: @auditTrace.v1}",
        "{format: @auditTrace.v1 events: [1u8]}",
        "{format: @auditTrace.v1 events: [] extra: 1u8}",
    ] {
        assert!(decode_trace("trace.vibon", bad).is_err(), "{bad}");
    }
    // Another format is a value fault at the format, whatever follows it.
    let diagnostics =
        decode_trace("trace.vibon", "{format: @auditTrace.v2 events: 1u8}")
            .expect_err("another format");
    assert_eq!(
        diagnostics.iter().map(|d| d.code()).collect::<Vec<_>>(),
        [DiagnosticCode::DataInvalidValue]
    );
}

/// `audit_trace_round_trip`: a trace of three host events, written as
/// canonical text, decodes to equal events.
#[test]
fn audit_trace_round_trip() {
    let events = vec![
        AuditEvent::completed(
            &HostCall::WriteFile("/out.bin".to_owned(), vec![1, 2]),
            &Ok(HostOk::Void),
        ),
        AuditEvent::completed(
            &HostCall::Stat("/out.bin".to_owned()),
            &Ok(HostOk::Metadata(Metadata {
                kind: FsKind::File,
                size: 2,
            })),
        ),
        AuditEvent::completed(
            &HostCall::EnvVar("HOME".to_owned()),
            &Err(OperationError::Env(EnvError::NotPresent)),
        ),
    ];
    let text = trace_to_vibon(&events);
    let decoded = decode_trace("trace.vibon", &text).expect("decodes");
    assert_eq!(decoded, events);
    assert_eq!(trace_to_vibon(&decoded), text);
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
        result(Some(Tree::List(vec![
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
fn every_provider_error_is_a_case_of_the_registry_type_in_order() {
    fn names<T: Copy>(all: &[T], name: impl Fn(T) -> &'static str) -> Vec<String> {
        all.iter()
            .map(|variant| name(*variant).to_owned())
            .collect()
    }
    // The major-1 registry spells a case with hyphens; its atom is camelCase.
    fn atoms(registry: &[&str]) -> Vec<String> {
        registry
            .iter()
            .map(|name| {
                let mut parts = name.split('-');
                let mut atom = parts.next().unwrap_or_default().to_owned();
                for part in parts {
                    let mut characters = part.chars();
                    atom.extend(
                        characters.next().map(|first| first.to_ascii_uppercase()),
                    );
                    atom.push_str(characters.as_str());
                }
                atom
            })
            .collect()
    }
    assert_eq!(
        names(IoError::ALL, IoError::name),
        atoms(HostErrorType::Io.variants())
    );
    assert_eq!(
        names(FsError::ALL, FsError::name),
        atoms(HostErrorType::Fs.variants())
    );
    assert_eq!(
        names(EnvError::ALL, EnvError::name),
        atoms(HostErrorType::Env.variants())
    );
    assert_eq!(
        names(TimeError::ALL, TimeError::name),
        atoms(HostErrorType::Time.variants())
    );
    assert_eq!(
        names(RandomError::ALL, RandomError::name),
        atoms(HostErrorType::Random.variants())
    );
    assert_eq!(
        names(FsKind::ALL, FsKind::name),
        atoms(&vibra_ir::external::host::FS_KIND_VARIANTS)
    );
    for (index, variant) in FsError::ALL.iter().enumerate() {
        assert_eq!(variant.index() as usize, index);
    }
}

#[test]
fn a_call_performed_on_a_world_gives_the_event_and_the_value_from_one_reply() {
    let world = HostWorld::decode(
        "world.vibon",
        "{format: @hostWorld.v1 files: [\"/in.txt\": \"hi\"]}",
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
    assert_eq!(event.result, err("std.fs", "fsError", "notFound"));
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

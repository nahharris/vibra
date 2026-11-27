//! The audit trace: `@auditTrace.v2`, its events, the grant projections,
//! and the decoder (M2 step 17), over the scripted driver of step 15.
//!
//! `EXAMPLE` is the chapter's example document, copied from
//! `docs/spec/06-runtime.md`, "Audit trace". Its identities are placeholders;
//! [`with_seeds`] replaces each `seed` with the identity of the declaration
//! its `path` names in the library `std` 0.0.2, computed by
//! [`declaration_identity`], which the chapter's test vector checks.

use std::collections::BTreeSet;

use serde_json::json;
use vibra_diagnostics::{ByteSpan, DiagnosticCode};
use vibra_host::broker::{Context, RootGrant};
use vibra_host::driver::{
    Acquire, Action, Completion, Driver, DriverIdentity, Gate, Measured, Progress,
    Resolved, ResourceId,
};
use vibra_host::policy::{self, Policy, Rule};
use vibra_host::recipe::{LeafName, Limits, Provider};
use vibra_host::trace::{
    AuditTrace, Event, FailureCode, GrantOrigin, HostEvent, Phase, StartupFailure,
    StartupStatus, StdlibIdentity, TraceHeader, declaration_identity, decode, encode,
};
use vibra_ir::authority::{
    GrantLimits, GrantToken, HostArgument, HostCompletion, HostContext, HostErrorCase,
    HostFault, HostOk, HostReply, HostRequest, NarrowRequest, Narrowed, Receipt,
};
use vibra_ir::external::host::{HostInterface, HostOp};
use vibra_ir::{SourceOrigin, Value};
use vibra_syntax::{DataValue, parse_data};

use crate::scripted::{Scripted, Tweak};

/// The chapter's example, copied as written.
const EXAMPLE: &str = r#"{
  format: @auditTrace.v2
  revision: "sha256:5fdc01744c45544c5765d4ec386b7a45bb990bc4b4c0ca415493546c7f36c8ec"
  target: @hello
  drivers: [{provider: @host.io.stdout version: "0.0.2"}]
  policies: [:]
  redaction: @none
  events: [
    {
      kind: @startup
      binding: @out
      provider: @host.io.stdout
      status: @acquired
      grant: {
        issuedBy: {kind: @binding leaf: @out}
        policy: ""
        rights: [
          {
            path: @std.io.StdoutText
            seed: "sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad"
          }
        ]
        root: 1u64
      }
    }
    {
      kind: @host
      invocation: 2u64
      parent: 1u64
      phase: @entry
      op: "io.stdoutText"
      driver: {provider: @host.io.stdout version: "0.0.2"}
      origin: {sourceId: "src/hello/main.vib" start: 42u64 end: 68u64}
      grant: {
        issuedBy: {kind: @binding leaf: @out}
        policy: ""
        rights: [
          {
            path: @std.io.StdoutText
            seed: "sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad"
          }
        ]
        root: 1u64
      }
      args: ["hello\n"]
      result: {
        kind: @enum
        typeInfo: {
          kind: @declared
          path: @std.result.Result
          seed: "sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad"
          arguments: [
            @void
            {
              kind: @declared
              path: @std.io.IoError
              seed: "sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad"
              arguments: []
            }
          ]
        }
        variant: @ok
        payload: {value: void}
      }
    }
  ]
}"#;

const EXAMPLE_REVISION: &str =
    "sha256:5fdc01744c45544c5765d4ec386b7a45bb990bc4b4c0ca415493546c7f36c8ec";

const PLACEHOLDER: &str =
    "sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad";

fn library() -> StdlibIdentity {
    StdlibIdentity {
        package: "std".to_owned(),
        version: "0.0.2".to_owned(),
    }
}

/// The header of a test run: no target, the library `std` 0.0.2.
pub(crate) fn header() -> TraceHeader {
    TraceHeader {
        revision: EXAMPLE_REVISION.to_owned(),
        target: None,
        library: library(),
    }
}

/// The identity of the library declaration at `path` (with its `@`), of
/// kind `kind`.
fn seed(path: &str, kind: &str) -> String {
    let (module, name) = path.rsplit_once('.').expect("a dotted path");
    declaration_identity("std", "0.0.2", module, "", kind, name)
}

/// The kind of the library declaration at `path`: the operation interfaces
/// are interfaces, everything else the example names is a type.
fn kind_of(path: &str) -> &'static str {
    if HostInterface::ALL
        .iter()
        .any(|interface| interface.owner().path() == path)
    {
        "@interface"
    } else {
        "@type"
    }
}

/// `text` with each placeholder `seed` replaced by the identity of the
/// declaration the `path` line above it names.
fn with_seeds(text: &str) -> String {
    let mut path = String::new();
    let mut out = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("path: ") {
            path = rest.trim().to_owned();
        }
        if trimmed.starts_with("seed: ") && line.contains(PLACEHOLDER) {
            out.push(line.replace(PLACEHOLDER, &seed(&path, kind_of(&path))));
        } else {
            out.push(line.to_owned());
        }
    }
    out.join("\n")
}

/// The canonical bytes of a laid-out document, written without the
/// encoder: every run of white space outside a string is one space, and no
/// space follows an opening bracket or precedes a closing one.
fn one_line(text: &str) -> String {
    let mut out = String::new();
    let mut in_string = false;
    let mut escaped = false;
    let mut space = false;
    for character in text.chars() {
        if in_string {
            out.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }
        if character.is_whitespace() {
            space = true;
            continue;
        }
        if space
            && !out.is_empty()
            && !out.ends_with(['{', '['])
            && !matches!(character, '}' | ']')
        {
            out.push(' ');
        }
        space = false;
        if character == '"' {
            in_string = true;
        }
        out.push(character);
    }
    out
}

/// The `index`-th event of `EXAMPLE`, as laid-out text.
fn example_event(index: usize) -> String {
    let document = with_seeds(EXAMPLE);
    let parsed = parse_data(None, document.as_bytes());
    let root = parsed.value().expect("the example parses");
    let DataValue::Record(fields) = root.value() else {
        panic!("a record");
    };
    let events = fields
        .iter()
        .find(|field| field.name() == "events")
        .expect("events");
    let DataValue::List(events) = events.value().value() else {
        panic!("a list");
    };
    let span = events[index].span();
    document[span.start()..span.end()].to_owned()
}

/// A driver that answers as the scripted one and reports `version` for
/// every provider.
struct Versioned {
    inner: Scripted,
    version: &'static str,
}

impl Driver for Versioned {
    fn identity(&self, provider: Provider) -> Option<DriverIdentity> {
        Some(DriverIdentity {
            provider: provider.atom().to_owned(),
            version: self.version.to_owned(),
        })
    }

    fn acquire(&mut self, provider: Provider, resource: &Resolved) -> Acquire {
        self.inner.acquire(provider, resource)
    }

    fn release(&mut self, resource: ResourceId) -> Result<(), String> {
        self.inner.release(resource)
    }

    fn call(
        &mut self,
        resource: ResourceId,
        action: &Action<'_>,
        gate: &mut dyn Gate,
    ) -> Completion {
        self.inner.call(resource, action, gate)
    }
}

fn names(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

fn decoded_policy(expression: &str) -> Policy {
    policy::decode(
        format!(
            "{{format: @scopePolicy.v1 family: @filesystem expression: {expression}}}"
        )
        .as_bytes(),
    )
    .expect("a policy")
}

fn rule(root: &str, prefix: &str, actions: &str) -> String {
    format!(
        "{{kind: @fileRule root: {root} prefix: [{prefix}] depth: @tree actions: [{actions}] \
         names: {{kind: @anyName}}}}"
    )
}

fn root_grant(
    interface: HostInterface,
    resource: u64,
    origin: GrantOrigin,
    policy: Option<Policy>,
    limits: Limits,
) -> RootGrant {
    RootGrant {
        interface,
        resource: ResourceId(resource),
        origin,
        policy,
        variables: None,
        limits,
    }
}

fn binding(leaf: &str) -> GrantOrigin {
    GrantOrigin::Binding {
        leaf: LeafName::new(leaf),
    }
}

fn stream(cx: &mut Context<'_>, interface: HostInterface, leaf: &str) -> GrantToken {
    cx.root_for_tests(root_grant(
        interface,
        1,
        binding(leaf),
        None,
        Limits::default(),
    ))
    .expect("a root")
}

fn request<'v>(
    op: HostOp,
    grant: GrantToken,
    arguments: &'v [HostArgument<'v>],
    invocation: u64,
    origin: Option<&'v SourceOrigin>,
) -> HostRequest<'v> {
    HostRequest {
        op,
        grant,
        arguments,
        invocation,
        parent: Some(1),
        origin,
        room: u64::MAX,
    }
}

/// Calls and commits the receipt, when there is one.
fn run(cx: &mut Context<'_>, request: HostRequest<'_>) -> HostCompletion {
    let completion = cx.host(request).expect("a completion");
    let reply = completion.reply.clone();
    let due = completion.receipt.is_some();
    if let Some(receipt) = completion.receipt {
        cx.commit(receipt).expect("the pending receipt");
    }
    HostCompletion {
        reply,
        receipt: due.then(|| Receipt::new(0, 0)),
    }
}

fn write(cx: &mut Context<'_>, grant: GrantToken, text: &str, invocation: u64) {
    let arguments = [HostArgument::Text(text)];
    run(
        cx,
        request(HostOp::IoStdoutText, grant, &arguments, invocation, None),
    );
}

fn read(cx: &mut Context<'_>, grant: GrantToken, at: &[&str]) -> HostCompletion {
    let at = names(at);
    let arguments = [HostArgument::Path(&at)];
    run(cx, request(HostOp::FsReadText, grant, &arguments, 3, None))
}

fn host_events(trace: &AuditTrace) -> Vec<&HostEvent> {
    trace
        .events
        .iter()
        .filter_map(|event| match event {
            Event::Host(event) => Some(event),
            _ => None,
        })
        .collect()
}

fn only_event(trace: &AuditTrace) -> &HostEvent {
    let events = host_events(trace);
    assert_eq!(events.len(), 1, "{trace:#?}");
    events[0]
}

fn err(op: HostOp, case: &str) -> HostReply {
    HostReply::Err(HostErrorCase::new(op.row().error, case).expect("a case"))
}

fn declared(path: &str, arguments: &str) -> String {
    format!(
        "{{kind: @declared path: {path} seed: \"{}\" arguments: [{arguments}]}}",
        seed(path, "@type")
    )
}

// Source policy values, as the interpreter encodes an `FsPolicy`.

fn variant(case: &str, fields: Vec<(&str, Value)>) -> Value {
    Value::Variant {
        case: case.to_owned(),
        fields: fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    }
}

fn files(prefix: &[&str], actions: &[&str]) -> Value {
    variant(
        "files",
        vec![
            (
                "prefix",
                Value::Wrapper(Box::new(Value::List(
                    prefix
                        .iter()
                        .map(|name| Value::Str((*name).to_owned()))
                        .collect(),
                ))),
            ),
            ("depth", variant("tree", Vec::new())),
            (
                "actions",
                Value::List(
                    actions
                        .iter()
                        .map(|action| variant(action, Vec::new()))
                        .collect(),
                ),
            ),
            ("names", variant("anyName", Vec::new())),
        ],
    )
}

fn granted(narrowed: Narrowed) -> GrantToken {
    match narrowed {
        Narrowed::Grant(token) => token,
        Narrowed::Refused(refusal) => panic!("refused: {refusal:?}"),
    }
}

fn restrict(cx: &mut Context<'_>, grant: GrantToken, policy: &Value) -> GrantToken {
    granted(
        cx.narrow(NarrowRequest::Restrict { grant, policy })
            .expect("a restriction"),
    )
}

fn sub(cx: &mut Context<'_>, grant: GrantToken, prefix: &[&str]) -> GrantToken {
    let prefix = names(prefix);
    granted(
        cx.narrow(NarrowRequest::Sub {
            grant,
            prefix: &prefix,
        })
        .expect("a sub"),
    )
}

/// The roots of every rule of `policy`.
fn rule_roots(policy: &Policy) -> BTreeSet<String> {
    policy
        .expression()
        .rules()
        .map(|rule: Rule<'_>| rule.root().to_owned())
        .collect()
}

/// The example run: the `@out` leaf acquired, then `io.stdoutText
/// "hello\n"` as invocation 2 under invocation 1, with the source origin
/// `(src/hello/main.vib 42 68)`.
fn example_run(cx: &mut Context<'_>) {
    let out = stream(cx, HostInterface::StdoutText, "@out");
    cx.startup_event_for_tests(
        LeafName::new("@out"),
        Provider::IoStdout,
        StartupStatus::Acquired,
        Some(out),
        None,
    )
    .expect("the startup event");
    let origin = SourceOrigin::new("src/hello/main.vib", ByteSpan::new(42, 68));
    let arguments = [HostArgument::Text("hello\n")];
    let completion = run(
        cx,
        request(HostOp::IoStdoutText, out, &arguments, 2, Some(&origin)),
    );
    assert_eq!(completion.reply, HostReply::Ok(HostOk::Void));
}

fn example_header() -> TraceHeader {
    TraceHeader {
        target: Some("@hello".to_owned()),
        ..header()
    }
}

#[test]
fn the_declaration_identity_holds_the_chapter_test_vector() {
    assert_eq!(
        declaration_identity("hello", "0.1.0", "@hello.main", "", "@proc", "main"),
        PLACEHOLDER
    );
}

#[test]
fn one_completed_call_is_the_host_event_of_the_example() {
    let mut driver = Versioned {
        inner: Scripted::new(),
        version: "0.0.2",
    };
    let mut cx = Context::new(&mut driver, example_header());
    example_run(&mut cx);
    let event = Event::Host(only_event(cx.trace()).clone());
    assert_eq!(event.canonical_bytes(), one_line(&example_event(1)));
    // The whole document, field for field.
    assert_eq!(encode(cx.trace()), one_line(&with_seeds(EXAMPLE)));
    assert_eq!(
        decode(None, with_seeds(EXAMPLE).as_bytes()).as_ref(),
        Ok(cx.trace())
    );
}

#[test]
fn the_fields_of_a_host_event_are_in_the_chapter_order() {
    let mut driver = Versioned {
        inner: Scripted::new(),
        version: "0.0.2",
    };
    let mut cx = Context::new(&mut driver, example_header());
    example_run(&mut cx);
    let bytes = only_event(cx.trace()).canonical_bytes();
    let parsed = parse_data(None, bytes.as_bytes());
    let DataValue::Record(fields) = parsed.value().expect("a value").value() else {
        panic!("a record");
    };
    let order = fields.iter().map(|field| field.name()).collect::<Vec<_>>();
    assert_eq!(
        order,
        [
            "kind",
            "invocation",
            "parent",
            "phase",
            "op",
            "driver",
            "origin",
            "grant",
            "args",
            "result"
        ]
    );
}

#[test]
fn a_startup_read_has_no_parent() {
    let script = Scripted::new();
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let args = stream(&mut cx, HostInterface::EnvReadArgs, "@args");
    let completion = run(
        &mut cx,
        HostRequest {
            parent: None,
            ..request(HostOp::EnvReadArgs, args, &[], 1, None)
        },
    );
    assert!(completion.receipt.is_some());
    let event = only_event(cx.trace());
    assert_eq!((event.parent, event.phase), (None, Phase::Startup));
    let bytes = event.canonical_bytes();
    assert!(!bytes.contains(" parent: "), "{bytes}");
    assert!(bytes.contains(" phase: @startup "), "{bytes}");
}

#[test]
fn an_unknown_source_origin_is_absent() {
    let mut driver = Scripted::new();
    let mut cx = Context::new(&mut driver, header());
    let out = stream(&mut cx, HostInterface::StdoutText, "@out");
    write(&mut cx, out, "x", 2);
    let event = only_event(cx.trace());
    assert_eq!(event.origin, None);
    assert!(!event.canonical_bytes().contains(" origin: "));
}

#[test]
fn a_denial_before_the_adapter_appends_nothing() {
    let script = Scripted::new();
    script.file(&["b", "x.txt"], "x");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let notes = cx
        .root_for_tests(root_grant(
            HostInterface::DirReadText,
            1,
            binding("@notes"),
            Some(decoded_policy(&rule("@notes", "\"a\"", "@read"))),
            Limits::default(),
        ))
        .expect("a root");
    let completion = read(&mut cx, notes, &["b", "x.txt"]);
    assert_eq!(completion.reply, err(HostOp::FsReadText, "denied"));
    assert!(completion.receipt.is_none());
    assert!(script.calls().is_empty());
    assert!(cx.trace().events.is_empty());
}

#[test]
fn a_grant_at_its_bound_appends_nothing() {
    let mut driver = Scripted::new();
    let mut cx = Context::new(&mut driver, header());
    let out = cx
        .root_for_tests(root_grant(
            HostInterface::StdoutText,
            1,
            binding("@out"),
            None,
            Limits {
                invocations: Some(1),
                ..Limits::default()
            },
        ))
        .expect("a root");
    cx.preset_for_tests(out, [1, 0, 0]);
    let arguments = [HostArgument::Text("x")];
    let completion = run(
        &mut cx,
        request(HostOp::IoStdoutText, out, &arguments, 2, None),
    );
    assert_eq!(completion.reply, err(HostOp::IoStdoutText, "limitExceeded"));
    assert!(completion.receipt.is_none());
    assert!(cx.trace().events.is_empty());
}

#[test]
fn a_size_refusal_at_the_gate_has_no_receipt_and_no_event() {
    let script = Scripted::new();
    script.file(&["a.txt"], "hello");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let notes = cx
        .root_for_tests(root_grant(
            HostInterface::DirReadText,
            1,
            binding("@notes"),
            Some(decoded_policy(&rule("@notes", "", "@read"))),
            Limits {
                input_bytes: Some(4),
                ..Limits::default()
            },
        ))
        .expect("a root");
    let completion = read(&mut cx, notes, &["a.txt"]);
    assert_eq!(completion.reply, err(HostOp::FsReadText, "limitExceeded"));
    assert!(completion.receipt.is_none());
    assert_eq!(script.calls().len(), 1);
    assert!(cx.trace().events.is_empty());
    assert_eq!(cx.uncommitted(), 0);
}

#[test]
fn a_completed_denial_is_one_event() {
    let script = Scripted::new();
    script.file(&["b.txt"], "x");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let notes = cx
        .root_for_tests(root_grant(
            HostInterface::DirReadText,
            1,
            binding("@notes"),
            Some(decoded_policy(
                "{kind: @fileRule root: @notes prefix: [] depth: @tree actions: [@read] \
                 names: {kind: @exact names: [\"b.txt\"]}}",
            )),
            Limits::default(),
        ))
        .expect("a root");
    script.tweak(Tweak {
        real_names: Some(names(&["B.txt"])),
        ..Tweak::default()
    });
    let completion = read(&mut cx, notes, &["b.txt"]);
    assert_eq!(completion.reply, err(HostOp::FsReadText, "denied"));
    assert!(completion.receipt.is_some());
    let fs_error = declared("@std.fs.FsError", "");
    let expected = format!(
        "{{kind: @enum typeInfo: {} variant: @err payload: {{error: {{kind: @enum \
         typeInfo: {fs_error} variant: @denied}}}}}}",
        declared(
            "@std.result.Result",
            &format!("{} {fs_error}", declared("@std.str.Str", ""))
        )
    );
    assert_eq!(only_event(cx.trace()).result, expected);
}

#[test]
fn an_unknown_outcome_is_one_event() {
    let script = Scripted::new();
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let out = stream(&mut cx, HostInterface::StdoutText, "@out");
    script.tweak(Tweak {
        completion: Some(Completion::Completed {
            reply: err(HostOp::IoStdoutText, "outcomeUnknown"),
            progress: Progress {
                input: Measured::Known(0),
                output: Measured::Uncertain,
            },
        }),
        ..Tweak::default()
    });
    write(&mut cx, out, "x", 2);
    let io_error = declared("@std.io.IoError", "");
    let expected = format!(
        "{{kind: @enum typeInfo: {} variant: @err payload: {{error: {{kind: @enum \
         typeInfo: {io_error} variant: @outcomeUnknown}}}}}}",
        declared("@std.result.Result", &format!("@void {io_error}"))
    );
    assert_eq!(only_event(cx.trace()).result, expected);
}

#[test]
fn events_are_in_completion_order() {
    let mut driver = Scripted::new();
    let mut cx = Context::new(&mut driver, header());
    let out = stream(&mut cx, HostInterface::StdoutText, "@out");
    let err_stream = stream(&mut cx, HostInterface::StderrText, "@err");
    write(&mut cx, out, "1", 5);
    let arguments = [HostArgument::Text("2")];
    run(
        &mut cx,
        request(HostOp::IoStderrText, err_stream, &arguments, 6, None),
    );
    write(&mut cx, out, "3", 7);
    let events = host_events(cx.trace());
    let seen = events
        .iter()
        .map(|event| {
            (
                event.invocation,
                event.op,
                event.args.clone(),
                event.grant.root,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        seen,
        [
            (5, HostOp::IoStdoutText, vec!["\"1\"".to_owned()], 1),
            (6, HostOp::IoStderrText, vec!["\"2\"".to_owned()], 2),
            (7, HostOp::IoStdoutText, vec!["\"3\"".to_owned()], 1),
        ]
    );
}

#[test]
fn a_stream_grant_projects_no_policy_and_no_limits() {
    let mut driver = Scripted::new();
    let mut cx = Context::new(&mut driver, header());
    let out = stream(&mut cx, HostInterface::StdoutText, "@out");
    write(&mut cx, out, "x", 2);
    let grant = &only_event(cx.trace()).grant;
    assert_eq!((grant.policy.as_str(), grant.limits), ("", None));
    let bytes = grant.canonical_bytes();
    assert!(bytes.contains("policy: \"\""), "{bytes}");
    assert!(!bytes.contains("limits"), "{bytes}");
    assert!(cx.trace().policies.is_empty());
}

#[test]
fn a_directory_grant_projects_the_hash_of_its_normal_form() {
    let script = Scripted::new();
    script.file(&["a.txt"], "x");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    // Written out of normal form: the actions unsorted, the rule repeated.
    let written = format!(
        "{{kind: @anyOf items: [{} {}]}}",
        rule("@notes", "", "@stat @read"),
        rule("@notes", "", "@stat @read")
    );
    let p = decoded_policy(&written);
    let notes = cx
        .root_for_tests(root_grant(
            HostInterface::DirReadText,
            1,
            binding("@notes"),
            Some(p.clone()),
            Limits::default(),
        ))
        .expect("a root");
    read(&mut cx, notes, &["a.txt"]);
    let normal = decoded_policy(&rule("@notes", "", "@read @stat"));
    let hash = policy::hash(&normal);
    assert_eq!(only_event(cx.trace()).grant.policy, hash);
    assert_eq!(cx.trace().policies.get(&hash), Some(&normal));
    assert_eq!(cx.trace().policies.len(), 1);
}

#[test]
fn a_restricted_grant_projects_its_effective_policy() {
    let script = Scripted::new();
    script.file(&["a.txt"], "x");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let p = rule("@notes", "", "@read @stat");
    let notes = cx
        .root_for_tests(root_grant(
            HostInterface::DirReadText,
            1,
            binding("@notes"),
            Some(decoded_policy(&p)),
            Limits::default(),
        ))
        .expect("a root");
    let child = restrict(&mut cx, notes, &files(&[], &["read"]));
    read(&mut cx, notes, &["a.txt"]);
    read(&mut cx, child, &["a.txt"]);
    // Q7: the conjunction of the root's policy and the restriction, whose
    // rule names the root's leaf.
    let effective = decoded_policy(&format!(
        "{{kind: @allOf items: [{p} {}]}}",
        rule("@notes", "", "@read")
    ));
    let events = host_events(cx.trace());
    assert_eq!(events[0].grant.policy, policy::hash(&decoded_policy(&p)));
    assert_eq!(events[1].grant.policy, policy::hash(&effective));
    let policies = &cx.trace().policies;
    assert_eq!(policies.len(), 2);
    assert_eq!(policies.get(&events[1].grant.policy), Some(&effective));
    assert_eq!(
        policies.get(&events[0].grant.policy),
        Some(&decoded_policy(&p))
    );
}

#[test]
fn every_rule_after_restrict_and_sub_names_the_leaf() {
    let script = Scripted::new();
    script.file(&["d", "a.txt"], "x");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let notes = cx
        .root_for_tests(root_grant(
            HostInterface::DirReadText,
            1,
            binding("@notes"),
            Some(decoded_policy(&rule("@notes", "", "@read"))),
            Limits::default(),
        ))
        .expect("a root");
    let restricted = restrict(&mut cx, notes, &files(&[], &["read"]));
    let child = sub(&mut cx, restricted, &["d"]);
    read(&mut cx, child, &["a.txt"]);
    let hash = &only_event(cx.trace()).grant.policy;
    let effective = cx.trace().policies.get(hash).expect("the effective policy");
    assert_eq!(rule_roots(effective), BTreeSet::from(["@notes".to_owned()]));
}

#[test]
fn every_rule_of_a_test_world_grant_names_the_test_world() {
    let script = Scripted::new();
    script.file(&["a.txt"], "x");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let root = cx
        .root_for_tests(root_grant(
            HostInterface::DirReadText,
            1,
            GrantOrigin::TestWorld,
            Some(decoded_policy(&rule("@testWorld", "", "@read @stat"))),
            Limits::default(),
        ))
        .expect("a root");
    let child = restrict(&mut cx, root, &files(&[], &["read"]));
    read(&mut cx, root, &["a.txt"]);
    read(&mut cx, child, &["a.txt"]);
    let policies = &cx.trace().policies;
    assert_eq!(policies.len(), 2);
    for policy in policies.values() {
        assert_eq!(
            rule_roots(policy),
            BTreeSet::from(["@testWorld".to_owned()])
        );
    }
}

#[test]
fn a_limited_grant_projects_the_bound_it_sets() {
    let mut driver = Scripted::new();
    let mut cx = Context::new(&mut driver, header());
    let out = stream(&mut cx, HostInterface::StdoutText, "@out");
    let limited = granted(
        cx.narrow(NarrowRequest::Limit {
            grant: out,
            limits: GrantLimits {
                invocations: Some(5),
                ..GrantLimits::default()
            },
        })
        .expect("a limit"),
    );
    write(&mut cx, limited, "x", 2);
    let grant = &only_event(cx.trace()).grant;
    assert_eq!(
        grant.limits,
        Some(Limits {
            invocations: Some(5),
            ..Limits::default()
        })
    );
    assert!(
        grant
            .canonical_bytes()
            .contains(" limits: {invocations: 5u64} root: 1u64}")
    );
}

#[test]
fn narrowing_keeps_the_origin_and_the_root() {
    let script = Scripted::new();
    script.file(&["d", "a.txt"], "x");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let other = stream(&mut cx, HostInterface::StdoutText, "@out");
    let root = cx
        .root_for_tests(root_grant(
            HostInterface::DirReadText,
            2,
            GrantOrigin::TestWorld,
            Some(decoded_policy(&rule("@testWorld", "", "@read"))),
            Limits::default(),
        ))
        .expect("a root");
    let restricted = restrict(&mut cx, root, &files(&[], &["read"]));
    let child = sub(&mut cx, restricted, &["d"]);
    write(&mut cx, other, "x", 2);
    read(&mut cx, child, &["a.txt"]);
    let events = host_events(cx.trace());
    assert_eq!(events[1].grant.issued_by, GrantOrigin::TestWorld);
    assert_eq!(events[1].grant.root, 2);
    assert!(
        events[1]
            .grant
            .canonical_bytes()
            .starts_with("{issuedBy: {kind: @testWorld} ")
    );
}

#[test]
fn two_roots_have_two_ordinals() {
    let mut driver = Scripted::new();
    let mut cx = Context::new(&mut driver, header());
    let first = stream(&mut cx, HostInterface::StdoutText, "@out");
    let second = stream(&mut cx, HostInterface::StdoutText, "@log");
    write(&mut cx, first, "1", 2);
    write(&mut cx, second, "2", 3);
    let roots = host_events(cx.trace())
        .iter()
        .map(|event| event.grant.canonical_bytes())
        .collect::<Vec<_>>();
    assert!(roots[0].ends_with(" root: 1u64}"), "{roots:?}");
    assert!(roots[1].ends_with(" root: 2u64}"), "{roots:?}");
}

#[test]
fn the_drivers_are_sorted_by_provider() {
    let mut driver = Scripted::new();
    let mut cx = Context::new(&mut driver, header());
    stream(&mut cx, HostInterface::StdoutText, "@out");
    cx.root_for_tests(root_grant(
        HostInterface::DirReadText,
        2,
        binding("@notes"),
        Some(decoded_policy(&rule("@notes", "", "@read"))),
        Limits::default(),
    ))
    .expect("a root");
    let providers = cx
        .trace()
        .drivers
        .iter()
        .map(|driver| driver.provider.as_str())
        .collect::<Vec<_>>();
    assert_eq!(providers, ["@host.fs.directory", "@host.io.stdout"]);
}

#[test]
fn an_event_names_the_bound_driver() {
    let mut driver = Versioned {
        inner: Scripted::new(),
        version: "9.9.9",
    };
    let mut cx = Context::new(&mut driver, header());
    let out = stream(&mut cx, HostInterface::StdoutText, "@out");
    write(&mut cx, out, "x", 2);
    let expected = DriverIdentity {
        provider: "@host.io.stdout".to_owned(),
        version: "9.9.9".to_owned(),
    };
    assert_eq!(only_event(cx.trace()).driver, expected);
    assert_eq!(cx.trace().drivers, [expected]);
}

/// `ATK-trace-uncommitted-no-event`.
#[test]
fn atk_trace_uncommitted_no_event() {
    let mut driver = Scripted::new();
    let mut cx = Context::new(&mut driver, header());
    let out = stream(&mut cx, HostInterface::StdoutText, "@out");
    let arguments = [HostArgument::Text("x")];
    let completion = cx
        .host(request(HostOp::IoStdoutText, out, &arguments, 2, None))
        .expect("a completion");
    assert!(completion.receipt.is_some());
    drop(completion);
    assert!(cx.trace().events.is_empty());
    assert_eq!(cx.uncommitted(), 1);
}

/// `ATK-trace-commit-once`.
#[test]
fn atk_trace_commit_once() {
    let mut driver = Scripted::new();
    let mut cx = Context::new(&mut driver, header());
    let out = stream(&mut cx, HostInterface::StdoutText, "@out");
    let arguments = [HostArgument::Text("x")];
    let completion = cx
        .host(request(HostOp::IoStdoutText, out, &arguments, 2, None))
        .expect("a completion");
    let receipt = completion.receipt.expect("a receipt");
    let (context, serial) = (receipt.context(), receipt.serial());
    cx.commit(receipt).expect("the pending receipt");
    let again = cx.commit(Receipt::new(context, serial));
    assert!(
        matches!(again, Err(HostFault::InvalidHostValue(_))),
        "{again:?}"
    );
    assert_eq!(cx.trace().events.len(), 1);
}

#[test]
fn an_acquired_leaf_is_the_startup_event_of_the_example() {
    let mut driver = Versioned {
        inner: Scripted::new(),
        version: "0.0.2",
    };
    let mut cx = Context::new(&mut driver, example_header());
    let out = stream(&mut cx, HostInterface::StdoutText, "@out");
    cx.startup_event_for_tests(
        LeafName::new("@out"),
        Provider::IoStdout,
        StartupStatus::Acquired,
        Some(out),
        None,
    )
    .expect("the startup event");
    assert_eq!(cx.trace().events.len(), 1);
    assert_eq!(
        cx.trace().events[0].canonical_bytes(),
        one_line(&example_event(0))
    );
}

#[test]
fn a_refused_optional_leaf_has_no_grant_and_no_failure() {
    let mut driver = Scripted::new();
    let mut cx = Context::new(&mut driver, header());
    cx.startup_event_for_tests(
        LeafName::new("@notes"),
        Provider::FsDirectory,
        StartupStatus::Refused,
        None,
        None,
    )
    .expect("the startup event");
    assert_eq!(
        cx.trace().events[0].canonical_bytes(),
        "{kind: @startup binding: @notes provider: @host.fs.directory status: @refused}"
    );
}

#[test]
fn a_failed_leaf_records_its_failure() {
    let mut driver = Scripted::new();
    let mut cx = Context::new(&mut driver, header());
    cx.startup_event_for_tests(
        LeafName::new("@notes"),
        Provider::FsDirectory,
        StartupStatus::Failed,
        None,
        Some(StartupFailure {
            code: FailureCode::AcquisitionFailed,
            message: "the directory cannot be opened".to_owned(),
        }),
    )
    .expect("the startup event");
    assert_eq!(
        cx.trace().events[0].canonical_bytes(),
        "{kind: @startup binding: @notes provider: @host.fs.directory status: @failed \
         failure: {code: @grant.acquisitionFailed message: \"the directory cannot be opened\"}}"
    );
    // A failed leaf without its failure, or an acquired one without a grant,
    // is not a startup event.
    for (status, failure) in [
        (StartupStatus::Failed, None),
        (StartupStatus::Acquired, None),
        (
            StartupStatus::Refused,
            Some(StartupFailure {
                code: FailureCode::AcquisitionFailed,
                message: String::new(),
            }),
        ),
    ] {
        let refused = cx.startup_event_for_tests(
            LeafName::new("@x"),
            Provider::FsDirectory,
            status,
            None,
            failure,
        );
        assert!(matches!(refused, Err(HostFault::InvalidHostValue(_))));
    }
    assert_eq!(cx.trace().events.len(), 1);
}

/// A trace with both kinds, a policy, limits, an origin, and a startup
/// read.
fn full_trace() -> AuditTrace {
    let script = Scripted::new();
    script.file(&["d", "a.txt"], "hello");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, example_header());
    let notes = cx
        .root_for_tests(root_grant(
            HostInterface::DirReadText,
            1,
            binding("@notes"),
            Some(decoded_policy(&rule("@notes", "", "@read"))),
            Limits {
                input_bytes: Some(64),
                ..Limits::default()
            },
        ))
        .expect("a root");
    cx.startup_event_for_tests(
        LeafName::new("@notes"),
        Provider::FsDirectory,
        StartupStatus::Acquired,
        Some(notes),
        None,
    )
    .expect("the startup event");
    cx.startup_event_for_tests(
        LeafName::new("@log"),
        Provider::IoStderr,
        StartupStatus::Refused,
        None,
        Some(StartupFailure {
            code: FailureCode::RequiredUnavailable,
            message: "no stream".to_owned(),
        }),
    )
    .expect("the startup event");
    let args = stream(&mut cx, HostInterface::EnvReadArgs, "@args");
    run(
        &mut cx,
        HostRequest {
            parent: None,
            ..request(HostOp::EnvReadArgs, args, &[], 1, None)
        },
    );
    let child = sub(&mut cx, notes, &["d"]);
    let origin = SourceOrigin::new("src/a/main.vib", ByteSpan::new(0, 3));
    let at = names(&["a.txt"]);
    let arguments = [HostArgument::Path(&at)];
    run(
        &mut cx,
        request(HostOp::FsReadText, child, &arguments, 3, Some(&origin)),
    );
    let out = stream(&mut cx, HostInterface::StdoutBytes, "@out");
    let arguments = [HostArgument::Bytes(&[0, 255])];
    run(
        &mut cx,
        request(HostOp::IoStdoutBytes, out, &arguments, 4, None),
    );
    let stat = stream(&mut cx, HostInterface::DirStat, "@stat");
    let at = names(&["d", "a.txt"]);
    let arguments = [HostArgument::Path(&at)];
    run(
        &mut cx,
        request(HostOp::FsMetadataStat, stat, &arguments, 5, None),
    );
    cx.trace().clone()
}

#[test]
fn a_trace_round_trips() {
    let trace = full_trace();
    assert_eq!(trace.events.len(), 6, "{trace:#?}");
    let bytes = encode(&trace);
    assert_eq!(decode(Some("trace.vibon"), bytes.as_bytes()), Ok(trace));
}

#[test]
fn an_unknown_field_of_a_host_event_is_rejected() {
    let document = with_seeds(EXAMPLE).replace(
        "      kind: @host\n",
        "      kind: @host\n      extra: 1u64\n",
    );
    let faults = decode(None, document.as_bytes()).expect_err("rejected");
    assert_eq!(faults.len(), 1);
    assert_eq!(faults[0].code(), DiagnosticCode::DataInvalidShape);
    assert!(faults[0].message().contains("extra"), "{faults:?}");
}

#[test]
fn an_unknown_kind_is_kept_as_opaque_data() {
    let document = with_seeds(EXAMPLE).replace(
        "  events: [\n",
        "  events: [\n    {kind: @attempt zeta: 1 alpha: \"x\" nested: [\"b\": 2u8 \"a\": 1u8]}\n",
    );
    let trace = decode(None, document.as_bytes()).expect("a trace");
    let Event::Opaque(event) = &trace.events[0] else {
        panic!("an opaque event: {:?}", trace.events[0]);
    };
    assert_eq!(event.kind(), "@attempt");
    let canonical =
        "{alpha: \"x\" kind: @attempt nested: [\"a\": 1u8 \"b\": 2u8] zeta: 1}";
    assert_eq!(event.canonical_bytes(), canonical);
    assert!(
        encode(&trace).contains(&format!("events: [{canonical} {{kind: @startup "))
    );
    assert_eq!(decode(None, encode(&trace).as_bytes()), Ok(trace));
}

/// Every integer a trace holds.
fn integers(trace: &AuditTrace) -> BTreeSet<u64> {
    let mut found = BTreeSet::new();
    for event in &trace.events {
        let grant = match event {
            Event::Host(event) => {
                found.insert(event.invocation);
                found.extend(event.parent);
                if let Some(origin) = &event.origin {
                    found.insert(u64::try_from(origin.span().start()).expect("small"));
                    found.insert(u64::try_from(origin.span().end()).expect("small"));
                }
                Some(&event.grant)
            }
            Event::Startup(event) => event.grant.as_ref(),
            Event::Opaque(_) => None,
        };
        if let Some(grant) = grant {
            found.insert(grant.root);
            if let Some(limits) = grant.limits {
                found.extend(limits.invocations);
                found.extend(limits.input_bytes);
                found.extend(limits.output_bytes);
            }
        }
    }
    found
}

/// `ATK-trace-decode-grants-nothing`.
#[test]
fn atk_trace_decode_grants_nothing() {
    let trace = decode(None, encode(&full_trace()).as_bytes()).expect("a trace");
    let found = integers(&trace);
    assert!(found.contains(&0) && found.contains(&1), "{found:?}");
    loop {
        let mut driver = Scripted::new();
        let mut cx = Context::new(&mut driver, header());
        // Slot 0, generation 0: a live stdout root of the new context.
        let out = stream(&mut cx, HostInterface::StdoutText, "@out");
        if found.contains(&out.context()) {
            // The context's identity is a process counter: take another, so
            // that no token below can carry it.
            continue;
        }
        let arguments = [HostArgument::Text("x")];
        for context in &found {
            for slot in &found {
                for generation in &found {
                    let (Ok(slot), Ok(generation)) =
                        (u32::try_from(*slot), u32::try_from(*generation))
                    else {
                        continue;
                    };
                    let token = GrantToken::new(*context, slot, generation);
                    let refused = cx.host(request(
                        HostOp::IoStdoutText,
                        token,
                        &arguments,
                        2,
                        None,
                    ));
                    assert!(
                        matches!(refused, Err(HostFault::InvalidHostValue(_))),
                        "{token:?}: {refused:?}"
                    );
                }
            }
        }
        assert!(cx.trace().events.is_empty());
        break;
    }
}

#[test]
fn the_json_projection_gives_each_field_its_own_member() {
    let mut driver = Versioned {
        inner: Scripted::new(),
        version: "0.0.2",
    };
    let mut cx = Context::new(&mut driver, example_header());
    example_run(&mut cx);
    let event = Event::Host(only_event(cx.trace()).clone());
    let span = |origin: &SourceOrigin| {
        json!({
            "sourceId": origin.source_id(),
            "start": origin.span().start(),
            "end": origin.span().end(),
            "startPosition": {"line": 3, "column": 5},
            "endPosition": {"line": 3, "column": 31},
        })
    };
    let projected = event.json(&span).expect("a JSON projection");
    let stdout = seed("@std.io.StdoutText", "@interface");
    let result = one_line(&example_event(1));
    let result = &result[result.find("result: ").expect("a result") + "result: ".len()
        ..result.len() - 1];
    assert_eq!(
        projected,
        json!({
            "kind": "@host",
            "invocation": 2,
            "parent": 1,
            "phase": "@entry",
            "op": "io.stdoutText",
            "driver": {"provider": "@host.io.stdout", "version": "0.0.2"},
            "origin": {
                "sourceId": "src/hello/main.vib", "start": 42, "end": 68,
                "startPosition": {"line": 3, "column": 5},
                "endPosition": {"line": 3, "column": 31},
            },
            "grant": {
                "issuedBy": {"kind": "@binding", "leaf": "@out"},
                "policy": "",
                "rights": [{"path": "@std.io.StdoutText", "seed": stdout}],
                "limits": null,
                "root": 1,
            },
            "args": ["\"hello\\n\""],
            "result": result,
        })
    );
}

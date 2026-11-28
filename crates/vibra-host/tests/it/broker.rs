//! The broker: the grant table, narrowing, the counters, and the five steps
//! of `Context::host`, over the scripted driver (M2 step 15).
//!
//! `root(I)` issues a root grant of interface `I` with the policy `P0`: a
//! file rule `prefix: [] depth: @tree actions: [@read @replace @create]
//! names: @anyName`. A call that completes returns a receipt, which these
//! tests commit; "no receipt" means no event is due.

use std::time::{SystemTime, UNIX_EPOCH};

use vibra_host::SplitMix64;
use vibra_host::broker::{Context, RootGrant};
use vibra_host::driver::{Completion, Measured, Progress, ResourceId};
use vibra_host::policy::Policy;
use vibra_host::recipe::{LeafName, Limits};
use vibra_host::trace::GrantOrigin;
use vibra_ir::Value;
use vibra_ir::authority::{
    GrantLimits, GrantToken, HostArgument, HostCompletion, HostContext, HostErrorCase,
    HostFault, HostOk, HostReply, HostRequest, NarrowRequest, Narrowed, ScopeErrorCase,
};
use vibra_ir::external::host::{HostInterface, HostOp};

use crate::scripted::{Event, Scripted, Step, Tweak};
use crate::support::{
    ANY_NAME, VOID, counters, err, granted, names, on_small_stack, policy, text,
    variant,
};
use crate::trace::header;

const NOTES: ResourceId = ResourceId(1);
const OTHER: ResourceId = ResourceId(2);

fn file_rule(prefix: &str, actions: &str, test: &str) -> Policy {
    policy(&format!(
        "{{kind: @fileRule root: @notes prefix: [{prefix}] depth: @tree actions: [{actions}] \
         names: {test}}}"
    ))
}

fn p0() -> Policy {
    file_rule("", "@read @replace @create", ANY_NAME)
}

fn grant(
    interface: HostInterface,
    policy: Option<Policy>,
    limits: Limits,
) -> RootGrant {
    RootGrant {
        interface,
        resource: NOTES,
        origin: GrantOrigin::Binding {
            leaf: LeafName::new("@notes"),
        },
        policy,
        variables: None,
        limits,
    }
}

fn root(cx: &mut Context<'_>, interface: HostInterface) -> GrantToken {
    cx.root_for_tests(grant(interface, Some(p0()), Limits::default()))
        .expect("a root")
}

fn bounded(
    cx: &mut Context<'_>,
    interface: HostInterface,
    limits: Limits,
) -> GrantToken {
    cx.root_for_tests(grant(interface, Some(p0()), limits))
        .expect("a root")
}

fn with_policy(
    cx: &mut Context<'_>,
    interface: HostInterface,
    policy: Policy,
) -> GrantToken {
    cx.root_for_tests(grant(interface, Some(policy), Limits::default()))
        .expect("a root")
}

fn limits(invocations: Option<u64>, input: Option<u64>, output: Option<u64>) -> Limits {
    Limits {
        invocations,
        input_bytes: input,
        output_bytes: output,
    }
}

fn request<'v>(
    op: HostOp,
    grant: GrantToken,
    arguments: &'v [HostArgument<'v>],
    room: u64,
) -> HostRequest<'v> {
    HostRequest {
        op,
        grant,
        arguments,
        invocation: 1,
        parent: None,
        origin: None,
        room,
    }
}

fn call(
    cx: &mut Context<'_>,
    op: HostOp,
    grant: GrantToken,
    arguments: &[HostArgument<'_>],
) -> Result<HostCompletion, HostFault> {
    cx.host(request(op, grant, arguments, u64::MAX))
}

/// Calls and commits: the reply, and whether an event was due.
fn run(
    cx: &mut Context<'_>,
    op: HostOp,
    grant: GrantToken,
    arguments: &[HostArgument<'_>],
) -> Result<(HostReply, bool), HostFault> {
    let completion = call(cx, op, grant, arguments)?;
    let due = completion.receipt.is_some();
    if let Some(receipt) = completion.receipt {
        cx.commit(receipt).expect("the pending receipt");
    }
    Ok((completion.reply, due))
}

fn read(
    cx: &mut Context<'_>,
    grant: GrantToken,
    at: &[&str],
) -> Result<(HostReply, bool), HostFault> {
    let at = names(at);
    run(cx, HostOp::FsReadText, grant, &[HostArgument::Path(&at)])
}

fn write(
    cx: &mut Context<'_>,
    grant: GrantToken,
    at: &[&str],
    contents: &str,
) -> Result<(HostReply, bool), HostFault> {
    let at = names(at);
    run(
        cx,
        HostOp::FsWriteText,
        grant,
        &[HostArgument::Path(&at), HostArgument::Text(contents)],
    )
}

fn stdin(
    cx: &mut Context<'_>,
    op: HostOp,
    grant: GrantToken,
    max: u64,
) -> Result<(HostReply, bool), HostFault> {
    run(cx, op, grant, &[HostArgument::Count(max)])
}

fn bytes(value: &str) -> HostReply {
    HostReply::Ok(HostOk::Bytes(value.as_bytes().to_vec()))
}

fn completed(reply: HostReply, input: Measured, output: Measured) -> Completion {
    Completion::Completed {
        reply,
        progress: Progress { input, output },
    }
}

/// Invocations, input bytes, and output bytes charged.
fn used(cx: &Context<'_>, grant: GrantToken) -> [u64; 3] {
    let counters = counters(cx, grant);
    [
        counters.invocations().used(),
        counters.input_bytes().used(),
        counters.output_bytes().used(),
    ]
}

fn invalid(result: Result<(HostReply, bool), HostFault>) -> bool {
    matches!(result, Err(HostFault::InvalidHostValue(_)))
}

// Source policy values, as the interpreter encodes an `FsPolicy`.

fn unit(case: &str) -> Value {
    variant(case, Vec::new())
}

fn relative(at: &[&str]) -> Value {
    Value::Wrapper(Box::new(Value::List(
        at.iter()
            .map(|name| Value::Str((*name).to_owned()))
            .collect(),
    )))
}

fn files(prefix: &[&str], actions: &[&str], test: Value) -> Value {
    variant(
        "files",
        vec![
            ("prefix", relative(prefix)),
            ("depth", unit("tree")),
            (
                "actions",
                Value::List(actions.iter().map(|action| unit(action)).collect()),
            ),
            ("names", test),
        ],
    )
}

fn suffix(value: &str) -> Value {
    variant("suffix", vec![("value", Value::Str(value.to_owned()))])
}

fn restrict(cx: &mut Context<'_>, grant: GrantToken, policy: &Value) -> Narrowed {
    cx.narrow(NarrowRequest::Restrict { grant, policy })
        .expect("a valid restriction")
}

fn sub(cx: &mut Context<'_>, grant: GrantToken, prefix: &[&str]) -> Narrowed {
    let prefix = names(prefix);
    cx.narrow(NarrowRequest::Sub {
        grant,
        prefix: &prefix,
    })
    .expect("a valid sub")
}

fn limit(cx: &mut Context<'_>, grant: GrantToken, limits: GrantLimits) -> GrantToken {
    granted(
        cx.narrow(NarrowRequest::Limit { grant, limits })
            .expect("a valid limit"),
    )
}

fn invocations(bound: Option<u64>) -> GrantLimits {
    GrantLimits {
        invocations: bound,
        ..GrantLimits::default()
    }
}

#[test]
fn a_permitted_read_completes_with_a_receipt_and_charges_its_size() {
    let script = Scripted::new();
    script.file(&["a.txt"], "hello");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let notes = root(&mut cx, HostInterface::DirReadText);
    assert_eq!(read(&mut cx, notes, &["a.txt"]), Ok((text("hello"), true)));
    assert_eq!(script.calls().len(), 1);
    assert_eq!(used(&cx, notes), [1, 5, 0]);
    assert_eq!(counters(&cx, notes).input_bytes().reserved(), 0);
}

/// F3 of the drivers review: an error the driver decided below directories
/// whose real names the policy does not permit is a completed `denied`; below
/// a permitted one it is the error. The driver asks once.
#[test]
fn an_error_below_an_unpermitted_directory_is_denied() {
    let script = Scripted::new();
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let notes = with_policy(
        &mut cx,
        HostInterface::DirReadText,
        file_rule("\"pub\"", "@read", ANY_NAME),
    );
    for (real, expected, admitted) in [
        ("PUB", err(HostOp::FsReadText, "denied"), false),
        ("pub", err(HostOp::FsReadText, "notFound"), true),
    ] {
        script.clear();
        script.tweak(Tweak {
            failed_below: Some(names(&[real])),
            ..Tweak::default()
        });
        assert_eq!(
            read(&mut cx, notes, &["pub", "y.txt"]),
            Ok((expected, true)),
            "{real}"
        );
        assert!(
            script.state().log.contains(&Event::GateError {
                names: names(&[real, "y.txt"]),
                admitted,
            }),
            "{real}"
        );
    }
    assert_eq!(used(&cx, notes), [2, 0, 0]);
    script.tweak(Tweak {
        failed_below: Some(names(&["pub"])),
        admit_after: true,
        ..Tweak::default()
    });
    assert!(
        invalid(read(&mut cx, notes, &["pub", "y.txt"])),
        "a driver that asks its gate twice"
    );
}

#[test]
fn atk_forged_token() {
    let script = Scripted::new();
    script.file(&["a.txt"], "hello");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let notes = root(&mut cx, HostInterface::DirReadText);
    let forged = GrantToken::new(notes.context(), 99, notes.generation());
    assert!(invalid(read(&mut cx, forged, &["a.txt"])));
    assert!(script.calls().is_empty());
}

#[test]
fn atk_foreign_grant() {
    let script = Scripted::new();
    script.file(&["a.txt"], "hello");
    let mut first = script.clone();
    let mut second = script.clone();
    let mut here = Context::new(&mut first, header());
    let mut there = Context::new(&mut second, header());
    let _ = root(&mut here, HostInterface::DirReadText);
    let foreign = root(&mut there, HostInterface::DirReadText);
    assert!(invalid(read(&mut here, foreign, &["a.txt"])));
    assert!(script.calls().is_empty());
}

#[test]
fn atk_stale_grant() {
    let script = Scripted::new();
    script.file(&["a.txt"], "hello");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let notes = root(&mut cx, HostInterface::DirReadText);
    assert_eq!(cx.release_for_tests(NOTES), Ok(()));
    assert!(invalid(read(&mut cx, notes, &["a.txt"])));
    assert!(script.calls().is_empty());
}

#[test]
fn atk_stale_derived_grant() {
    let script = Scripted::new();
    script.file(&["d", "a.txt"], "hello");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let notes = root(&mut cx, HostInterface::DirReadText);
    let restricted = granted(restrict(
        &mut cx,
        notes,
        &files(&[], &["read"], unit("anyName")),
    ));
    let below = granted(sub(&mut cx, restricted, &["d"]));
    let kept = limit(&mut cx, below, invocations(Some(5)));
    assert_eq!(read(&mut cx, kept, &["a.txt"]), Ok((text("hello"), true)));
    script.clear();
    let before = cx.slot_for_tests(kept);
    assert_eq!(cx.release_for_tests(NOTES), Ok(()));
    assert!(invalid(read(&mut cx, kept, &["a.txt"])));
    let narrowed = cx.narrow(NarrowRequest::Limit {
        grant: kept,
        limits: invocations(Some(1)),
    });
    assert!(matches!(narrowed, Err(HostFault::InvalidHostValue(_))));
    assert!(script.calls().is_empty());
    assert_eq!(cx.slot_for_tests(kept), before, "release wrote no slot");
}

#[test]
fn wrong_operation_for_the_grant() {
    let script = Scripted::new();
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let notes = root(&mut cx, HostInterface::DirReadText);
    assert!(invalid(write(&mut cx, notes, &["a.txt"], "x")));
    assert!(script.calls().is_empty());
}

#[test]
fn denied_before_the_adapter() {
    let script = Scripted::new();
    script.file(&["out", "a.txt"], "hello");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let inside = with_policy(
        &mut cx,
        HostInterface::DirReadText,
        file_rule("\"in\"", "@read", ANY_NAME),
    );
    assert_eq!(
        read(&mut cx, inside, &["out", "a.txt"]),
        Ok((err(HostOp::FsReadText, "denied"), false))
    );
    assert!(script.calls().is_empty());
    assert_eq!(used(&cx, inside), [0, 0, 0]);
}

#[test]
fn atk_policy_ancestor_dropped() {
    let script = Scripted::new();
    script.file(&["a.bin"], "binary");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let text_only = with_policy(
        &mut cx,
        HostInterface::DirReadText,
        file_rule("", "@read", "{kind: @suffix value: \".txt\"}"),
    );
    let child = granted(restrict(
        &mut cx,
        text_only,
        &files(&[], &["read"], unit("anyName")),
    ));
    assert_eq!(
        read(&mut cx, child, &["a.bin"]),
        Ok((err(HostOp::FsReadText, "denied"), false))
    );
    assert!(script.calls().is_empty());
}

#[test]
fn atk_narrow_broader_policy() {
    let script = Scripted::new();
    script.file(&["out", "a"], "outside");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let inside = with_policy(
        &mut cx,
        HostInterface::DirReadText,
        file_rule("\"in\"", "@read", ANY_NAME),
    );
    let broader = granted(restrict(
        &mut cx,
        inside,
        &files(&[], &["read"], unit("anyName")),
    ));
    assert_eq!(
        read(&mut cx, broader, &["out", "a"]),
        Ok((err(HostOp::FsReadText, "denied"), false))
    );
    assert!(script.calls().is_empty());
}

#[test]
fn atk_narrow_other_resource() {
    let script = Scripted::new();
    script.file(&["a.txt"], "hello");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let first = root(&mut cx, HostInterface::DirReadText);
    let second = cx
        .root_for_tests(RootGrant {
            resource: OTHER,
            ..grant(HostInterface::DirReadText, Some(p0()), Limits::default())
        })
        .expect("a root");
    let child = granted(restrict(
        &mut cx,
        first,
        &files(&[], &["read"], unit("anyName")),
    ));
    for _ in 0..3 {
        assert_eq!(read(&mut cx, child, &["a.txt"]), Ok((text("hello"), true)));
        assert_eq!(read(&mut cx, second, &["a.txt"]), Ok((text("hello"), true)));
    }
    let resources = script
        .calls()
        .into_iter()
        .map(|event| match event {
            Event::Call { resource, .. } => resource,
            _ => unreachable!(),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        resources,
        [NOTES, OTHER, NOTES, OTHER, NOTES, OTHER],
        "every call through the child names the first root's resource"
    );
}

#[test]
fn the_parent_stays_valid() {
    let script = Scripted::new();
    script.file(&["b.bin"], "B");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let notes = root(&mut cx, HostInterface::DirReadText);
    let _ = granted(restrict(
        &mut cx,
        notes,
        &files(&[], &["read"], suffix(".txt")),
    ));
    assert_eq!(read(&mut cx, notes, &["b.bin"]), Ok((text("B"), true)));
}

#[test]
fn sub_rebases() {
    let script = Scripted::new();
    script.file(&["reports", "a.txt"], "report");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let notes = root(&mut cx, HostInterface::DirReadText);
    let reports = granted(sub(&mut cx, notes, &["reports"]));
    assert_eq!(
        read(&mut cx, reports, &["a.txt"]),
        Ok((text("report"), true))
    );
    assert!(matches!(
        script.calls().as_slice(),
        [Event::Call { names: asked, .. }] if *asked == names(&["reports", "a.txt"])
    ));
}

#[test]
fn sub_of_a_missing_directory() {
    let script = Scripted::new();
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let notes = root(&mut cx, HostInterface::DirReadText);
    assert!(matches!(sub(&mut cx, notes, &["nope"]), Narrowed::Grant(_)));
    assert!(script.state().log.is_empty());
}

#[test]
fn restrict_empty_suffix_is_an_invalid_policy() {
    let script = Scripted::new();
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let notes = root(&mut cx, HostInterface::DirReadText);
    assert_eq!(
        restrict(&mut cx, notes, &files(&[], &["read"], suffix(""))),
        Narrowed::Refused(ScopeErrorCase::InvalidPolicy)
    );
}

/// Restricts `notes` by the same `.txt` policy until the effective policy is
/// at depth 64: `@allOf [P0 R]` has depth 3, plus one for each `restrict`,
/// so 61 are accepted (Q9).
fn at_depth_64(cx: &mut Context<'_>, notes: GrantToken) -> GrantToken {
    let policy = files(&[], &["read"], suffix(".txt"));
    let mut grant = notes;
    for _ in 0..61 {
        grant = granted(restrict(cx, grant, &policy));
    }
    grant
}

#[test]
fn restrict_too_complex() {
    let script = Scripted::new();
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let notes = root(&mut cx, HostInterface::DirReadText);
    let deep = at_depth_64(&mut cx, notes);
    assert_eq!(
        restrict(&mut cx, deep, &files(&[], &["read"], suffix(".txt"))),
        Narrowed::Refused(ScopeErrorCase::PolicyTooComplex)
    );
    // `limit` adds nothing to the depth.
    let _ = limit(&mut cx, deep, invocations(None));
}

#[test]
fn sub_too_complex() {
    let script = Scripted::new();
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let notes = root(&mut cx, HostInterface::DirReadText);
    let deep = at_depth_64(&mut cx, notes);
    assert_eq!(
        sub(&mut cx, deep, &["d"]),
        Narrowed::Refused(ScopeErrorCase::PolicyTooComplex)
    );
}

#[test]
fn limit_keeps_the_smaller() {
    let script = Scripted::new();
    script.file(&["a.txt"], "a");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let notes = root(&mut cx, HostInterface::DirReadText);
    let five = limit(&mut cx, notes, invocations(Some(5)));
    let nine = limit(&mut cx, five, invocations(Some(9)));
    for _ in 0..5 {
        assert_eq!(read(&mut cx, nine, &["a.txt"]), Ok((text("a"), true)));
    }
    assert_eq!(
        read(&mut cx, nine, &["a.txt"]),
        Ok((err(HostOp::FsReadText, "limitExceeded"), false))
    );
}

#[test]
fn limit_with_none_leaves_the_bound() {
    let script = Scripted::new();
    script.file(&["a.txt"], "a");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let notes = bounded(
        &mut cx,
        HostInterface::DirReadText,
        limits(Some(2), None, None),
    );
    let child = limit(&mut cx, notes, invocations(None));
    assert_eq!(read(&mut cx, child, &["a.txt"]), Ok((text("a"), true)));
    assert_eq!(read(&mut cx, child, &["a.txt"]), Ok((text("a"), true)));
    assert_eq!(
        read(&mut cx, child, &["a.txt"]),
        Ok((err(HostOp::FsReadText, "limitExceeded"), false))
    );
}

#[test]
fn siblings_share_the_parent() {
    let script = Scripted::new();
    script.file(&["a.txt"], "a");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let parent = bounded(
        &mut cx,
        HostInterface::DirReadText,
        limits(Some(3), None, None),
    );
    let first = limit(&mut cx, parent, invocations(None));
    let second = limit(&mut cx, parent, invocations(None));
    assert_eq!(read(&mut cx, first, &["a.txt"]), Ok((text("a"), true)));
    assert_eq!(read(&mut cx, second, &["a.txt"]), Ok((text("a"), true)));
    assert_eq!(read(&mut cx, first, &["a.txt"]), Ok((text("a"), true)));
    assert_eq!(
        read(&mut cx, second, &["a.txt"]),
        Ok((err(HostOp::FsReadText, "limitExceeded"), false))
    );
}

#[test]
fn a_childs_own_limit() {
    let script = Scripted::new();
    script.file(&["a.txt"], "a");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let parent = root(&mut cx, HostInterface::DirReadText);
    let child = limit(&mut cx, parent, invocations(Some(1)));
    assert_eq!(read(&mut cx, child, &["a.txt"]), Ok((text("a"), true)));
    assert_eq!(
        read(&mut cx, child, &["a.txt"]),
        Ok((err(HostOp::FsReadText, "limitExceeded"), false))
    );
    assert_eq!(read(&mut cx, parent, &["a.txt"]), Ok((text("a"), true)));
}

#[test]
fn limit_exceeded_dispatches_nothing() {
    let script = Scripted::new();
    script.file(&["a.txt"], "a");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let notes = bounded(
        &mut cx,
        HostInterface::DirReadText,
        limits(Some(1), None, None),
    );
    assert_eq!(read(&mut cx, notes, &["a.txt"]), Ok((text("a"), true)));
    script.clear();
    assert_eq!(
        read(&mut cx, notes, &["a.txt"]),
        Ok((err(HostOp::FsReadText, "limitExceeded"), false))
    );
    assert!(script.calls().is_empty());
}

#[test]
fn read_over_the_byte_limit() {
    let script = Scripted::new();
    script.file(&["a.txt"], "hello");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let notes = bounded(
        &mut cx,
        HostInterface::DirReadText,
        limits(None, Some(4), None),
    );
    assert_eq!(
        read(&mut cx, notes, &["a.txt"]),
        Ok((err(HostOp::FsReadText, "limitExceeded"), false))
    );
    assert_eq!(script.last_gate(), Some(None));
    assert_eq!(script.last_returned(), Some("refused"));
    assert!(!script.stepped(Step::Read));
    assert_eq!(used(&cx, notes), [0, 0, 0]);
}

#[test]
fn atk_counter_size_refusal_holds_no_invocation() {
    let script = Scripted::new();
    script.file(&["five.txt"], "fives");
    script.file(&["four.txt"], "four");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let notes = bounded(
        &mut cx,
        HostInterface::DirReadText,
        limits(Some(1), Some(4), None),
    );
    assert_eq!(
        read(&mut cx, notes, &["five.txt"]),
        Ok((err(HostOp::FsReadText, "limitExceeded"), false))
    );
    assert_eq!(
        read(&mut cx, notes, &["four.txt"]),
        Ok((text("four"), true))
    );
}

#[test]
fn result_above_the_room() {
    let script = Scripted::new();
    script.file(&["a.txt"], "hello");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let notes = root(&mut cx, HostInterface::DirReadText);
    let at = names(&["a.txt"]);
    let arguments = [HostArgument::Path(&at)];
    assert_eq!(
        cx.host(request(HostOp::FsReadText, notes, &arguments, 4)),
        Err(HostFault::MemoryExhausted)
    );
    assert!(!script.stepped(Step::Read));
}

#[test]
fn read_at_the_byte_limit() {
    let script = Scripted::new();
    script.file(&["a.txt"], "hello");
    script.file(&["b.txt"], "b");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let notes = bounded(
        &mut cx,
        HostInterface::DirReadText,
        limits(None, Some(5), None),
    );
    assert_eq!(read(&mut cx, notes, &["a.txt"]), Ok((text("hello"), true)));
    assert_eq!(
        read(&mut cx, notes, &["b.txt"]),
        Ok((err(HostOp::FsReadText, "limitExceeded"), false))
    );
}

#[test]
fn write_reserves_its_size() {
    let script = Scripted::new();
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let out = bounded(
        &mut cx,
        HostInterface::DirWriteText,
        limits(None, None, Some(3)),
    );
    assert_eq!(
        write(&mut cx, out, &["a.txt"], "hello"),
        Ok((err(HostOp::FsWriteText, "limitExceeded"), false))
    );
    assert!(script.calls().is_empty());
}

#[test]
fn metadata_counts_an_invocation_only() {
    let script = Scripted::new();
    script.file(&["a.txt"], "hello");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let stat = cx
        .root_for_tests(grant(
            HostInterface::DirStat,
            Some(file_rule("", "@stat", ANY_NAME)),
            limits(None, Some(0), None),
        ))
        .expect("a root");
    let at = names(&["a.txt"]);
    let (reply, due) = run(
        &mut cx,
        HostOp::FsMetadataStat,
        stat,
        &[HostArgument::Path(&at)],
    )
    .expect("a call");
    assert!(due);
    assert!(matches!(
        reply,
        HostReply::Ok(HostOk::Metadata { size: 5, .. })
    ));
    assert_eq!(used(&cx, stat), [1, 0, 0]);
}

#[test]
fn atk_counter_overflow() {
    let script = Scripted::new();
    script.file(&["a.txt"], "hello");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let notes = root(&mut cx, HostInterface::DirReadText);
    let near = u64::MAX - 1;
    cx.preset_for_tests(notes, [near, near, near]);
    let before = counters(&cx, notes);
    assert_eq!(
        read(&mut cx, notes, &["a.txt"]),
        Ok((err(HostOp::FsReadText, "denied"), false))
    );
    assert_eq!(counters(&cx, notes), before);
}

#[test]
fn atk_counter_refund_unknown() {
    let script = Scripted::new();
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let out = bounded(
        &mut cx,
        HostInterface::DirWriteText,
        limits(None, None, Some(10)),
    );
    script.tweak(Tweak {
        completion: Some(completed(
            err(HostOp::FsWriteText, "outcomeUnknown"),
            Measured::Known(0),
            Measured::Known(0),
        )),
        ..Tweak::default()
    });
    assert_eq!(
        write(&mut cx, out, &["a.txt"], "sixsix"),
        Ok((err(HostOp::FsWriteText, "outcomeUnknown"), true))
    );
    assert_eq!(
        write(&mut cx, out, &["b.txt"], "sixsix"),
        Ok((err(HostOp::FsWriteText, "limitExceeded"), false))
    );
}

/// Row `ATK-driver-completion-class` (F1 of the review of grants, and the
/// ruling on completion classes for step 30): the classes partition the
/// cases, over every case of every host error type. A refusal (`denied`,
/// `limitExceeded`, `unsupported`) may be known not submitted; `denied` may
/// also be completed (the adapter's gate); a case of the operation's own
/// error type is always completed, since the driver observed the world;
/// `outcomeUnknown` is never known not submitted (06 "Host calls"), and
/// `limitExceeded` and `unsupported` are never completed (06 "Audit
/// trace"). Every other pairing is `@runtime.invalidHostValue` with no
/// event, and the whole reservation stays charged: what the driver did may
/// have happened.
#[test]
fn atk_driver_completion_class() {
    use vibra_ir::external::host::HostErrorType;
    // One operation of each error type, its interface, its operands, and
    // what the call reserves, at step 3 and at the gate (the two bytes of the
    // variable `HOME`): invocations, input, output.
    let operations: [(HostErrorType, HostOp, HostInterface, [u64; 3]); 5] = [
        (
            HostErrorType::Io,
            HostOp::IoStdoutBytes,
            HostInterface::StdoutBytes,
            [1, 0, 6],
        ),
        (
            HostErrorType::Fs,
            HostOp::FsWriteText,
            HostInterface::DirWriteText,
            [1, 0, 6],
        ),
        (
            HostErrorType::Env,
            HostOp::EnvReadVar,
            HostInterface::EnvReadVar,
            [1, 2, 0],
        ),
        (
            HostErrorType::Time,
            HostOp::TimeNowWall,
            HostInterface::WallClock,
            [1, 0, 0],
        ),
        (
            HostErrorType::Random,
            HostOp::RandomGenerateBytes,
            HostInterface::RandomBytes,
            [1, 6, 0],
        ),
    ];
    assert_eq!(operations.len(), HostErrorType::ALL.len());
    let at = names(&["a.txt"]);
    let mut pairings = 0;
    for (error, op, interface, reserved) in operations {
        assert_eq!(op.row().error, error);
        let arguments = match op {
            HostOp::IoStdoutBytes => vec![HostArgument::Bytes(b"sixsix")],
            HostOp::FsWriteText => {
                vec![HostArgument::Path(&at), HostArgument::Text("sixsix")]
            }
            HostOp::EnvReadVar => vec![HostArgument::Text("HOME")],
            HostOp::RandomGenerateBytes => vec![HostArgument::Count(6)],
            _ => Vec::new(),
        };
        for name in error.cases() {
            let case = HostErrorCase::new(error, name).expect("a case");
            let refusal = matches!(name, "denied" | "limitExceeded" | "unsupported");
            for submitted in [false, true] {
                pairings += 1;
                let (completion, allowed) = if submitted {
                    (
                        completed(
                            HostReply::Err(case),
                            Measured::Known(0),
                            Measured::Known(0),
                        ),
                        !matches!(name, "limitExceeded" | "unsupported"),
                    )
                } else {
                    (Completion::NotSubmitted(case), refusal)
                };
                let pairing = format!(
                    "{} {name}, {}",
                    error.name(),
                    if submitted {
                        "completed"
                    } else {
                        "not submitted"
                    }
                );
                let script = Scripted::new();
                script
                    .state()
                    .env
                    .insert("HOME".to_owned(), "/h".to_owned());
                let mut driver = script.clone();
                let mut cx = Context::new(&mut driver, header());
                let policy = (interface == HostInterface::DirWriteText).then(p0);
                let grant = cx
                    .root_for_tests(grant(
                        interface,
                        policy,
                        limits(None, Some(10), Some(10)),
                    ))
                    .expect("a root");
                script.tweak(Tweak {
                    completion: Some(completion),
                    ..Tweak::default()
                });
                let result = run(&mut cx, op, grant, &arguments);
                if allowed {
                    let charged = if name == "outcomeUnknown" {
                        reserved
                    } else {
                        [1, 0, 0]
                    };
                    assert_eq!(
                        result,
                        Ok((HostReply::Err(case), submitted)),
                        "{pairing}"
                    );
                    assert_eq!(used(&cx, grant), charged, "{pairing}: the charge");
                    assert_eq!(
                        cx.trace().events.len(),
                        usize::from(submitted),
                        "{pairing}: the events"
                    );
                } else {
                    assert!(invalid(result), "{pairing}: a driver fault");
                    assert_eq!(
                        used(&cx, grant),
                        reserved,
                        "{pairing}: the whole charge"
                    );
                    assert!(cx.trace().events.is_empty(), "{pairing}: no event");
                }
                let counters = counters(&cx, grant);
                assert_eq!(
                    [
                        counters.input_bytes().reserved(),
                        counters.output_bytes().reserved()
                    ],
                    [0, 0],
                    "{pairing}: nothing stays reserved"
                );
            }
        }
    }
    // Two classes of the 4 shared cases and the 3, 7, 2, 1, and 1 cases of
    // the five types' own.
    assert_eq!(pairings, 2 * (5 * 4 + 3 + 7 + 2 + 1 + 1));
}

/// The control of `atk_driver_completion_class`: the pairings the chapters
/// allow keep their class. A refusal known not submitted (`unsupported`,
/// `limitExceeded`, `denied`) refunds the bytes and records nothing; a
/// completed `denied`, `outcomeUnknown`, or own case is one event.
#[test]
fn consistent_completions_keep_their_class() {
    let not_submitted = |name: &str| {
        Completion::NotSubmitted(
            HostErrorCase::new(HostOp::FsWriteText.row().error, name).expect("a case"),
        )
    };
    let done = |name: &str| {
        completed(
            err(HostOp::FsWriteText, name),
            Measured::Known(0),
            Measured::Known(0),
        )
    };
    let pairings = [
        (
            not_submitted("unsupported"),
            "unsupported",
            false,
            [1, 0, 0],
        ),
        (
            not_submitted("limitExceeded"),
            "limitExceeded",
            false,
            [1, 0, 0],
        ),
        (not_submitted("denied"), "denied", false, [1, 0, 0]),
        (done("denied"), "denied", true, [1, 0, 0]),
        (done("outcomeUnknown"), "outcomeUnknown", true, [1, 0, 6]),
        (
            done("permissionDenied"),
            "permissionDenied",
            true,
            [1, 0, 0],
        ),
    ];
    for (completion, name, due, charged) in pairings {
        let script = Scripted::new();
        let mut driver = script.clone();
        let mut cx = Context::new(&mut driver, header());
        let out = bounded(
            &mut cx,
            HostInterface::DirWriteText,
            limits(None, None, Some(10)),
        );
        script.tweak(Tweak {
            completion: Some(completion),
            ..Tweak::default()
        });
        assert_eq!(
            write(&mut cx, out, &["a.txt"], "sixsix"),
            Ok((err(HostOp::FsWriteText, name), due)),
            "{name}"
        );
        assert_eq!(used(&cx, out), charged, "{name}");
        assert_eq!(cx.trace().events.len(), usize::from(due), "{name}");
    }
}

#[test]
fn atk_counter_partial_known() {
    let script = Scripted::new();
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let out = bounded(
        &mut cx,
        HostInterface::DirWriteText,
        limits(None, None, Some(10)),
    );
    script.tweak(Tweak {
        completion: Some(completed(
            err(HostOp::FsWriteText, "other"),
            Measured::Known(0),
            Measured::Known(2),
        )),
        ..Tweak::default()
    });
    assert_eq!(
        write(&mut cx, out, &["a.txt"], "sixsix"),
        Ok((err(HostOp::FsWriteText, "other"), true))
    );
    assert_eq!(
        write(&mut cx, out, &["b.txt"], "eighteig"),
        Ok((VOID, true))
    );
    assert_eq!(
        write(&mut cx, out, &["c.txt"], "1"),
        Ok((err(HostOp::FsWriteText, "limitExceeded"), false))
    );
}

#[test]
fn atk_counter_partial_unknown() {
    let script = Scripted::new();
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let out = bounded(
        &mut cx,
        HostInterface::DirWriteText,
        limits(None, None, Some(10)),
    );
    script.tweak(Tweak {
        completion: Some(completed(
            err(HostOp::FsWriteText, "other"),
            Measured::Known(0),
            Measured::Uncertain,
        )),
        ..Tweak::default()
    });
    assert_eq!(
        write(&mut cx, out, &["a.txt"], "sixsix"),
        Ok((err(HostOp::FsWriteText, "other"), true))
    );
    assert_eq!(
        write(&mut cx, out, &["b.txt"], "fivef"),
        Ok((err(HostOp::FsWriteText, "limitExceeded"), false))
    );
    assert_eq!(write(&mut cx, out, &["c.txt"], "four"), Ok((VOID, true)));
}

#[test]
fn settlement_reaches_every_ancestor() {
    let script = Scripted::new();
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let parent = bounded(
        &mut cx,
        HostInterface::DirWriteText,
        limits(None, None, Some(10)),
    );
    let child = limit(
        &mut cx,
        parent,
        GrantLimits {
            output_bytes: Some(8),
            ..GrantLimits::default()
        },
    );
    script.tweak(Tweak {
        completion: Some(completed(
            err(HostOp::FsWriteText, "other"),
            Measured::Known(0),
            Measured::Known(2),
        )),
        ..Tweak::default()
    });
    assert_eq!(
        write(&mut cx, child, &["a.txt"], "sixsix"),
        Ok((err(HostOp::FsWriteText, "other"), true))
    );
    assert_eq!(used(&cx, child)[2], 2);
    assert_eq!(used(&cx, parent)[2], 2);
    assert_eq!(
        write(&mut cx, parent, &["b.txt"], "eighteig"),
        Ok((VOID, true))
    );
}

#[test]
fn progress_above_the_reservation() {
    let script = Scripted::new();
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let out = root(&mut cx, HostInterface::DirWriteText);
    script.tweak(Tweak {
        completion: Some(completed(VOID, Measured::Known(0), Measured::Known(7))),
        ..Tweak::default()
    });
    assert!(invalid(write(&mut cx, out, &["a.txt"], "sixsix")));
    assert_eq!(used(&cx, out), [1, 0, 6]);
    assert_eq!(counters(&cx, out).output_bytes().reserved(), 0);
}

#[test]
fn atk_counter_invalid_text_charged() {
    let script = Scripted::new();
    script.file(&["bad.txt"], "\u{7f}bad!");
    script.file(&["b.txt"], "b");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let notes = bounded(
        &mut cx,
        HostInterface::DirReadText,
        limits(None, Some(5), None),
    );
    script.tweak(Tweak {
        completion: Some(completed(
            err(HostOp::FsReadText, "invalidText"),
            Measured::Known(5),
            Measured::Known(0),
        )),
        ..Tweak::default()
    });
    assert_eq!(
        read(&mut cx, notes, &["bad.txt"]),
        Ok((err(HostOp::FsReadText, "invalidText"), true))
    );
    assert_eq!(
        read(&mut cx, notes, &["b.txt"]),
        Ok((err(HostOp::FsReadText, "limitExceeded"), false))
    );
}

#[test]
fn atk_counter_reservation_all_or_none() {
    let script = Scripted::new();
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let out = bounded(
        &mut cx,
        HostInterface::DirWriteText,
        limits(Some(2), None, Some(3)),
    );
    assert_eq!(
        write(&mut cx, out, &["a.txt"], "hello"),
        Ok((err(HostOp::FsWriteText, "limitExceeded"), false))
    );
    assert_eq!(write(&mut cx, out, &["b.txt"], "ab"), Ok((VOID, true)));
    assert_eq!(write(&mut cx, out, &["c.txt"], "c"), Ok((VOID, true)));
}

#[test]
fn refund_when_known_unused() {
    let script = Scripted::new();
    script.file(&["a.txt"], "hello");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let notes = bounded(
        &mut cx,
        HostInterface::DirReadText,
        limits(None, Some(5), None),
    );
    assert_eq!(
        read(&mut cx, notes, &["missing.txt"]),
        Ok((err(HostOp::FsReadText, "notFound"), true))
    );
    assert_eq!(script.last_gate(), None, "the size was never learned");
    assert_eq!(read(&mut cx, notes, &["a.txt"]), Ok((text("hello"), true)));
}

fn text_only(cx: &mut Context<'_>) -> GrantToken {
    with_policy(
        cx,
        HostInterface::DirReadText,
        file_rule("", "@read", "{kind: @suffix value: \".txt\"}"),
    )
}

#[test]
fn gate_real_name_fails() {
    let script = Scripted::new();
    script.file(&["a.txt"], "hello");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let notes = text_only(&mut cx);
    script.tweak(Tweak {
        real_names: Some(names(&["A.TXT"])),
        ..Tweak::default()
    });
    assert_eq!(
        read(&mut cx, notes, &["a.txt"]),
        Ok((err(HostOp::FsReadText, "denied"), true))
    );
    assert_eq!(script.last_returned(), Some("refused"));
    assert!(!script.stepped(Step::Read));
}

#[test]
fn atk_driver_ignores_gate() {
    let script = Scripted::new();
    script.file(&["a.txt"], "hello");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let notes = text_only(&mut cx);
    // A driver that reports the byte it read, and one that also hides it,
    // so that only the gate's refusal stands against it.
    for input in [1, 0] {
        script.tweak(Tweak {
            real_names: Some(names(&["A.TXT"])),
            ignore_gate: true,
            completion: Some(completed(
                text("x"),
                Measured::Known(input),
                Measured::Known(0),
            )),
            ..Tweak::default()
        });
        let at = names(&["a.txt"]);
        let outcome = call(
            &mut cx,
            HostOp::FsReadText,
            notes,
            &[HostArgument::Path(&at)],
        );
        assert!(matches!(outcome, Err(HostFault::InvalidHostValue(_))));
        assert_eq!(script.last_gate(), Some(None));
    }
    // No event is pending: the next call is not refused for one.
    assert_eq!(read(&mut cx, notes, &["a.txt"]), Ok((text("hello"), true)));
}

/// F6 of the drivers review, 06 "Memory": a driver that reports memory
/// exhaustion, for an allocation the system refused after its gate
/// admitted the read, ends the call as memory exhaustion: no event is due,
/// and the invocation is charged with what the adapter moved.
#[test]
fn a_driver_out_of_memory_is_memory_exhaustion() {
    let script = Scripted::new();
    script.file(&["a.txt"], "hello");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let notes = text_only(&mut cx);
    script.tweak(Tweak {
        completion: Some(Completion::MemoryExhausted(Progress::NONE)),
        ..Tweak::default()
    });
    let at = names(&["a.txt"]);
    let outcome = call(
        &mut cx,
        HostOp::FsReadText,
        notes,
        &[HostArgument::Path(&at)],
    );
    assert!(
        matches!(outcome, Err(HostFault::MemoryExhausted)),
        "{outcome:?}"
    );
    assert_eq!(script.last_returned(), Some("memoryExhausted"));
    assert_eq!(used(&cx, notes), [1, 0, 0]);
    // No event is pending: the next call is not refused for one.
    assert_eq!(read(&mut cx, notes, &["a.txt"]), Ok((text("hello"), true)));
}

#[test]
fn atk_gate_create_on_real_parent() {
    let script = Scripted::new();
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let out = with_policy(
        &mut cx,
        HostInterface::DirWriteText,
        file_rule("\"OUT\"", "@create", ANY_NAME),
    );
    script.tweak(Tweak {
        real_names: Some(names(&["out", "n.txt"])),
        ..Tweak::default()
    });
    assert_eq!(
        write(&mut cx, out, &["OUT", "n.txt"], "new"),
        Ok((err(HostOp::FsWriteText, "denied"), true))
    );
    assert!(script.state().log.iter().any(|event| matches!(
        event,
        Event::Gate { found, answer: None } if found.action == vibra_host::driver::FoundAction::Create
    )));
    assert!(!script.stepped(Step::Create));
}

#[test]
fn gate_found_action() {
    let script = Scripted::new();
    script.file(&["a.txt"], "old");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let out = with_policy(
        &mut cx,
        HostInterface::DirWriteText,
        file_rule("", "@create", ANY_NAME),
    );
    assert_eq!(
        write(&mut cx, out, &["a.txt"], "new"),
        Ok((err(HostOp::FsWriteText, "denied"), true))
    );
    assert!(!script.stepped(Step::Truncate));
}

#[test]
fn gate_a_write_either_way() {
    let script = Scripted::new();
    script.file(&["old.txt"], "old");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let out = with_policy(
        &mut cx,
        HostInterface::DirWriteText,
        file_rule("", "@create @replace", ANY_NAME),
    );
    assert_eq!(write(&mut cx, out, &["new.txt"], "new"), Ok((VOID, true)));
    assert_eq!(write(&mut cx, out, &["old.txt"], "new"), Ok((VOID, true)));
    assert!(script.stepped(Step::Create));
    assert!(script.stepped(Step::Truncate));
}

#[test]
fn write_with_neither_action() {
    let script = Scripted::new();
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let out = with_policy(
        &mut cx,
        HostInterface::DirWriteText,
        file_rule("", "@read", ANY_NAME),
    );
    assert_eq!(
        write(&mut cx, out, &["a.txt"], "new"),
        Ok((err(HostOp::FsWriteText, "denied"), false))
    );
    assert!(script.calls().is_empty());
}

#[test]
fn gate_hard_link() {
    let script = Scripted::new();
    script.file(&["a.txt"], "old");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let out = root(&mut cx, HostInterface::DirWriteText);
    script.tweak(Tweak {
        links: Some(Some(2)),
        ..Tweak::default()
    });
    assert_eq!(
        write(&mut cx, out, &["a.txt"], "new"),
        Ok((err(HostOp::FsWriteText, "denied"), true))
    );
    assert!(!script.stepped(Step::Truncate));
}

#[test]
fn atk_gate_replace_without_link_count() {
    let script = Scripted::new();
    script.file(&["a.txt"], "old");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let out = root(&mut cx, HostInterface::DirWriteText);
    script.tweak(Tweak {
        links: Some(None),
        ..Tweak::default()
    });
    assert_eq!(
        write(&mut cx, out, &["a.txt"], "new"),
        Ok((err(HostOp::FsWriteText, "unsupported"), false))
    );
    assert!(!script.stepped(Step::Truncate));
    // Unlike a size refusal, it passed reservation and holds its invocation
    // (Q29); the output bytes are refunded.
    assert_eq!(used(&cx, out), [1, 0, 0]);
}

#[test]
fn read_of_a_hard_link() {
    let script = Scripted::new();
    script.file(&["a.txt"], "linked");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let notes = root(&mut cx, HostInterface::DirReadText);
    script.tweak(Tweak {
        links: Some(Some(2)),
        ..Tweak::default()
    });
    assert_eq!(read(&mut cx, notes, &["a.txt"]), Ok((text("linked"), true)));
}

fn variables(cx: &mut Context<'_>, permitted: &[&str]) -> GrantToken {
    cx.root_for_tests(RootGrant {
        variables: Some(names(permitted)),
        ..grant(HostInterface::EnvReadVar, None, Limits::default())
    })
    .expect("a root")
}

#[test]
fn env_name_inside_the_set() {
    let script = Scripted::new();
    script
        .state()
        .env
        .insert("HOME".to_owned(), "/home/h".to_owned());
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let env = variables(&mut cx, &["HOME"]);
    assert_eq!(
        run(
            &mut cx,
            HostOp::EnvReadVar,
            env,
            &[HostArgument::Text("HOME")]
        ),
        Ok((text("/home/h"), true))
    );
    assert_eq!(used(&cx, env), [1, 7, 0]);
}

#[test]
fn env_name_outside_the_set() {
    let script = Scripted::new();
    script
        .state()
        .env
        .insert("PATH".to_owned(), "/bin".to_owned());
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let env = variables(&mut cx, &["HOME"]);
    assert_eq!(
        run(
            &mut cx,
            HostOp::EnvReadVar,
            env,
            &[HostArgument::Text("PATH")]
        ),
        Ok((err(HostOp::EnvReadVar, "denied"), false))
    );
    assert!(script.calls().is_empty());
}

fn streams(
    cx: &mut Context<'_>,
    interface: HostInterface,
    input: Option<u64>,
) -> GrantToken {
    cx.root_for_tests(grant(interface, None, limits(None, input, None)))
        .expect("a root")
}

#[test]
fn stdin_known_size_within() {
    let script = Scripted::new();
    script.stdin("abc", true);
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let input = streams(&mut cx, HostInterface::StdinBytes, Some(3));
    assert_eq!(
        stdin(&mut cx, HostOp::IoStdinBytes, input, 3),
        Ok((bytes("abc"), true))
    );
    assert_eq!(used(&cx, input), [1, 3, 0]);
}

#[test]
fn stdin_known_size_over() {
    let script = Scripted::new();
    script.stdin("abc", true);
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let input = streams(&mut cx, HostInterface::StdinBytes, Some(2));
    assert_eq!(
        stdin(&mut cx, HostOp::IoStdinBytes, input, 3),
        Ok((err(HostOp::IoStdinBytes, "limitExceeded"), false))
    );
    assert!(!script.stepped(Step::Read));
    let unbounded = streams(&mut cx, HostInterface::StdinBytes, None);
    assert_eq!(
        stdin(&mut cx, HostOp::IoStdinBytes, unbounded, 3),
        Ok((bytes("abc"), true))
    );
}

#[test]
fn stdin_known_size_below_the_count() {
    let script = Scripted::new();
    script.stdin("abc", true);
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let input = streams(&mut cx, HostInterface::StdinBytes, Some(3));
    assert_eq!(
        stdin(&mut cx, HostOp::IoStdinBytes, input, 10),
        Ok((bytes("abc"), true))
    );
    assert_eq!(script.last_gate(), Some(Some(3)));
}

#[test]
fn stdin_known_size_empty_at_zero() {
    let script = Scripted::new();
    script.stdin("", true);
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let input = streams(&mut cx, HostInterface::StdinBytes, Some(0));
    assert_eq!(
        stdin(&mut cx, HostOp::IoStdinBytes, input, 1),
        Ok((bytes(""), true))
    );
}

#[test]
fn stdin_unsized_within_the_count() {
    let script = Scripted::new();
    script.stdin("abc", false);
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let input = streams(&mut cx, HostInterface::StdinBytes, Some(10));
    assert_eq!(
        stdin(&mut cx, HostOp::IoStdinBytes, input, 10),
        Ok((bytes("abc"), true))
    );
    assert_eq!(script.last_gate(), Some(Some(10)));
    assert_eq!(used(&cx, input), [1, 3, 0]);
    assert_eq!(
        stdin(&mut cx, HostOp::IoStdinBytes, input, 7),
        Ok((bytes(""), true))
    );
    assert_eq!(script.last_gate(), Some(Some(7)));
}

#[test]
fn stdin_unsized_a_short_count() {
    let script = Scripted::new();
    script.stdin("abc", false);
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let input = streams(&mut cx, HostInterface::StdinBytes, Some(2));
    assert_eq!(
        stdin(&mut cx, HostOp::IoStdinBytes, input, 2),
        Ok((bytes("ab"), true))
    );
    assert_eq!(used(&cx, input)[1], 2);
    assert_eq!(script.state().stdin, b"c");
}

#[test]
fn stdin_unsized_a_count_above_the_bound() {
    let script = Scripted::new();
    script.stdin("abc", false);
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let input = streams(&mut cx, HostInterface::StdinBytes, Some(2));
    assert_eq!(
        stdin(&mut cx, HostOp::IoStdinBytes, input, 3),
        Ok((bytes("ab"), true))
    );
    assert_eq!(script.last_gate(), Some(Some(2)));
    assert_eq!(used(&cx, input)[1], 2);
    assert_eq!(script.state().stdin, b"c");
}

#[test]
fn stdin_unsized_nothing_remains() {
    let script = Scripted::new();
    script.stdin("abc", false);
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let input = streams(&mut cx, HostInterface::StdinBytes, Some(2));
    assert_eq!(
        stdin(&mut cx, HostOp::IoStdinBytes, input, 3),
        Ok((bytes("ab"), true))
    );
    script.clear();
    assert_eq!(
        stdin(&mut cx, HostOp::IoStdinBytes, input, 3),
        Ok((err(HostOp::IoStdinBytes, "limitExceeded"), false))
    );
    assert!(!script.stepped(Step::Read));
    let unbounded = streams(&mut cx, HostInterface::StdinBytes, None);
    assert_eq!(
        stdin(&mut cx, HostOp::IoStdinBytes, unbounded, 3),
        Ok((bytes("c"), true))
    );
}

#[test]
fn stdin_unsized_a_remaining_bound_below_the_count() {
    let script = Scripted::new();
    script.stdin("abcd", false);
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let parent = streams(&mut cx, HostInterface::StdinBytes, Some(5));
    cx.preset_for_tests(parent, [0, 3, 0]);
    let child = limit(&mut cx, parent, GrantLimits::default());
    assert_eq!(
        stdin(&mut cx, HostOp::IoStdinBytes, child, 10),
        Ok((bytes("ab"), true))
    );
    assert_eq!(script.last_gate(), Some(Some(2)));
}

#[test]
fn stdin_text_unsized_below_one_scalar() {
    let script = Scripted::new();
    script.stdin("abc", false);
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let input = streams(&mut cx, HostInterface::StdinText, Some(2));
    assert_eq!(
        stdin(&mut cx, HostOp::IoStdinText, input, 16),
        Ok((err(HostOp::IoStdinText, "limitExceeded"), false))
    );
    assert!(!script.stepped(Step::Read));
    assert_eq!(used(&cx, input), [0, 0, 0]);
}

/// F7 of the review of grants: a reserved device name in any case reaches
/// the broker as a path operand, a `sub` prefix, or a policy prefix only
/// from a value checked source cannot build, since the `RelativePath`
/// constructor refuses it (03 "Filesystem policies": a name is rejected
/// "when its stem, after each character is mapped to its Unicode simple
/// uppercase", is a device stem, as `conın$` is). The broker refuses each
/// one as a boundary value that is invalid, `@runtime.invalidHostValue`
/// (06 "Traps"), before the driver is asked, and appends no event.
#[test]
fn atk_reserved_names_in_any_case_through_the_broker() {
    let reserved = [
        "nul",
        "Nul.txt",
        "CON .log",
        "con\u{131}n$",
        "Com\u{B9}",
        "lPt3.x",
        "CONOUT$",
        "aUx.tar.gz",
    ];
    for name in reserved {
        let script = Scripted::new();
        script.file(&[name], "secret");
        let mut driver = script.clone();
        let mut cx = Context::new(&mut driver, header());
        let notes = root(&mut cx, HostInterface::DirReadText);
        assert!(invalid(read(&mut cx, notes, &[name])), "{name}: a read");
        assert!(
            invalid(read(&mut cx, notes, &["sub", name, "a.txt"])),
            "{name}: a read below it"
        );
        let prefix = names(&[name]);
        assert!(
            matches!(
                cx.narrow(NarrowRequest::Sub {
                    grant: notes,
                    prefix: &prefix,
                }),
                Err(HostFault::InvalidHostValue(_))
            ),
            "{name}: a sub"
        );
        let policy = files(&[name], &["read"], unit("anyName"));
        assert!(
            matches!(
                cx.narrow(NarrowRequest::Restrict {
                    grant: notes,
                    policy: &policy,
                }),
                Err(HostFault::InvalidHostValue(_))
            ),
            "{name}: a restrict"
        );
        assert!(script.calls().is_empty(), "{name}: the driver is not asked");
        assert!(cx.trace().events.is_empty(), "{name}: no event");
    }
    // The nearest accepted names pass the same checks.
    let script = Scripted::new();
    script.file(&["nul2"], "ok");
    script.file(&["con\u{130}n$"], "ok");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let notes = root(&mut cx, HostInterface::DirReadText);
    assert_eq!(read(&mut cx, notes, &["nul2"]), Ok((text("ok"), true)));
    assert_eq!(
        read(&mut cx, notes, &["con\u{130}n$"]),
        Ok((text("ok"), true))
    );
    granted(sub(&mut cx, notes, &["nul2"]));
}

#[test]
fn deep_ancestry() {
    let reply = on_small_stack(|| {
        let script = Scripted::new();
        script.file(&["a.txt"], "deep");
        let mut driver = script.clone();
        let mut cx = Context::new(&mut driver, header());
        let mut grant = root(&mut cx, HostInterface::DirReadText);
        for _ in 0..10_000 {
            grant = limit(&mut cx, grant, invocations(Some(3)));
        }
        let reply = read(&mut cx, grant, &["a.txt"]);
        drop(cx);
        reply
    });
    assert_eq!(reply, Ok((text("deep"), true)));
}

#[test]
fn a_call_while_an_event_is_pending() {
    let script = Scripted::new();
    script.file(&["a.txt"], "a");
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let notes = root(&mut cx, HostInterface::DirReadText);
    let at = names(&["a.txt"]);
    let first = call(
        &mut cx,
        HostOp::FsReadText,
        notes,
        &[HostArgument::Path(&at)],
    )
    .expect("a call");
    assert!(first.receipt.is_some());
    assert!(matches!(
        call(
            &mut cx,
            HostOp::FsReadText,
            notes,
            &[HostArgument::Path(&at)]
        ),
        Err(HostFault::InvalidHostValue(_))
    ));
}

#[test]
fn a_receipt_of_another_context() {
    let script = Scripted::new();
    script.file(&["a.txt"], "a");
    let mut first = script.clone();
    let mut second = script.clone();
    let mut here = Context::new(&mut first, header());
    let mut there = Context::new(&mut second, header());
    let mine = root(&mut here, HostInterface::DirReadText);
    let theirs = root(&mut there, HostInterface::DirReadText);
    let at = names(&["a.txt"]);
    let pending = call(
        &mut here,
        HostOp::FsReadText,
        mine,
        &[HostArgument::Path(&at)],
    )
    .expect("a call");
    let foreign = call(
        &mut there,
        HostOp::FsReadText,
        theirs,
        &[HostArgument::Path(&at)],
    )
    .expect("a call")
    .receipt
    .expect("a receipt");
    assert!(matches!(
        here.commit(foreign),
        Err(HostFault::InvalidHostValue(_))
    ));
    assert_eq!(here.commit(pending.receipt.expect("a receipt")), Ok(()));
}

/// One node of a random narrowing tree, and what the model expects of it.
struct Node {
    parent: Option<usize>,
    kind: Kind,
    token: GrantToken,
    /// The whole `sub` prefix in root coordinates.
    prefix: Vec<String>,
    /// The invocations and input bytes charged on its own counters.
    used: [u64; 2],
}

#[derive(Clone, Copy)]
enum Kind {
    Counters {
        invocations: Option<u64>,
        input: Option<u64>,
    },
    TextOnly,
    AnyName,
    Sub(&'static str),
}

fn below(random: &mut SplitMix64, bound: u64) -> u64 {
    random.next_u64() % bound
}

fn maybe(random: &mut SplitMix64, bound: u64) -> Option<u64> {
    (below(random, 3) != 0).then(|| below(random, bound))
}

fn ancestors(nodes: &[Node], from: usize) -> Vec<usize> {
    let mut found = Vec::new();
    let mut next = Some(from);
    while let Some(index) = next {
        found.push(index);
        next = nodes[index].parent;
    }
    found
}

/// One tree: a root with random bounds, up to eleven narrowings of random
/// parents, and twenty reads through random nodes, each compared with the
/// model's result and every node's counters.
fn model_tree(random: &mut SplitMix64, seed: u64, tree: usize) {
    let script = Scripted::new();
    let mut driver = script.clone();
    let mut cx = Context::new(&mut driver, header());
    let (root_invocations, root_input) = (maybe(random, 12), maybe(random, 20));
    let token = bounded(
        &mut cx,
        HostInterface::DirReadText,
        limits(root_invocations, root_input, None),
    );
    let mut nodes = vec![Node {
        parent: None,
        kind: Kind::Counters {
            invocations: root_invocations,
            input: root_input,
        },
        token,
        prefix: Vec::new(),
        used: [0, 0],
    }];
    for _ in 0..below(random, 12) {
        let parent = usize::try_from(below(random, nodes.len() as u64)).expect("small");
        let (kind, narrowed) = match below(random, 4) {
            0 => {
                let (invocations, input) = (maybe(random, 10), maybe(random, 15));
                let token = limit(
                    &mut cx,
                    nodes[parent].token,
                    GrantLimits {
                        invocations,
                        input_bytes: input,
                        output_bytes: None,
                    },
                );
                (Kind::Counters { invocations, input }, token)
            }
            1 => {
                let policy = files(&[], &["read"], suffix(".a"));
                (
                    Kind::TextOnly,
                    granted(restrict(&mut cx, nodes[parent].token, &policy)),
                )
            }
            2 => {
                let policy = files(&[], &["read"], unit("anyName"));
                (
                    Kind::AnyName,
                    granted(restrict(&mut cx, nodes[parent].token, &policy)),
                )
            }
            _ => {
                let name = if below(random, 2) == 0 { "d" } else { "e" };
                (
                    Kind::Sub(name),
                    granted(sub(&mut cx, nodes[parent].token, &[name])),
                )
            }
        };
        let mut prefix = nodes[parent].prefix.clone();
        if let Kind::Sub(name) = kind {
            prefix.push(name.to_owned());
        }
        nodes.push(Node {
            parent: Some(parent),
            kind,
            token: narrowed,
            prefix,
            used: [0, 0],
        });
    }
    for turn in 0..20 {
        let node = usize::try_from(below(random, nodes.len() as u64)).expect("small");
        let name = if below(random, 2) == 0 { "f.a" } else { "f.b" };
        let size = below(random, 7);
        let mut at = nodes[node].prefix.clone();
        at.push(name.to_owned());
        let contents = "x".repeat(usize::try_from(size).expect("small"));
        script.state().files.insert(
            at,
            crate::scripted::File {
                contents: contents.clone().into_bytes(),
                links: Some(1),
            },
        );
        // The model.
        let line = ancestors(&nodes, node);
        let permitted = line.iter().all(|index| {
            !matches!(nodes[*index].kind, Kind::TextOnly) || name.ends_with(".a")
        });
        let fits = |nodes: &[Node], invocations: u64, input: u64| {
            line.iter().all(|index| match nodes[*index].kind {
                Kind::Counters {
                    invocations: bound_invocations,
                    input: bound_input,
                } => {
                    bound_invocations.is_none_or(|bound| {
                        nodes[*index].used[0] + invocations <= bound
                    }) && bound_input
                        .is_none_or(|bound| nodes[*index].used[1] + input <= bound)
                }
                _ => true,
            })
        };
        let expected = if !permitted {
            (err(HostOp::FsReadText, "denied"), false)
        } else if !fits(&nodes, 1, 0) || !fits(&nodes, 1, size) {
            (err(HostOp::FsReadText, "limitExceeded"), false)
        } else {
            for index in &line {
                if let Kind::Counters { .. } = nodes[*index].kind {
                    nodes[*index].used[0] += 1;
                    nodes[*index].used[1] += size;
                }
            }
            (text(&contents), true)
        };
        let actual = read(&mut cx, nodes[node].token, &[name]);
        assert_eq!(
            actual,
            Ok(expected),
            "seed {seed}, tree {tree}, turn {turn}: the result differs from the model"
        );
        for (index, node) in nodes.iter().enumerate() {
            if let Kind::Counters { .. } = node.kind {
                let counters = counters(&cx, node.token);
                assert_eq!(
                    [counters.invocations().used(), counters.input_bytes().used()],
                    node.used,
                    "seed {seed}, tree {tree}, turn {turn}: node {index}'s counters differ"
                );
                assert_eq!(counters.invocations().reserved(), 0);
                assert_eq!(counters.input_bytes().reserved(), 0);
            }
        }
    }
}

#[test]
fn counters_against_a_model() {
    let seed = std::env::var("VIBRA_BROKER_SEED")
        .ok()
        .and_then(|seed| seed.parse().ok())
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |elapsed| {
                    elapsed.as_secs() ^ u64::from(elapsed.subsec_nanos())
                })
        });
    println!("broker model seed: {seed} (rerun with VIBRA_BROKER_SEED={seed})");
    let mut random = SplitMix64::new(seed);
    for tree in 0..10_000 {
        model_tree(&mut random, seed, tree);
    }
}

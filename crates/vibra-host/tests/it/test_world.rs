//! The test world: the test root, the six `TestWorld` members over a
//! `WorldDriver`, and the roots they issue (M2 step 21).
//!
//! Each test makes a context over a fresh default world with
//! `Context::for_world` and issues its test root.

use vibra_host::broker::Context;
use vibra_host::policy::{self, Policy};
use vibra_host::trace::{Event, GrantOrigin};
use vibra_host::world::{HostWorld, WorldDriver};
use vibra_ir::Value;
use vibra_ir::authority::{
    GrantToken, HostArgument, HostCompletion, HostContext, HostFault, HostOk,
    HostReply, HostRequest, Issued,
};
use vibra_ir::external::host::{HostOp, TestWorldOp};

use crate::trace::header;

fn path(names: &[&str]) -> Value {
    Value::Wrapper(Box::new(Value::List(
        names
            .iter()
            .map(|name| Value::Str((*name).to_owned()))
            .collect(),
    )))
}

fn files(entries: &[(&[&str], &str)]) -> Value {
    Value::Dict(
        entries
            .iter()
            .map(|(names, contents)| (path(names), Value::Str((*contents).to_owned())))
            .collect(),
    )
}

fn member(
    cx: &mut Context<'_>,
    op: TestWorldOp,
    world: GrantToken,
    arguments: &[Value],
) -> Result<Issued, HostFault> {
    cx.test_world(op, world, arguments, 1)
}

/// The grants of a bundle, by field name.
fn bundle(issued: Issued) -> Vec<(&'static str, GrantToken)> {
    match issued {
        Issued::Bundle(grants) => grants,
        other => panic!("not a bundle: {other:?}"),
    }
}

fn grant(grants: &[(&'static str, GrantToken)], name: &str) -> GrantToken {
    grants
        .iter()
        .find(|(field, _)| *field == name)
        .map(|(_, token)| *token)
        .expect("the field")
}

/// Runs `op` through `grant` and commits its receipt.
fn call(
    cx: &mut Context<'_>,
    op: HostOp,
    grant: GrantToken,
    arguments: &[HostArgument<'_>],
) -> HostReply {
    let completion: HostCompletion = cx
        .host(HostRequest {
            op,
            grant,
            arguments,
            invocation: 2,
            parent: Some(1),
            origin: None,
            room: u64::MAX,
        })
        .expect("a completion");
    if let Some(receipt) = completion.receipt {
        cx.commit(receipt).expect("committed");
    }
    completion.reply
}

fn rule(text: &str) -> Policy {
    policy::decode(
        format!("{{format: @scopePolicy.v1 family: @filesystem expression: {text}}}")
            .as_bytes(),
    )
    .expect("a policy")
}

#[test]
fn directory_issues_six_roots_in_field_name_order() {
    let mut world = WorldDriver::new(&HostWorld::default());
    let mut cx = Context::for_world(&mut world, header());
    let root = cx.test_root().expect("the test root");
    let fixture = files(&[(&["d", "a.txt"], "1"), (&["b.txt"], "2")]);
    let grants = bundle(
        member(&mut cx, TestWorldOp::Directory, root, &[fixture]).expect("issued"),
    );
    let names = grants.iter().map(|(name, _)| *name).collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            "list",
            "readBytes",
            "readText",
            "stat",
            "writeBytes",
            "writeText"
        ]
    );
    assert!(cx.trace().events.is_empty(), "a member appends no event");

    let reply = call(
        &mut cx,
        HostOp::FsReadText,
        grant(&grants, "readText"),
        &[HostArgument::Path(&["b.txt".to_owned()])],
    );
    assert_eq!(reply, HostReply::Ok(HostOk::Text("2".to_owned())));
    let reply = call(
        &mut cx,
        HostOp::FsReadEntries,
        grant(&grants, "list"),
        &[HostArgument::Path(&[])],
    );
    assert_eq!(
        reply,
        HostReply::Ok(HostOk::Names(vec!["b.txt".to_owned(), "d".to_owned()]))
    );
    let [Event::Host(read), Event::Host(list)] = cx.trace().events.as_slice() else {
        panic!("two host events: {:?}", cx.trace().events);
    };
    assert_eq!(read.grant.issued_by, GrantOrigin::TestWorld);
    assert_eq!(read.grant.root, 3, "readText is the third field name");
    assert_eq!(list.grant.root, 1);
}

/// The policy of the `stat` root, in the normal form of 03 "The wire form":
/// a `stat` of a directory is decided by the directory rules and of any
/// other entry by the file rules, so the root holds one of each, the
/// directory rule first by its canonical bytes.
const STAT_POLICY: &str = "{kind: @anyOf items: [{kind: @directoryRule root: @testWorld \
     prefix: [] depth: @tree actions: [@stat]} {kind: @fileRule root: @testWorld prefix: [] \
     depth: @tree actions: [@stat] names: {kind: @anyName}}]}";

#[test]
fn each_directory_root_has_the_policy_of_the_chapter() {
    let mut world = WorldDriver::new(&HostWorld::default());
    let mut cx = Context::for_world(&mut world, header());
    let root = cx.test_root().expect("the test root");
    let fixture = files(&[(&["a.txt"], "x")]);
    let grants = bundle(
        member(&mut cx, TestWorldOp::Directory, root, &[fixture]).expect("issued"),
    );
    let at = ["a.txt".to_owned()];
    let cases = [
        (
            "readBytes",
            HostOp::FsReadFile,
            vec![HostArgument::Path(&at)],
            "{kind: @fileRule root: @testWorld prefix: [] depth: @tree actions: [@read] \
             names: {kind: @anyName}}",
        ),
        (
            "readText",
            HostOp::FsReadText,
            vec![HostArgument::Path(&at)],
            "{kind: @fileRule root: @testWorld prefix: [] depth: @tree actions: [@read] \
             names: {kind: @anyName}}",
        ),
        (
            "writeBytes",
            HostOp::FsWriteFile,
            vec![HostArgument::Path(&at), HostArgument::Bytes(b"y")],
            "{kind: @fileRule root: @testWorld prefix: [] depth: @tree actions: [@create \
             @replace] names: {kind: @anyName}}",
        ),
        (
            "writeText",
            HostOp::FsWriteText,
            vec![HostArgument::Path(&at), HostArgument::Text("z")],
            "{kind: @fileRule root: @testWorld prefix: [] depth: @tree actions: [@create \
             @replace] names: {kind: @anyName}}",
        ),
        (
            "list",
            HostOp::FsReadEntries,
            vec![HostArgument::Path(&[])],
            "{kind: @directoryRule root: @testWorld prefix: [] depth: @tree actions: \
             [@list]}",
        ),
        (
            "stat",
            HostOp::FsMetadataStat,
            vec![HostArgument::Path(&[])],
            STAT_POLICY,
        ),
        (
            "stat",
            HostOp::FsMetadataStat,
            vec![HostArgument::Path(&at)],
            STAT_POLICY,
        ),
    ];
    for (index, (name, op, arguments, expected)) in cases.iter().enumerate() {
        let reply = call(&mut cx, *op, grant(&grants, name), arguments);
        assert!(matches!(reply, HostReply::Ok(_)), "{name}: {reply:?}");
        let Some(Event::Host(event)) = cx.trace().events.get(index) else {
            panic!("{name}: no event");
        };
        assert_eq!(
            event.grant.policy,
            policy::hash(&rule(expected)),
            "the policy of `{name}`"
        );
    }
}

#[test]
fn members_share_the_environment_clocks_and_random_and_make_new_streams() {
    let mut world = WorldDriver::new(&HostWorld::default());
    let mut cx = Context::for_world(&mut world, header());
    let root = cx.test_root().expect("the test root");
    let variables = |name: &str, value: &str| {
        Value::Dict(vec![(
            Value::Str(name.to_owned()),
            Value::Str(value.to_owned()),
        )])
    };
    let first = bundle(
        member(
            &mut cx,
            TestWorldOp::Environment,
            root,
            &[Value::List(Vec::new()), variables("HOME", "/h")],
        )
        .expect("issued"),
    );
    let names = first.iter().map(|(name, _)| *name).collect::<Vec<_>>();
    assert_eq!(names, ["arguments", "variables"]);
    member(
        &mut cx,
        TestWorldOp::Environment,
        root,
        &[
            Value::List(vec![Value::Str("a".to_owned())]),
            variables("HOME", "/g"),
        ],
    )
    .expect("issued");
    let reply = call(
        &mut cx,
        HostOp::EnvReadVar,
        grant(&first, "variables"),
        &[HostArgument::Text("HOME")],
    );
    assert_eq!(reply, HostReply::Ok(HostOk::Text("/g".to_owned())));
    let reply = call(
        &mut cx,
        HostOp::EnvReadVar,
        grant(&first, "variables"),
        &[HostArgument::Text("PATH")],
    );
    assert!(
        matches!(&reply, HostReply::Err(case) if case.case() == "notPresent"),
        "every name is permitted: {reply:?}"
    );
    let reply = call(
        &mut cx,
        HostOp::EnvReadArgs,
        grant(&first, "arguments"),
        &[],
    );
    assert_eq!(reply, HostReply::Ok(HostOk::Names(vec!["a".to_owned()])));

    let streams = bundle(
        member(
            &mut cx,
            TestWorldOp::Streams,
            root,
            &[Value::Str("in".to_owned())],
        )
        .expect("issued"),
    );
    let again = bundle(
        member(
            &mut cx,
            TestWorldOp::Streams,
            root,
            &[Value::Str(String::new())],
        )
        .expect("issued"),
    );
    call(
        &mut cx,
        HostOp::IoStdoutText,
        grant(&streams, "stdoutText"),
        &[HostArgument::Text("a")],
    );
    call(
        &mut cx,
        HostOp::IoStdoutBytes,
        grant(&again, "stdoutBytes"),
        &[HostArgument::Bytes(b"b")],
    );
    call(
        &mut cx,
        HostOp::IoStderrText,
        grant(&streams, "stderrText"),
        &[HostArgument::Text("e")],
    );
    let reply = call(
        &mut cx,
        HostOp::IoStdinText,
        grant(&streams, "stdinText"),
        &[HostArgument::Count(10)],
    );
    assert_eq!(reply, HostReply::Ok(HostOk::Text("in".to_owned())));
    let reply = call(
        &mut cx,
        HostOp::IoStdinText,
        grant(&again, "stdinText"),
        &[HostArgument::Count(10)],
    );
    assert_eq!(reply, HostReply::Ok(HostOk::Text(String::new())));
    assert_eq!(
        member(&mut cx, TestWorldOp::Output, root, &[]).expect("issued"),
        Issued::Output {
            stdout: b"ab".to_vec(),
            stderr: b"e".to_vec(),
        }
    );

    let clocks =
        bundle(member(&mut cx, TestWorldOp::Clocks, root, &[]).expect("issued"));
    let more = bundle(member(&mut cx, TestWorldOp::Clocks, root, &[]).expect("issued"));
    let first_read = call(
        &mut cx,
        HostOp::TimeNowMonotonic,
        grant(&clocks, "monotonic"),
        &[],
    );
    let second_read = call(
        &mut cx,
        HostOp::TimeNowMonotonic,
        grant(&more, "monotonic"),
        &[],
    );
    assert_eq!(first_read, HostReply::Ok(HostOk::U64(0)));
    assert_eq!(second_read, HostReply::Ok(HostOk::U64(1)), "one clock pair");
    let Issued::Grant(random) =
        member(&mut cx, TestWorldOp::Random, root, &[]).expect("issued")
    else {
        panic!("one grant");
    };
    let reply = call(
        &mut cx,
        HostOp::RandomGenerateBytes,
        random,
        &[HostArgument::Count(1)],
    );
    assert!(matches!(reply, HostReply::Ok(HostOk::Bytes(bytes)) if bytes.len() == 1));
}

/// ATK-test-world-op-on-leaf: a token of a directory root is no test
/// world.
#[test]
fn atk_test_world_op_on_leaf() {
    let mut world = WorldDriver::new(&HostWorld::default());
    let mut cx = Context::for_world(&mut world, header());
    let root = cx.test_root().expect("the test root");
    let grants = bundle(
        member(&mut cx, TestWorldOp::Directory, root, &[files(&[])]).expect("issued"),
    );
    for (name, leaf) in grants {
        let outcome = member(&mut cx, TestWorldOp::Directory, leaf, &[files(&[])]);
        assert!(
            matches!(outcome, Err(HostFault::InvalidHostValue(_))),
            "`{name}` is no test world: {outcome:?}"
        );
    }
}

#[test]
fn the_test_root_performs_no_host_operation() {
    let mut world = WorldDriver::new(&HostWorld::default());
    let mut cx = Context::for_world(&mut world, header());
    let root = cx.test_root().expect("the test root");
    let outcome = cx.host(HostRequest {
        op: HostOp::TimeNowWall,
        grant: root,
        arguments: &[],
        invocation: 1,
        parent: Some(1),
        origin: None,
        room: u64::MAX,
    });
    assert!(matches!(outcome, Err(HostFault::InvalidHostValue(_))));
    assert!(matches!(
        cx.test_root(),
        Err(HostFault::InvalidHostValue(_))
    ));
}

#[test]
fn a_context_over_another_driver_has_no_test_root() {
    let mut world = WorldDriver::new(&HostWorld::default());
    let mut cx = Context::new(&mut world, header());
    assert!(matches!(
        cx.test_root(),
        Err(HostFault::InvalidHostValue(_))
    ));
}

/// Grants from one test's world are unusable in another.
#[test]
fn a_foreign_world_and_its_grants_are_invalid_host_values() {
    let mut first = WorldDriver::new(&HostWorld::default());
    let mut second = WorldDriver::new(&HostWorld::default());
    let mut here = Context::for_world(&mut first, header());
    let mut there = Context::for_world(&mut second, header());
    let root = here.test_root().expect("the test root");
    there.test_root().expect("the other test root");
    let clocks =
        bundle(member(&mut here, TestWorldOp::Clocks, root, &[]).expect("issued"));
    let outcome = member(&mut there, TestWorldOp::Clocks, root, &[]);
    assert!(matches!(outcome, Err(HostFault::InvalidHostValue(_))));
    let outcome = there.host(HostRequest {
        op: HostOp::TimeNowWall,
        grant: grant(&clocks, "wall"),
        arguments: &[],
        invocation: 1,
        parent: Some(1),
        origin: None,
        room: u64::MAX,
    });
    assert!(matches!(outcome, Err(HostFault::InvalidHostValue(_))));
}

/// A fixture that holds a path as a file and as a directory, or holds the
/// empty path, makes nothing and issues nothing: it is the typed
/// `ConflictingFixture` with that path, which ends the test
/// `@test.invalid` (04 "Test worlds and grants"), never an invalid host
/// value.
#[test]
fn a_conflicting_fixture_issues_nothing() {
    let mut world = WorldDriver::new(&HostWorld::default());
    let mut cx = Context::for_world(&mut world, header());
    let root = cx.test_root().expect("the test root");
    for (fixture, conflict) in [
        (files(&[(&["a"], "1"), (&["a", "b"], "2")]), vec!["a"]),
        (files(&[(&["a", "b"], "2"), (&["a"], "1")]), vec!["a"]),
        (files(&[(&[], "1")]), vec![]),
    ] {
        let outcome = member(&mut cx, TestWorldOp::Directory, root, &[fixture]);
        assert_eq!(
            outcome,
            Err(HostFault::ConflictingFixture(
                conflict.into_iter().map(str::to_owned).collect()
            ))
        );
    }
    let grants = bundle(
        member(&mut cx, TestWorldOp::Directory, root, &[files(&[])]).expect("issued"),
    );
    call(
        &mut cx,
        HostOp::FsReadEntries,
        grant(&grants, "list"),
        &[HostArgument::Path(&[])],
    );
    let [Event::Host(event)] = cx.trace().events.as_slice() else {
        panic!("one event");
    };
    assert_eq!(event.grant.root, 1, "the refused fixtures issued no root");
}

#[test]
fn a_member_with_arguments_it_does_not_take_is_an_invalid_host_value() {
    let mut world = WorldDriver::new(&HostWorld::default());
    let mut cx = Context::for_world(&mut world, header());
    let root = cx.test_root().expect("the test root");
    for (op, arguments) in [
        (TestWorldOp::Directory, vec![]),
        (TestWorldOp::Streams, vec![Value::Void]),
        (TestWorldOp::Clocks, vec![Value::Void]),
        (TestWorldOp::Directory, vec![files(&[(&[".."], "x")])]),
    ] {
        let outcome = member(&mut cx, op, root, &arguments);
        assert!(
            matches!(outcome, Err(HostFault::InvalidHostValue(_))),
            "{op:?}: {outcome:?}"
        );
    }
}

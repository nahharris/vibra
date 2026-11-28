//! `vibra_host::recipe`: the provider table, `BindingPlan::validate`, the
//! startup order, and the recipe identity
//! (`docs/spec/04-programs-and-packages.md`, "Targets" and "Startup order";
//! `docs/spec/05-tooling.md`, `StartupRecord.bindings[]`).
//!
//! The expected table, faults, orders, bytes, and identities are written from
//! the chapters.

use vibra_diagnostics::ByteSpan;
use vibra_host::policy::{Policy, decode};
use vibra_host::recipe::{
    BindingFault, BindingPlan, Field, Leaf, LeafName, Limits, PROVIDERS, Problem,
    Provider, Resource, ResourceKind, recipe_bytes, recipe_identity,
};
use vibra_ir::external::host::HostInterface;

use crate::support::name;

/// A `.txt` read policy rooted at `root`, as the chapter's example writes it.
fn text_policy(root: &str) -> Policy {
    let text = format!(
        "{{format: @scopePolicy.v1 family: @filesystem expression: {{kind: @fileRule \
         root: {root} prefix: [] depth: @tree actions: [@read] names: {{kind: @suffix \
         value: \".txt\"}}}}}}"
    );
    decode(text.as_bytes()).expect("the policy decodes")
}

/// A leaf whose recipe identity is computed from its parts.
fn leaf(
    atom: &str,
    interface: HostInterface,
    provider: Provider,
    resource: Resource,
    policy: Option<Policy>,
    after: &[&str],
) -> Leaf {
    let after = after.iter().map(|atom| name(atom)).collect::<Vec<_>>();
    let limits = Limits::default();
    let recipe_identity =
        recipe_identity(provider, &resource, policy.as_ref(), &limits, &after);
    Leaf {
        name: name(atom),
        interface,
        optional: false,
        provider,
        resource,
        policy,
        limits,
        after,
        recipe_identity,
        span: ByteSpan::new(0, 0),
    }
}

/// A standard-output leaf, which needs no source and no policy.
fn stdout(atom: &str, after: &[&str]) -> Leaf {
    leaf(
        atom,
        HostInterface::StdoutText,
        Provider::IoStdout,
        Resource::ProcessStream,
        None,
        after,
    )
}

fn args(atom: &str) -> Leaf {
    leaf(
        atom,
        HostInterface::EnvReadArgs,
        Provider::EnvArgs,
        Resource::ProcessArguments,
        None,
        &[],
    )
}

fn vars(atom: &str, names: &[&str]) -> Leaf {
    leaf(
        atom,
        HostInterface::EnvReadVar,
        Provider::EnvVars,
        Resource::VariableSet {
            names: names.iter().map(|name| (*name).to_owned()).collect(),
        },
        None,
        &[],
    )
}

fn plan(leaves: Vec<Leaf>) -> BindingPlan {
    BindingPlan {
        target: "@hello".to_owned(),
        leaves,
    }
}

fn faults(plan: &BindingPlan) -> Vec<BindingFault> {
    plan.validate().expect_err("the plan is rejected")
}

fn fault(leaf: &str, field: Field, problem: Problem) -> BindingFault {
    BindingFault {
        leaf: name(leaf),
        field,
        problem,
    }
}

fn order(plan: &BindingPlan) -> Vec<&str> {
    plan.order()
        .into_iter()
        .map(|leaf| leaf.name.as_str())
        .collect()
}

// ---------------------------------------------------------------------------
// The provider table.

#[test]
fn the_provider_table_is_the_chapters() {
    use HostInterface as I;
    use ResourceKind as K;
    let expected: [(&str, &[HostInterface], &[ResourceKind]); 9] = [
        (
            "@host.fs.directory",
            &[
                I::DirReadBytes,
                I::DirReadText,
                I::DirList,
                I::DirWriteBytes,
                I::DirWriteText,
                I::DirStat,
            ],
            &[K::FixedPath, K::ArgumentPath, K::VariablePath],
        ),
        (
            "@host.io.stdin",
            &[I::StdinText, I::StdinBytes],
            &[K::ProcessStream],
        ),
        (
            "@host.io.stdout",
            &[I::StdoutText, I::StdoutBytes],
            &[K::ProcessStream],
        ),
        (
            "@host.io.stderr",
            &[I::StderrText, I::StderrBytes],
            &[K::ProcessStream],
        ),
        ("@host.env.vars", &[I::EnvReadVar], &[K::VariableSet]),
        ("@host.env.args", &[I::EnvReadArgs], &[K::ProcessArguments]),
        ("@host.time.wall", &[I::WallClock], &[K::SystemClock]),
        (
            "@host.time.monotonic",
            &[I::MonotonicClock],
            &[K::SystemClock],
        ),
        ("@host.random", &[I::RandomBytes], &[K::SystemRandom]),
    ];
    for (row, (atom, types, kinds)) in PROVIDERS.iter().zip(expected) {
        assert_eq!(row.provider.atom(), atom);
        assert_eq!(row.grant_types, types, "{atom}");
        assert_eq!(row.resource_kinds, kinds, "{atom}");
        assert_eq!(Provider::from_atom(atom), Some(row.provider));
        assert_eq!(row.provider.row(), row);
    }
    let kinds = [
        (K::FixedPath, "@fixedPath"),
        (K::ArgumentPath, "@argumentPath"),
        (K::VariablePath, "@variablePath"),
        (K::ProcessStream, "@processStream"),
        (K::VariableSet, "@variableSet"),
        (K::ProcessArguments, "@processArguments"),
        (K::SystemClock, "@systemClock"),
        (K::SystemRandom, "@systemRandom"),
    ];
    for (kind, atom) in kinds {
        assert_eq!(kind.atom(), atom);
    }
    assert_eq!(Provider::from_atom("@host.net"), None);
}

/// A resource of `kind`, and the leaves its `source` needs.
fn resource_of(kind: ResourceKind) -> (Resource, Vec<Leaf>, Vec<&'static str>) {
    match kind {
        ResourceKind::FixedPath => (
            Resource::FixedPath {
                path: "/data".to_owned(),
            },
            Vec::new(),
            Vec::new(),
        ),
        ResourceKind::ArgumentPath => (
            Resource::ArgumentPath {
                index: 0,
                source: name("@args"),
            },
            vec![args("@args")],
            vec!["@args"],
        ),
        ResourceKind::VariablePath => (
            Resource::VariablePath {
                name: "HOME".to_owned(),
                source: name("@vars"),
            },
            vec![vars("@vars", &["HOME"])],
            vec!["@vars"],
        ),
        ResourceKind::ProcessStream => {
            (Resource::ProcessStream, Vec::new(), Vec::new())
        }
        ResourceKind::VariableSet => (
            Resource::VariableSet {
                names: vec!["HOME".to_owned()],
            },
            Vec::new(),
            Vec::new(),
        ),
        ResourceKind::ProcessArguments => {
            (Resource::ProcessArguments, Vec::new(), Vec::new())
        }
        ResourceKind::SystemClock => (Resource::SystemClock, Vec::new(), Vec::new()),
        ResourceKind::SystemRandom => (Resource::SystemRandom, Vec::new(), Vec::new()),
    }
}

#[test]
fn every_provider_validates_each_grant_type_and_resource_kind() {
    let mut checked = 0;
    for row in &PROVIDERS {
        for &interface in row.grant_types {
            for &kind in row.resource_kinds {
                let (resource, mut leaves, after) = resource_of(kind);
                let policy =
                    (row.provider == Provider::FsDirectory).then(|| text_policy("@x"));
                leaves.push(leaf(
                    "@x",
                    interface,
                    row.provider,
                    resource,
                    policy,
                    &after,
                ));
                let plan = plan(leaves);
                assert_eq!(
                    plan.validate(),
                    Ok(()),
                    "{} {interface:?} {kind:?}",
                    row.provider.atom()
                );
                checked += 1;
            }
        }
    }
    // 6 × 3 for the directory, 2 for each stream, 1 for each other row.
    assert_eq!(checked, 29);
}

// ---------------------------------------------------------------------------
// Faults.

#[test]
fn a_provider_that_cannot_issue_the_type() {
    let wrong = plan(vec![leaf(
        "@notes",
        HostInterface::DirReadText,
        Provider::IoStdout,
        Resource::ProcessStream,
        None,
        &[],
    )]);
    assert_eq!(
        faults(&wrong),
        vec![fault("@notes", Field::Provider, Problem::TypeNotIssued)]
    );
}

#[test]
fn a_resource_of_a_kind_the_provider_does_not_accept() {
    let wrong = plan(vec![leaf(
        "@random",
        HostInterface::RandomBytes,
        Provider::Random,
        Resource::FixedPath {
            path: "/dev/urandom".to_owned(),
        },
        None,
        &[],
    )]);
    assert_eq!(
        faults(&wrong),
        vec![fault("@random", Field::Resource, Problem::KindNotAccepted)]
    );
}

#[test]
fn an_argument_source_of_the_wrong_type() {
    let wrong = plan(vec![
        vars("@vars", &["HOME"]),
        leaf(
            "@notes",
            HostInterface::DirReadText,
            Provider::FsDirectory,
            Resource::ArgumentPath {
                index: 0,
                source: name("@vars"),
            },
            Some(text_policy("@notes")),
            &["@vars"],
        ),
    ]);
    assert_eq!(
        faults(&wrong),
        vec![fault("@notes", Field::Source, Problem::WrongSourceType)]
    );
}

#[test]
fn a_variable_outside_the_sources_names() {
    let wrong = plan(vec![
        vars("@vars", &["TMP"]),
        leaf(
            "@notes",
            HostInterface::DirReadText,
            Provider::FsDirectory,
            Resource::VariablePath {
                name: "HOME".to_owned(),
                source: name("@vars"),
            },
            Some(text_policy("@notes")),
            &["@vars"],
        ),
    ]);
    assert_eq!(
        faults(&wrong),
        vec![fault("@notes", Field::Source, Problem::VariableNotNamed)]
    );
}

#[test]
fn a_source_that_is_no_leaf() {
    let wrong = plan(vec![leaf(
        "@notes",
        HostInterface::DirReadText,
        Provider::FsDirectory,
        Resource::ArgumentPath {
            index: 0,
            source: name("@args"),
        },
        Some(text_policy("@notes")),
        &[],
    )]);
    assert_eq!(
        faults(&wrong),
        vec![
            fault("@notes", Field::Source, Problem::UnknownSource),
            fault("@notes", Field::After, Problem::AfterLacksSource),
        ]
    );
}

#[test]
fn after_lacks_the_source() {
    let wrong = plan(vec![
        args("@args"),
        leaf(
            "@notes",
            HostInterface::DirReadText,
            Provider::FsDirectory,
            Resource::ArgumentPath {
                index: 0,
                source: name("@args"),
            },
            Some(text_policy("@notes")),
            &[],
        ),
    ]);
    assert_eq!(
        faults(&wrong),
        vec![fault("@notes", Field::After, Problem::AfterLacksSource)]
    );
}

#[test]
fn after_names_leaves_once() {
    let wrong = plan(vec![
        stdout("@a", &["@b", "@b", "@missing"]),
        stdout("@b", &[]),
    ]);
    assert_eq!(
        faults(&wrong),
        vec![
            fault("@a", Field::After, Problem::RepeatedAfter(name("@b"))),
            fault("@a", Field::After, Problem::UnknownAfter(name("@missing"))),
        ]
    );
}

#[test]
fn an_after_cycle_is_reported_at_the_later_edge() {
    let expected = vec![fault("@b", Field::After, Problem::Cycle(name("@a")))];
    let written = plan(vec![stdout("@a", &["@b"]), stdout("@b", &["@a"])]);
    assert_eq!(faults(&written), expected);
    let reversed = plan(vec![stdout("@b", &["@a"]), stdout("@a", &["@b"])]);
    assert_eq!(
        faults(&reversed),
        expected,
        "the written order plays no part"
    );
    let own = plan(vec![stdout("@a", &["@a"])]);
    assert_eq!(
        faults(&own),
        vec![fault("@a", Field::After, Problem::Cycle(name("@a")))]
    );
}

#[test]
fn a_policy_on_another_provider() {
    let wrong = plan(vec![leaf(
        "@out",
        HostInterface::StdoutText,
        Provider::IoStdout,
        Resource::ProcessStream,
        Some(text_policy("@out")),
        &[],
    )]);
    assert_eq!(
        faults(&wrong),
        vec![fault("@out", Field::Policy, Problem::UnexpectedPolicy)]
    );
}

#[test]
fn a_directory_without_a_policy() {
    let wrong = plan(vec![leaf(
        "@notes",
        HostInterface::DirReadText,
        Provider::FsDirectory,
        Resource::FixedPath {
            path: "notes".to_owned(),
        },
        None,
        &[],
    )]);
    assert_eq!(
        faults(&wrong),
        vec![fault("@notes", Field::Policy, Problem::MissingPolicy)]
    );
}

#[test]
fn a_policy_with_another_root() {
    let wrong = plan(vec![leaf(
        "@notes",
        HostInterface::DirReadText,
        Provider::FsDirectory,
        Resource::FixedPath {
            path: "notes".to_owned(),
        },
        Some(text_policy("@logs")),
        &[],
    )]);
    assert_eq!(
        faults(&wrong),
        vec![fault(
            "@notes",
            Field::Policy,
            Problem::ForeignRoot("@logs".to_owned())
        )]
    );
}

// ---------------------------------------------------------------------------
// The startup order.

#[test]
fn the_order_is_by_spelling() {
    let leaves = plan(vec![
        stdout("@b", &[]),
        stdout("@a", &[]),
        stdout("@logs.reader", &[]),
    ]);
    assert_eq!(order(&leaves), vec!["@a", "@b", "@logs.reader"]);
}

#[test]
fn the_order_follows_after() {
    let leaves = plan(vec![stdout("@a", &["@z"]), stdout("@z", &[])]);
    assert_eq!(order(&leaves), vec!["@z", "@a"]);
}

#[test]
fn the_order_recomputes_readiness_after_each_leaf() {
    let make = |atom: &str| match atom {
        "@a" => stdout("@a", &["@b"]),
        other => stdout(other, &[]),
    };
    let permutations = [
        ["@a", "@b", "@z"],
        ["@a", "@z", "@b"],
        ["@b", "@a", "@z"],
        ["@b", "@z", "@a"],
        ["@z", "@a", "@b"],
        ["@z", "@b", "@a"],
    ];
    for written in permutations {
        let leaves = plan(written.iter().map(|atom| make(atom)).collect());
        assert_eq!(order(&leaves), vec!["@b", "@a", "@z"], "{written:?}");
    }
}

// ---------------------------------------------------------------------------
// The recipe identity.

#[test]
fn a_recipe_with_reordered_policy_items_has_one_identity() {
    let policy = |items: &str| {
        let text = format!(
            "{{format: @scopePolicy.v1 family: @filesystem expression: {{kind: @anyOf \
             items: [{items}]}}}}"
        );
        decode(text.as_bytes()).expect("the policy decodes")
    };
    let read = "{kind: @fileRule root: @notes prefix: [] depth: @tree actions: [@read] \
                names: {kind: @anyName}}";
    let list =
        "{kind: @directoryRule root: @notes prefix: [] depth: @tree actions: [@list]}";
    let first = policy(&format!("{read} {list}"));
    let second = policy(&format!("{list} {read}"));
    let resource = Resource::FixedPath {
        path: "notes".to_owned(),
    };
    let identity = |policy: &Policy| {
        recipe_identity(
            Provider::FsDirectory,
            &resource,
            Some(policy),
            &Limits::default(),
            &[],
        )
    };
    assert_eq!(identity(&first), identity(&second));
}

/// The `@notes` recipe of the programs chapter's project file example.
fn example_notes() -> (Resource, Policy, Vec<LeafName>) {
    (
        Resource::ArgumentPath {
            index: 0,
            source: name("@args"),
        },
        text_policy("@notes"),
        vec![name("@args")],
    )
}

#[test]
fn the_chapters_example_recipes_have_these_bytes_and_identities() {
    let (resource, policy, after) = example_notes();
    let limits = Limits::default();
    assert_eq!(
        recipe_bytes(
            Provider::FsDirectory,
            &resource,
            Some(&policy),
            &limits,
            &after
        ),
        "{provider: @host.fs.directory resource: {kind: @argumentPath index: 0u64 source: \
         @args} policy: {format: @scopePolicy.v1 family: @filesystem expression: {kind: \
         @fileRule root: @notes prefix: [] depth: @tree actions: [@read] names: {kind: \
         @suffix value: \".txt\"}}} after: [@args]}"
    );
    // Computed outside the crate: the SHA-256 of `vibraRecipeIdentityV2`, a
    // zero byte, and the bytes above.
    assert_eq!(
        recipe_identity(
            Provider::FsDirectory,
            &resource,
            Some(&policy),
            &limits,
            &after
        ),
        "sha256:6b727fa409b44c5e10beac220765d1eb5dd3ca200bfbe4ec3bfb6d9ee4e6ce87"
    );
    assert_eq!(
        recipe_identity(
            Provider::EnvArgs,
            &Resource::ProcessArguments,
            None,
            &limits,
            &[]
        ),
        "sha256:1da163cfe646db833307d96150a8d84b55afedc0fc64b1d877032060500275ff"
    );
    assert_eq!(
        recipe_identity(
            Provider::IoStdout,
            &Resource::ProcessStream,
            None,
            &limits,
            &[]
        ),
        "sha256:c04d73b45a27fb886443acf450affe9dabbb976b10970e5fbcb0728e6d591fcb"
    );
}

#[test]
fn the_identity_covers_the_limits_and_after() {
    let (resource, policy, after) = example_notes();
    let unbounded = Limits::default();
    let bounded = Limits {
        invocations: Some(5),
        input_bytes: None,
        output_bytes: Some(10),
    };
    assert_eq!(
        recipe_bytes(
            Provider::IoStdout,
            &Resource::ProcessStream,
            None,
            &bounded,
            &[]
        ),
        "{provider: @host.io.stdout resource: {kind: @processStream} limits: \
         {invocations: 5u64 outputBytes: 10u64} after: []}"
    );
    let base = recipe_identity(
        Provider::FsDirectory,
        &resource,
        Some(&policy),
        &unbounded,
        &after,
    );
    assert_ne!(
        base,
        recipe_identity(
            Provider::FsDirectory,
            &resource,
            Some(&policy),
            &bounded,
            &after
        )
    );
    assert_ne!(
        base,
        recipe_identity(
            Provider::FsDirectory,
            &resource,
            Some(&policy),
            &unbounded,
            &[]
        )
    );
}

#[test]
fn the_resource_records_write_kind_first() {
    let cases = [
        (
            Resource::FixedPath {
                path: "C:\\data".to_owned(),
            },
            "{kind: @fixedPath path: \"C:\\\\data\"}",
        ),
        (
            Resource::VariablePath {
                name: "HOME".to_owned(),
                source: name("@vars"),
            },
            "{kind: @variablePath name: \"HOME\" source: @vars}",
        ),
        (
            Resource::VariableSet {
                names: vec!["TMP".to_owned(), "HOME".to_owned()],
            },
            "{kind: @variableSet names: [\"TMP\" \"HOME\"]}",
        ),
        (Resource::SystemClock, "{kind: @systemClock}"),
        (Resource::SystemRandom, "{kind: @systemRandom}"),
    ];
    for (resource, bytes) in cases {
        assert_eq!(resource.canonical_bytes(), bytes);
    }
}

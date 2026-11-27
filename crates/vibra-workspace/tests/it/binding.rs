//! The binding recipes of a binary target, checked through a captured
//! project. Expectations come from the programs chapter, "Targets" and
//! "Startup order", and the effects chapter, "Target admission".

use vibra_diagnostics::{ByteSpan, DiagnosticCode, Level};
use vibra_host::recipe::{BindingPlan, Provider, Resource};
use vibra_workspace::binding::plan_of;
use vibra_workspace::{WorkspaceCheck, check_snapshot};

use crate::project::BINARY;
use crate::support::{TempDir, capture};

/// The chapter's `main`, under the root of [`BINARY`].
const MAIN: &str = "use @std.env.EnvReadArgs enduse\n\n\
                    use @std.fs.DirReadText enduse\n\n\
                    use @std.io.StdoutText enduse\n\n\
                    use @std.process.ExitCode enduse\n\n\
                    proc main\n  \
                    takes {args: EnvReadArgs notes: DirReadText out: Option(StdoutText)}\n  \
                    returns code: ExitCode\n  \
                    performs {EnvReadArgs DirReadText StdoutText}\n  \
                    do\n    return ExitCode(0u8)\nendproc\n";

fn check(document: &str, main: &str) -> WorkspaceCheck {
    let tree = TempDir::project("binding", document, &[("src/hello/main.vib", main)]);
    let snapshot = capture(tree.path()).expect("the project captures");
    check_snapshot(&snapshot, None)
}

/// The project-file diagnostics: code and span.
fn project_faults(check: &WorkspaceCheck) -> Vec<(DiagnosticCode, ByteSpan)> {
    check
        .diagnostics()
        .iter()
        .filter(|diagnostic| diagnostic.source_id() == Some("project.vibon"))
        .map(|diagnostic| (diagnostic.code(), diagnostic.primary_span()))
        .collect()
}

fn at(text: &str, needle: &str) -> ByteSpan {
    let start = text.find(needle).expect("the needle occurs");
    ByteSpan::new(start, start + needle.len())
}

fn names(plan: &BindingPlan) -> Vec<&str> {
    plan.order().iter().map(|leaf| leaf.name.as_str()).collect()
}

/// The recipe of `key` in [`BINARY`], from its key to its closing brace.
fn recipe(key: &str) -> String {
    let start = BINARY.find(&format!("        {key}: {{")).expect("the key");
    let end = start
        + BINARY
            .get(start..)
            .and_then(|rest| rest.find("\n        }\n"))
            .expect("the recipe's end")
        + "\n        }\n".len();
    BINARY.get(start..end).expect("the recipe").to_owned()
}

#[test]
fn the_chapters_target_has_a_validated_plan_in_startup_order() {
    let result = check(BINARY, MAIN);
    assert!(!result.has_errors(), "{:?}", result.diagnostics());
    let plan = plan_of(&result, "@hello").expect("the plan");
    assert_eq!(plan.validate(), Ok(()));
    assert_eq!(plan.target, "@hello");
    assert_eq!(names(plan), ["@args", "@notes", "@out"]);
    let notes = plan
        .leaves
        .iter()
        .find(|leaf| leaf.name.as_str() == "@notes")
        .expect("the leaf");
    assert_eq!(notes.provider, Provider::FsDirectory);
    assert!(matches!(
        &notes.resource,
        Resource::ArgumentPath { index: 0, source } if source.as_str() == "@args"
    ));
    assert!(notes.policy.is_some());
    let out = plan
        .leaves
        .iter()
        .find(|leaf| leaf.name.as_str() == "@out")
        .expect("the leaf");
    assert!(out.optional);
    let recipe_start = BINARY
        .find("{\n          provider: @host.io.stdout")
        .expect("@out");
    assert_eq!(out.span.start(), recipe_start);
    assert!(plan_of(&result, "@nope").is_none());
}

#[test]
fn the_written_order_of_grants_plays_no_part() {
    let [args, notes, out] = ["@args", "@notes", "@out"].map(recipe);
    let written = format!("{args}{notes}{out}");
    assert!(BINARY.contains(&written));
    let permuted = BINARY.replace(&written, &format!("{out}{notes}{args}"));
    assert_ne!(permuted, BINARY);
    let first = check(BINARY, MAIN);
    let second = check(&permuted, MAIN);
    let (Some(first), Some(second)) =
        (plan_of(&first, "@hello"), plan_of(&second, "@hello"))
    else {
        panic!(
            "both plans: {:?} {:?}",
            first.diagnostics(),
            second.diagnostics()
        );
    };
    assert_eq!(names(first), names(second));
    let identities = |plan: &BindingPlan| {
        plan.order()
            .iter()
            .map(|leaf| leaf.recipe_identity.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(identities(first), identities(second));
}

#[test]
fn a_target_with_a_faulty_recipe_has_no_plan() {
    let document =
        BINARY.replace("provider: @host.io.stdout", "provider: @host.io.stderr");
    let result = check(&document, MAIN);
    assert_eq!(
        project_faults(&result),
        [(
            DiagnosticCode::GrantInvalidBinding,
            at(&document, "@host.io.stderr")
        )]
    );
    assert!(plan_of(&result, "@hello").is_none());
    assert!(result.entries().is_empty());
}

#[test]
fn a_cycle_of_three_is_reported_once_related_along_it() {
    // @a after @c, @b after @a, @c after @b: the walk from @a follows @c,
    // then @b, whose entry @a closes the cycle; the rest of it is @a's
    // entry @c and @c's entry @b.
    let directory = |leaf: &str, after: &str| {
        format!(
            "{leaf}: {{provider: @host.fs.directory resource: {{kind: @fixedPath path: \"d\"}} \
             policy: {{format: @scopePolicy.v1 family: @filesystem expression: {{kind: @fileRule \
             root: {leaf} prefix: [] depth: @tree actions: [@read] names: {{kind: @anyName}}}}}} \
             after: [{after}]}}"
        )
    };
    let document = format!(
        "{{format: @project.v2 package: {{name: \"hello\" version: \"0.1.0\"}} targets: \
         [{{name: @hello kind: @bin root: \"src/hello\" entry: @hello.main.main effects: [] \
         grants: [{} {} {}]}}] dependencies: [:]}}",
        directory("@a", "@c"),
        directory("@b", "@a"),
        directory("@c", "@b")
    );
    let main = "use @std.fs.DirReadText enduse\n\nuse @std.process.ExitCode enduse\n\n\
                proc main takes {a: DirReadText b: DirReadText c: DirReadText} \
                returns code: ExitCode do return ExitCode(0u8) endproc\n";
    let result = check(&document, main);
    let entry = |list: &str| {
        let span = at(&document, list);
        ByteSpan::new(span.start() + 1, span.end() - 1)
    };
    let [cycle] = result.diagnostics() else {
        panic!("one diagnostic: {:?}", result.diagnostics());
    };
    assert_eq!(cycle.code(), DiagnosticCode::GrantBindingCycle);
    assert_eq!(cycle.primary_span(), entry("[@a]"));
    assert_eq!(
        cycle
            .related()
            .iter()
            .map(|related| related.span)
            .collect::<Vec<_>>(),
        [entry("[@c]"), entry("[@b]")]
    );
}

#[test]
fn a_malformed_policy_on_a_stream_is_an_invalid_policy_only() {
    // The policy is decoded first: a document that is no policy is
    // reported as one, and not again as a policy the issuer does not take.
    let document = BINARY.replace(
        "          resource: {kind: @processStream}\n",
        "          resource: {kind: @processStream}\n          policy: {format: @scopePolicy.v9}\n",
    );
    let result = check(&document, MAIN);
    assert_eq!(
        project_faults(&result),
        [(
            DiagnosticCode::GrantInvalidPolicy,
            at(&document, "{format: @scopePolicy.v9}")
        )]
    );
}

#[test]
fn an_extra_key_is_related_to_the_parameter_it_names() {
    // `@notes.inner` is no leaf: `notes` is a plain parameter. The key names
    // the parameter, so the fault relates to it.
    let document = BINARY.replace("        @out: {", "        @notes.inner: {\n          provider: @host.io.stdout\n          resource: {kind: @processStream}\n          after: []\n        }\n        @out: {");
    let result = check(&document, MAIN);
    let faults = result
        .diagnostics()
        .iter()
        .filter(|diagnostic| diagnostic.level() == Level::Error)
        .collect::<Vec<_>>();
    let [extra] = faults.as_slice() else {
        panic!("one error: {faults:?}");
    };
    assert_eq!(extra.code(), DiagnosticCode::GrantInvalidBinding);
    let related = extra.related();
    let [parameter] = related else {
        panic!("one related span: {related:?}");
    };
    assert_eq!(parameter.source_id.as_deref(), Some("src/hello/main.vib"));
    let start = MAIN.find("notes: DirReadText").expect("the parameter");
    assert_eq!(parameter.span, ByteSpan::new(start, start + "notes".len()));
}

#[test]
fn the_module_holds_no_copy_of_the_provider_table() {
    // Architecture: the provider table lives in `vibra-host::recipe`; the
    // project checker names no provider and calls `BindingPlan::validate`.
    let source = include_str!("../../src/binding.rs");
    assert!(source.contains("plan.validate()"));
    for row in vibra_host::recipe::PROVIDERS {
        assert!(
            !source.contains(&format!("\"{}\"", row.provider.atom())),
            "{}",
            row.provider.atom()
        );
    }
    assert!(!source.contains("grant_types") && !source.contains("resource_kinds"));
}

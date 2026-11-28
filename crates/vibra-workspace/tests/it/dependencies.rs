//! Local dependencies (M2 step 27): `docs/spec/04-programs-and-packages.md`,
//! "Dependencies and lock" and "Packages, units, and targets", and the rows
//! of `docs/spec/07-diagnostics-and-conformance.md`, "Availability,
//! discovery and resolution". A `@path` dependency binds the library target
//! its key names; unit names are global in the snapshot.

use std::path::Path;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_interp::MemoryBudget;
use vibra_workspace::{
    SourceSnapshot, TestOutcome, WorkspaceCheck, check_snapshot, resolve_input,
    run_tests, workspace_revision,
};

use crate::support::{TempDir, capture};

/// The dict of `entries`, `[:]` when there is none.
fn dict(entries: &str) -> String {
    if entries.is_empty() {
        "[:]".to_owned()
    } else {
        format!("[{entries}]")
    }
}

/// The root project `app`: one library target `@app` and `dependencies`.
fn app(dependencies: &str) -> String {
    let dependencies = dict(dependencies);
    format!(
        "{{format: @project.v2 package: {{name: \"app\" version: \"0.1.0\"}} targets: [{{name: @app kind: @lib root: \"src/app\"}}] dependencies: {dependencies}}}"
    )
}

/// A package `name` with the library targets `libraries` (each rooted at
/// `src/<target>`) and `dependencies`.
fn package(name: &str, libraries: &[&str], dependencies: &str) -> String {
    let targets = libraries
        .iter()
        .map(|target| format!("{{name: @{target} kind: @lib root: \"src/{target}\"}}"))
        .collect::<Vec<_>>()
        .join(" ");
    let dependencies = dict(dependencies);
    format!(
        "{{format: @project.v2 package: {{name: \"{name}\" version: \"1.0.0\"}} targets: [{targets}] dependencies: {dependencies}}}"
    )
}

const AMOUNT: &str = "fn cents\n  takes {value: I32}\n  returns result: I32\n  visibility @public\n  do\n    return value\nendfn\n\nfn secret\n  takes {value: I32}\n  returns result: I32\n  do\n    return value\nendfn\n";

const USES_MONEY: &str = "use @money.amount enduse\n\nfn total\n  takes {value: I32}\n  returns result: I32\n  do\n    return amount.cents(value)\nendfn\n";

/// A path dependency `"money"` on `lib/money`, with `main` as the root's
/// module and `extra` beside them.
fn money_tree(label: &str, main: &str, extra: &[(&str, &str)]) -> TempDir {
    let money = package("money", &["money"], "");
    let mut files = vec![
        ("src/app/main.vib", main),
        ("lib/money/project.vibon", money.as_str()),
        ("lib/money/src/money/amount.vib", AMOUNT),
    ];
    files.extend_from_slice(extra);
    TempDir::project(
        label,
        &app("\"money\": {kind: @path path: \"lib/money\"}"),
        &files,
    )
}

fn check(root: &Path) -> WorkspaceCheck {
    let snapshot = capture(root).expect("the project captures");
    check_snapshot(&snapshot, None)
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<(DiagnosticCode, Option<&str>)> {
    diagnostics
        .iter()
        .map(|diagnostic| (diagnostic.code(), diagnostic.source_id()))
        .collect()
}

fn span_of(text: &str, needle: &str) -> ByteSpan {
    let start = text.find(needle).expect("the needle occurs");
    ByteSpan::new(start, start + needle.len())
}

fn errors(check: &WorkspaceCheck) -> Vec<(DiagnosticCode, Option<&str>)> {
    codes(check.diagnostics())
        .into_iter()
        .filter(|(code, _)| code.as_atom() != "@contract.unusedEffect")
        .collect()
}

/// `V2-PROJECT-path-dependency-no-lock`: accepted, and a `project-lock.vibon`
/// that is not even a document is never read (Q23).
#[test]
fn a_path_dependency_binds_a_library_without_a_lock() {
    let tree = money_tree(
        "dep-no-lock",
        USES_MONEY,
        &[("project-lock.vibon", "not a lock")],
    );
    let result = check(tree.path());
    assert!(!result.has_errors(), "{:?}", result.diagnostics());
    let snapshot = capture(tree.path()).expect("captured");
    let unit = snapshot
        .units()
        .iter()
        .find(|unit| unit.name() == "@money")
        .expect("the dependency's unit");
    assert_eq!(unit.kind(), vibra_workspace::UnitKind::Dependency);
    assert_eq!(
        unit.package().map(vibra_workspace::Package::name),
        Some("money")
    );
    assert_eq!(
        unit.modules()
            .iter()
            .map(|module| (module.path(), module.source_id()))
            .collect::<Vec<_>>(),
        [("@money.amount", "lib/money/src/money/amount.vib")]
    );
}

/// 04 "Packages, units, and targets": a package's name and version are part
/// of every declaration's identity, so the dependency's declarations carry
/// `money`, not the root package.
#[test]
fn a_dependency_s_declarations_carry_its_package() {
    let tree = money_tree("dep-identity", USES_MONEY, &[]);
    let snapshot = capture(tree.path()).expect("captured");
    let input = resolve_input(&snapshot).expect("the library loads");
    let resolution = vibra_resolve::resolve(&input);
    let cents = resolution
        .declarations()
        .iter()
        .find(|declaration| declaration.anchor().name() == "cents")
        .expect("the dependency's function");
    assert_eq!(
        (
            cents.anchor().package(),
            cents.anchor().version(),
            cents.anchor().module()
        ),
        ("money", "1.0.0", "@money.amount")
    );
    let total = resolution
        .declarations()
        .iter()
        .find(|declaration| declaration.anchor().name() == "total")
        .expect("the root's function");
    assert_eq!(total.anchor().package(), "app");
}

/// `V2-PROJECT-path-dependency-from-test`.
#[test]
fn a_test_module_imports_a_dependency() {
    let test = "use @std.assert enduse\n\nuse @money.amount enduse\n\ntest \"keeps cents\" do assert.equal(5i32 amount.cents(5i32)) endtest\n";
    let tree = money_tree("dep-test", USES_MONEY, &[("tests/money.vib", test)]);
    let snapshot = capture(tree.path()).expect("captured");
    let run =
        run_tests(&snapshot, None, MemoryBudget::DEFAULT).expect("a valid selection");
    let outcomes = run
        .items()
        .iter()
        .map(|item| item.outcome().clone())
        .collect::<Vec<_>>();
    assert_eq!(outcomes, [TestOutcome::Passed]);
}

/// `V2-PROJECT-path-dependency-private`, by import and by a call through the
/// module.
#[test]
fn a_dependency_s_private_declarations_stay_private() {
    let by_import = "use @money.amount.secret enduse\n";
    let tree = money_tree("dep-private", by_import, &[]);
    assert_eq!(
        errors(&check(tree.path())),
        [(DiagnosticCode::NamePrivateAccess, Some("src/app/main.vib"))]
    );
    let by_call = USES_MONEY.replace("amount.cents", "amount.secret");
    let tree = money_tree("dep-private-call", &by_call, &[]);
    assert_eq!(
        errors(&check(tree.path())).first().map(|(code, _)| *code),
        Some(DiagnosticCode::NamePrivateAccess)
    );
}

/// `V2-PROJECT-path-dependency-undeclared`: the unit exists in no graph and
/// is not declared.
#[test]
fn an_undeclared_unit_is_an_unknown_path() {
    let tree = TempDir::project(
        "dep-undeclared",
        &app(""),
        &[("src/app/main.vib", USES_MONEY)],
    );
    assert_eq!(
        errors(&check(tree.path())),
        [(DiagnosticCode::ModuleUnknownPath, Some("src/app/main.vib"))]
    );
}

/// `V2-PROJECT-path-dependency-escape`, lexically and through a link.
#[test]
fn a_path_outside_the_workspace_root_escapes() {
    let document = app("\"money\": {kind: @path path: \"../money\"}");
    let tree = TempDir::project("dep-escape", &document, &[("src/app/main.vib", "")]);
    let diagnostics = capture(tree.path()).expect_err("refused");
    assert_eq!(
        codes(&diagnostics),
        [(DiagnosticCode::ModulePathEscape, Some("project.vibon"))]
    );
    assert_eq!(
        diagnostics[0].primary_span(),
        span_of(&document, "\"../money\"")
    );

    let outside = TempDir::project(
        "dep-escape-target",
        &package("money", &["money"], ""),
        &[("src/money/amount.vib", AMOUNT)],
    );
    let document = app("\"money\": {kind: @path path: \"lib/money\"}");
    let tree = TempDir::project(
        "dep-escape-link",
        &document,
        &[("src/app/main.vib", ""), ("lib/", "")],
    );
    crate::support::link_directory(outside.path(), &tree.join("lib/money"));
    if !tree.join("lib/money").exists() {
        println!("NOT RUN: this machine cannot make a directory link");
        return;
    }
    let diagnostics = capture(tree.path()).expect_err("refused");
    assert_eq!(
        codes(&diagnostics),
        [(DiagnosticCode::ModulePathEscape, Some("project.vibon"))]
    );
}

/// `V2-PROJECT-path-dependency-missing`, at the `path` value (ruling 6), for
/// a directory that is absent and one that holds no `project.vibon`.
#[test]
fn a_dependency_without_a_project_file_is_not_found() {
    for (label, extra) in [
        ("dep-missing", vec![]),
        ("dep-empty", vec![("lib/nope/", "")]),
    ] {
        let document = app("\"money\": {kind: @path path: \"lib/nope\"}");
        let mut files = vec![("src/app/main.vib", "")];
        files.extend(extra);
        let tree = TempDir::project(label, &document, &files);
        let diagnostics = capture(tree.path()).expect_err("refused");
        assert_eq!(
            codes(&diagnostics),
            [(DiagnosticCode::ProjectNotFound, Some("project.vibon"))]
        );
        assert_eq!(
            diagnostics[0].primary_span(),
            span_of(&document, "\"lib/nope\"")
        );
    }
}

/// `V2-PROJECT-dependency-target-binary` and a key that names no target.
#[test]
fn the_key_names_a_library_target_of_the_package() {
    let document = app("\"money\": {kind: @path path: \"lib/money\"}");
    let binary = "{format: @project.v2 package: {name: \"money\" version: \"1.0.0\"} targets: [{name: @money kind: @bin root: \"src/money\" entry: @money.main.main effects: [] grants: [:]}] dependencies: [:]}";
    let tree = TempDir::project(
        "dep-binary",
        &document,
        &[
            ("src/app/main.vib", ""),
            ("lib/money/project.vibon", binary),
            ("lib/money/src/money/main.vib", ""),
        ],
    );
    let diagnostics = capture(tree.path()).expect_err("refused");
    assert_eq!(
        codes(&diagnostics),
        [(DiagnosticCode::NameWrongEntityKind, Some("project.vibon"))]
    );
    assert_eq!(
        diagnostics[0].primary_span(),
        span_of(&document, "\"money\"")
    );

    let other = package("money", &["cash"], "");
    let tree = TempDir::project(
        "dep-no-target",
        &document,
        &[
            ("src/app/main.vib", ""),
            ("lib/money/project.vibon", &other),
            ("lib/money/src/cash/a.vib", ""),
        ],
    );
    let diagnostics = capture(tree.path()).expect_err("refused");
    assert_eq!(
        codes(&diagnostics),
        [(DiagnosticCode::NameUnknownSymbol, Some("project.vibon"))]
    );
    assert_eq!(
        diagnostics[0].primary_span(),
        span_of(&document, "\"money\"")
    );
}

/// `V2-PROJECT-dependency-target-named`: of two library targets, the key
/// binds the one it names.
#[test]
fn the_key_selects_one_of_several_libraries() {
    let money = package("money", &["money", "amounts"], "");
    let tree = TempDir::project(
        "dep-two-libs",
        &app("\"amounts\": {kind: @path path: \"lib/money\"}"),
        &[
            ("src/app/main.vib", "use @amounts.total enduse\n"),
            ("lib/money/project.vibon", &money),
            ("lib/money/src/money/amount.vib", AMOUNT),
            ("lib/money/src/amounts/total.vib", AMOUNT),
        ],
    );
    let result = check(tree.path());
    assert!(!result.has_errors(), "{:?}", result.diagnostics());
}

/// A dependency's own modules name its unit by its target name, the one name
/// it has in the whole snapshot.
#[test]
fn a_dependency_imports_its_own_modules() {
    let inner = "use @money.amount enduse\n\nfn twice\n  takes {value: I32}\n  returns result: I32\n  visibility @public\n  do\n    return amount.cents(value)\nendfn\n";
    let tree = money_tree(
        "dep-self-import",
        "use @money.twice enduse\n",
        &[("lib/money/src/money/twice.vib", inner)],
    );
    let result = check(tree.path());
    assert!(!result.has_errors(), "{:?}", result.diagnostics());
}

/// `units`, nested in `money` so that its path is relative to `money`'s
/// directory.
fn transitive(label: &str, main: &str) -> TempDir {
    let money = package(
        "money",
        &["money"],
        "\"units\": {kind: @path path: \"lib/units\"}",
    );
    let units = package("units", &["units"], "");
    let amount = "use @units.scale enduse\n\nfn cents\n  takes {value: I32}\n  returns result: I32\n  visibility @public\n  do\n    return scale.one(value)\nendfn\n";
    let scale = "fn one\n  takes {value: I32}\n  returns result: I32\n  visibility @public\n  do\n    return value\nendfn\n";
    TempDir::project(
        label,
        &app("\"money\": {kind: @path path: \"lib/money\"}"),
        &[
            ("src/app/main.vib", main),
            ("lib/money/project.vibon", &money),
            ("lib/money/src/money/amount.vib", amount),
            ("lib/money/lib/units/project.vibon", &units),
            ("lib/money/lib/units/src/units/scale.vib", scale),
        ],
    )
}

/// `V2-PROJECT-dependency-transitive` and
/// `V2-PROJECT-dependency-transitive-not-visible`.
#[test]
fn a_transitive_dependency_is_loaded_and_not_visible() {
    let result = check(transitive("dep-transitive", USES_MONEY).path());
    assert!(!result.has_errors(), "{:?}", result.diagnostics());
    let result =
        check(transitive("dep-not-visible", "use @units.scale enduse\n").path());
    assert_eq!(
        errors(&result),
        [(DiagnosticCode::ModuleUnknownPath, Some("src/app/main.vib"))]
    );
}

/// `V2-PROJECT-dependency-diamond`: `a` and `b` both reach `units`, which is
/// one unit.
#[test]
fn a_diamond_has_one_shared_unit() {
    let a = package("a", &["a"], "\"units\": {kind: @path path: \"../units\"}");
    let b = package("b", &["b"], "\"units\": {kind: @path path: \"../units\"}");
    let units = package("units", &["units"], "");
    let tree = TempDir::project(
        "dep-diamond",
        &app(
            "\"a\": {kind: @path path: \"lib/a\"} \"b\": {kind: @path path: \"lib/b\"}",
        ),
        &[
            (
                "src/app/main.vib",
                "use @a.x enduse\n\nuse @b.x as y enduse\n",
            ),
            ("lib/a/project.vibon", &a),
            ("lib/a/src/a/x.vib", "use @units.u enduse\n"),
            ("lib/b/project.vibon", &b),
            ("lib/b/src/b/x.vib", "use @units.u enduse\n"),
            ("lib/units/project.vibon", &units),
            ("lib/units/src/units/u.vib", ""),
        ],
    );
    let result = check(tree.path());
    assert!(!result.has_errors(), "{:?}", result.diagnostics());
    let snapshot = capture(tree.path()).expect("captured");
    assert_eq!(
        snapshot
            .units()
            .iter()
            .filter(|unit| unit.name() == "@units")
            .count(),
        1
    );
}

/// `V2-PROJECT-dependency-cycle`: `money` depends on the root package, the
/// later entry in discovery order, related along the cycle; and a package
/// that depends on itself, with no related span.
#[test]
fn a_dependency_cycle_is_reported_at_the_later_entry() {
    let document = app("\"money\": {kind: @path path: \"lib/money\"}");
    let money = package(
        "money",
        &["money"],
        "\"app\": {kind: @path path: \"../..\"}",
    );
    let tree = TempDir::project(
        "dep-cycle",
        &document,
        &[
            ("src/app/main.vib", ""),
            ("lib/money/project.vibon", &money),
            ("lib/money/src/money/a.vib", ""),
        ],
    );
    let diagnostics = capture(tree.path()).expect_err("refused");
    assert_eq!(
        codes(&diagnostics),
        [(
            DiagnosticCode::ProjectDependencyCycle,
            Some("lib/money/project.vibon")
        )]
    );
    let entry = span_of(&money, "\"app\": {kind: @path path: \"../..\"}");
    assert_eq!(diagnostics[0].primary_span(), entry);
    let related = diagnostics[0]
        .related()
        .iter()
        .map(|related| (related.source_id.as_deref(), related.span))
        .collect::<Vec<_>>();
    assert_eq!(
        related,
        [(
            Some("project.vibon"),
            span_of(&document, "\"money\": {kind: @path path: \"lib/money\"}")
        )]
    );

    let document = app("\"app\": {kind: @path path: \".\"}");
    let tree = TempDir::project("dep-self", &document, &[("src/app/main.vib", "")]);
    let diagnostics = capture(tree.path()).expect_err("refused");
    let found = codes(&diagnostics);
    assert!(
        found.contains(&(DiagnosticCode::ProjectDuplicateName, Some("project.vibon"))),
        "{found:?}"
    );
    let document = app("\"loop\": {kind: @path path: \".\"}");
    let tree =
        TempDir::project("dep-self-other", &document, &[("src/app/main.vib", "")]);
    let diagnostics = capture(tree.path()).expect_err("refused");
    assert_eq!(
        codes(&diagnostics),
        [(
            DiagnosticCode::ProjectDependencyCycle,
            Some("project.vibon")
        )]
    );
    assert!(diagnostics[0].related().is_empty());
}

/// Ruling of 2026-11-25: names are unique in the whole snapshot. Two
/// packages that each have a library `units`, and a transitive dependency
/// named like a root target, are `@project.duplicateName` at the later key,
/// related to the earlier name.
#[test]
fn unit_names_are_unique_in_the_whole_snapshot() {
    let a = package("a", &["a"], "\"units\": {kind: @path path: \"lib/units\"}");
    let b = package("b", &["b"], "\"units\": {kind: @path path: \"lib/units\"}");
    let units = package("units", &["units"], "");
    let tree = TempDir::project(
        "dep-two-units",
        &app(
            "\"a\": {kind: @path path: \"lib/a\"} \"b\": {kind: @path path: \"lib/b\"}",
        ),
        &[
            ("src/app/main.vib", ""),
            ("lib/a/project.vibon", &a),
            ("lib/a/src/a/x.vib", ""),
            ("lib/a/lib/units/project.vibon", &units),
            ("lib/a/lib/units/src/units/u.vib", ""),
            ("lib/b/project.vibon", &b),
            ("lib/b/src/b/x.vib", ""),
            ("lib/b/lib/units/project.vibon", &units),
            ("lib/b/lib/units/src/units/u.vib", ""),
        ],
    );
    let diagnostics = capture(tree.path()).expect_err("refused");
    assert_eq!(
        codes(&diagnostics),
        [(
            DiagnosticCode::ProjectDuplicateName,
            Some("lib/b/project.vibon")
        )]
    );
    assert_eq!(diagnostics[0].primary_span(), span_of(&b, "\"units\""));
    assert_eq!(
        diagnostics[0].related()[0].source_id.as_deref(),
        Some("lib/a/project.vibon")
    );

    let money = package(
        "money",
        &["money"],
        "\"app\": {kind: @path path: \"lib/app\"}",
    );
    let other = package("other", &["app"], "");
    let document = app("\"money\": {kind: @path path: \"lib/money\"}");
    let tree = TempDir::project(
        "dep-named-root",
        &document,
        &[
            ("src/app/main.vib", ""),
            ("lib/money/project.vibon", &money),
            ("lib/money/src/money/a.vib", ""),
            ("lib/money/lib/app/project.vibon", &other),
            ("lib/money/lib/app/src/app/a.vib", ""),
        ],
    );
    let diagnostics = capture(tree.path()).expect_err("refused");
    assert_eq!(
        codes(&diagnostics),
        [(
            DiagnosticCode::ProjectDuplicateName,
            Some("lib/money/project.vibon")
        )]
    );
    assert_eq!(
        diagnostics[0]
            .related()
            .iter()
            .map(|related| (related.source_id.as_deref(), related.span))
            .collect::<Vec<_>>(),
        [(Some("project.vibon"), span_of(&document, "@app"))]
    );
}

/// `V2-PROJECT-dependency-name-collision` and
/// `V2-PROJECT-dependency-named-std`, in the root project file and in a
/// dependency's.
#[test]
fn a_dependency_key_is_checked_in_its_own_project_file() {
    let document = app("\"app\": {kind: @path path: \"lib/app\"}");
    let tree =
        TempDir::project("dep-collision", &document, &[("src/app/main.vib", "")]);
    let diagnostics = capture(tree.path()).expect_err("refused");
    assert_eq!(
        codes(&diagnostics),
        [(DiagnosticCode::ProjectDuplicateName, Some("project.vibon"))]
    );
    let key = document.rfind("\"app\"").expect("the key");
    assert_eq!(diagnostics[0].primary_span(), ByteSpan::new(key, key + 5));

    let money = package(
        "money",
        &["money"],
        "\"std\": {kind: @path path: \"lib/std\"}",
    );
    let tree = TempDir::project(
        "dep-named-std",
        &app("\"money\": {kind: @path path: \"lib/money\"}"),
        &[
            ("src/app/main.vib", ""),
            ("lib/money/project.vibon", &money),
            ("lib/money/src/money/a.vib", ""),
        ],
    );
    let diagnostics = capture(tree.path()).expect_err("refused");
    assert_eq!(
        codes(&diagnostics),
        [(
            DiagnosticCode::ProjectReservedUnitName,
            Some("lib/money/project.vibon")
        )]
    );
}

/// `V2-PROJECT-git-dependency-unavailable`, in the root and in a dependency.
#[test]
fn a_git_dependency_is_unavailable_at_its_entry() {
    let entry = "\"money\": {kind: @git git: \"https://example.com/money.git\" rev: \"0123456789abcdef0123456789abcdef01234567\"}";
    let document = app(entry);
    let tree = TempDir::project("dep-git", &document, &[("src/app/main.vib", "")]);
    let result = check(tree.path());
    assert!(result.only_unavailable(), "{:?}", result.diagnostics());
    assert_eq!(
        errors(&result),
        [(DiagnosticCode::ToolUnavailable, Some("project.vibon"))]
    );
    assert_eq!(
        result.diagnostics()[0].primary_span(),
        span_of(&document, entry)
    );

    let money = package("money", &["money"], &entry.replace("money", "cash"));
    let tree = TempDir::project(
        "dep-git-inner",
        &app("\"money\": {kind: @path path: \"lib/money\"}"),
        &[
            ("src/app/main.vib", ""),
            ("lib/money/project.vibon", &money),
            ("lib/money/src/money/a.vib", ""),
        ],
    );
    let result = check(tree.path());
    assert_eq!(
        errors(&result),
        [(
            DiagnosticCode::ToolUnavailable,
            Some("lib/money/project.vibon")
        )]
    );
}

fn revision(snapshot: &SourceSnapshot) -> String {
    workspace_revision(&snapshot.revision_records()).expect("a revision")
}

/// `V2-PROJECT-dependency-revision`: one byte of a dependency's module
/// changes the workspace revision, and so does its project file, which is
/// one `dependency` record (ruling 1).
#[test]
fn a_dependency_is_fingerprinted_in_the_snapshot() {
    let tree = money_tree("dep-revision", USES_MONEY, &[]);
    let first = capture(tree.path()).expect("captured");
    let records = first.revision_records();
    let kinds = records
        .iter()
        .map(|record| (record.kind().as_str(), record.source_id()))
        .collect::<Vec<_>>();
    assert!(
        kinds.contains(&("dependency", "lib/money/project.vibon")),
        "{kinds:?}"
    );
    assert!(
        kinds.contains(&("source", "lib/money/src/money/amount.vib")),
        "{kinds:?}"
    );
    tree.write(
        "lib/money/src/money/amount.vib",
        &AMOUNT.replace("cents", "centz"),
    );
    let second = capture(tree.path()).expect("captured");
    assert_ne!(revision(&first), revision(&second));
    tree.write("lib/money/src/money/amount.vib", AMOUNT);
    assert_eq!(
        revision(&first),
        revision(&capture(tree.path()).expect("captured"))
    );
    tree.write(
        "lib/money/project.vibon",
        &package("money", &["money"], "").replace("1.0.0", "1.0.1"),
    );
    assert_ne!(
        revision(&first),
        revision(&capture(tree.path()).expect("captured"))
    );
}

/// `V2-PROJECT-dependency-operation-interface`: a `proc` of the root
/// performs an operation interface the dependency declares; the root is the
/// dependency's declaration.
#[test]
fn a_root_may_be_a_dependency_s_operation_interface() {
    let log = "interface Log\n  visibility @public\n  proc append\n    takes {log: Self entry: Str}\n    returns done: Void\n  endproc\nendinterface\n";
    let main = "use @money.log.Log enduse\n\nproc note(L) takes {sink: L} returns done: Void given {L: Log} performs {Log} do\n  return await Log.append(sink \"x\")\nendproc\n";
    let tree = money_tree(
        "dep-operation",
        main,
        &[("lib/money/src/money/log.vib", log)],
    );
    let result = check(tree.path());
    assert!(!result.has_errors(), "{:?}", result.diagnostics());
}

/// Completion evidence: a chain of 1,000 packages loads, with no host-stack
/// recursion proportional to its length, on a 256 KiB thread.
#[test]
fn a_chain_of_a_thousand_packages_loads_on_a_small_stack() {
    const LENGTH: usize = 1_000;
    let tree = TempDir::project(
        "dep-chain",
        &app("\"p0\": {kind: @path path: \"lib/p0\"}"),
        &[("src/app/main.vib", "")],
    );
    for index in 0..LENGTH {
        let next = if index + 1 < LENGTH {
            format!(
                "\"p{}\": {{kind: @path path: \"../p{}\"}}",
                index + 1,
                index + 1
            )
        } else {
            String::new()
        };
        let name = format!("p{index}");
        tree.write(
            &format!("lib/{name}/project.vibon"),
            &package(&name, &[&name], &next),
        );
        tree.write(&format!("lib/{name}/src/{name}/m.vib"), "");
    }
    let root = tree.path().to_path_buf();
    let units = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(move || capture(&root).map(|snapshot| snapshot.units().len()))
        .expect("a thread")
        .join()
        .expect("no overflow");
    assert_eq!(units.expect("captured"), LENGTH + 2);
}

/// 04 "Packages, units, and targets" and 05 `check`: a binary target names
/// the library targets of its own package, and checking the program alone
/// covers what it imports, so a fault in its own library is reported there,
/// as a fault in a dependency is. A library module the program does not
/// import is outside that check.
#[test]
fn checking_a_program_reports_a_fault_of_its_own_library() {
    let document = "{format: @project.v2 package: {name: \"hello\" version: \"0.1.0\"} targets: [{name: @greeting kind: @lib root: \"src/greeting\"} {name: @hello kind: @bin root: \"src/hello\" entry: @hello.main.main effects: [@std.io.StdoutText] grants: [@out: {provider: @host.io.stdout resource: {kind: @processStream} after: []}]}] dependencies: [:]}";
    let main = "use @std.io.StdoutText enduse\n\nuse @std.process.ExitCode enduse\n\nuse @greeting.text enduse\n\nproc main\n  takes {out: StdoutText}\n  returns code: ExitCode\n  performs {StdoutText}\n  do\n    let _ be await StdoutText.write(out text.hello()) endlet\n    return ExitCode(0u8)\nendproc\n";
    let faulty = "fn hello\n  returns text: Str\n  visibility @public\n  do\n    return 1i32\nendfn\n";
    let tree = TempDir::project(
        "own-library-fault",
        document,
        &[
            ("src/hello/main.vib", main),
            ("src/greeting/text.vib", faulty),
            ("src/greeting/unused.vib", faulty),
        ],
    );
    let snapshot = capture(tree.path()).expect("captured");
    let program = check_snapshot(&snapshot, Some("@hello"));
    let sources = program
        .diagnostics()
        .iter()
        .filter(|diagnostic| diagnostic.code() == DiagnosticCode::TypeMismatch)
        .map(Diagnostic::source_id)
        .collect::<Vec<_>>();
    assert_eq!(
        sources,
        [Some("src/greeting/text.vib")],
        "{:?}",
        program.diagnostics()
    );
}

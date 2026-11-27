//! Unit tests of `test` declarations and assertions. Expectations come from
//! the programs chapter, "Tests" and "Assertions", and the source chapter,
//! "`test`".

use std::sync::Arc;

use vibra_diagnostics::DiagnosticCode;
use vibra_interp::{FailureKind, MemoryBudget, TestExecution, run_test};
use vibra_ir::CheckedProgram;

use super::{at, faults_of, workspace};
use crate::check::{CheckResult, CheckedTest};

const MATH: &str = "fn add\n  takes {left: I32 right: I32}\n  returns sum: I32\n  \
                    visibility @public\n  do\n    return I32.add(left right)\nendfn\n";

/// Checks the library module `@app.math` with `app` and the test modules
/// `tests`, each a module path and its source, covering `units`.
fn check(
    app: &str,
    tests: &[(&str, &str)],
    units: &[&str],
) -> Result<CheckResult, String> {
    workspace(&[("@app.math", app)], tests, units)
}

/// Runs the lowered test `name`.
fn run(result: &CheckResult, name: &str) -> Result<TestExecution, String> {
    let set = result
        .test_set()
        .ok_or_else(|| format!("no test set: {:#?}", result.diagnostics()))?;
    let function = result
        .tests()
        .iter()
        .find(|test| test.name() == name)
        .and_then(CheckedTest::function)
        .ok_or_else(|| format!("`{name}` was not lowered"))?;
    let program = CheckedProgram::for_entry(Arc::clone(set), function)
        .map_err(|error| error.to_string())?;
    run_test(&program, None, None, MemoryBudget::DEFAULT)
        .map_err(|error| error.to_string())
}

#[test]
fn a_test_lowers_to_a_function_the_interpreter_runs() -> Result<(), String> {
    let tests = "use @std.assert enduse\n\nuse @app.math enduse\n\n\
                 test \"adds\" do assert.equal(3i32 math.add(1i32 2i32)) endtest\n\n\
                 test \"fails\" do\n  assert.equal(4i32 math.add(1i32 2i32))\n  \
                 assert.true(false)\nendtest\n";
    let result = check(MATH, &[("@tests.math", tests)], &["@tests"])?;
    assert!(!result.has_errors(), "{:#?}", result.diagnostics());
    let names = result
        .tests()
        .iter()
        .map(|test| (test.module(), test.name()))
        .collect::<Vec<_>>();
    assert_eq!(names, [("@tests.math", "adds"), ("@tests.math", "fails")]);
    let passed = run(&result, "adds")?;
    assert_eq!(passed.failure, None);
    let failure = run(&result, "fails")?
        .failure
        .ok_or("the first assertion fails")?;
    assert_eq!(failure.kind, FailureKind::Assertion);
    assert_eq!(failure.assertion, "assert.equal");
    assert_eq!(failure.expected.as_deref(), Some("4i32"));
    assert_eq!(failure.actual.as_deref(), Some("3i32"));
    assert_eq!(
        failure.origin.span(),
        at(tests, "assert.equal(4i32 math.add(1i32 2i32))", 0)
    );
    Ok(())
}

#[test]
fn a_test_outside_tests_is_reported_at_the_declaration() -> Result<(), String> {
    let app = format!("{MATH}\ntest \"stray\" do endtest\n");
    let result = check(&app, &[], &["@app"])?;
    assert_eq!(
        faults_of(&result),
        [(
            DiagnosticCode::ModuleTestOutsideTests,
            at(&app, "test \"stray\" do endtest", 0)
        )]
    );
    Ok(())
}

#[test]
fn a_test_module_imports_the_assertions_under_any_name() -> Result<(), String> {
    let missing = "test \"first\" do endtest\n\ntest \"second\" do endtest\n";
    let result = check(MATH, &[("@tests.math", missing)], &["@tests"])?;
    assert_eq!(
        faults_of(&result),
        [(
            DiagnosticCode::ModuleMissingRequiredImport,
            at(missing, "\"first\"", 0)
        )]
    );
    assert!(result.tests().is_empty());
    let renamed =
        "use @std.assert as check enduse\n\ntest \"x\" do check.true(true) endtest\n";
    let result = check(MATH, &[("@tests.math", renamed)], &["@tests"])?;
    assert!(!result.has_errors(), "{:#?}", result.diagnostics());
    assert_eq!(run(&result, "x")?.failure, None);
    Ok(())
}

#[test]
fn a_test_reaching_an_anonymous_proc_through_the_library_runs() -> Result<(), String> {
    // A library function that builds an anonymous `proc` lowers, so the test
    // that reaches it runs beside the others.
    let app = format!(
        "{MATH}\nfn viaClosure\n  takes {{}}\n  returns r: I32\n  visibility @public\n  \
         do\n    let f be proc takes {{x: I32}} returns s: I32 \
         do return x endproc endlet\n    \
         return 1i32\nendfn\n"
    );
    let tests = "use @std.assert enduse\n\nuse @app.math enduse\n\n\
                 test \"closure\" do assert.equal(1i32 math.viaClosure()) endtest\n\n\
                 test \"plain\" do assert.equal(3i32 math.add(1i32 2i32)) endtest\n";
    let result = check(&app, &[("@tests.math", tests)], &["@tests"])?;
    assert_eq!(faults_of(&result), []);
    let [closure, plain] = result.tests() else {
        return Err(format!("two tests: {:#?}", result.tests()));
    };
    assert_eq!(closure.unavailable(), None);
    assert_eq!(plain.unavailable(), None);
    assert_eq!(run(&result, "closure")?.failure, None);
    assert_eq!(run(&result, "plain")?.failure, None);
    Ok(())
}
#[test]
fn a_test_takes_one_test_world_under_any_name() -> Result<(), String> {
    let tests = "use @std.assert enduse\n\n\
                 use @std.test.TestWorld enduse\n\n\
                 test \"world\" takes {sandbox: TestWorld} do endtest\n\n\
                 test \"plain\" do assert.true(true) endtest\n";
    let result = check(MATH, &[("@tests.math", tests)], &["@tests"])?;
    assert_eq!(faults_of(&result), []);
    let names = result
        .tests()
        .iter()
        .map(|test| (test.name(), test.function().is_some()))
        .collect::<Vec<_>>();
    assert_eq!(names, [("world", true), ("plain", true)]);
    Ok(())
}

#[test]
fn a_test_that_takes_another_type_is_an_invalid_form_at_the_section()
-> Result<(), String> {
    let tests = "use @std.assert enduse\n\n\
                 type World of I32 endtype\n\n\
                 test \"world\" takes {world: World} do endtest\n\n\
                 test \"plain\" do assert.true(true) endtest\n";
    let result = check(MATH, &[("@tests.math", tests)], &["@tests"])?;
    assert_eq!(
        faults_of(&result),
        [(
            DiagnosticCode::SyntaxInvalidForm,
            at(tests, "takes {world: World}", 0)
        )]
    );
    // An error makes every test invalid: none is lowered.
    assert!(result.tests().iter().all(|test| test.function().is_none()));
    Ok(())
}

/// A test's row names operation interfaces (`docs/spec/03-effects.md`,
/// "Effect ceilings"): one that names a plain type is
/// `@effect.invalidReference` at the entry, an error, so no test runs.
#[test]
fn a_test_row_that_names_a_type_is_an_invalid_reference() -> Result<(), String> {
    let tests = "use @std.assert enduse\n\n\
                 type Missing of I32 endtype\n\n\
                 test \"effects\" performs {Missing} do endtest\n\n\
                 test \"plain\" do assert.true(true) endtest\n";
    let result = check(MATH, &[("@tests.math", tests)], &["@tests"])?;
    assert_eq!(
        faults_of(&result),
        [(
            DiagnosticCode::EffectInvalidReference,
            at(tests, "Missing", 1)
        )]
    );
    assert!(result.tests().is_empty());
    Ok(())
}

#[test]
fn a_module_path_covers_that_test_module_only() -> Result<(), String> {
    let a = "use @std.assert enduse\n\ntest \"a\" do assert.true(true) endtest\n";
    let b = "use @std.assert enduse\n\ntest \"b\" do assert.true(1i32) endtest\n";
    let result = check(MATH, &[("@tests.a", a), ("@tests.b", b)], &["@tests.a"])?;
    assert!(!result.has_errors(), "{:#?}", result.diagnostics());
    let names = result
        .tests()
        .iter()
        .map(|test| test.name())
        .collect::<Vec<_>>();
    assert_eq!(names, ["a"]);
    Ok(())
}

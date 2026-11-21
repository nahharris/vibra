//! Unit tests of `test` declarations and assertions. Expectations come from
//! the programs chapter, "Tests" and "Assertions", and the source chapter,
//! "`test`".

use std::sync::Arc;

use vibra_diagnostics::{ByteSpan, DiagnosticCode};
use vibra_interp::{FailureKind, MemoryBudget, TestExecution, run_test};
use vibra_ir::CheckedProgram;
use vibra_resolve::{PackageId, ResolveInput, SourceModule, SourceUnit, UnitKind};

use super::at;
use crate::check::{CheckResult, CheckedTest, check_workspace};
use crate::stdlib::load_stdlib;

const MATH: &str = "fn add\n  takes {left: I32 right: I32}\n  returns sum: I32\n  \
                    visibility @public\n  do\n    return I32.add(left right)\nendfn\n";

/// Checks the library module `@app.math` with `app` and the test modules
/// `tests`, each a module path and its source, covering `units`.
fn check(
    app: &str,
    tests: &[(&str, &str)],
    units: &[&str],
) -> Result<CheckResult, String> {
    let stdlib = load_stdlib().map_err(|error| error.to_string())?;
    let test_modules = tests
        .iter()
        .map(|(path, source)| {
            let file = path.trim_start_matches("@tests.").replace('.', "/");
            SourceModule::new(
                *path,
                format!("tests/{file}.vib"),
                source.as_bytes().to_vec(),
            )
        })
        .collect();
    let input = ResolveInput::new(
        PackageId::new("hello", "0.1.0"),
        vec![
            SourceUnit::new(
                "@app",
                UnitKind::Lib,
                vec![SourceModule::new(
                    "@app.math",
                    "src/app/math.vib",
                    app.as_bytes().to_vec(),
                )],
            ),
            SourceUnit::new("@tests", UnitKind::Tests, test_modules),
        ],
        stdlib.overlay().embedded_unit(),
    );
    let units = units
        .iter()
        .map(|unit| (*unit).to_owned())
        .collect::<Vec<_>>();
    Ok(check_workspace(&input, &units))
}

/// The code and span of every diagnostic.
fn faults(result: &CheckResult) -> Vec<(DiagnosticCode, ByteSpan)> {
    result
        .diagnostics()
        .iter()
        .map(|diagnostic| (diagnostic.code(), diagnostic.primary_span()))
        .collect()
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
    run_test(&program, MemoryBudget::DEFAULT).map_err(|error| error.to_string())
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
        faults(&result),
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
        faults(&result),
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
fn an_assertion_outside_tests_is_reported_at_the_use() -> Result<(), String> {
    let called = format!(
        "use @std.assert enduse\n\n{MATH}\nfn check\n  takes {{}}\n  returns done: Void\n  \
         do\n    assert.true(true)\n    return void\nendfn\n"
    );
    let result = check(&called, &[], &["@app"])?;
    assert_eq!(
        faults(&result),
        [(
            DiagnosticCode::ModuleAssertionOutsideTests,
            at(&called, "assert.true", 0)
        )]
    );
    let value = format!(
        "use @std.assert enduse\n\n{MATH}\nfn check\n  takes {{}}\n  returns done: Void\n  \
         do\n    let f: Fn({{Bool}} Void) be assert.true endlet\n    return void\nendfn\n"
    );
    let result = check(&value, &[], &["@app"])?;
    assert_eq!(
        faults(&result),
        [(
            DiagnosticCode::ModuleAssertionOutsideTests,
            at(&value, "assert.true", 0)
        )]
    );
    Ok(())
}

#[test]
fn a_test_body_has_no_return_and_no_try() -> Result<(), String> {
    let returns = "use @std.assert enduse\n\ntest \"x\" do return void endtest\n";
    let result = check(MATH, &[("@tests.math", returns)], &["@tests"])?;
    assert_eq!(
        faults(&result),
        [(
            DiagnosticCode::TypeInvalidReturn,
            at(returns, "return void", 0)
        )]
    );
    let tries = "use @std.assert enduse\n\nfn f\n  takes {}\n  returns r: Option(I32)\n  \
                 do\n    return Option.some(value: 1i32)\nendfn\n\n\
                 test \"x\" do let v be try f() endlet endtest\n";
    let result = check(MATH, &[("@tests.math", tries)], &["@tests"])?;
    assert_eq!(
        faults(&result),
        [(DiagnosticCode::TypeInvalidTry, at(tries, "try f()", 0))]
    );
    Ok(())
}

#[test]
fn an_operand_of_assert_equal_that_holds_a_callable_is_not_equatable()
-> Result<(), String> {
    let source = "use @std.assert enduse\n\nuse @app.math enduse\n\n\
                  test \"x\" do assert.equal(math.add math.add) endtest\n";
    let result = check(MATH, &[("@tests.math", source)], &["@tests"])?;
    assert_eq!(
        faults(&result),
        [
            (
                DiagnosticCode::TypeFunctionNotEquatable,
                at(source, "math.add", 0)
            ),
            (
                DiagnosticCode::TypeFunctionNotEquatable,
                at(source, "math.add", 1)
            ),
        ]
    );
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
    assert_eq!(faults(&result), []);
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
fn a_test_that_takes_a_world_is_unavailable_over_the_test() -> Result<(), String> {
    let tests = "use @std.assert enduse\n\n\
                 type World of I32 endtype\n\n\
                 test \"world\" takes {world: World} do endtest\n\n\
                 test \"plain\" do assert.true(true) endtest\n";
    let result = check(MATH, &[("@tests.math", tests)], &["@tests"])?;
    assert_eq!(
        faults(&result),
        [(
            DiagnosticCode::ToolUnavailable,
            at(tests, "test \"world\" takes {world: World} do endtest", 0)
        )]
    );
    let names = result
        .tests()
        .iter()
        .map(|test| (test.name(), test.function().is_some()))
        .collect::<Vec<_>>();
    assert_eq!(names, [("world", false), ("plain", true)]);
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
        faults(&result),
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

#[test]
fn test_names_are_unique_in_a_module() -> Result<(), String> {
    let tests = "use @std.assert enduse\n\ntest \"adds\" do endtest\n\ntest \"adds\" do endtest\n";
    let result = check(MATH, &[("@tests.math", tests)], &["@tests"])?;
    assert_eq!(
        faults(&result),
        [(DiagnosticCode::NameRedeclaration, at(tests, "\"adds\"", 1))]
    );
    Ok(())
}

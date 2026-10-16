//! A native's checked body in typed IR (milestone 4 Step 8d): a native whose
//! meaning applies a function value, `array.fold`, has no import, so a module
//! runs the Vibra body that the standard library writes beside `native:`. The
//! checked function carries that one body beside the wrapper the interpreter
//! runs as the native, and the emitter lowers it as any function.
//!
//! The reference interpreter is the oracle. A program is checked from source,
//! run by both backends, and held to one result.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs
)]

use vibra_ir::{CheckedProgram, Type};
use vibra_types::check_source;
use vibra_wasm_run::{LiveSizes, MemoryLimit, Observed, Runner};

const MIB: usize = 1024 * 1024;

fn checked(source: &str) -> CheckedProgram {
    let result = check_source("input.vib", source);
    assert!(result.accepted(), "{:?}\n{source}", result.diagnostics());
    result.program().expect("an accepted program").clone()
}

fn entry_type(program: &CheckedProgram) -> Type {
    program.entry().signature().result()
}

fn oracle(program: &CheckedProgram) -> String {
    vibra_interp::run(program)
        .expect("the interpreter runs the program")
        .canonical_result()
}

fn run_with(runner: &Runner, program: &CheckedProgram) -> (String, LiveSizes) {
    let ty = entry_type(program);
    let module = vibra_wasm::emit(program)
        .unwrap_or_else(|error| panic!("the program lowers: {error}"));
    match runner
        .run_observed(module.bytes(), &ty, program.types())
        .expect("the module runs")
    {
        Observed::Completed { value, live } => (value.canonical_observation(&ty), live),
        Observed::Stopped(outcome) => panic!("the entry did not complete: {outcome:?}"),
    }
}

/// Runs `source` in both backends and asserts they agree.
fn agree(source: &str) -> (String, LiveSizes) {
    let program = checked(source);
    let expected = oracle(&program);
    let runner =
        Runner::new(MemoryLimit::new(64 * MIB)).expect("the engine configures");
    let (observed, live) = run_with(&runner, &program);
    assert_eq!(observed, expected, "the backends disagree on:\n{source}");
    (observed, live)
}

const SUM: &str = "(lambda (total i32 item i32) i32 (match (i32.add-checked total item) (result.ok sum) sum (result.err -) 0i32))";

#[test]
fn a_program_that_names_array_fold_lowers_and_a_module_returns_the_interpreters_result()
{
    let (result, _) = agree(&format!(
        "(defn main () i32 (array.fold (array.of 1i32 2i32 3i32) 10i32 {SUM}))"
    ));
    assert_eq!(result, "(record type: @i32 value: 16i32)\n");
}

/// Module functions every program below shares.
const PRELUDE: &str = "(defn add (a i32 b i32) i32 (match (i32.add-checked a b) (result.ok sum) sum (result.err -) 0i32))\n(defn mul (a i32 b i32) i32 (match (i32.mul-checked a b) (result.ok product) product (result.err -) 0i32))\n(defn lower (count i32) i32 (match (i32.sub-checked count 1i32) (result.ok value) value (result.err -) 0i32))\n";

fn with_prelude(body: &str) -> String {
    format!("{body}\n{PRELUDE}")
}

#[test]
fn a_fold_over_an_empty_one_and_many_elements_agrees_in_both_backends() {
    let empty = "(defn main () i32 (array.fold (as (array i32) (array.of)) 7i32 add))";
    let one = "(defn main () i32 (array.fold (array.of 5i32) 0i32 add))";
    let many = "(defn main () i32 (array.fold (array.of 1i32 2i32 3i32 4i32 5i32 6i32 7i32 8i32 9i32 10i32 11i32 12i32 13i32 14i32 15i32 16i32 17i32 18i32 19i32 20i32) 0i32 add))";
    // Subtraction does not commute, so the order of the fold is fixed too.
    let ordered = "(defn main () i32 (array.fold (array.of 1i32 2i32 3i32) 10i32 (lambda (total i32 item i32) i32 (match (i32.sub-checked total item) (result.ok d) d (result.err -) 0i32))))";
    for (body, expected) in [
        (empty, "(record type: @i32 value: 7i32)\n"),
        (one, "(record type: @i32 value: 5i32)\n"),
        (many, "(record type: @i32 value: 210i32)\n"),
        (ordered, "(record type: @i32 value: 4i32)\n"),
    ] {
        let (result, _) = agree(&with_prelude(body));
        assert_eq!(result, expected, "{body}");
    }
}

#[test]
fn a_closure_that_captures_and_a_module_function_step_agree() {
    let capture = "(defn main () i32 (do (let k 3i32) (array.fold (array.of 1i32 2i32 3i32) 0i32 (lambda (total i32 item i32) i32 (add total (mul item k))))))";
    let (result, _) = agree(&with_prelude(capture));
    assert_eq!(result, "(record type: @i32 value: 18i32)\n");
    let named = "(defn main () i32 (array.fold (array.of 4i32 5i32) 1i32 mul))";
    let (result, _) = agree(&with_prelude(named));
    assert_eq!(result, "(record type: @i32 value: 20i32)\n");
}

#[test]
fn an_accumulator_that_is_an_array_or_a_record_agrees() {
    let array = "(defn main () (array i32) (array.fold (array.of 1i32 2i32 3i32) (as (array i32) (array.of)) (lambda (acc (array i32) item i32) (array i32) (array.append acc (mul item 2i32)))))";
    agree(&with_prelude(array));
    let record = "(deftype totals (record sum i32 count i32))\n(defn main () totals (array.fold (array.of 1i32 2i32 3i32) (totals sum: 0i32 count: 0i32) (lambda (acc totals item i32) totals (totals sum: (add (acc @sum) item) count: (add (acc @count) 1i32)))))";
    agree(&with_prelude(record));
}

#[test]
fn a_fold_inside_a_tail_loop_and_a_fold_nested_in_a_fold_agree() {
    let looped = "(defn main () i32 (rounds 1000i32 0i32))\n(defn rounds (n i32 acc i32) i32 (if (i32.equal n 0i32) acc (rounds (lower n) (array.fold (array.of 1i32 2i32) acc add))))";
    let (result, _) = agree(&with_prelude(looped));
    assert_eq!(result, "(record type: @i32 value: 3000i32)\n");
    let nested = "(defn main () i32 (array.fold (array.of (array.of 1i32 2i32) (as (array i32) (array.of)) (array.of 3i32 4i32)) 0i32 (lambda (total i32 row (array i32)) i32 (array.fold row total add))))";
    let (result, _) = agree(&with_prelude(nested));
    assert_eq!(result, "(record type: @i32 value: 10i32)\n");
}

/// A fold over `count` elements of `1`.
fn long_fold(count: usize) -> String {
    let items = vec!["1i32"; count].join(" ");
    with_prelude(&format!(
        "(defn main () i32 (array.fold (array.of {items}) 0i32 add))"
    ))
}

#[test]
fn a_ten_thousand_element_fold_uses_no_engine_stack_and_leaves_the_baseline_live_size()
{
    let program = checked(&long_fold(10_000));
    let expected = oracle(&program);
    assert_eq!(expected, "(record type: @i32 value: 10000i32)\n");
    // The engine's stack is 64 KiB: one nested WebAssembly call per element
    // could not fit, so the fold's recursion is frames in the arena.
    let small = Runner::with_wasm_stack(MemoryLimit::new(64 * MIB), 64 * 1024)
        .expect("a runner with a small engine stack");
    let (observed, live) = run_with(&small, &program);
    assert_eq!(observed, expected);
    // The live size starts at zero and returns to what the same program holds
    // after a one-element fold: the module values and nothing else.
    let baseline = checked(&long_fold(1));
    let (_, base) = run_with(&small, &baseline);
    assert_eq!(live.start, 0);
    assert_eq!(base.start, 0);
    assert_eq!(live.end, base.end, "live size returns to its baseline");
    println!("fold of 10000: live {live:?}, baseline {base:?}");
}

#[test]
fn the_wrapper_carries_one_checked_body_beside_it() {
    let program = checked(&long_fold(3));
    let fold = program
        .functions()
        .iter()
        .find(|function| function.name() == "array.fold")
        .expect("array.fold is a function of the program");
    assert!(fold.is_external_wrapper());
    let body = fold.native_body().expect("the checked body");
    assert_ne!(body, fold.body(), "the body is not the wrapper");
    // The body names the members the standard library writes, which join the
    // program with it.
    for name in ["array.slice", "array.length"] {
        let member = program
            .functions()
            .iter()
            .find(|function| function.name() == name)
            .unwrap_or_else(|| panic!("{name} joins the program"));
        assert!(member.native_body().is_none());
    }
    // Only a native with no import carries a body.
    for function in program.functions() {
        assert_eq!(
            function.native_body().is_some(),
            function.name() == "array.fold",
            "{}",
            function.name()
        );
    }
    let canonical = program.canonical_vibon();
    assert!(canonical.contains("native-body:"), "{canonical}");
    assert_eq!(canonical, checked(&long_fold(3)).canonical_vibon());
}

#[test]
fn every_native_has_an_import_or_a_checked_body_and_never_both() {
    let stdlib = vibra_types::load_stdlib().expect("standard library");
    let mut bodies = Vec::new();
    for symbol in stdlib.native_symbols() {
        let row = vibra_ir::external::CompilerIntrinsic::from_symbol(symbol)
            .unwrap_or_else(|| panic!("`{symbol}` is not a registry row"));
        assert_ne!(
            row.native_import().is_some(),
            row.runs_body(),
            "`{symbol}` must have exactly one of an import and a body"
        );
        if row.runs_body() {
            bodies.push(symbol.as_str());
        }
    }
    assert_eq!(bodies, ["array.fold"]);
}

#[test]
fn the_workspace_checker_checks_the_same_body() {
    let project = "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.execute effects: (array))) dependencies: (dict))\n";
    let parent =
        std::fs::canonicalize(std::env::temp_dir()).expect("temporary directory");
    let root = parent.join(format!("vibra-native-body-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src/app")).expect("project directory");
    std::fs::write(root.join("project.vibon"), project).expect("project marker");
    std::fs::write(
        root.join("src/app/main.vib"),
        "(defn execute () void (do (array.fold (array.of 1i32 2i32) 0i32 (lambda (total i32 item i32) i32 item)) (do)))\n",
    )
    .expect("main module");
    let snapshot = vibra_workspace::WorkspaceSnapshot::load(&root).expect("snapshot");
    let target = snapshot
        .project()
        .project()
        .targets()
        .first()
        .expect("binary target");
    let run = vibra_workspace::semantic::run_target(&snapshot, target);
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(
        run.check().status(),
        vibra_workspace::semantic::CheckStatus::Accepted,
        "{:?}",
        run.check().diagnostics()
    );
    let program = run.check().program_for_target(target).expect("a program");
    let fold = program
        .functions()
        .iter()
        .find(|function| function.name().ends_with("array.fold"))
        .expect("array.fold is a function of the program");
    assert!(fold.native_body().is_some(), "the checked body");
    vibra_wasm::emit(program).expect("a module lowers the workspace program");
}

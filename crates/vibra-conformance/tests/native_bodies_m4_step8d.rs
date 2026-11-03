//! A native's checked body in typed IR (milestone 4 Step 8d): a native whose
//! meaning applies a function value, `array.fold`, has no import, so a module
//! runs the Vibra body that the standard library writes beside `native:`. The
//! checked function carries that one body beside the wrapper the interpreter
//! runs as the native, and the emitter lowers it as any function.
//!
//! What the corpus states, in both backends, is `V1-RUNTIME-array-fold`: the
//! results of folds over empty, short, and long arrays, with closures and module
//! functions, accumulating arrays and records, nested, in a tail loop, and as the
//! tail call of a loop. This file holds what a case cannot: the engine stack, the
//! tail mark in the checked IR, and the shape of the checked body.

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

fn run_with(runner: &Runner, program: &CheckedProgram) -> (String, LiveSizes) {
    let ty = entry_type(program);
    let module = vibra_wasm::emit(program)
        .unwrap_or_else(|error| panic!("the program lowers: {error}"));
    match runner
        .run_observed(module.bytes(), &ty, program.types())
        .expect("the module runs")
    {
        Observed::Completed { value, live, .. } => {
            (value.canonical_observation(&ty), live)
        }
        Observed::Stopped(outcome) => panic!("the entry did not complete: {outcome:?}"),
    }
}

/// Module functions every program below shares.
const PRELUDE: &str = "(defn add (a i32 b i32) i32 (match (i32.add-checked a b) (result.ok sum) sum (result.err -) 0i32))\n(defn lower (count i32) i32 (match (i32.sub-checked count 1i32) (result.ok value) value (result.err -) 0i32))\n";

fn with_prelude(body: &str) -> String {
    format!("{body}\n{PRELUDE}")
}

#[test]
fn the_call_of_array_fold_in_tail_position_is_a_tail_call() {
    // `spin` calls `array.fold` in tail position, and the fold's step calls
    // `spin` again: the call of a native that runs its body is marked by its
    // position like any other, in user code and in the body alike. The run of
    // this loop is in `V1-RUNTIME-array-fold`.
    let program = checked(&with_prelude(
        "(defn main () i32 (spin 2000i32))\n(defn spin (n i32) i32 (if (i32.equal n 0i32) 7i32 (array.fold (array.of n) 0i32 (lambda (total i32 k i32) i32 (spin (lower k))))))",
    ));
    let spin = program
        .functions()
        .iter()
        .find(|function| function.name() == "spin")
        .expect("spin");
    assert!(
        format!("{:?}", spin.body()).contains("tail: true"),
        "the call of array.fold in tail position is a tail call"
    );
}

/// A fold over `count` elements of `1`.
fn long_fold(count: usize) -> String {
    let items = vec!["1i32"; count].join(" ");
    with_prelude(&format!(
        "(defn main () i32 (array.fold (array.of {items}) 0i32 add))"
    ))
}

#[test]
fn the_engine_stack_a_long_fold_needs_does_not_depend_on_its_length() {
    // The smallest engine stack, in KiB, under which a fold runs: the same for a
    // fold a hundred times longer, because the fold's recursion is frames in the
    // arena and no element takes any engine stack.
    let smallest = |count: usize| {
        let program = checked(&long_fold(count));
        let module = vibra_wasm::emit(&program).expect("the program lowers");
        [8, 12, 16, 24, 32, 48, 64].into_iter().find(|kib| {
            let runner =
                Runner::with_wasm_stack(MemoryLimit::new(64 * MIB), kib * 1024)
                    .expect("a runner with a small engine stack");
            matches!(
                runner.run_observed(
                    module.bytes(),
                    &entry_type(&program),
                    program.types()
                ),
                Ok(Observed::Completed { .. })
            )
        })
    };
    let short = smallest(10).expect("some stack up to 64 KiB runs it");
    let long = smallest(1_000).expect("some stack up to 64 KiB runs it");
    assert_eq!(short, long, "the engine stack depends on the fold's length");

    // The live size starts at zero and returns to what the same program holds
    // after a one-element fold: the module values and nothing else.
    let small = Runner::with_wasm_stack(MemoryLimit::new(64 * MIB), long * 1024)
        .expect("a runner with a small engine stack");
    let (observed, live) = run_with(&small, &checked(&long_fold(1_000)));
    assert_eq!(observed, "(record type: @i32 value: 1000i32)\n");
    let (_, base) = run_with(&small, &checked(&long_fold(1)));
    assert_eq!(live.start, 0);
    assert_eq!(base.start, 0);
    assert_eq!(live.end, base.end, "live size returns to its baseline");
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

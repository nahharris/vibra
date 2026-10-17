//! Tests, assertions, and traps in the WebAssembly backend (milestone 4
//! Step 11).
//!
//! The reference interpreter is the oracle. A workspace of tests is checked and
//! run by the interpreter, then lowered to a test module and run test by test in
//! fresh instances; the two observations must be the same rows, with the same
//! outcomes, the same assertion operands, the same trap codes, and the same
//! origins.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs
)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use vibra_conformance::{TestRow, TestsObserved, observe_wasm_tests};
use vibra_ir::boundary::TrapCode;
use vibra_wasm_run::{MemoryLimit, Outcome, Runner, TestRun};
use vibra_workspace::WorkspaceSnapshot;
use vibra_workspace::semantic::TestItemStatus;

static NEXT: AtomicUsize = AtomicUsize::new(0);

const MAIN: &str = "(defn execute () void void)\n";
const PROJECT: &str = "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.execute effects: (array))) dependencies: (dict))\n";

/// A project tree that removes itself.
struct Tree {
    root: PathBuf,
}

impl Tree {
    fn new(tests: &str) -> Self {
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir()
            .join(format!("vibra-step11-{}-{serial}", std::process::id()));
        std::fs::create_dir_all(root.join("src/app")).expect("directory");
        std::fs::create_dir_all(root.join("tests")).expect("directory");
        std::fs::write(root.join("project.vibon"), PROJECT).expect("project");
        std::fs::write(root.join("src/app/main.vib"), MAIN).expect("main");
        std::fs::write(root.join("tests/t.vib"), tests).expect("tests");
        Self { root }
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The interpreter's items and the Wasm backend's rows for a test module.
struct Both {
    interpreter: Vec<TestRow>,
    wasm: Vec<TestRow>,
}

fn run(tests: &str) -> Both {
    let tree = Tree::new(&format!("(import assert @std.assert)\n{tests}"));
    let snapshot = WorkspaceSnapshot::load_confined(&tree.root).expect("a workspace");
    let stdlib = vibra_types::load_stdlib().expect("the standard library");
    let result = vibra_workspace::semantic::run_tests(&snapshot, None, Some(&stdlib));
    assert!(
        matches!(
            result.status(),
            vibra_workspace::semantic::TestSuiteStatus::Ok
                | vibra_workspace::semantic::TestSuiteStatus::TestFailed
                | vibra_workspace::semantic::TestSuiteStatus::Trap
        ),
        "{:?} {:?}",
        result.status(),
        result.diagnostics()
    );
    let items = result.items().to_vec();
    let interpreter = items.iter().map(TestRow::from_item).collect::<Vec<_>>();
    let TestsObserved::Rows(wasm) = observe_wasm_tests(&items) else {
        panic!("the Wasm backend did not run the tests");
    };
    Both { interpreter, wasm }
}

/// Runs `tests` in both backends and asserts the same rows.
fn agree(tests: &str) -> Vec<TestRow> {
    let both = run(tests);
    assert_eq!(
        both.wasm, both.interpreter,
        "the backends disagree on:\n{tests}"
    );
    both.wasm
}

fn status(row: &TestRow) -> &str {
    &row.status
}

#[test]
fn each_assertion_passes_in_both_backends() {
    let rows = agree(
        "(test \"true\" (assert.true true))\n\
         (test \"false\" (assert.false false))\n\
         (test \"equal\" (assert.equal 4i32 4i32))\n\
         (test \"text\" (assert.equal \"héllo\" \"héllo\"))\n\
         (test \"tuple\" (assert.equal (tupleof 1u8 \"a\" @b) (tupleof 1u8 \"a\" @b)))\n\
         (test \"array\" (assert.equal (array.of 1i32 2i32) (array.of 1i32 2i32)))\n",
    );
    assert!(
        rows.iter().all(|row| status(row) == "@test.passed"),
        "{rows:?}"
    );
}

#[test]
fn a_false_assertion_stops_only_its_test_and_is_not_a_trap() {
    let rows = agree(
        "(test \"a true\" (assert.true false))\n\
         (test \"b false\" (assert.false true))\n\
         (test \"c equal\" (assert.equal 4i32 5i32))\n\
         (test \"d after\" (assert.equal 1u8 1u8))\n",
    );
    let outcomes = rows.iter().map(status).collect::<Vec<_>>();
    assert_eq!(
        outcomes,
        [
            "@test.assertion-failed",
            "@test.assertion-failed",
            "@test.assertion-failed",
            "@test.passed"
        ]
    );
    let failure = rows[2].failure.as_ref().expect("a failure");
    assert_eq!(
        (
            failure.assertion.as_str(),
            failure.expected.as_str(),
            failure.actual.as_str()
        ),
        ("@std.assert.equal", "4i32", "5i32")
    );
    let first = rows[0].failure.as_ref().expect("a failure");
    assert_eq!(
        (first.expected.as_str(), first.actual.as_str()),
        ("true", "false")
    );
    assert!(rows.iter().all(|row| row.trap.is_none()));
}

#[test]
fn a_failed_assertion_inside_a_called_function_stops_the_test() {
    let rows = agree(
        "(defn check (value i32) void (assert.equal value 3i32))\n\
         (defn deeper (value i32) void (do (check value) (assert.true false)))\n\
         (test \"first\" (deeper 4i32))\n\
         (test \"second\" (deeper 3i32))\n",
    );
    assert_eq!(rows[0].failure.as_ref().expect("failed").expected, "4i32");
    assert_eq!(rows[1].failure.as_ref().expect("failed").expected, "true");
}

#[test]
fn operands_cross_as_scalars_or_as_values_by_their_type() {
    let rows = agree(
        "(import text @std.text)\n\
         (deftype point (record x i32 y i32))\n\
         (deftype shade (enum dark void light void))\n\
         (deftype wide u64)\n\
         (defn pick-atom (flag bool) atom (if flag @a @b))\n\
         (test \"wide\" (assert.equal (wide 1u64) (wide 2u64)))\n\
         (test \"i8\" (assert.equal -3i8 4i8))\n\
         (test \"u16\" (assert.equal 65535u16 1u16))\n\
         (test \"i64\" (assert.equal -9i64 9i64))\n\
         (test \"u64\" (assert.equal 18446744073709551615u64 0u64))\n\
         (test \"f32\" (assert.equal 1.5f32 2.5f32))\n\
         (test \"f64\" (assert.equal 0.1f64 0.2f64))\n\
         (test \"char\" (assert.equal \\a \\b))\n\
         (test \"void\" (assert.equal void void))\n\
         (test \"bool\" (assert.equal true false))\n\
         (test \"record\" (assert.equal (point x: 1i32 y: 2i32) (point x: 1i32 y: 3i32)))\n\
         (test \"enum\" (assert.equal (shade.dark) (shade.light)))\n\
         (test \"array\" (assert.equal (array.of 1i32) (array.of 1i32 2i32)))\n\
         (test \"bytes\" (assert.equal (text.to-utf8 \"a\") (text.to-utf8 \"b\")))\n\
         (test \"atom\" (assert.equal (pick-atom true) (pick-atom false)))\n\
         (test \"any\" (assert.equal (as any 1i32) (as any 1u32)))\n",
    );
    let failed = rows
        .iter()
        .filter(|row| status(row) == "@test.assertion-failed")
        .count();
    // `void` has one value, so its assertion passes; every other fails.
    assert_eq!(failed, rows.len() - 1, "{rows:?}");
}

#[test]
fn a_nan_equals_a_nan_and_zeros_differ_by_sign() {
    let rows = agree(
        "(def nan64 f64 (f64.div 0.0f64 0.0f64))\n\
         (defn nan32 () f32 (f32.div 0.0f32 0.0f32))\n\
         (test \"nan\" (assert.equal nan64 nan64))\n\
         (test \"nan of either sign\" (assert.equal nan64 (f64.neg nan64)))\n\
         (test \"nan32\" (assert.equal (nan32) (f32.neg (nan32))))\n\
         (test \"nan in a tuple\" (assert.equal (tupleof nan64 1u8) (tupleof (f64.neg nan64) 1u8)))\n\
         (test \"nan in an array\" (assert.equal (array.of nan64) (array.of (f64.neg nan64))))\n\
         (test \"parsed nan\" (assert.equal (f64.parse \"nan\") (f64.parse \"nan\")))\n\
         (test \"zero\" (assert.equal 0.0f64 0.0f64))\n\
         (test \"negative zero\" (assert.equal 0.0f64 (f64.neg 0.0f64)))\n\
         (test \"negative zero 32\" (assert.equal 0.0f32 (f32.neg 0.0f32)))\n\
         (test \"negative zero in a record\" (assert.equal (tupleof 0.0f64) (tupleof (f64.neg 0.0f64))))\n",
    );
    let outcomes = rows.iter().map(status).collect::<Vec<_>>();
    assert_eq!(
        outcomes,
        [
            "@test.passed",
            "@test.passed",
            "@test.passed",
            "@test.passed",
            "@test.passed",
            "@test.passed",
            "@test.passed",
            "@test.assertion-failed",
            "@test.assertion-failed",
            "@test.assertion-failed",
        ]
    );
    let zero = rows[7].failure.as_ref().expect("a failure");
    assert_eq!(
        (zero.expected.as_str(), zero.actual.as_str()),
        ("0.0f64", "-0.0f64")
    );
}

#[test]
fn values_nested_five_thousand_deep_compare_and_differ() {
    let rows = agree(
        "(deftype chain (record next (array chain)))\n\
         (defn build (count u64 tip chain) chain\n\
           (if (u64.equal count 0u64)\n\
             tip\n\
             (build (lower count) (chain next: (array.of tip)))))\n\
         (defn lower (count u64) u64\n\
           (match (u64.sub-checked count 1u64) (result.ok value) value (result.err -) 0u64))\n\
         (test \"equal\" (assert.equal (build 5000u64 (chain next: (array.of))) (build 5000u64 (chain next: (array.of)))))\n\
         (test \"differ at the bottom\" (assert.equal (build 5000u64 (chain next: (array.of))) (build 5000u64 (chain next: (array.of (chain next: (array.of)))))))\n",
    );
    assert_eq!(status(&rows[0]), "@test.passed");
    assert_eq!(status(&rows[1]), "@test.assertion-failed");
}

#[test]
fn a_union_and_a_dict_compare_by_member_and_by_entries() {
    let rows = agree(
        "(deftype either (union i32 str))\n\
         (test \"union\" (assert.equal (as either 1i32) (as either 1i32)))\n\
         (test \"union members\" (assert.equal (as either 1i32) (as either \"1\")))\n\
         (test \"dict\" (assert.equal (dict.of \"a\" 1i32 \"b\" 2i32) (dict.of \"b\" 2i32 \"a\" 1i32)))\n\
         (test \"dict values\" (assert.equal (dict.of \"a\" 1i32) (dict.of \"a\" 2i32)))\n",
    );
    let outcomes = rows.iter().map(status).collect::<Vec<_>>();
    assert_eq!(
        outcomes,
        [
            "@test.passed",
            "@test.assertion-failed",
            "@test.passed",
            "@test.assertion-failed"
        ]
    );
}

#[test]
fn an_operand_of_a_generic_type_crosses_as_the_value_it_holds() {
    let rows = agree(
        "(defn same (left t right t) void where: (t any) (assert.equal left right))\n\
         (test \"numbers\" (same 1i32 2i32))\n\
         (test \"texts\" (same \"a\" \"a\"))\n\
         (test \"tuples\" (same (tupleof 1u8 2u8) (tupleof 1u8 3u8)))\n",
    );
    let outcomes = rows.iter().map(status).collect::<Vec<_>>();
    assert_eq!(
        outcomes,
        [
            "@test.assertion-failed",
            "@test.passed",
            "@test.assertion-failed"
        ]
    );
    let first = rows[0].failure.as_ref().expect("a failure");
    assert_eq!(
        (first.expected.as_str(), first.actual.as_str()),
        ("1i32", "2i32")
    );
}

#[test]
fn a_function_hidden_behind_an_interface_is_the_trap_at_the_assertion_call() {
    let rows = agree(
        "(defint shape (defn name (value self) str) (impl (fn (i32) i32) (defn name (value self) str \"fn\")))\n\
         (defn inc (x i32) i32 x)\n\
         (test \"any\" (assert.equal (as any inc) (as any inc)))\n\
         (test \"differ first\" (assert.equal (tupleof (as any 1i32) (as any inc)) (tupleof (as any 2i32) (as any inc))))\n\
         (test \"different types\" (assert.equal (as any 1i32) (as any inc)))\n\
         (test \"interface\" (assert.equal (as shape inc) (as shape inc)))\n\
         (test \"number\" (assert.equal (as any 1i32) (as any 1i32)))\n\
         (test \"after\" (assert.true true))\n",
    );
    let outcomes = rows.iter().map(status).collect::<Vec<_>>();
    assert_eq!(
        outcomes,
        [
            "@test.trap",
            "@test.trap",
            "@test.trap",
            "@test.trap",
            "@test.passed",
            "@test.passed"
        ]
    );
    for row in &rows[..4] {
        let trap = row.trap.as_ref().expect("a trap");
        assert_eq!(trap.code, "@runtime.unobservable-function");
        assert!(trap.origin.is_some(), "the origin is the assertion call");
    }
}

#[test]
fn a_test_after_a_failed_one_runs_normally_and_tests_are_isolated() {
    let rows = agree(
        "(def counter i32 7i32)\n\
         (test \"fails\" (assert.equal counter 8i32))\n\
         (test \"sees a fresh value\" (assert.equal counter 7i32))\n\
         (test \"and again\" (assert.equal counter 7i32))\n",
    );
    let outcomes = rows.iter().map(status).collect::<Vec<_>>();
    assert_eq!(
        outcomes,
        ["@test.assertion-failed", "@test.passed", "@test.passed"]
    );
}

#[test]
fn try_propagation_inside_a_test_behaves_alike() {
    let rows = agree(
        "(defn source (flag bool) (option i32) (if flag (option.some 1i32) (option.none)))\n\
         (defn pick (flag bool) (option i32)\n\
           (let value (try (source flag)))\n\
           (option.some value))\n\
         (test \"present\" (assert.equal (pick true) (option.some 1i32)))\n\
         (test \"absent\" (assert.equal (pick false) (option.none)))\n\
         (test \"wrong\" (assert.equal (pick false) (option.some 1i32)))\n",
    );
    let outcomes = rows.iter().map(status).collect::<Vec<_>>();
    assert_eq!(
        outcomes,
        ["@test.passed", "@test.passed", "@test.assertion-failed"]
    );
}

#[test]
fn every_origin_is_the_assertion_call_the_interpreter_reports() {
    let both = run("(test \"one\" (assert.true false))\n\
         (test \"two\" (assert.equal 1u8 2u8))\n\
         (test \"three\" (assert.equal (as any (lambda (x i32) i32 x)) (as any (lambda (x i32) i32 x))))\n");
    for (wasm, interpreter) in both.wasm.iter().zip(&both.interpreter) {
        let origin = |row: &TestRow| {
            row.failure
                .as_ref()
                .map(|failure| failure.origin.clone())
                .or_else(|| row.trap.as_ref().and_then(|trap| trap.origin.clone()))
        };
        assert!(origin(wasm).is_some());
        assert_eq!(origin(wasm), origin(interpreter));
    }
}

#[test]
fn a_passing_test_returns_the_live_size_to_its_baseline() {
    let tree = Tree::new(
        "(import assert @std.assert)\n\
         (deftype chain (record next (array chain)))\n\
         (defn build (count u64 tip chain) chain\n\
           (if (u64.equal count 0u64) tip (build (lower count) (chain next: (array.of tip)))))\n\
         (defn lower (count u64) u64\n\
           (match (u64.sub-checked count 1u64) (result.ok value) value (result.err -) 0u64))\n\
         (test \"small\" (assert.equal (build 3u64 (chain next: (array.of))) (build 3u64 (chain next: (array.of)))))\n\
         (test \"large\" (assert.equal (build 3000u64 (chain next: (array.of))) (build 3000u64 (chain next: (array.of)))))\n\
         (test \"texts\" (assert.equal (array.of \"a\" \"b\") (array.of \"a\" \"b\")))\n",
    );
    let snapshot = WorkspaceSnapshot::load_confined(&tree.root).expect("a workspace");
    let stdlib = vibra_types::load_stdlib().expect("the standard library");
    let result = vibra_workspace::semantic::run_tests(&snapshot, None, Some(&stdlib));
    let items = result.items();
    assert!(
        items
            .iter()
            .all(|item| item.status() == TestItemStatus::Passed)
    );
    let first = items[0].program().expect("a program");
    let entries = items
        .iter()
        .map(|item| item.program().expect("a program").entry_index())
        .collect::<Vec<_>>();
    let module = vibra_wasm::emit_tests(first, &entries).expect("a test module");
    let runner = Runner::new(MemoryLimit::new(64 * 1024 * 1024)).expect("an engine");
    let heads = |head: u32| module.descriptors().type_of(head).cloned();
    let operand = |ordinal: u32| module.origins().operand_type(ordinal).cloned();
    let mut ends = Vec::new();
    for index in 0..3 {
        let TestRun::Passed { live } = runner
            .run_test(module.bytes(), index, first.types(), &heads, &operand)
            .expect("the module runs")
        else {
            panic!("test {index} did not pass");
        };
        ends.push((live.start, live.end));
    }
    // Every test starts at the same size and ends where a test with nothing to
    // free would: what a test holds is the table of its module values, which
    // does not depend on the work it did.
    assert_eq!(ends[0].0, ends[1].0);
    assert_eq!(ends[0].1, ends[1].1, "{ends:?}");
    assert_eq!(ends[0].1, ends[2].1, "{ends:?}");
}

#[test]
fn a_test_position_outside_the_module_is_a_defect_of_the_caller() {
    let tree =
        Tree::new("(import assert @std.assert)\n(test \"only\" (assert.true true))\n");
    let snapshot = WorkspaceSnapshot::load_confined(&tree.root).expect("a workspace");
    let stdlib = vibra_types::load_stdlib().expect("the standard library");
    let result = vibra_workspace::semantic::run_tests(&snapshot, None, Some(&stdlib));
    let program = result.items()[0].program().expect("a program");
    let module = vibra_wasm::emit_tests(program, &[program.entry_index()])
        .expect("a test module");
    let runner = Runner::new(MemoryLimit::new(64 * 1024 * 1024)).expect("an engine");
    let none = |_: u32| None;
    let run = runner
        .run_test(module.bytes(), 5, program.types(), &none, &none)
        .expect("the module runs");
    assert_eq!(
        run,
        TestRun::Trapped {
            code: TrapCode::InvalidCheckedProgram,
            origin: None
        }
    );
}

#[test]
fn memory_exhaustion_in_a_test_is_the_host_event_and_not_a_trap() {
    let tree = Tree::new(
        "(import assert @std.assert)
\n         (defn forever (count u64) u64
\n           (match (u64.add-checked (forever count) 1u64) (result.ok total) total (result.err -) 0u64))
\n         (test \"runs out\" (assert.equal (forever 0u64) 0u64))
\n         (test \"fine\" (assert.true true))
",
    );
    let snapshot = WorkspaceSnapshot::load_confined(&tree.root).expect("a workspace");
    let stdlib = vibra_types::load_stdlib().expect("the standard library");
    let budget = vibra_interp::MemoryBudget::new(1 << 20);
    let result = vibra_workspace::semantic::run_tests_with_budget(
        &snapshot,
        None,
        Some(&stdlib),
        budget,
    );
    assert_eq!(
        result.status(),
        vibra_workspace::semantic::TestSuiteStatus::OperationalFailure
    );
    assert!(
        result.items().is_empty(),
        "no test counts after a host event"
    );
    // The Wasm backend ends the same way. The module set is the one of the test
    // that passes, selected alone, and the exhausting test is a function of it.
    let selector = vibra_workspace::semantic::TestSelector::parse("@tests.t::\"fine\"")
        .expect("a selector");
    let fine =
        vibra_workspace::semantic::run_tests(&snapshot, Some(&selector), Some(&stdlib));
    let program = fine.items()[0].program().expect("a program").clone();
    let runs_out = program
        .functions()
        .iter()
        .position(|function| function.name().contains("runs out"))
        .expect("the exhausting test is a function");
    let module = vibra_wasm::emit_tests(&program, &[runs_out]).expect("a test module");
    let runner = Runner::new(MemoryLimit::new(1 << 20)).expect("an engine");
    let none = |_: u32| None;
    let run = runner
        .run_test(module.bytes(), 0, program.types(), &none, &none)
        .expect("the module runs");
    assert_eq!(run, TestRun::MemoryExhausted);
}

#[test]
fn the_engine_trap_with_no_record_is_the_toolchain_defect() {
    // A module that stops with status 0 is reported as the defect, which is
    // `@runtime.invalid-checked-program` with no origin and which no case
    // expects (the protocol of `Outcome::Defect`).
    assert_eq!(
        Outcome::defect_trap().diagnostic_code().as_atom(),
        "@runtime.invalid-checked-program"
    );
}

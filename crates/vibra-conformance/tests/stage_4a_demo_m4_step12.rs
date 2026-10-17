//! The Stage 4A demo (milestone 4 Step 12): the tests of the M3 demo library
//! produce identical results in both backends, and a deep tail-recursive walk
//! grows neither the engine stack nor the arena.
//!
//! The CLI has no backend option in M4, so the demo reaches the Wasm backend
//! through the conformance harness. This test repeats the demo on every CI run;
//! `cargo test -p vibra-conformance --test stage_4a_demo_m4_step12 --
//! --nocapture` prints the numbers the stage report records.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs
)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use vibra_conformance::{TestRow, TestsObserved, format_test_run, observe_wasm_tests};
use vibra_wasm::layout::state;
use vibra_wasm_run::{MemoryLimit, Runner, Started, TestRun};
use vibra_workspace::WorkspaceSnapshot;
use vibra_workspace::semantic::{TestItem, TestItemStatus, TestSuiteStatus};

static NEXT: AtomicUsize = AtomicUsize::new(0);

const MIB: usize = 1024 * 1024;
const WALK: &str = "a deep walk over a line of stock";
/// The rounds the demo's own test walks, and the units it starts with.
const DEMO_ROUNDS: u64 = 100_000;
const START_COUNT: u64 = 7;

fn demo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/m3-catalog")
        .canonicalize()
        .expect("the demo project")
}

fn copy_directory(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("directory");
    for entry in std::fs::read_dir(from).expect("a directory") {
        let entry = entry.expect("an entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("a type").is_dir() {
            copy_directory(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).expect("copy");
        }
    }
}

/// A copy of the demo whose walk runs `rounds` rounds, which removes itself.
struct Copy {
    root: PathBuf,
}

impl Copy {
    fn with_rounds(rounds: u64) -> Self {
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir()
            .join(format!("vibra-step12-{}-{serial}", std::process::id()));
        copy_directory(&demo(), &root);
        let path = root.join("tests/stock.vib");
        let text = std::fs::read_to_string(&path).expect("tests");
        let from = format!("{DEMO_ROUNDS}u64");
        let expected = format!("{}u64", DEMO_ROUNDS + START_COUNT);
        assert!(text.contains(&from) && text.contains(&expected));
        let text = text
            .replace(&expected, &format!("{}u64", rounds + START_COUNT))
            .replace(&from, &format!("{rounds}u64"));
        std::fs::write(&path, text).expect("rewritten tests");
        Self { root }
    }
}

impl Drop for Copy {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn run_suite(root: &Path) -> vibra_workspace::semantic::WorkspaceTestResult {
    let snapshot = WorkspaceSnapshot::load_confined(root).expect("a workspace");
    let stdlib = vibra_types::load_stdlib().expect("the standard library");
    vibra_workspace::semantic::run_tests(&snapshot, None, Some(&stdlib))
}

#[test]
fn the_demo_tests_produce_identical_results_in_both_backends() {
    let result = run_suite(&demo());
    assert_eq!(
        result.status(),
        TestSuiteStatus::Ok,
        "{:?}",
        result.diagnostics()
    );
    assert_eq!(result.selected(), 8);
    assert_eq!(result.passed(), 8);
    let interpreter = result
        .items()
        .iter()
        .map(TestRow::from_item)
        .collect::<Vec<_>>();
    let TestsObserved::Rows(wasm) = observe_wasm_tests(result.items()) else {
        panic!("the Wasm backend did not run the demo's tests");
    };
    assert_eq!(wasm, interpreter);
    assert_eq!(
        format_test_run(vibra_conformance::command_result(&wasm), &wasm),
        format_test_run("@command.ok", &interpreter)
    );
    println!(
        "demo: 8 of 8 tests passed in the interpreter and in Wasm, rows identical"
    );
}

/// What the walk at one size left in the Wasm backend.
struct WasmWalk {
    rounds: u64,
    live_start: u64,
    live_end: u64,
    arena_used: u32,
    smallest_stack_kib: Option<usize>,
}

/// What the walk at one size cost the interpreter.
struct InterpreterWalk {
    max_activation_depth: usize,
    tail_transfers: usize,
}

fn walk_index(items: &[TestItem]) -> (usize, usize) {
    // The position among the tests that ran, which is the module's test index.
    let ran = items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.program().is_some())
        .collect::<Vec<_>>();
    let (index, (position, _)) = ran
        .iter()
        .enumerate()
        .find(|(_, (_, item))| item.name().contains(WALK))
        .expect("the walk test");
    (index, *position)
}

fn measure(rounds: u64) -> (InterpreterWalk, WasmWalk) {
    let copy = Copy::with_rounds(rounds);
    let result = run_suite(&copy.root);
    assert_eq!(result.status(), TestSuiteStatus::Ok, "{rounds}");
    let items = result.items();
    let (index, position) = walk_index(items);
    assert_eq!(items[position].status(), TestItemStatus::Passed);
    let program = items[position].program().expect("the walk's program");

    let execution = vibra_interp::Interpreter::run_test_with_budget(
        program,
        vibra_interp::MemoryBudget::new(64 * MIB),
    )
    .expect("the interpreter runs the walk");
    assert!(execution.assertion_failure().is_none());
    let interpreter = InterpreterWalk {
        max_activation_depth: execution.max_activation_depth(),
        tail_transfers: execution.tail_transfer_count(),
    };

    let first = items
        .iter()
        .find_map(TestItem::program)
        .expect("a test program");
    let entries = items
        .iter()
        .filter_map(TestItem::program)
        .map(vibra_ir::CheckedProgram::entry_index)
        .collect::<Vec<_>>();
    let module = vibra_wasm::emit_tests(first, &entries).expect("the tests lower");
    let heads = |head: u32| module.descriptors().type_of(head).cloned();
    let operand_of = |ordinal: u32| module.origins().operand_type(ordinal).cloned();
    let runner = Runner::new(MemoryLimit::new(64 * MIB)).expect("a runner");
    let Started::Ready(mut instance) =
        runner.start(module.bytes()).expect("a v1 module")
    else {
        panic!("the module's own memory exceeds the limit");
    };
    let live_start = instance.live_size().expect("live size");
    let index_u32 = u32::try_from(index).expect("a small index");
    let outcome = instance.call_test(index_u32).expect("the test runs");
    assert!(
        matches!(outcome, vibra_wasm_run::Outcome::Completed { .. }),
        "{outcome:?}"
    );
    let live_end = instance.live_size().expect("live size");
    let bytes = instance
        .read_memory_for_tests(state::ARENA_USED, 4)
        .expect("the state is in memory");
    let arena_used = u32::from_le_bytes(bytes.try_into().expect("four bytes"));

    // The smallest engine stack, in KiB, under which the walk passes.
    let smallest_stack_kib =
        [8_usize, 12, 16, 24, 32, 48, 64].into_iter().find(|kib| {
            let runner =
                Runner::with_wasm_stack(MemoryLimit::new(64 * MIB), kib * 1024)
                    .expect("a runner");
            matches!(
                runner.run_test(
                    module.bytes(),
                    index_u32,
                    program.types(),
                    &heads,
                    &operand_of
                ),
                Ok(TestRun::Passed { .. })
            )
        });
    (
        interpreter,
        WasmWalk {
            rounds,
            live_start,
            live_end,
            arena_used,
            smallest_stack_kib,
        },
    )
}

#[test]
fn the_deep_walk_grows_neither_the_engine_stack_nor_the_arena() {
    let (small_i, small_w) = measure(10_000);
    let (large_i, large_w) = measure(100_000);
    for (interpreter, wasm) in [(&small_i, &small_w), (&large_i, &large_w)] {
        println!(
            "walk {}: interpreter max activation depth {}, tail transfers {}; wasm live size {} at start and {} at end, arena high-water {} bytes, smallest engine stack {:?} KiB",
            wasm.rounds,
            interpreter.max_activation_depth,
            interpreter.tail_transfers,
            wasm.live_start,
            wasm.live_end,
            wasm.arena_used,
            wasm.smallest_stack_kib
        );
        assert!(interpreter.tail_transfers >= usize::try_from(wasm.rounds).unwrap());
    }
    // The interpreter's activations do not pile up with the rounds.
    assert_eq!(small_i.max_activation_depth, large_i.max_activation_depth);
    // The arena does not grow with a factor of ten more rounds.
    assert_eq!(small_w.live_end, large_w.live_end);
    assert!(
        large_w.arena_used.abs_diff(small_w.arena_used) <= 4 * 1024,
        "arena {} against {}",
        small_w.arena_used,
        large_w.arena_used
    );
    // The engine stack the walk needs does not depend on its length.
    let stack = small_w
        .smallest_stack_kib
        .expect("some stack up to 64 KiB runs it");
    assert_eq!(Some(stack), large_w.smallest_stack_kib);
}

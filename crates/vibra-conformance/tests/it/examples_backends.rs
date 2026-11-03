//! The checked-in examples in both backends.
//!
//! The CLI has no backend option, so the Wasm backend reaches the examples
//! through the conformance harness. The tests of every example give identical
//! rows in the interpreter and in Wasm, and a deep tail-recursive walk grows
//! neither the engine stack nor the arena. `examples/m4-demo` is one source: a
//! host world supplies the program argument and the file it names, and the
//! accepted program's standard output and audit trace are byte-identical in both
//! backends. Its rejection by an effect ceiling is asserted through the real
//! binary in `vibra-cli` (`process_examples`) and by the corpus cases
//! `V1-EFFECT-demo-gate-*` and `V1-EFFECT-host-*`.
//!
//! `cargo test -p vibra-conformance --test examples_backends -- --nocapture`
//! prints the trace and the walk numbers the exit evidence records.

use crate::support::{MIB, TempDir, copy_directory, runner, workspace_root};

use std::path::{Path, PathBuf};

use vibra_conformance::{
    INSTANCE_MEMORY_LIMIT_BYTES, TestRow, TestsObserved, WasmObservation,
    format_test_run, observe_wasm, observe_wasm_tests,
};
use vibra_host::{AuditEvent, HostWorld, WorldProvider, trace_to_vibon};
use vibra_ir::CheckedProgram;
use vibra_wasm::layout::state;
use vibra_wasm_run::{MemoryLimit, Runner, Started, TestRun};
use vibra_workspace::WorkspaceSnapshot;
use vibra_workspace::project::TargetKind;
use vibra_workspace::semantic::{
    CheckStatus, TestItem, TestItemStatus, TestSuiteStatus, check_all_with_bootstrap,
};

const WORLD: &str = "(record format: @host-world.v1 args: (array \"/in/note.txt\") files: (dict \"/in/note.txt\" \"alpha\\nbeta\\n\"))";
const STDOUT: &str = "> alpha\n> beta\n";
const WALK: &str = "a deep walk over a line of stock";
/// The rounds the catalog's own test walks, and the units it starts with.
const DEMO_ROUNDS: u64 = 100_000;
const START_COUNT: u64 = 7;

fn examples() -> PathBuf {
    workspace_root()
        .join("examples")
        .canonicalize()
        .expect("the examples")
}

fn example(name: &str) -> PathBuf {
    examples().join(name)
}

fn run_suite(root: &Path) -> vibra_workspace::semantic::WorkspaceTestResult {
    let snapshot = WorkspaceSnapshot::load_confined(root).expect("a workspace");
    let stdlib = vibra_types::load_stdlib().expect("the standard library");
    vibra_workspace::semantic::run_tests(&snapshot, None, Some(&stdlib))
}

fn demo_program() -> CheckedProgram {
    let stdlib = vibra_types::load_stdlib().expect("stdlib");
    let snapshot =
        WorkspaceSnapshot::load_confined(example("m4-demo")).expect("snapshot");
    let target = snapshot
        .project()
        .project()
        .targets()
        .iter()
        .find(|target| target.kind() == TargetKind::Bin)
        .expect("a binary target")
        .clone();
    let checked = check_all_with_bootstrap(&snapshot, Some(&stdlib));
    assert_eq!(
        checked.status(),
        CheckStatus::Accepted,
        "{:?}",
        checked.diagnostics()
    );
    checked
        .program_for_target(&target)
        .cloned()
        .expect("the entry's program")
}

fn stdout_of(trace: &[AuditEvent]) -> String {
    trace
        .iter()
        .filter_map(|event| match event {
            AuditEvent::Host(host) if host.op == "io.stdout-text" => {
                Some(host.args[0].clone())
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

#[test]
fn the_tests_of_every_example_give_identical_rows_in_both_backends() {
    let mut names = std::fs::read_dir(examples())
        .expect("the examples")
        .map(|entry| entry.expect("an entry").path())
        .filter(|path| path.join("project.vibon").is_file())
        .collect::<Vec<_>>();
    names.sort();
    let mut with_tests = 0;
    for root in names {
        let result = run_suite(&root);
        assert_eq!(
            result.status(),
            TestSuiteStatus::Ok,
            "{}: {:?}",
            root.display(),
            result.diagnostics()
        );
        if result.selected() == 0 {
            continue;
        }
        with_tests += 1;
        assert_eq!(result.passed(), result.selected(), "{}", root.display());
        let interpreter = result
            .items()
            .iter()
            .map(TestRow::from_item)
            .collect::<Vec<_>>();
        let TestsObserved::Rows(wasm) =
            observe_wasm_tests(result.items(), &HostWorld::default())
        else {
            panic!(
                "the Wasm backend did not run the tests of {}",
                root.display()
            );
        };
        assert_eq!(wasm, interpreter, "{}", root.display());
        assert_eq!(
            format_test_run(vibra_conformance::command_result(&wasm), &wasm),
            format_test_run("@command.ok", &interpreter),
            "{}",
            root.display()
        );
        println!(
            "{}: {} tests passed in the interpreter and in Wasm, rows identical",
            root.display(),
            interpreter.len()
        );
    }
    assert_eq!(with_tests, 3, "the catalog, the configuration, the console");
}

#[test]
fn the_demo_has_identical_stdout_and_audit_trace_in_both_backends() {
    let program = demo_program();
    let world = HostWorld::decode("world.vibon", WORLD).expect("world");

    let mut provider = WorldProvider::new(&world);
    let interpreted = vibra_interp::Interpreter::run_with_host(
        &program,
        vibra_interp::MemoryBudget::new(INSTANCE_MEMORY_LIMIT_BYTES),
        &mut provider,
    )
    .expect("interpreter run");
    assert!(!interpreted.program_failed());

    let WasmObservation::Completed(wasm) = observe_wasm(&program, &world) else {
        panic!("the Wasm backend did not complete the demo");
    };

    // The result, and the audit trace as events and as bytes.
    assert_eq!(
        wasm.result.as_deref(),
        Some(interpreted.canonical_result().as_str())
    );
    assert_eq!(wasm.audit_trace, interpreted.audit_trace());
    let interpreter_bytes = trace_to_vibon(interpreted.audit_trace());
    let wasm_bytes = trace_to_vibon(&wasm.audit_trace);
    assert_eq!(interpreter_bytes, wasm_bytes);

    // Standard output is what the trace's `io.stdout-text` events carry.
    let expected = format!("{STDOUT:?}");
    assert_eq!(stdout_of(interpreted.audit_trace()), expected);
    assert_eq!(stdout_of(&wasm.audit_trace), expected);

    // The three operations, in order: the argument, the file it names, the output.
    let ops = interpreted
        .audit_trace()
        .iter()
        .map(|event| match event {
            AuditEvent::Host(host) => host.op.clone(),
            AuditEvent::Opaque(opaque) => opaque.kind.clone(),
        })
        .collect::<Vec<_>>();
    assert_eq!(ops, ["env.read-args", "fs.read-text", "io.stdout-text"]);
    println!("{interpreter_bytes}");
}

#[test]
fn a_different_world_gives_a_different_trace_from_the_same_module() {
    // The path is chosen at run time: the same checked program reads another file.
    let program = demo_program();
    let world = HostWorld::decode(
        "world.vibon",
        "(record format: @host-world.v1 args: (array \"/in/other.txt\") files: (dict \"/in/other.txt\" \"one\\n\"))",
    )
    .expect("world");
    let WasmObservation::Completed(wasm) = observe_wasm(&program, &world) else {
        panic!("the Wasm backend did not complete the demo");
    };
    assert_eq!(stdout_of(&wasm.audit_trace), format!("{:?}", "> one\n"));
}

/// A copy of the catalog whose walk runs `rounds` rounds, which removes itself.
struct Copy {
    root: TempDir,
}

impl Copy {
    fn with_rounds(rounds: u64) -> Self {
        let root = TempDir::new("walk");
        copy_directory(&example("m3-catalog"), &root);
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
    let runner = runner();
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

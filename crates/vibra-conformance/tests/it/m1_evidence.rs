//! Milestone 1 evidence that can be gathered today (package X3-09a), one test
//! for each gate clause G3.M1.9 to G3.M1.11.
//!
//! The fixture is `examples/nexus-m1`: two modules of one package. The loop of
//! `app.ticks` counts a `u64` down by one each round and ends with an indirect
//! tail call of `finish`, a function value that `app.main` defined; `app.main`
//! writes the farewell with one `io.stdout.text`. The loop count is the one
//! constant in the call of `ticks.count`, which `with_rounds` rewrites. The
//! division variant is a second corpus case whose loop divides by the counter
//! and so ends in `@runtime.division-by-zero`.
//!
//! The harness launcher's observation document carries the outcome, the result
//! kind, the trap, the audit trace, the context's counters (yields, peak frames,
//! tail transfers, peak live size, live size at the end), and the entry's
//! result. G3.M1.9 and G3.M1.10 are therefore evidenced on the reference
//! stepper, which drives the same `Context`, dispatcher, and transfer routines
//! the linked executable uses, and, on Windows, on the linked executable
//! itself, read from its counters. The refusals are evidenced on linked
//! executables.

use std::path::{Path, PathBuf};

use vibra_exec::{BlockId, Callee, Program, Slot, Terminator, VerifyErrorKind, verify};
use vibra_ir::CheckedProgram;
use vibra_lower::lower;
use vibra_nexus::{Code, Context, Outcome, Step, UNBOUNDED, run};
use vibra_nexus_eval::Evaluator;
use vibra_workspace::WorkspaceSnapshot;
use vibra_workspace::project::TargetKind;
use vibra_workspace::semantic::{CheckStatus, check_all_with_bootstrap};

use crate::support::{TempDir, copy_directory, workspace_root};

fn fixture_directory() -> PathBuf {
    workspace_root().join("examples").join("nexus-m1")
}

/// The loop count of the fixture's call of `ticks.count`, as the committed text
/// writes it.
const COUNT: &str = "1000u64";

/// The fixture's `app.main`, with the loop count of its call of `ticks.count`
/// set to `rounds`. The count is the one constant in the file.
fn with_rounds(main: &str, rounds: u32) -> String {
    assert_eq!(
        main.matches(COUNT).count(),
        1,
        "the loop count is one constant"
    );
    main.replace(COUNT, &format!("{rounds}u64"))
}

/// Checks the workspace at `root` and answers the program of its binary target.
fn check_tree(root: &Path) -> CheckedProgram {
    let snapshot = WorkspaceSnapshot::load_confined(root).expect("a workspace");
    let stdlib = vibra_types::load_stdlib().expect("the standard library");
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

fn checked_fixture(rounds: u32) -> CheckedProgram {
    let scratch = TempDir::new("nexus-m1");
    copy_directory(&fixture_directory(), scratch.path());
    let main = scratch.path().join("src").join("app").join("main.vib");
    let source = std::fs::read_to_string(&main).expect("the fixture's main");
    std::fs::write(&main, with_rounds(&source.replace("\r\n", "\n"), rounds))
        .expect("a rewritten main");
    check_tree(scratch.path())
}

fn lowered_fixture(rounds: u32) -> Program {
    lower(&checked_fixture(rounds)).expect("the fixture lowers")
}

/// The reference stepper drives `Code`; this counts the steps it takes. Each
/// step ends at a checkpoint, so the count is the number of checkpoints reached.
struct Counting<'a> {
    inner: &'a Evaluator<'a>,
    steps: std::cell::Cell<u64>,
}

impl Code for Counting<'_> {
    fn step(&self, ctx: &mut Context<'_>) -> Step {
        self.steps.set(self.steps.get() + 1);
        self.inner.step(ctx)
    }
}

/// What one run of the stepper counted.
#[derive(Debug)]
struct Counted {
    checkpoints: u64,
    yields: u64,
    max_depth: usize,
    peak_size: u64,
    audit_events: usize,
}

fn count(program: &Program, quota: u64) -> Counted {
    let code = Evaluator::new(program).expect("the stepper takes the fixture");
    let counting = Counting {
        inner: &code,
        steps: std::cell::Cell::new(0),
    };
    let mut host = vibra_host::WorldProvider::new(&vibra_host::HostWorld::default());
    let mut ctx = Context::new(256 * 1024 * 1024, code.globals(), &mut host);
    code.start(&mut ctx).expect("room for the entry frame");
    let outcome = loop {
        match run(&mut ctx, &counting, quota) {
            Outcome::Yielded => {}
            other => break other,
        }
    };
    assert_eq!(outcome, Outcome::Completed);
    Counted {
        checkpoints: counting.steps.get(),
        yields: ctx.stats().yields,
        max_depth: ctx.stats().max_depth,
        peak_size: ctx.peak_size(),
        audit_events: ctx.audit().len(),
    }
}

#[test]
fn the_committed_fixture_is_the_template_at_one_thousand_rounds() {
    let main = std::fs::read_to_string(fixture_directory().join("src/app/main.vib"))
        .expect("the fixture's main")
        .replace("\r\n", "\n");
    assert_eq!(with_rounds(&main, 1000), main);
    assert_ne!(with_rounds(&main, 7), main);
}

#[test]
fn the_fixture_case_of_the_corpus_is_the_example() {
    let case = workspace_root()
        .join("conformance")
        .join("cases")
        .join("V1-RUNTIME-nexus-m1-fixture")
        .join("tree");
    for file in ["project.vibon", "src/app/main.vib", "src/app/ticks.vib"] {
        let read = |root: &PathBuf| {
            std::fs::read_to_string(root.join(file))
                .expect("a fixture file")
                .replace("\r\n", "\n")
        };
        assert_eq!(read(&case), read(&fixture_directory()), "{file}");
    }
}

const VARIANT: &str = "V1-RUNTIME-nexus-m1-fixture-division-by-zero";

fn case_directory(id: &str) -> PathBuf {
    workspace_root().join("conformance").join("cases").join(id)
}

fn read_text(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .replace("\r\n", "\n")
}

#[test]
fn the_division_variant_differs_from_the_fixture_only_in_the_test_of_its_loop() {
    let variant = case_directory(VARIANT).join("tree");
    for file in ["project.vibon", "src/app/main.vib"] {
        assert_eq!(
            read_text(&variant.join(file)),
            read_text(&fixture_directory().join(file)),
            "{file}"
        );
    }
    let ticks = |root: &Path| read_text(&root.join("src/app/ticks.vib"));
    let (fixture, divides) = (ticks(&fixture_directory()), ticks(&variant));
    assert_ne!(fixture, divides);
    assert!(divides.contains("u64.rem 1000000u64 remaining"));
    assert!(!fixture.contains("u64.rem"));
}

#[test]
fn the_linked_fixture_equals_the_interpreter_on_result_trap_origin_and_trace() {
    // G3.M1.8. The expectation is the corpus case's own files: the fixture's
    // result and trace, and the variant's trap diagnostic, whose origin the
    // runtime chapter leaves out. The interpreter must meet the expectation
    // first; the linked executable is then held to the interpreter's result,
    // trap, origin, and trace, and to the reference stepper's whole document.
    let completed_case = case_directory("V1-RUNTIME-nexus-m1-fixture");
    let completed = check_tree(&completed_case.join("tree"));
    let execution =
        vibra_interp::Interpreter::run(&completed).expect("the fixture runs");
    assert_eq!(
        read_text(&completed_case.join("result.vibon")).trim_end(),
        execution.canonical_result().trim_end()
    );
    let trace = vibra_host::trace_to_vibon(execution.audit_trace());
    assert_eq!(
        read_text(&completed_case.join("audit.vibon")).trim_end(),
        trace
    );
    assert_eq!(execution.audit_trace().len(), 1, "one write");

    let variant_case = case_directory(VARIANT);
    let variant = check_tree(&variant_case.join("tree"));
    let error =
        vibra_interp::Interpreter::run(&variant).expect_err("the variant traps");
    let (code, origin) = error.program_trap().expect("a trap, not a host event");
    assert_eq!(origin, None, "the runtime chapter gives the trap no origin");
    let atom = code.as_atom();
    assert_eq!(atom, "@runtime.division-by-zero");
    let case = read_text(&variant_case.join("case.toml"));
    assert!(case.contains(&format!("code = \"{atom}\"")), "{case}");
    assert!(case.contains("span = [0, 0]"), "{case}");
    let silent = vibra_host::trace_to_vibon(&[]);

    // The stepper meets the same expectation.
    let lowered = lower(&variant).expect("the variant lowers");
    let ran = crate::lowering_slice::stepped(&lowered, 64 * 1024 * 1024, UNBOUNDED);
    assert_eq!(
        ran.outcome,
        Outcome::Trapped(vibra_nexus::Trap {
            code: vibra_ir::boundary::TrapCode::DivisionByZero,
            origin: 0
        })
    );
    assert!(ran.audit.is_empty(), "the variant writes nothing");

    #[cfg(windows)]
    {
        use crate::linked_exe_support::{
            Profile, Scratch, WORLD, archive, options, reference, run_exe,
        };
        let _ = (archive(Profile::Harness), archive(Profile::System));
        for (label, program, outcome, result, trap, origin, audit, exit) in [
            (
                "fixture",
                &completed,
                "completed",
                "void-or-ok",
                "@none",
                0,
                &trace,
                0,
            ),
            ("variant", &variant, "trapped", "none", atom, 0, &silent, 5),
        ] {
            let lowered = lower(program).expect("lowers");
            let scratch = Scratch::new("m1-equal", "");
            let harness = vibra_build::build_executable(
                program,
                &options(&scratch, Profile::Harness, "harness.exe"),
            )
            .unwrap_or_else(|error| panic!("{label}: {error}"));
            let ran = run_exe(&harness.executable, &[], WORLD.as_bytes());
            assert_eq!(ran.code, Some(0), "{label}: {}", harness.command);
            let document = String::from_utf8_lossy(&ran.stdout).into_owned();
            let head = format!(
                "(record outcome: @{outcome} result: @{result} trap: {trap} origin: {origin} audit: {audit} counters: "
            );
            assert!(document.starts_with(&head), "{label}: {document}");
            let expected = reference(&lowered, None, UNBOUNDED, false);
            assert_eq!(document, format!("{}\n", expected.observation), "{label}");

            // The shipped launcher: the exit code of the tooling chapter and the
            // trap's atom on standard error.
            let shipped = vibra_build::build_executable(
                program,
                &options(&scratch, Profile::System, "system.exe"),
            )
            .unwrap_or_else(|error| panic!("{label}: {error}"));
            let ran = run_exe(&shipped.executable, &[], b"");
            assert_eq!(ran.code, Some(exit), "{label}: {}", shipped.command);
            if exit == 5 {
                assert_eq!(String::from_utf8_lossy(&ran.stderr), format!("{trap}\n"));
                assert!(ran.stdout.is_empty());
            } else {
                assert_eq!(String::from_utf8_lossy(&ran.stdout), "done\n");
            }
        }
    }
}

#[test]
fn quota_one_has_one_stdout_event_and_yields_at_every_checkpoint() {
    // G3.M1.9. On the reference stepper the checkpoints are the steps the
    // dispatcher takes (`Counting`) and the yields are `Context::stats()`. The
    // linked executable's part is its own observation document of the quota-1
    // run: exactly one `io.stdout-text` event in its audit trace, and its yield
    // count, which it reports, held to the checkpoint count of the same
    // program (the stepper's step count: the executable runs the same IR
    // through the same dispatcher, which takes one step per checkpoint, and
    // with quota 1 yields after every step but the last). The whole document is
    // also equal to the stepper's.
    let program = lowered_fixture(1000);
    let unbounded = count(&program, UNBOUNDED);
    let one = count(&program, 1);
    eprintln!("unbounded {unbounded:?}\nquota 1 {one:?}");
    assert_eq!(unbounded.yields, 0);
    assert_eq!(
        one.audit_events, 1,
        "one stdout event in the stepper's trace"
    );
    assert_eq!(unbounded.audit_events, 1);
    assert_eq!(unbounded.checkpoints, one.checkpoints, "the same steps");
    // The run completes in its last step, which does not yield.
    assert!(
        one.yields + 1 >= one.checkpoints,
        "{} yields for {} checkpoints",
        one.yields,
        one.checkpoints
    );

    #[cfg(windows)]
    linked_quota_one(&unbounded, &one);
}

/// The counter `name` of an observation document.
#[cfg(windows)]
fn counter(document: &str, name: &str) -> u64 {
    let key = format!(" {name}: ");
    let at = document
        .find(&key)
        .unwrap_or_else(|| panic!("no `{name}` in {document}"))
        + key.len();
    document[at..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .unwrap_or_else(|_| panic!("`{name}` is not a number in {document}"))
}

#[cfg(windows)]
fn linked_quota_one(unbounded: &Counted, one: &Counted) {
    use crate::linked_exe_support::{
        Profile, Scratch, WORLD, archive, options, reference, run_exe,
    };
    let checked = checked_fixture(1000);
    let scratch = Scratch::new("m1-quota", "");
    let _ = archive(Profile::Harness);
    let built = vibra_build::build_executable(
        &checked,
        &options(&scratch, Profile::Harness, "fixture.exe"),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let lowered = lower(&checked).expect("lowers");
    let ran = run_exe(&built.executable, &["1"], WORLD.as_bytes());
    assert_eq!(ran.code, Some(0), "link: {}", built.command);
    let document = String::from_utf8_lossy(&ran.stdout).into_owned();
    assert_eq!(
        document.matches("op: \"io.stdout-text\"").count(),
        1,
        "{document}"
    );
    let expected = reference(&lowered, None, 1, false);
    assert_eq!(document, format!("{}\n", expected.observation));
    // The executable's own counters.
    let yields = counter(&document, "yields");
    eprintln!(
        "linked quota 1: {yields} yields, {} checkpoints",
        one.checkpoints
    );
    assert_eq!(yields, one.yields, "as many yields as the stepper");
    assert!(
        yields + 1 >= one.checkpoints,
        "{yields} yields for {} checkpoints",
        one.checkpoints
    );
    assert_eq!(counter(&document, "peak-frames"), one.max_depth as u64);

    let ran = run_exe(&built.executable, &[], WORLD.as_bytes());
    assert_eq!(ran.code, Some(0), "link: {}", built.command);
    let document = String::from_utf8_lossy(&ran.stdout).into_owned();
    assert_eq!(
        counter(&document, "yields"),
        0,
        "an unbounded run never yields"
    );
    assert_eq!(document.matches("op: \"io.stdout-text\"").count(), 1);
    assert_eq!(unbounded.yields, 0);
}

#[test]
fn peak_frames_and_live_size_do_not_grow_with_the_loop_count() {
    // G3.M1.10. `Context::stats().max_depth` is the peak frame count and
    // `Context::peak_size()` the peak live size in bytes; the harness launcher
    // reports both as `peak-frames` and `peak-live`, so on Windows the linked
    // executables are measured too, at the same loop counts.
    let small = count(&lowered_fixture(10_000), UNBOUNDED);
    let large = count(&lowered_fixture(100_000), UNBOUNDED);
    eprintln!("10,000 rounds {small:?}\n100,000 rounds {large:?}");
    assert_eq!(small.max_depth, large.max_depth, "peak frames");
    assert!(
        small.peak_size.abs_diff(large.peak_size) <= 64,
        "{} against {}",
        small.peak_size,
        large.peak_size
    );
    assert!(
        large.checkpoints > 10 * small.checkpoints / 2,
        "the work grew"
    );

    #[cfg(windows)]
    linked_peaks(&small, &large);
}

#[cfg(windows)]
fn linked_peaks(small: &Counted, large: &Counted) {
    use crate::linked_exe_support::{
        Profile, Scratch, WORLD, archive, options, run_exe,
    };
    let _ = archive(Profile::Harness);
    let mut seen = Vec::new();
    for (rounds, stepper) in [(10_000, small), (100_000, large)] {
        let scratch = Scratch::new("m1-peaks", "");
        let built = vibra_build::build_executable(
            &checked_fixture(rounds),
            &options(&scratch, Profile::Harness, "fixture.exe"),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let ran = run_exe(&built.executable, &[], WORLD.as_bytes());
        assert_eq!(ran.code, Some(0), "link: {}", built.command);
        let document = String::from_utf8_lossy(&ran.stdout).into_owned();
        let frames = counter(&document, "peak-frames");
        let peak = counter(&document, "peak-live");
        eprintln!("linked {rounds} rounds: {frames} frames, {peak} bytes peak");
        assert_eq!(
            frames, stepper.max_depth as u64,
            "{rounds}: the stepper's frames"
        );
        assert_eq!(peak, stepper.peak_size, "{rounds}: the stepper's peak");
        seen.push((frames, peak));
    }
    assert_eq!(seen[0].0, seen[1].0, "peak frames of the executables");
    assert!(
        seen[0].1.abs_diff(seen[1].1) <= 64,
        "{} against {}",
        seen[0].1,
        seen[1].1
    );
}

// -- G3.M1.11: three refusals -----------------------------------------------------------

/// Applies `mutate` to a copy of the lowered fixture and returns the copy.
fn mutated(mutate: impl FnOnce(&mut Program)) -> Program {
    let mut program = lowered_fixture(1000);
    verify(&program).expect("the fixture verifies before it is mutated");
    mutate(&mut program);
    program
}

fn first_terminator<T>(
    program: &mut Program,
    pick: impl Fn(&mut Terminator) -> Option<T>,
) -> T {
    for function in &mut program.functions {
        for block in &mut function.blocks {
            if let Some(found) = pick(&mut block.term) {
                return found;
            }
        }
    }
    panic!("the fixture has no such terminator");
}

#[test]
fn a_program_mutated_after_verification_is_rejected_by_verify() {
    // G3.M1.11 (a). `Verified` has a private field and `verified` is its only
    // constructor, so the generator, which takes a `Verified`, cannot be handed
    // a program that was not verified: `build_program` calls `verified` first.
    let cases: Vec<(&str, Program)> = vec![
        (
            "a slot out of range",
            mutated(|program| {
                first_terminator(program, |term| match term {
                    Terminator::TailCall { args, .. } if !args.is_empty() => {
                        args[0] = Slot(65_000);
                        Some(())
                    }
                    _ => None,
                });
            }),
        ),
        (
            "a jump to a missing block",
            mutated(|program| {
                first_terminator(program, |term| match term {
                    Terminator::Branch { then_block, .. } => {
                        *then_block = BlockId(9_999);
                        Some(())
                    }
                    _ => None,
                });
            }),
        ),
        (
            "a wrong operand count",
            mutated(|program| {
                first_terminator(program, |term| match term {
                    Terminator::TailCall {
                        callee: Callee::Direct(_),
                        args,
                        ..
                    } if !args.is_empty() => {
                        args.pop();
                        Some(())
                    }
                    _ => None,
                });
            }),
        ),
        (
            "an entry that takes an operand",
            mutated(|program| {
                let entry = program.entry.index();
                program.functions[entry].params = 1;
                program.functions[entry].slots += 1;
            }),
        ),
    ];
    let mut kinds = Vec::new();
    for (label, program) in &cases {
        let error = verify(program).expect_err(label);
        eprintln!("{label}: {error}");
        kinds.push(std::mem::discriminant(&error.kind));
    }
    assert!(matches!(
        verify(&cases[0].1).expect_err("slot").kind,
        VerifyErrorKind::UnknownSlot(_)
    ));
    assert!(matches!(
        verify(&cases[1].1).expect_err("block").kind,
        VerifyErrorKind::UnknownBlock(_)
    ));
    assert!(matches!(
        verify(&cases[2].1).expect_err("count").kind,
        VerifyErrorKind::Arity { .. }
    ));
    assert!(matches!(
        verify(&cases[3].1).expect_err("entry").kind,
        VerifyErrorKind::RootTakesOperands(_)
    ));
    kinds.sort_by_key(|kind| format!("{kind:?}"));
    kinds.dedup();
    assert_eq!(kinds.len(), 4, "four distinct violations");

    // The build entry point verifies before it generates or links anything.
    #[cfg(windows)]
    {
        use crate::linked_exe_support::{Profile, Scratch, options};
        for (label, program) in &cases {
            let scratch = Scratch::new("m1-mutated", "");
            let opts = options(&scratch, Profile::Harness, "mutated.exe");
            let inputs = vibra_build::Inputs {
                entry_outcome: 0,
                registry_minor: program.registry_minor(),
            };
            let error =
                vibra_build::build_program(program, inputs, &opts).expect_err(label);
            assert!(
                matches!(error, vibra_build::BuildError::Verify(_)),
                "{label}: {error}"
            );
            assert!(!opts.output.exists(), "{label}: no executable");
            assert!(!opts.work_dir.exists(), "{label}: no object was written");
        }
    }
}

#[cfg(windows)]
mod launcher_refusals {
    use object::{Object, ObjectSection, ObjectSymbol};

    use super::checked_fixture;
    use crate::linked_exe_support::{Profile, Scratch, WORLD, options, run_exe};

    /// Builds the fixture, edits its object file with `edit`, and links the
    /// edited object again with the same command. Returns the executable and
    /// the scratch directory that keeps it.
    fn patched(
        profile: Profile,
        edit: impl FnOnce(&mut [u8]),
    ) -> (Scratch, std::path::PathBuf) {
        let scratch = Scratch::new("m1-refused", "");
        let opts = options(&scratch, profile, "patched.exe");
        let built = vibra_build::build_executable(&checked_fixture(1000), &opts)
            .unwrap_or_else(|error| panic!("{error}"));
        let mut bytes = std::fs::read(&built.object).expect("the object");
        edit(&mut bytes);
        std::fs::write(&built.object, bytes).expect("the patched object");
        vibra_build::link_object(&built.command, &opts.work_dir, &built.executable)
            .unwrap_or_else(|error| panic!("{error}"));
        (scratch, built.executable)
    }

    /// Method: the object is a COFF file; the `object` crate finds the symbol
    /// `vibra_program`, its section, and the section's range in the file. The
    /// descriptor's `abi` is the `u32` at its offset 0 (the layout of the bridge
    /// crate documentation). Returns its position in the file and its value.
    fn abi_field(bytes: &[u8]) -> (usize, u32) {
        let file = object::File::parse(bytes).expect("a COFF object");
        let symbol = file
            .symbols()
            .find(|symbol| symbol.name() == Ok("vibra_program"))
            .expect("the object defines vibra_program");
        let section = file
            .section_by_index(symbol.section_index().expect("a section"))
            .expect("the section");
        let (start, _) = section.file_range().expect("section data");
        let offset = symbol.address() - section.address();
        let position = usize::try_from(start + offset).expect("fits");
        let value = u32::from_le_bytes(
            bytes[position..position + 4]
                .try_into()
                .expect("four bytes"),
        );
        (position, value)
    }

    /// Adds one to the `abi` of the descriptor, in place.
    fn bump_abi(bytes: &mut [u8]) {
        let (position, value) = abi_field(bytes);
        bytes[position..position + 4].copy_from_slice(&(value + 1).to_le_bytes());
    }
    /// Method: the descriptor's symbol table holds each registry row as UTF-8
    /// text. `io.stdout-text` occurs exactly once in the object (asserted), as
    /// the text of that row; its last byte becomes `u`, so the symbol keeps its
    /// length and names no registry row.
    fn rename_symbol(bytes: &mut [u8]) {
        let needle = b"io.stdout-text";
        let found = bytes
            .windows(needle.len())
            .enumerate()
            .filter(|(_, window)| window == needle)
            .map(|(position, _)| position)
            .collect::<Vec<_>>();
        assert_eq!(found.len(), 1, "a unique byte pattern: {found:?}");
        bytes[found[0] + needle.len() - 1] = b'u';
    }

    fn assert_refused_by_both_launchers(
        edit: fn(&mut [u8]),
        message: impl Fn(&str) -> bool,
    ) {
        // The shipped launcher: the exit code, standard error, and silence.
        let (_scratch, executable) = patched(Profile::System, edit);
        let ran = run_exe(&executable, &[], b"");
        let stderr = String::from_utf8_lossy(&ran.stderr).into_owned();
        assert_eq!(ran.code, Some(3), "stderr: {stderr}");
        assert!(
            stderr.starts_with("@runtime.incompatible-artifact: "),
            "{stderr}"
        );
        assert!(message(&stderr), "{stderr}");
        assert!(
            ran.stdout.is_empty(),
            "nothing was written to standard output"
        );

        // The harness launcher: the observation says refused, with no event.
        let (_scratch, executable) = patched(Profile::Harness, edit);
        let ran = run_exe(&executable, &[], WORLD.as_bytes());
        assert_eq!(ran.code, Some(0));
        assert_eq!(
            String::from_utf8_lossy(&ran.stdout),
            "(record outcome: @refused result: @none trap: @none origin: 0 audit: \
             (record format: @audit-trace.v1 events: (array)) counters: @none \
             value: @none)\n"
        );
    }

    #[test]
    fn an_object_with_another_abi_is_refused_by_the_launcher() {
        // G3.M1.11 (b).
        // The runtime's ABI is the one the unpatched object states.
        let scratch = Scratch::new("m1-abi", "");
        let built = vibra_build::build_executable(
            &checked_fixture(1000),
            &options(&scratch, Profile::System, "probe.exe"),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let (_, runtime) =
            abi_field(&std::fs::read(&built.object).expect("the object"));
        let program = runtime + 1;
        assert_refused_by_both_launchers(bump_abi, |stderr| {
            stderr.contains(&format!(
                "the program needs runtime ABI {program}, this runtime implements {runtime}"
            ))
        });
    }
    #[test]
    fn a_descriptor_naming_an_unknown_symbol_is_refused() {
        // G3.M1.11 (c).
        assert_refused_by_both_launchers(rename_symbol, |stderr| {
            stderr.contains("the program names the unknown host symbol `io.stdout-tex")
        });
    }
}

//! Milestone 1 evidence that can be gathered today (package X3-09a), one test
//! for each gate clause G3.M1.9 to G3.M1.11.
//!
//! The fixture is `examples/nexus-m1`: two modules of one package. The loop of
//! `app.ticks` counts a binary counter down in `bool` parameters (the lowering
//! slice has no integer arithmetic) and ends with an indirect tail call of
//! `finish`, a function value that `app.main` defined; `app.main` writes the
//! farewell with one `io.stdout.text`. The loop count is the constant in the
//! call of `ticks.count`, which `with_rounds` rewrites.
//!
//! What the launchers report limits where each clause is evidenced. The
//! harness launcher's observation document carries the outcome, the result
//! kind, the trap, and the audit trace; it reports neither the yield count nor
//! the peak frame count nor the peak live size. Those two clauses are therefore
//! evidenced on the reference stepper, which drives the same `Context`,
//! dispatcher, and transfer routines the linked executable uses, and each test
//! says so. The refusals are evidenced on linked executables.

use std::path::PathBuf;

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

/// The fixture's `app.main`, with the loop count of its call of `ticks.count`
/// set to `rounds`. The counter is 17 `bool` arguments, lowest bit first, each
/// on a line of its own after `finish`; `rounds` may not exceed 131,071.
fn with_rounds(main: &str, rounds: u32) -> String {
    assert!(rounds < 1 << 17, "the counter has 17 bits");
    let mut lines = main.lines().map(str::to_owned).collect::<Vec<_>>();
    let start = lines
        .iter()
        .position(|line| line.trim() == "finish")
        .expect("the call of ticks.count passes finish");
    for bit in 0..17 {
        let line = &mut lines[start + 1 + bit];
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        let closing = trimmed.len() - trimmed.trim_end_matches(')').len();
        let word = if (rounds >> bit) & 1 == 1 {
            "true"
        } else {
            "false"
        };
        *line = format!("{}{word}{}", " ".repeat(indent), ")".repeat(closing));
    }
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

fn checked_fixture(rounds: u32) -> CheckedProgram {
    let scratch = TempDir::new("nexus-m1");
    copy_directory(&fixture_directory(), scratch.path());
    let main = scratch.path().join("src").join("app").join("main.vib");
    let source = std::fs::read_to_string(&main).expect("the fixture's main");
    std::fs::write(&main, with_rounds(&source.replace("\r\n", "\n"), rounds))
        .expect("a rewritten main");
    let snapshot =
        WorkspaceSnapshot::load_confined(scratch.path()).expect("a workspace");
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

#[test]
fn quota_one_has_one_stdout_event_and_yields_at_every_checkpoint() {
    // G3.M1.9. The yield count is not reported by either launcher, so it is
    // counted on the reference stepper, which runs the same dispatcher over the
    // same `Context` as the linked executable. The linked executable's part is
    // the audit trace of its quota-1 run: exactly one `io.stdout-text`, and the
    // whole observation equal to the stepper's.
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
    linked_quota_one();
}

#[cfg(windows)]
fn linked_quota_one() {
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
}

#[test]
fn peak_frames_and_live_size_do_not_grow_with_the_loop_count() {
    // G3.M1.10, on the reference stepper: the harness launcher reports no peak
    // counter. `Context::stats().max_depth` is the peak frame count and
    // `Context::peak_size()` the peak live size in bytes.
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
             (record format: @audit-trace.v1 events: (array)))\n"
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

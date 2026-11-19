//! Generated machine code runs (package X3-07).
//!
//! These are the first tests of the project that execute code the generator
//! wrote. A program is lowered, verified, generated, and linked with the
//! runtime archive by `vibra-build`, launched as a process, and everything the
//! process exposes today is compared with the reference stepper's run of the
//! same lowered program (`linked_exe_support::assert_agrees`). Ledger N30:
//! differential tests live in this harness.
//!
//! Windows only: the driver links with MSVC `link.exe`.

use vibra_build::{
    BuildError, Inputs, build_executable, build_program, entry_outcome, link_command,
};
use vibra_exec::Program;
use vibra_lower::{LowerError, lower};

use crate::linked_exe_programs as hand;
use crate::linked_exe_support::{
    Expected, Linked, Profile, Scope, Scratch, archive, assert_agrees, options,
    run_exe, toolchain,
};
use crate::lowering_slice::{admitted_sources, checked, countdown};

/// The tooling chapter's exit codes (`docs/spec/05-tooling.md`), written here
/// from the specification and not taken from the launcher.
const OK: i32 = 0;
const TRAP: i32 = 5;
const PROGRAM_FAILED: i32 = 6;

/// Builds `program` once for each launcher profile into `scratch`.
fn link_checked(
    label: &str,
    scratch: &Scratch,
    program: &vibra_ir::CheckedProgram,
) -> Linked {
    let build = |profile, name| {
        build_executable(program, &options(scratch, profile, name))
            .unwrap_or_else(|error| panic!("{label}: {error}"))
    };
    Linked {
        system: build(Profile::System, "system.exe"),
        harness: build(Profile::Harness, "harness.exe"),
    }
}

fn link_hand(
    label: &str,
    scratch: &Scratch,
    program: &Program,
    entry_outcome: u32,
) -> Linked {
    let inputs = Inputs {
        entry_outcome,
        registry_minor: program.registry_minor(),
    };
    let build = |profile, name| {
        build_program(program, inputs, &options(scratch, profile, name))
            .unwrap_or_else(|error| panic!("{label}: {error}"))
    };
    Linked {
        system: build(Profile::System, "system.exe"),
        harness: build(Profile::Harness, "harness.exe"),
    }
}

/// Links a checked program of the slice and holds it to `expected`.
fn agree_checked(label: &str, source: &str, expected: Expected, scope: Scope) {
    let program = checked(source);
    let scratch = Scratch::new("agree", "");
    let started = std::time::Instant::now();
    let linked = link_checked(label, &scratch, &program);
    let linking = started.elapsed();
    let lowered = lower(&program).unwrap_or_else(|error| panic!("{label}: {error}"));
    let err_variant = entry_outcome(&program).checked_sub(1);
    let started = std::time::Instant::now();
    assert_agrees(label, &linked, &lowered, err_variant, expected, scope);
    eprintln!(
        "agrees: {label}: exit {}, stdout {:?}, system.exe {} bytes, two links {linking:?}, comparison {:?}",
        expected.exit_code,
        expected.stdout,
        std::fs::metadata(&linked.system.executable).map_or(0, |m| m.len()),
        started.elapsed()
    );
}

fn silent() -> Expected {
    Expected {
        exit_code: OK,
        stdout: "",
    }
}

#[test]
fn a_linked_demo_runs_as_an_exe() {
    // The demo of the lowering slice: a module value, a closure, a module
    // function used as a value, three host writes, and an early `return`.
    let (_, source) = admitted_sources()
        .into_iter()
        .find(|(name, _)| *name == "everything")
        .expect("the demo program");
    let program = checked(&source);
    let scratch = Scratch::new("demo", "");
    let linked = link_checked("demo", &scratch, &program);
    let lowered = lower(&program).expect("the demo lowers");
    let expected = Expected {
        exit_code: OK,
        stdout: "labellateend",
    };
    assert_agrees("demo", &linked, &lowered, None, expected, Scope::Full);

    for (name, built) in [("system", &linked.system), ("harness", &linked.harness)] {
        let size = std::fs::metadata(&built.executable)
            .expect("the executable exists")
            .len();
        eprintln!(
            "linked demo ({name}): {size} bytes, entry_outcome {}, registry_minor {}\n  {}",
            built.inputs.entry_outcome, built.inputs.registry_minor, built.command
        );
        assert!(size > 0);
        assert_eq!(built.inputs.entry_outcome, 2, "(result void io-error)");
    }
}

#[test]
fn linked_programs_agree_with_the_stepper() {
    // Each of these is a program the lowering slice builds and the generator
    // accepts, linked and compared with the stepper.
    let cases: Vec<(&str, String, Expected)> = vec![
        (
            "a scalar",
            "(defn main () i32 7i32)".to_owned(),
            silent(),
        ),
        (
            "a module value of the standard library read twice",
            "(defn main () i32 (let a (if true 1i32 2i32)) (if true 5i32 6i32))".to_owned(),
            silent(),
        ),
        (
            "a module value (Global) that keeps a str",
            "(def label str \"label\")\n(defn main () str (let a label) (let b label) b)".to_owned(),
            silent(),
        ),
        (
            "a scalar module value",
            "(def answer i32 41i32)\n(defn main () i32 (let a answer) (let b answer) b)".to_owned(),
            silent(),
        ),
        (
            "a host call (io.stdout-text) twice",
            "(import io @std.io)\n\
             (defn main () (result void io.io-error)\n\
             \x20 effects: (io.stdout)\n\
             \x20 (let - (io.stdout.text \"one\\n\"))\n\
             \x20 (io.stdout.text \"two\\n\"))"
                .to_owned(),
            Expected {
                exit_code: OK,
                stdout: "one\ntwo\n",
            },
        ),
        (
            "an indirect call whose callee comes from a host-writing call",
            "(import io @std.io)\n\
             (defn main () i32\n\
             \x20 effects: (io.stdout)\n\
             \x20 (let r ((choose \"first\\n\") (say \"second\\n\")))\n\
             \x20 r)\n\
             (defn choose (text str) (fn (i32) i32)\n\
             \x20 effects: (io.stdout)\n\
             \x20 (let - (io.stdout.text text))\n\
             \x20 id)\n\
             (defn say (text str) i32\n\
             \x20 effects: (io.stdout)\n\
             \x20 (let - (io.stdout.text text))\n\
             \x20 4i32)\n\
             (defn id (n i32) i32 n)"
                .to_owned(),
            Expected {
                exit_code: OK,
                stdout: "first\nsecond\n",
            },
        ),
        (
            "a tail call of a module function",
            "(defn main () i32 (leaf 5i32))\n(defn leaf (n i32) i32 n)".to_owned(),
            silent(),
        ),
        (
            "a tail call of a closure",
            "(defn main () i32 ((lambda () i32 6i32)))".to_owned(),
            silent(),
        ),
        (
            "an indirect tail call of a function value in a parameter",
            "(defn main () i32 (apply leaf 7i32))\n\
             (defn apply (f (fn (i32) i32) n i32) i32 (f n))\n\
             (defn leaf (n i32) i32 n)"
                .to_owned(),
            silent(),
        ),
        (
            "an indirect tail call of a module function used as a value",
            "(defn main () i32 ((pick) 8i32))\n\
             (defn pick () (fn (i32) i32) leaf)\n\
             (defn leaf (n i32) i32 n)"
                .to_owned(),
            silent(),
        ),
        (
            "mutual tail calls",
            "(defn main () i32 (first true))\n\
             (defn first (flag bool) i32 (if flag (second false) 1i32))\n\
             (defn second (flag bool) i32 (if flag (first false) 2i32))"
                .to_owned(),
            silent(),
        ),
        (
            "a closure that reads its captures",
            "(defn main () str\n\
             \x20 (let first \"one\")\n\
             \x20 (let second \"two\")\n\
             \x20 (let f (lambda (flag bool) str (if flag first second)))\n\
             \x20 (let a (f true))\n\
             \x20 (let b (f false))\n\
             \x20 b)"
                .to_owned(),
            silent(),
        ),
        (
            "an early return in the middle",
            "(defn main () i32 (let - (leaf)) 1i32)\n\
             (defn leaf () str (let a \"a\") (let - (if true (return a) \"x\")) \"b\")"
                .to_owned(),
            silent(),
        ),
        (
            "a deep loop: 100000 direct tail calls",
            countdown(100_000, 17, false),
            silent(),
        ),
        (
            "a deep loop: 100000 indirect tail calls through a module value",
            countdown(100_000, 17, true),
            silent(),
        ),
    ];
    for (label, source, expected) in cases {
        let scope = if label.starts_with("a deep loop") {
            Scope::Deep
        } else {
            Scope::Full
        };
        agree_checked(label, &source, expected, scope);
    }
}

#[test]
fn linked_hand_built_programs_agree_with_the_stepper() {
    // The lowering emits no `Trap` and cannot build `result.err` (package
    // X3-11), so a trap, an `err` entry, and the foundation's demo are
    // execution IR written by hand, built with `build_program`.
    let cases: Vec<(&str, Program, u32, Option<u32>, Expected)> = vec![
        (
            "the foundation demo: 1000 indirect tail transfers, one host write",
            hand::demo(1000, false),
            0,
            None,
            Expected {
                exit_code: OK,
                stdout: "done\n",
            },
        ),
        (
            "the foundation demo ending in a trap",
            hand::demo(1000, true),
            0,
            None,
            Expected {
                exit_code: TRAP,
                stdout: "done\n",
            },
        ),
        (
            "a trap at once",
            hand::trap_at_once(),
            0,
            None,
            Expected {
                exit_code: TRAP,
                stdout: "",
            },
        ),
        (
            "an entry that returns err",
            hand::entry_returns_err(),
            2,
            Some(1),
            Expected {
                exit_code: PROGRAM_FAILED,
                stdout: "",
            },
        ),
        (
            "non-tail recursion 100000 frames deep",
            hand::deep(100_000),
            0,
            None,
            silent(),
        ),
    ];
    for (label, program, outcome, err_variant, expected) in cases {
        let scratch = Scratch::new("hand", "");
        let linked = link_hand(label, &scratch, &program, outcome);
        let scope = if label.contains("deep") || label.contains("100000") {
            Scope::Deep
        } else {
            Scope::Full
        };
        assert_agrees(label, &linked, &program, err_variant, expected, scope);
    }
}

#[test]
fn the_output_path_may_contain_spaces_and_non_ascii() {
    let scratch = Scratch::new("spaces", "sa\u{ed}da de teste \u{fc}n\u{ef}");
    assert!(
        scratch.path().to_string_lossy().contains(' '),
        "the directory has a space"
    );
    // A trap, so that the exit code is a thing only this program does.
    let trap = hand::trap_at_once();
    let built = build_program(
        &trap,
        Inputs {
            entry_outcome: 0,
            registry_minor: trap.registry_minor(),
        },
        &options(
            &scratch,
            Profile::System,
            "execut\u{e1}vel \u{fc}n\u{ef}.exe",
        ),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    assert!(built.executable.is_file());
    assert!(
        built.object.starts_with(scratch.path()),
        "the object is in the work directory"
    );
    let ran = run_exe(&built.executable, &[], b"");
    assert_eq!(
        ran.code,
        Some(TRAP),
        "link: {}\nstderr: {}",
        built.command,
        String::from_utf8_lossy(&ran.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&ran.stderr),
        "@runtime.division-by-zero at tests/trap.vib:3..9\n"
    );

    // And a checked program in the same directory, which completes.
    let program = checked("(defn main () i32 7i32)");
    let built = build_executable(
        &program,
        &options(&scratch, Profile::System, "outro \u{e7}\u{e3}o.exe"),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let ran = run_exe(&built.executable, &[], b"");
    assert_eq!(ran.code, Some(OK), "link: {}", built.command);
}

#[test]
fn a_link_failure_reports_the_command_and_the_linker_output() {
    let scratch = Scratch::new("failure", "");
    let program = checked("(defn main () i32 7i32)");

    // A runtime archive that does not exist is the caller's mistake and is
    // reported before anything is written or linked: `link.exe` itself does
    // not report a missing library given by a `\\?\` path.
    let mut broken = options(&scratch, Profile::System, "missing-archive.exe");
    broken.archive = scratch.path().join("no-such-archive.lib");
    let error = build_executable(&program, &broken).expect_err("no archive");
    assert!(
        matches!(&error, BuildError::Io { path, .. } if *path == broken.archive),
        "{error}"
    );
    assert!(!broken.output.exists(), "no executable remains");
    assert!(!broken.work_dir.exists(), "nothing was written");

    // An object that is not an object, beside a valid archive, and a stale
    // executable at the output path that a failed link must not leave behind.
    let work = scratch.path().join("work");
    std::fs::create_dir_all(&work).expect("a work directory");
    let object = work.join("garbage.obj");
    std::fs::write(&object, b"this is not a COFF file").expect("an object file");
    let output_path = scratch.path().join("stale.exe");
    std::fs::write(&output_path, b"a previous build").expect("a stale executable");
    let command =
        link_command(toolchain(), &object, archive(Profile::System), &output_path);
    let error = vibra_build::link_object(&command, &work, &output_path)
        .expect_err("the link fails");
    match &error {
        BuildError::LinkFailed {
            command: reported,
            status,
            output,
        } => {
            assert_eq!(reported, &command);
            assert!(status.is_some_and(|code| code != 0));
            assert!(output.contains("LNK"), "the linker's text: {output}");
            assert!(
                output.contains("garbage.obj"),
                "the linker names the file: {output}"
            );
        }
        other => panic!("not a link failure: {other}"),
    }
    assert!(!output_path.exists(), "a failed link leaves no executable");
}

#[test]
fn the_link_command_is_the_recorded_one() {
    // The command of a real build, with this machine's paths replaced by
    // placeholders, is the literal list of the feasibility record (X3-S2).
    let scratch = Scratch::new("command", "");
    let program = checked("(defn main () i32 7i32)");
    let opts = options(&scratch, Profile::System, "recorded.exe");
    let built =
        build_executable(&program, &opts).unwrap_or_else(|error| panic!("{error}"));
    let toolchain = toolchain();
    let mut replacements: Vec<(String, &str)> = vec![
        (built.object.to_string_lossy().into_owned(), "<OBJECT>"),
        (opts.archive.to_string_lossy().into_owned(), "<ARCHIVE>"),
        (opts.output.to_string_lossy().into_owned(), "<OUT>"),
    ];
    for (directory, placeholder) in toolchain.library_dirs.iter().zip([
        "<MSVC-LIB-X64>",
        "<SDK-UCRT-X64>",
        "<SDK-UM-X64>",
    ]) {
        replacements.push((directory.to_string_lossy().into_owned(), placeholder));
    }
    let shown = built
        .command
        .args
        .iter()
        .map(|arg| {
            let mut text = arg.to_string_lossy().into_owned();
            for (path, placeholder) in &replacements {
                text = text.replace(path.as_str(), placeholder);
            }
            text
        })
        .collect::<Vec<_>>();
    assert_eq!(built.command.program, toolchain.link);
    assert_eq!(
        shown,
        [
            "/NOLOGO",
            "/MACHINE:X64",
            "/SUBSYSTEM:CONSOLE",
            "/OUT:<OUT>",
            "<OBJECT>",
            "<ARCHIVE>",
            "/LIBPATH:<MSVC-LIB-X64>",
            "/LIBPATH:<SDK-UCRT-X64>",
            "/LIBPATH:<SDK-UM-X64>",
            "kernel32.lib",
            "ntdll.lib",
            "userenv.lib",
            "ws2_32.lib",
            "dbghelp.lib",
            "/defaultlib:msvcrt",
        ],
        "link: {}",
        built.command
    );
}

#[test]
fn the_entry_outcome_is_computed_from_the_entry_result_type() {
    // `ok` is the first variant of the standard `result` and `err` the second.
    for (label, source, outcome) in [
        ("void", "(defn main () void (do))", 0),
        ("i32", "(defn main () i32 7i32)", 0),
        ("bool", "(defn main () bool true)", 0),
        (
            "a result with a record error",
            "(deftype failure (record code i32))\n(defn main () (result void failure) (result.ok))",
            2,
        ),
        (
            "a result that is err",
            "(deftype failure (record code i32))\n(defn main () (result void failure) (result.err (failure code: 1i32)))",
            2,
        ),
    ] {
        let program = checked(source);
        assert_eq!(entry_outcome(&program), outcome, "{label}");
        // And the interpreter agrees which of them is a program failure.
        let failed = vibra_interp::Interpreter::run(&program)
            .expect("runs")
            .program_failed();
        assert_eq!(failed, label == "a result that is err", "{label}");
    }
}

#[test]
fn the_slice_cannot_express_an_err_entry_yet() {
    // The reason the err program above is hand-built.
    let failing = checked(
        "(deftype failure (record code i32))\n(defn main () (result void failure) (result.err (failure code: 1i32)))",
    );
    assert!(
        matches!(lower(&failing), Err(LowerError::NotLowered(_))),
        "{:?}",
        lower(&failing)
    );
}

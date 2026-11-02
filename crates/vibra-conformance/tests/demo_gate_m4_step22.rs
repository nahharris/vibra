//! The M4 demo gate (milestone 4 Step 22), through the conformance harness.
//!
//! `examples/m4-demo` is one source. A host world supplies the program argument
//! and the file it names; the accepted program runs in the reference interpreter
//! and in the Wasm backend, and its standard output and audit trace are
//! byte-identical. The same source is rejected when its function ceiling or its
//! target array omits a performed root, each with the call witness. No path
//! grant, prompt, or runtime effect check takes part: the rejected variants are
//! refused by the checker, before any backend runs.
//!
//! `cargo test -p vibra-conformance --test demo_gate_m4_step22 -- --nocapture`
//! prints the trace the exit evidence records.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs
)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use vibra_conformance::{INSTANCE_MEMORY_LIMIT_BYTES, WasmObservation, observe_wasm};
use vibra_diagnostics::{Diagnostic, DiagnosticCode};
use vibra_host::{AuditEvent, HostWorld, WorldProvider, trace_to_vibon};
use vibra_ir::CheckedProgram;
use vibra_workspace::WorkspaceSnapshot;
use vibra_workspace::project::TargetKind;
use vibra_workspace::semantic::{CheckStatus, check_all_with_bootstrap};

static NEXT: AtomicUsize = AtomicUsize::new(0);

const WORLD: &str = "(record format: @host-world.v1 args: (array \"/in/note.txt\") files: (dict \"/in/note.txt\" \"alpha\\nbeta\\n\"))";
const STDOUT: &str = "> alpha\n> beta\n";

fn demo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/m4-demo")
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

/// A copy of the demo whose file `file` has its first `from` replaced by `to`,
/// removed on drop.
struct Variant(PathBuf);

impl Variant {
    fn new(file: &str, from: &str, to: &str) -> Self {
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir()
            .join(format!("vibra-demo22-{}-{serial}", std::process::id()));
        copy_directory(&demo(), &root);
        let path = root.join(file);
        let text = std::fs::read_to_string(&path).expect("file");
        assert!(text.contains(from), "{file} lacks {from:?}");
        std::fs::write(&path, text.replacen(from, to, 1)).expect("edit");
        Self(root)
    }
}

impl Drop for Variant {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Checked {
    status: CheckStatus,
    diagnostics: Vec<Diagnostic>,
    program: Option<CheckedProgram>,
}

fn check(root: &Path) -> Checked {
    let stdlib = vibra_types::load_stdlib().expect("stdlib");
    let snapshot = WorkspaceSnapshot::load_confined(root).expect("snapshot");
    let target = snapshot
        .project()
        .project()
        .targets()
        .iter()
        .find(|target| target.kind() == TargetKind::Bin)
        .expect("a binary target")
        .clone();
    let checked = check_all_with_bootstrap(&snapshot, Some(&stdlib));
    Checked {
        status: checked.status(),
        diagnostics: checked.diagnostics().to_vec(),
        program: checked.program_for_target(&target).cloned(),
    }
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
fn the_accepted_program_has_identical_stdout_and_audit_trace_in_both_backends() {
    let checked = check(&demo());
    assert_eq!(
        checked.status,
        CheckStatus::Accepted,
        "{:?}",
        checked.diagnostics
    );
    let program = checked.program.expect("the entry's program");
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
    let program = check(&demo()).program.expect("program");
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

fn outside_ceiling(root: &Path) -> Vec<Diagnostic> {
    let checked = check(root);
    assert_eq!(
        checked.status,
        CheckStatus::Diagnostics,
        "{:?}",
        checked.diagnostics
    );
    assert!(
        checked.program.is_none(),
        "a rejected source yields no program for a backend to run"
    );
    checked.diagnostics
}

#[test]
fn a_function_ceiling_that_omits_a_performed_root_is_rejected_with_the_witness() {
    for (omitted, ceiling, witness) in [
        (
            "@std.fs.read",
            "(env.read io.stdout)",
            "performs @std.fs.read via @std.fs.read.text",
        ),
        (
            "@std.env.read",
            "(fs.read io.stdout)",
            "performs @std.env.read via @std.env.read.args",
        ),
        (
            "@std.io.stdout",
            "(env.read fs.read)",
            "performs @std.io.stdout via @std.io.stdout.text",
        ),
    ] {
        let variant =
            Variant::new("src/app/main.vib", "(env.read fs.read io.stdout)", ceiling);
        let diagnostics = outside_ceiling(&variant.0);
        assert_eq!(diagnostics.len(), 1, "{omitted}: {diagnostics:?}");
        assert_eq!(diagnostics[0].code(), DiagnosticCode::EffectOutsideCeiling);
        assert_eq!(diagnostics[0].source_id(), Some("src/app/main.vib"));
        assert_eq!(diagnostics[0].notes(), [witness], "{omitted}");
    }
}

#[test]
fn a_target_array_that_omits_a_performed_root_is_rejected_with_the_witness() {
    for (omitted, removed, witness) in [
        (
            "@std.env.read",
            "@std.env.read ",
            "performs @std.env.read via @std.env.read.args",
        ),
        (
            "@std.fs.read",
            "@std.fs.read ",
            "performs @std.fs.read via @std.fs.read.text",
        ),
        (
            "@std.io.stdout",
            " @std.io.stdout",
            "performs @std.io.stdout via @std.io.stdout.text",
        ),
    ] {
        let variant = Variant::new("project.vibon", removed, "");
        let diagnostics = outside_ceiling(&variant.0);
        assert_eq!(diagnostics.len(), 1, "{omitted}: {diagnostics:?}");
        assert_eq!(diagnostics[0].code(), DiagnosticCode::EffectOutsideCeiling);
        assert_eq!(diagnostics[0].source_id(), Some("project.vibon"));
        let related = diagnostics[0]
            .related()
            .iter()
            .map(|related| related.message.as_str())
            .collect::<Vec<_>>();
        assert_eq!(related, [witness], "{omitted}");
    }
}

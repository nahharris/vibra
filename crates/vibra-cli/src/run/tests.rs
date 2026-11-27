//! The trap row. No trap follows from user data in checked source (06
//! "Traps"), and `vibra run` gives the engine no driver to fault, so the row
//! takes a real run of the hello program under a world and gives it the
//! ending of step 29's driver fault: the trap `@runtime.invalidHostValue` at
//! the program's `await`, no application code, and no program result. What
//! it checks is the command's side of the ending: the result, the exit
//! status, the trap record, and the envelope.

use std::fs;

use serde_json::{Value, json};
use vibra_diagnostics::{Diagnostic, DiagnosticCode};
use vibra_workspace::run::TrapRecord as Ending;
use vibra_workspace::{HostWorld, Trap};

use super::*;
use crate::render::emit;

const PROJECT: &str = "{format: @project.v2 package: {name: \"hello\" version: \"0.1.0\"} \
    targets: [{name: @hello kind: @bin root: \"src/hello\" entry: @hello.main.main \
    effects: [@std.io.StdoutText] grants: [@out: {provider: @host.io.stdout \
    resource: {kind: @processStream} after: []}]}] dependencies: [:]}";

const MAIN: &str = "use @std.io.StdoutText enduse\n\nuse @std.process.ExitCode enduse\n\n\
    proc main\n  takes {out: StdoutText}\n  returns code: ExitCode\n  \
    performs {StdoutText}\n  do\n    \
    let _ be await StdoutText.write(out \"hello\\n\") endlet\n    \
    return ExitCode(0u8)\nendproc\n";

const AWAIT: &str = "await StdoutText.write(out \"hello\\n\")";

/// F4 of the drivers review: 05 "logs to standard error". A release the
/// driver reported as failed is one line on the command's standard error, in
/// either format, and changes neither the result nor the exit status.
#[test]
fn a_failed_release_is_one_line_on_standard_error() {
    let root =
        std::env::temp_dir().join(format!("vibra-cli-run-log-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src/hello")).expect("the tree");
    fs::write(root.join("project.vibon"), PROJECT).expect("the project");
    fs::write(root.join("src/hello/main.vib"), MAIN).expect("the module");
    let capture = WorkspaceCapture::capture(&root, Discovery::Search);
    let snapshot = capture.snapshot().expect("the tree captures");
    let mut outcome = run_target(
        snapshot,
        None,
        Launch::World(&HostWorld::default()),
        MEMORY_BUDGET,
    );
    let _ = fs::remove_dir_all(&root);
    assert_eq!(outcome.result, CommandResult::Ok);
    let line = "releasing @out failed: the handle could not be closed";
    outcome.log.push(line.to_owned());
    for format in [OutputFormat::Json, OutputFormat::Human] {
        let response =
            Response::new(format, "run", CommandOutcome::Ok, Payload::Empty {});
        let response = respond(response, &outcome, "project.vibon");
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let code = emit(&response, &mut stdout, &mut stderr, &|envelope| {
            envelope.to_validated_json()
        });
        let stderr = String::from_utf8_lossy(&stderr);
        assert_eq!(code, 0, "{format:?}: {stderr}");
        assert_eq!(
            stderr.lines().filter(|seen| *seen == line).count(),
            1,
            "{format:?}: {stderr}"
        );
    }
}

#[test]
fn trap_is_exit_5_with_its_trap_record() {
    let root =
        std::env::temp_dir().join(format!("vibra-cli-run-trap-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src/hello")).expect("the tree");
    fs::write(root.join("project.vibon"), PROJECT).expect("the project");
    fs::write(root.join("src/hello/main.vib"), MAIN).expect("the module");
    let capture = WorkspaceCapture::capture(&root, Discovery::Search);
    let snapshot = capture.snapshot().expect("the tree captures");
    let mut outcome = run_target(
        snapshot,
        None,
        Launch::World(&HostWorld::default()),
        MEMORY_BUDGET,
    );
    let _ = fs::remove_dir_all(&root);
    assert_eq!(outcome.result, CommandResult::Ok);

    // The span of the `await`, counted from the source.
    let start = MAIN.find(AWAIT).expect("the await");
    let origin = SourceOrigin::new(
        "src/hello/main.vib",
        ByteSpan::new(start, start + AWAIT.len()),
    );
    outcome.result = CommandResult::Trap;
    outcome.application_code = None;
    outcome.program_result = None;
    outcome.trap = Some(Ending {
        trap: Trap::InvalidHostValue,
        origin: Some(origin.clone()),
    });
    outcome.diagnostics.push(
        Diagnostic::new(
            DiagnosticCode::RuntimeInvalidHostValue,
            origin.span(),
            "the driver answered against its gate",
        )
        .with_source_id(origin.source_id()),
    );

    let mut response = Response::new(
        OutputFormat::Json,
        "run",
        CommandOutcome::Ok,
        Payload::Empty {},
    );
    response.workspace_revision = capture.revision().map(str::to_owned);
    response
        .sources
        .insert("project.vibon".to_owned(), PROJECT.to_owned());
    response
        .sources
        .insert("src/hello/main.vib".to_owned(), MAIN.to_owned());
    let response = respond(response, &outcome, "project.vibon");
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let code = emit(&response, &mut stdout, &mut stderr, &|envelope| {
        envelope.to_validated_json()
    });
    assert_eq!(code, 5, "{}", String::from_utf8_lossy(&stderr));
    let document: Value =
        serde_json::from_slice(&stdout).expect("one envelope on standard output");
    assert_eq!(document["result"], "@command.trap");
    assert_eq!(document["status"], "unknown");
    assert_eq!(document["payload"]["applicationCode"], Value::Null);
    assert_eq!(document["payload"]["programResult"], Value::Null);
    let trap = &document["payload"]["trap"];
    assert_eq!(trap["trapCode"], "@runtime.invalidHostValue");
    assert_eq!(trap["origin"]["sourceId"], "src/hello/main.vib");
    assert_eq!(
        (&trap["origin"]["start"], &trap["origin"]["end"]),
        (&json!(start), &json!(start + AWAIT.len()))
    );
    assert_eq!(
        document["diagnostics"][0]["code"],
        "@runtime.invalidHostValue"
    );
}

use std::process::Command;

fn vibra_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_vibra"))
}

#[test]
fn emu_runs_a_hex_image_and_returns_a_json_trace() {
    let directory = tempfile::tempdir().unwrap();
    let image = directory.path().join("hello.vmi");
    std::fs::write(&image, "0x08040007\n0xDC000000\n").unwrap();

    let output = vibra_cmd()
        .args([
            "emu",
            image.to_str().unwrap(),
            "--trace",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "halted");
    assert_eq!(report["exit_code"], 7);
    assert_eq!(report["traces"].as_array().unwrap().len(), 2);
}

#[test]
fn emu_traps_are_reported_and_return_a_nonzero_exit_status() {
    let directory = tempfile::tempdir().unwrap();
    let image = directory.path().join("trap.vmi");
    std::fs::write(&image, "0xE8000000\n").unwrap();

    let output = vibra_cmd()
        .args(["emu", image.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "trapped");
    assert_eq!(report["trap"]["name"], "BADOP");
    assert!(report["traces"].as_array().unwrap().is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("BADOP"));
}

#[test]
fn emu_step_limits_are_reported_as_failures() {
    let directory = tempfile::tempdir().unwrap();
    let image = directory.path().join("loop.vmi");
    std::fs::write(&image, "0x00000000\n").unwrap();

    let output = vibra_cmd()
        .args([
            "emu",
            image.to_str().unwrap(),
            "--max-steps",
            "2",
            "--format",
            "human",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("step limit"));
}

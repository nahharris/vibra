use std::path::Path;

fn vibra_cmd() -> std::process::Command {
    std::process::Command::new(env!("CARGO_BIN_EXE_vibra"))
}

fn path(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

#[test]
fn public_commands_accept_a_typed_sexpression_project() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("src/app")).unwrap();
    std::fs::create_dir_all(root.path().join("tests")).unwrap();
    std::fs::write(
        root.path().join("project.vib"),
        "(project\n  (package \"typed-cli\" \"0.1.0\")\n  (target app kind: @bin root: \"src/app\" entry: \"main.vib\"))\n",
    )
    .unwrap();
    std::fs::write(
        root.path().join("src/app/main.vib"),
        "(defn main () void doc: \"Typed command entry.\" (do))\n",
    )
    .unwrap();
    std::fs::write(
        root.path().join("tests/typed.vib"),
        "(test.scenario \"typed-command\" (test.case \"typed-command\" profile: @core (do)))\n",
    )
    .unwrap();

    for command in [
        ["run", "src/app/main.vib"],
        ["lint", "src/app/main.vib"],
        ["test", "."],
        ["docs", "src/app/main.vib"],
        ["expand", "src/app/main.vib"],
        ["effects", "src/app/main.vib"],
    ] {
        let output = vibra_cmd()
            .current_dir(root.path())
            .args(command)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{} failed: {}",
            command.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let package = root.path().join("typed-cli.vapp");
    let build = vibra_cmd()
        .current_dir(root.path())
        .args(["build", ".", "--output", &path(&package)])
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );

    for command in [
        ["package", "verify", &path(&package)],
        ["package", "inspect", &path(&package)],
    ] {
        let output = vibra_cmd().args(command).output().unwrap();
        assert!(
            output.status.success(),
            "{} failed: {}",
            command.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

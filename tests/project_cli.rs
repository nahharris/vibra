use std::path::Path;

fn vibra_cmd() -> std::process::Command {
    std::process::Command::new(env!("CARGO_BIN_EXE_vibra"))
}

fn path_str(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

#[test]
fn docs_resolves_local_and_imported_symbols_in_all_formats() {
    let dir = tempfile::tempdir().unwrap();
    let helper = dir.path().join("helper.vibra");
    let main = dir.path().join("main.vibra");
    std::fs::write(
        &helper,
        r#"=doc: Helper module documentation.
greet:
  $function: {name: $string}
  return: $string
  =doc: |
    Return a friendly greeting.

    ```vibra
    $helper.greet: {name: Vibra}
    ```
  do:
    - $return: $args.name
"#,
    )
    .unwrap();
    std::fs::write(
        &main,
        r#"helper:
  $import: ./helper.vibra
main:
  $function: $void
  return: $void
  =doc: Run the example application.
  do: []
"#,
    )
    .unwrap();

    let plain = vibra_cmd()
        .args(["docs", &path_str(&main), "helper.greet"])
        .output()
        .unwrap();
    assert!(
        plain.status.success(),
        "{}",
        String::from_utf8_lossy(&plain.stderr)
    );
    let plain = String::from_utf8(plain.stdout).unwrap();
    assert!(plain.starts_with("Return a friendly greeting."));
    assert!(plain.contains("$helper.greet"));

    let markdown = vibra_cmd()
        .args([
            "docs",
            &path_str(&main),
            "$helper.greet",
            "--format",
            "markdown",
        ])
        .output()
        .unwrap();
    let markdown = String::from_utf8(markdown.stdout).unwrap();
    assert!(markdown.contains("## `helper.greet`"));
    assert!(markdown.contains("```vibra"));

    let json = vibra_cmd()
        .args(["docs", &path_str(&main), "--format", "json"])
        .output()
        .unwrap();
    assert!(
        json.status.success(),
        "{}",
        String::from_utf8_lossy(&json.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert!(json.as_array().unwrap().iter().any(|entry| {
        entry["symbol"] == "helper.greet"
            && entry["kind"] == "function"
            && entry["documentation"]
                .as_str()
                .unwrap()
                .contains("friendly greeting")
    }));

    let yaml = vibra_cmd()
        .args(["docs", &path_str(&main), "helper.greet", "--format", "yaml"])
        .output()
        .unwrap();
    assert!(
        yaml.status.success(),
        "{}",
        String::from_utf8_lossy(&yaml.stderr)
    );
    let yaml: serde_yaml::Value = serde_yaml::from_slice(&yaml.stdout).unwrap();
    assert_eq!(yaml["symbol"], "helper.greet");
    assert_eq!(yaml["kind"], "function");
    assert!(yaml["documentation"]
        .as_str()
        .unwrap()
        .contains("friendly greeting"));
}

#[test]
fn docs_reads_package_docs_and_requires_a_target_when_ambiguous() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("src/one")).unwrap();
    std::fs::create_dir_all(dir.path().join("src/two")).unwrap();
    std::fs::write(
        dir.path().join("project.vibra"),
        r#"manifest-version: 1
package:
  name: docs-sample
  version: 0.1.0
  =doc: Package-level documentation.
targets:
  libs:
    - name: one
      root: src/one
      entry: lib.vibra
    - name: two
      root: src/two
      entry: lib.vibra
"#,
    )
    .unwrap();
    for name in ["one", "two"] {
        std::fs::write(
            dir.path().join(format!("src/{name}/lib.vibra")),
            format!("=doc: The {name} module.\nvalue:\n  $literal: 1\n  =doc: A value.\n"),
        )
        .unwrap();
    }

    let ambiguous = vibra_cmd()
        .current_dir(dir.path())
        .args(["docs"])
        .output()
        .unwrap();
    assert!(!ambiguous.status.success());
    assert!(String::from_utf8_lossy(&ambiguous.stderr).contains("--target <name>"));

    let selected = vibra_cmd()
        .current_dir(dir.path())
        .args([
            "docs",
            ".",
            "docs-sample",
            "--target",
            "one",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        selected.status.success(),
        "{}",
        String::from_utf8_lossy(&selected.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&selected.stdout).unwrap();
    assert_eq!(json["kind"], "package");
    assert_eq!(json["documentation"], "Package-level documentation.");
}

#[test]
fn project_init_bin_template_creates_valid_project() {
    let dir = tempfile::tempdir().unwrap();

    let output = vibra_cmd()
        .current_dir(dir.path())
        .args(["init", "hello"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let init_report: serde_yaml::Value = serde_yaml::from_slice(&output.stdout).unwrap();
    assert_eq!(init_report["status"], "created");
    assert_eq!(init_report["path"], "hello");

    let project = dir.path().join("hello");
    let manifest = std::fs::read_to_string(project.join("project.vibra")).unwrap();
    let main = std::fs::read_to_string(project.join("src/hello/main.vibra")).unwrap();
    assert!(manifest.contains("manifest-version: 1"));
    assert!(main.contains("@std/io.vibra"));
    assert!(project.join("src/hello/main.vibra").exists());
    assert!(project.join("dep/std/src/io.vibra").exists());
    assert!(!project.join("dep/std/.git").exists());
    assert!(manifest.contains("git: https://github.com/nahharris/vibra-stdlib.git"));
    assert!(manifest.contains("rev: 6b9fa5838e4f4122ff141e13a5ef737e99955dad"));

    let check = vibra_cmd()
        .current_dir(dir.path())
        .args(["check", "hello"])
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "check failed: {}",
        String::from_utf8_lossy(&check.stderr)
    );

    let run = vibra_cmd()
        .current_dir(&project)
        .args(["run", "src/hello/main.vibra"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "run failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
}

#[test]
fn project_init_lib_and_workspace_templates_check() {
    let dir = tempfile::tempdir().unwrap();
    for (name, template, expected_entry) in [
        ("mylib", "lib", "src/mylib/lib.vibra"),
        ("myapp", "workspace", "src/core/lib.vibra"),
    ] {
        let init = vibra_cmd()
            .current_dir(dir.path())
            .args(["init", name, "--template", template])
            .output()
            .unwrap();
        assert!(
            init.status.success(),
            "{template} init failed: {}",
            String::from_utf8_lossy(&init.stderr)
        );
        assert!(dir.path().join(name).join(expected_entry).exists());

        let check = vibra_cmd()
            .current_dir(dir.path())
            .args(["check", name])
            .output()
            .unwrap();
        assert!(
            check.status.success(),
            "{template} check failed: {}",
            String::from_utf8_lossy(&check.stderr)
        );
    }
}

#[test]
fn project_builds_verifies_inspects_and_runs_deterministic_vapp() {
    let dir = tempfile::tempdir().unwrap();
    let init = vibra_cmd()
        .current_dir(dir.path())
        .args(["init", "hello"])
        .output()
        .unwrap();
    assert!(
        init.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );
    let first = dir.path().join("first.vapp");
    let second = dir.path().join("second.vapp");
    for (index, output) in [&first, &second].into_iter().enumerate() {
        let build = vibra_cmd()
            .current_dir(dir.path())
            .args(["build", "hello", "--output", &path_str(output)])
            .output()
            .unwrap();
        assert!(
            build.status.success(),
            "build failed: {}",
            String::from_utf8_lossy(&build.stderr)
        );
        if index == 0 {
            fn rewrite_text_as_crlf(directory: &Path) {
                for entry in std::fs::read_dir(directory).unwrap() {
                    let path = entry.unwrap().path();
                    if path.is_dir() {
                        rewrite_text_as_crlf(&path);
                    } else if matches!(
                        path.extension().and_then(|extension| extension.to_str()),
                        Some("vibra" | "yaml" | "md")
                    ) {
                        let text = std::fs::read_to_string(&path).unwrap();
                        std::fs::write(&path, text.replace("\r\n", "\n").replace('\n', "\r\n"))
                            .unwrap();
                    }
                }
            }
            rewrite_text_as_crlf(&dir.path().join("hello"));
        }
    }
    assert_eq!(
        std::fs::read(&first).unwrap(),
        std::fs::read(&second).unwrap()
    );

    let verify = vibra_cmd()
        .args(["package", "verify", &path_str(&first)])
        .output()
        .unwrap();
    assert!(
        verify.status.success(),
        "verify failed: {}",
        String::from_utf8_lossy(&verify.stderr)
    );
    let inspect = vibra_cmd()
        .args(["package", "inspect", &path_str(&first), "--format", "json"])
        .output()
        .unwrap();
    assert!(
        inspect.status.success(),
        "inspect failed: {}",
        String::from_utf8_lossy(&inspect.stderr)
    );
    let metadata: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(metadata["format-version"], 1);
    assert_eq!(metadata["runtime-abi"], "vibra-v1");
    assert_eq!(
        metadata["stdlib-rev"],
        "6b9fa5838e4f4122ff141e13a5ef737e99955dad"
    );

    let run = vibra_cmd()
        .args(["run", &path_str(&first)])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "run failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8(run.stdout).unwrap(), "Hello, World!\n");
}

#[test]
fn package_verify_rejects_tampered_vapp() {
    let dir = tempfile::tempdir().unwrap();
    assert!(vibra_cmd()
        .current_dir(dir.path())
        .args(["init", "hello"])
        .status()
        .unwrap()
        .success());
    let app = dir.path().join("hello.vapp");
    assert!(vibra_cmd()
        .current_dir(dir.path())
        .args(["build", "hello", "--output", &path_str(&app)])
        .status()
        .unwrap()
        .success());
    let mut bytes = std::fs::read(&app).unwrap();
    let needle = b"Hello, World!";
    let offset = bytes
        .windows(needle.len())
        .position(|window| window == needle)
        .expect("stored source text");
    bytes[offset] = b'J';
    std::fs::write(&app, bytes).unwrap();
    let verify = vibra_cmd()
        .args(["package", "verify", &path_str(&app)])
        .output()
        .unwrap();
    assert!(!verify.status.success());
    assert!(String::from_utf8_lossy(&verify.stderr).contains("E-PKG-009"));
}

#[test]
fn project_check_rejects_invalid_manifest_shapes() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("bad");
    std::fs::create_dir_all(project.join("src/a")).unwrap();
    std::fs::write(
        project.join("src/a/main.vibra"),
        "main:\n  $function: $void\n  return: $void\n  do: []\n",
    )
    .unwrap();
    std::fs::write(
        project.join("project.vibra"),
        r#"manifest-version: 1
package:
  name: bad
  version: 0.1.0
targets:
  libs:
    - name: dup
      root: src/a
      entry: main.vibra
  bins:
    - name: dup
      root: /tmp/outside
      entry: main.vibra
dependencies:
  remote:
    git: https://example.com/remote.git
"#,
    )
    .unwrap();

    let check = vibra_cmd()
        .current_dir(dir.path())
        .args(["check", "bad"])
        .output()
        .unwrap();
    assert!(!check.status.success());
    let stderr = String::from_utf8_lossy(&check.stderr);
    assert!(
        stderr.contains("duplicate target or dependency name `dup`")
            || stderr.contains("must be relative")
            || stderr.contains("git dependency `remote` must pin `rev`"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn project_check_rejects_abbreviated_git_revisions() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("short-rev");
    std::fs::create_dir_all(project.join("src/app")).unwrap();
    std::fs::write(project.join("src/app/main.vibra"), "main: 1\n").unwrap();
    std::fs::write(
        project.join("project.vibra"),
        r#"manifest-version: 1
package:
  name: short-rev
  version: 0.1.0
targets:
  bins:
    - name: app
      root: src/app
      entry: main.vibra
dependencies:
  dep:
    git: https://example.test/dep.git
    rev: deadbee
"#,
    )
    .unwrap();

    let check = vibra_cmd()
        .args(["check", &path_str(&project)])
        .output()
        .unwrap();
    assert!(!check.status.success());
    assert!(
        String::from_utf8_lossy(&check.stderr).contains("E-DEP-001"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&check.stderr)
    );
}

#[test]
fn project_check_resolves_local_dependency_without_copying_it() {
    let dir = tempfile::tempdir().unwrap();
    let dep = dir.path().join("local-utils");
    std::fs::create_dir_all(&dep).unwrap();
    std::fs::write(
        dep.join("util.vibra"),
        "io:\n  $import: \"@std/io.vibra\"\nanswer: 42\n",
    )
    .unwrap();
    let stdlib = Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib");

    let project = dir.path().join("app");
    std::fs::create_dir_all(project.join("src/app")).unwrap();
    std::fs::write(
        project.join("src/app/main.vibra"),
        "utils:\n  $import: \"@local-utils/util.vibra\"\nmain:\n  $function: $void\n  return: $void\n  do: []\n",
    )
    .unwrap();
    std::fs::write(
        project.join("project.vibra"),
        format!(
            r#"manifest-version: 1
package:
  name: app
  version: 0.1.0
targets:
  bins:
    - name: app
      root: src/app
      entry: main.vibra
dependencies:
  std:
    path: {}
  local-utils:
    path: {}
"#,
            path_str(&stdlib),
            path_str(&dep)
        ),
    )
    .unwrap();

    let check = vibra_cmd()
        .current_dir(dir.path())
        .args(["check", "app"])
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "check failed: {}",
        String::from_utf8_lossy(&check.stderr)
    );
    assert!(!project.join("dep/local-utils").exists());

    let run = vibra_cmd()
        .current_dir(&project)
        .args(["run", "src/app/main.vibra"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "run failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
}

#[test]
fn project_check_resolves_dependency_library_source_root() {
    let dir = tempfile::tempdir().unwrap();
    let dep = dir.path().join("packaged-utils");
    std::fs::create_dir_all(dep.join("src")).unwrap();
    std::fs::write(dep.join("src/util.vibra"), "answer: 42\n").unwrap();
    std::fs::write(
        dep.join("project.vibra"),
        r#"manifest-version: 1
package:
  name: packaged-utils
  version: 0.1.0
targets:
  libs:
    - name: packaged-utils
      root: src
      entry: util.vibra
"#,
    )
    .unwrap();

    let project = dir.path().join("app");
    std::fs::create_dir_all(project.join("src/app")).unwrap();
    std::fs::write(
        project.join("src/app/main.vibra"),
        "utils:\n  $import: \"@packaged-utils/util.vibra\"\nmain:\n  $function: $void\n  return: $void\n  do: []\n",
    )
    .unwrap();
    std::fs::write(
        project.join("project.vibra"),
        format!(
            r#"manifest-version: 1
package:
  name: app
  version: 0.1.0
targets:
  bins:
    - name: app
      root: src/app
      entry: main.vibra
dependencies:
  packaged-utils:
    path: {}
"#,
            path_str(&dep)
        ),
    )
    .unwrap();

    let check = vibra_cmd()
        .current_dir(dir.path())
        .args(["check", "app"])
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "check failed: {}",
        String::from_utf8_lossy(&check.stderr)
    );
}

#[test]
fn project_sync_clones_git_dependency_at_pinned_rev_from_relative_project_path() {
    let dir = tempfile::tempdir().unwrap();
    let remote = dir.path().join("remote-math");
    std::fs::create_dir_all(&remote).unwrap();
    std::fs::write(remote.join("math.vibra"), "pi: 3\n").unwrap();
    assert!(std::process::Command::new("git")
        .current_dir(&remote)
        .args(["init"])
        .output()
        .unwrap()
        .status
        .success());
    assert!(std::process::Command::new("git")
        .current_dir(&remote)
        .args(["add", "."])
        .output()
        .unwrap()
        .status
        .success());
    assert!(std::process::Command::new("git")
        .current_dir(&remote)
        .args([
            "-c",
            "user.name=Vibra Test",
            "-c",
            "user.email=vibra@example.test",
            "commit",
            "-m",
            "seed",
        ])
        .output()
        .unwrap()
        .status
        .success());
    let rev = std::process::Command::new("git")
        .current_dir(&remote)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(rev.status.success());
    let rev = String::from_utf8(rev.stdout).unwrap().trim().to_string();

    let project = dir.path().join("app");
    std::fs::create_dir_all(project.join("src/app")).unwrap();
    std::fs::write(
        project.join("src/app/main.vibra"),
        "math:\n  $import: \"@math/math.vibra\"\nmain:\n  $function: $void\n  return: $void\n  do: []\n",
    )
    .unwrap();
    std::fs::write(
        project.join("project.vibra"),
        format!(
            r#"manifest-version: 1
package:
  name: app
  version: 0.1.0
targets:
  bins:
    - name: app
      root: src/app
      entry: main.vibra
dependencies:
  math:
    git: {}
    rev: {}
"#,
            path_str(&remote),
            rev
        ),
    )
    .unwrap();

    let sync = vibra_cmd()
        .current_dir(dir.path())
        .args(["sync", "app"])
        .output()
        .unwrap();
    assert!(
        sync.status.success(),
        "sync failed: {}",
        String::from_utf8_lossy(&sync.stderr)
    );
    assert!(project.join("dep/math/math.vibra").exists());
    assert!(!project.join("dep/math/.git").exists());
    let lock = std::fs::read_to_string(project.join("project.lock.vibra")).unwrap();
    assert!(lock.contains("lock-version: 1"));
    assert!(lock.contains(&format!("rev: {rev}")));
    assert!(lock.contains("tree-sha256:"));
    assert!(lock.contains("vendor-path: dep/math"));
    std::fs::write(project.join("dep/math/math.vibra"), "dirty: 0\n").unwrap();

    let dirty_check = vibra_cmd()
        .current_dir(dir.path())
        .args(["check", "app"])
        .output()
        .unwrap();
    assert!(!dirty_check.status.success());
    assert!(
        String::from_utf8_lossy(&dirty_check.stderr).contains("E-DEP-004"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&dirty_check.stderr)
    );

    let resync = vibra_cmd()
        .current_dir(dir.path())
        .args(["sync", "app"])
        .output()
        .unwrap();
    assert!(
        resync.status.success(),
        "resync failed: {}",
        String::from_utf8_lossy(&resync.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(project.join("dep/math/math.vibra")).unwrap(),
        "pi: 3\n"
    );
    assert_eq!(
        std::fs::read_to_string(project.join("project.lock.vibra")).unwrap(),
        lock
    );

    let check = vibra_cmd()
        .current_dir(dir.path())
        .args(["check", "app"])
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "check failed: {}",
        String::from_utf8_lossy(&check.stderr)
    );
}

#[test]
fn project_sync_vendors_nested_diamond_dependencies_package_locally() {
    let dir = tempfile::tempdir().unwrap();
    let leaf = dir.path().join("leaf");
    let leaf_rev = create_git_package(&leaf, "leaf", "");
    let leaf_dependency = format!(
        "  leaf:\n    git: {}\n    rev: {}\n",
        path_str(&leaf),
        leaf_rev
    );
    let left = dir.path().join("left");
    let left_rev = create_git_package(&left, "left", &leaf_dependency);
    let right = dir.path().join("right");
    let right_rev = create_git_package(&right, "right", &leaf_dependency);

    let project = dir.path().join("diamond-app");
    std::fs::create_dir_all(project.join("src/app")).unwrap();
    std::fs::write(project.join("src/app/main.vibra"), "main: 1\n").unwrap();
    std::fs::write(
        project.join("project.vibra"),
        format!(
            r#"manifest-version: 1
package:
  name: diamond-app
  version: 0.1.0
targets:
  bins:
    - name: app
      root: src/app
      entry: main.vibra
dependencies:
  left:
    git: {}
    rev: {}
  right:
    git: {}
    rev: {}
"#,
            path_str(&left),
            left_rev,
            path_str(&right),
            right_rev
        ),
    )
    .unwrap();

    let sync = vibra_cmd()
        .args(["sync", &path_str(&project)])
        .output()
        .unwrap();
    assert!(
        sync.status.success(),
        "sync failed: {}",
        String::from_utf8_lossy(&sync.stderr)
    );
    assert!(project.join("dep/left/dep/leaf/src/lib.vibra").exists());
    assert!(project.join("dep/right/dep/leaf/src/lib.vibra").exists());
    assert!(!project.join("dep/left/.git").exists());
    assert!(!project.join("dep/left/dep/leaf/.git").exists());
    let lock = std::fs::read_to_string(project.join("project.lock.vibra")).unwrap();
    assert!(lock.contains("dep/left/dep/leaf"));
    assert!(lock.contains("dep/right/dep/leaf"));

    let check = vibra_cmd()
        .args(["check", &path_str(&project)])
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "check failed: {}",
        String::from_utf8_lossy(&check.stderr)
    );
}

fn create_git_package(path: &Path, name: &str, dependencies: &str) -> String {
    std::fs::create_dir_all(path.join("src")).unwrap();
    std::fs::write(path.join("src/lib.vibra"), "answer: 42\n").unwrap();
    std::fs::write(
        path.join("project.vibra"),
        format!(
            "manifest-version: 1\npackage:\n  name: {name}\n  version: 0.1.0\ntargets:\n  libs:\n    - name: {name}\n      root: src\n      entry: lib.vibra\ndependencies:\n{dependencies}"
        ),
    )
    .unwrap();
    assert!(std::process::Command::new("git")
        .current_dir(path)
        .args(["init"])
        .output()
        .unwrap()
        .status
        .success());
    assert!(std::process::Command::new("git")
        .current_dir(path)
        .args(["add", "."])
        .output()
        .unwrap()
        .status
        .success());
    assert!(std::process::Command::new("git")
        .current_dir(path)
        .args([
            "-c",
            "user.name=Vibra Test",
            "-c",
            "user.email=vibra@example.test",
            "commit",
            "-m",
            "seed",
        ])
        .output()
        .unwrap()
        .status
        .success());
    let rev = std::process::Command::new("git")
        .current_dir(path)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(rev.status.success());
    String::from_utf8(rev.stdout).unwrap().trim().to_string()
}

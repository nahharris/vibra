use std::path::Path;

fn vibra_cmd() -> std::process::Command {
    std::process::Command::new(env!("CARGO_BIN_EXE_vibra"))
}

fn path_str(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
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

    let project = dir.path().join("hello");
    let manifest = std::fs::read_to_string(project.join("project.vibra")).unwrap();
    let main = std::fs::read_to_string(project.join("src/hello/main.vibra")).unwrap();
    assert!(manifest.contains("manifest-version: 1"));
    assert!(main.contains("@std/io.vibra"));
    assert!(project.join("src/hello/main.vibra").exists());
    assert!(project.join("dep/std/src/io.vibra").exists());
    assert!(!project.join("dep/std/.git").exists());
    assert!(manifest.contains("git: https://github.com/nahharris/vibra-stdlib.git"));
    assert!(manifest.contains("rev: edc46c6eefb1c0df62b0b5fe4bace2e2f06fec31"));

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

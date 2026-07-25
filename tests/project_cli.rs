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

    let unsupported = vibra_cmd()
        .args(["docs", &path_str(&main), "--format", "yaml"])
        .output()
        .unwrap();
    assert!(!unsupported.status.success());
    assert!(String::from_utf8_lossy(&unsupported.stderr).contains("invalid value 'yaml'"));
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
    let init_report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
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
    let mut archive = zip::ZipArchive::new(std::fs::File::open(&first).unwrap()).unwrap();
    assert!(archive.by_name("package.vibra").is_err());
    let mut package_json = String::new();
    std::io::Read::read_to_string(
        &mut archive.by_name("package.json").unwrap(),
        &mut package_json,
    )
    .unwrap();
    assert!(package_json.ends_with('\n'));
    assert!(!package_json.contains('\r'));
    let decoded: serde_json::Value = serde_json::from_str(&package_json).unwrap();
    assert_eq!(decoded["format-version"], 2);
    assert!(package_json.starts_with("{\"format-version\":2,"));

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
    assert_eq!(metadata["format-version"], 2);
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

fn scalar_ffi_module(sum_bias: i32) -> Vec<u8> {
    use wasm_encoder::{
        CodeSection, ExportKind, ExportSection, Function, FunctionSection, Instruction, Module,
        TypeSection, ValType,
    };
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types
        .ty()
        .function([ValType::I32, ValType::I32], [ValType::I32]);
    types.ty().function([ValType::I32], []);
    module.section(&types);
    let mut functions = FunctionSection::new();
    functions.function(0);
    functions.function(1);
    module.section(&functions);
    let mut exports = ExportSection::new();
    exports.export("sum", ExportKind::Func, 0);
    exports.export("assert_42", ExportKind::Func, 1);
    module.section(&exports);
    let mut code = CodeSection::new();
    let mut sum = Function::new([]);
    sum.instruction(&Instruction::LocalGet(0));
    sum.instruction(&Instruction::LocalGet(1));
    sum.instruction(&Instruction::I32Add);
    sum.instruction(&Instruction::I32Const(sum_bias));
    sum.instruction(&Instruction::I32Add);
    sum.instruction(&Instruction::End);
    code.function(&sum);
    let mut assert = Function::new([]);
    assert.instruction(&Instruction::LocalGet(0));
    assert.instruction(&Instruction::I32Const(42));
    assert.instruction(&Instruction::I32Ne);
    assert.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
    assert.instruction(&Instruction::Unreachable);
    assert.instruction(&Instruction::End);
    assert.instruction(&Instruction::End);
    code.function(&assert);
    module.section(&code);
    module.finish()
}

#[test]
fn static_wasm_scalar_executes_from_source_and_deterministic_vapp() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("ffi-app");
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::create_dir_all(project.join("foreign")).unwrap();
    std::fs::write(project.join("foreign/math.wasm"), scalar_ffi_module(0)).unwrap();
    std::fs::write(
        project.join("project.vibra"),
        "manifest-version: 1\npackage:\n  name: ffi-app\n  version: 0.1.0\ntargets:\n  bins:\n    - name: ffi-app\n      root: src\n      entry: main.vibra\ndependencies:\n  math:\n    path: foreign\n    wasm: math.wasm\n",
    ).unwrap();
    std::fs::write(
        project.join("src/main.vibra"),
        r#"foreign-sum:
  $function:
    left: $int32
  args:
    right: $int32
  return: $int32
  do:
    - $wasm:
        import:
          module: "@math"
          name: sum
        args: [$args.left, $args.right]
foreign-assert:
  $function:
    value: $int32
  return: $void
  do:
    - $wasm:
        import:
          module: "@math"
          name: assert_42
        args: [$args.value]
main:
  $function: $void
  return: $void
  do:
    - $let:
        answer:
          $foreign-sum:
            left: 20
            right: 22
    - $foreign-assert: $answer
"#,
    )
    .unwrap();

    let run = vibra_cmd()
        .args(["run", &path_str(&project.join("src/main.vibra"))])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "source run failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    let first = dir.path().join("first.vapp");
    let second = dir.path().join("second.vapp");
    for output in [&first, &second] {
        let build = vibra_cmd()
            .args(["build", &path_str(&project), "--output", &path_str(output)])
            .output()
            .unwrap();
        assert!(
            build.status.success(),
            "build failed: {}",
            String::from_utf8_lossy(&build.stderr)
        );
    }
    assert_eq!(
        std::fs::read(&first).unwrap(),
        std::fs::read(&second).unwrap()
    );
    let packaged = vibra_cmd()
        .args(["run", &path_str(&first)])
        .output()
        .unwrap();
    assert!(
        packaged.status.success(),
        "packaged run failed: {}",
        String::from_utf8_lossy(&packaged.stderr)
    );

    std::fs::write(project.join("foreign/math.wasm"), scalar_ffi_module(1)).unwrap();
    let changed = dir.path().join("changed.vapp");
    let build = vibra_cmd()
        .args([
            "build",
            &path_str(&project),
            "--output",
            &path_str(&changed),
        ])
        .output()
        .unwrap();
    assert!(build.status.success());
    assert_ne!(
        std::fs::read(&first).unwrap(),
        std::fs::read(&changed).unwrap()
    );
}

fn buffer_ffi_module() -> Vec<u8> {
    use wasm_encoder::{
        CodeSection, EntityType, ExportKind, ExportSection, Function, FunctionSection,
        ImportSection, Instruction, MemArg, MemoryType, Module, TypeSection, ValType,
    };
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types
        .ty()
        .function([ValType::I32, ValType::I32], [ValType::I32]);
    types.ty().function([ValType::I32], []);
    module.section(&types);
    let mut imports = ImportSection::new();
    imports.import(
        "vibra_ffi",
        "memory",
        EntityType::Memory(MemoryType {
            minimum: 0,
            maximum: Some(2),
            memory64: false,
            shared: false,
            page_size_log2: None,
        }),
    );
    module.section(&imports);
    let mut functions = FunctionSection::new();
    functions.function(0);
    functions.function(1);
    module.section(&functions);
    let mut exports = ExportSection::new();
    exports.export("utf8_status", ExportKind::Func, 0);
    exports.export("assert_89", ExportKind::Func, 1);
    module.section(&exports);
    let mut code = CodeSection::new();
    // Status is first UTF-8 byte plus byte length. "Vï" is [86, 195, 175],
    // so the expected status is 89. This distinguishes byte length from
    // Unicode scalar count and proves the pointer addresses copied memory.
    let mut status = Function::new([]);
    status.instruction(&Instruction::LocalGet(0));
    status.instruction(&Instruction::I32Load8U(MemArg {
        offset: 0,
        align: 0,
        memory_index: 0,
    }));
    status.instruction(&Instruction::LocalGet(1));
    status.instruction(&Instruction::I32Add);
    status.instruction(&Instruction::End);
    code.function(&status);
    let mut assert = Function::new([]);
    assert.instruction(&Instruction::LocalGet(0));
    assert.instruction(&Instruction::I32Const(89));
    assert.instruction(&Instruction::I32Ne);
    assert.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
    assert.instruction(&Instruction::Unreachable);
    assert.instruction(&Instruction::End);
    assert.instruction(&Instruction::End);
    code.function(&assert);
    module.section(&code);
    module.finish()
}

#[test]
fn static_wasm_caller_owned_utf8_buffer_executes_from_source_and_vapp() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("ffi-buffer-app");
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::create_dir_all(project.join("foreign")).unwrap();
    std::fs::write(project.join("foreign/text.wasm"), buffer_ffi_module()).unwrap();
    std::fs::write(
        project.join("project.vibra"),
        "manifest-version: 1\npackage:\n  name: ffi-buffer-app\n  version: 0.1.0\ntargets:\n  bins:\n    - name: ffi-buffer-app\n      root: src\n      entry: main.vibra\ndependencies:\n  text-ffi:\n    path: foreign\n    wasm: text.wasm\n",
    ).unwrap();
    std::fs::write(
        project.join("src/main.vibra"),
        r#"foreign-status:
  $function:
    text: $str
  return: $int32
  do:
    - $wasm:
        import:
          module: "@text-ffi"
          name: utf8_status
        args: [$args.text]
foreign-assert:
  $function:
    status: $int32
  return: $void
  do:
    - $wasm:
        import:
          module: "@text-ffi"
          name: assert_89
        args: [$args.status]
main:
  $function: $void
  return: $void
  do:
    - $let:
        status:
          $foreign-status: "Vï"
    - $foreign-assert: $status
"#,
    )
    .unwrap();

    let check = vibra_cmd()
        .args(["check", &path_str(&project)])
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "check failed: {}",
        String::from_utf8_lossy(&check.stderr)
    );
    let run = vibra_cmd()
        .args(["run", &path_str(&project.join("src/main.vibra"))])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "source run failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let app = dir.path().join("buffer.vapp");
    let build = vibra_cmd()
        .args(["build", &path_str(&project), "--output", &path_str(&app)])
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    let packaged = vibra_cmd().args(["run", &path_str(&app)]).output().unwrap();
    assert!(
        packaged.status.success(),
        "packaged run failed: {}",
        String::from_utf8_lossy(&packaged.stderr)
    );
}

#[test]
fn runtime_plugin_load_is_capability_gated_typed_and_deterministic() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("plugin-host");
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::create_dir_all(project.join("plugins")).unwrap();
    std::fs::write(
        project.join("src/main.vibra"),
        "main:\n  $function: $void\n  return: $void\n  do: []\n",
    )
    .unwrap();
    std::fs::write(project.join("plugins/math.wasm"), scalar_ffi_module(0)).unwrap();
    std::fs::write(
        project.join("project.vibra"),
        "manifest-version: 1\npackage:\n  name: plugin-host\n  version: 0.1.0\ntargets:\n  bins:\n    - name: plugin-host\n      root: src\n      entry: main.vibra\nplugin-interfaces:\n  arithmetic:\n    functions:\n      sum:\n        params: [int32, int32]\n        result: int32\n",
    ).unwrap();
    let plugin = project.join("plugins/math.wasm");
    let base = vec![
        "plugin".to_string(),
        path_str(&project),
        "--interface".to_string(),
        "arithmetic".to_string(),
        "--path".to_string(),
        path_str(&plugin),
    ];
    let denied = vibra_cmd().args(&base).output().unwrap();
    assert!(!denied.status.success());
    assert!(String::from_utf8_lossy(&denied.stderr).contains("E-PLUGIN-003"));

    let mut approved_args = base.to_vec();
    approved_args.extend(["--allow-plugin-load".to_string(), path_str(&plugin)]);
    let first = vibra_cmd().args(&approved_args).output().unwrap();
    let second = vibra_cmd().args(&approved_args).output().unwrap();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert_eq!(first.stdout, second.stdout);
    let report: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(report["interface"], "arithmetic");
    assert_eq!(report["instantiated"], true);
    assert_eq!(report["report-version"], 1);
    assert_eq!(report["sha256"].as_str().unwrap().len(), 64);

    let manifest = std::fs::read_to_string(project.join("project.vibra")).unwrap();
    std::fs::write(
        project.join("project.vibra"),
        manifest.replace("params: [int32, int32]", "params: [int64, int64]"),
    )
    .unwrap();
    let mismatch = vibra_cmd().args(&approved_args).output().unwrap();
    assert!(!mismatch.status.success());
    assert!(String::from_utf8_lossy(&mismatch.stderr).contains("E-PLUGIN-006"));
}

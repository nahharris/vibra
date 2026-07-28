use serde_json::{json, Value};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn run_mcp(workspace: &Path, extra_args: &[&str], requests: &[Value]) -> Vec<Value> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_vibra"));
    command
        .arg("mcp")
        .arg("--workspace")
        .arg(workspace)
        .args(extra_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    {
        let input = child.stdin.as_mut().unwrap();
        for request in requests {
            writeln!(input, "{}", serde_json::to_string(request).unwrap()).unwrap();
        }
    }
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn call(id: u64, name: &str, arguments: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {"name": name, "arguments": arguments}
    })
}

fn write_project(root: &Path) {
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("tests")).unwrap();
    std::fs::write(
        root.join("project.vibra"),
        "(project\n  (package \"mcp-fixture\" \"0.1.0\")\n  (target app kind: bin root: \"src\" entry: \"main.vibra\"))\n",
    )
    .unwrap();
    std::fs::write(root.join("src/main.vibra"), "(fn main () void (do))\n").unwrap();
    std::fs::write(root.join("tests/basic.vibra"), "(test works core (do))\n").unwrap();
}

#[test]
fn project_and_test_introspection_are_structured_and_non_executing() {
    let workspace = tempfile::tempdir().unwrap();
    write_project(workspace.path());
    let responses = run_mcp(
        workspace.path(),
        &[],
        &[
            call(1, "vibra.project.inspect", json!({})),
            call(2, "vibra.tests.list", json!({})),
        ],
    );
    assert_eq!(
        responses[0]["result"]["structuredContent"]["package"]["name"],
        "mcp-fixture"
    );
    assert_eq!(
        responses[1]["result"]["structuredContent"]["tests"][0]["name"],
        "works"
    );
    assert_eq!(
        responses[1]["result"]["structuredContent"]["tests"][0]["path"],
        "tests/basic.vibra"
    );
}

#[test]
fn cli_tools_are_fixed_json_commands_and_escape_is_rejected() {
    let workspace = tempfile::tempdir().unwrap();
    write_project(workspace.path());
    let responses = run_mcp(
        workspace.path(),
        &[],
        &[
            call(1, "vibra.lint", json!({"paths": ["src/main.vibra"]})),
            call(2, "vibra.check", json!({"path": ".."})),
        ],
    );
    assert_eq!(responses[0]["result"]["isError"], false);
    assert!(responses[0]["result"]["structuredContent"]["output"]["summary"].is_object());
    assert_eq!(responses[1]["result"]["isError"], true);
    assert_eq!(
        responses[1]["result"]["structuredContent"]["code"],
        "path-outside-workspace"
    );
}

#[test]
fn transitive_imports_cannot_read_outside_the_workspace() {
    let parent = tempfile::tempdir().unwrap();
    let workspace = parent.path().join("project");
    std::fs::create_dir_all(&workspace).unwrap();
    write_project(&workspace);
    std::fs::write(
        parent.path().join("secret.vibra"),
        "secret: {$literal: hidden}\n",
    )
    .unwrap();
    std::fs::write(
        workspace.join("src/main.vibra"),
        "(import secret \"../../secret.vibra\")\n(fn main () void (do))\n",
    )
    .unwrap();
    let responses = run_mcp(
        &workspace,
        &[],
        &[call(1, "vibra.lint", json!({"paths": ["src/main.vibra"]}))],
    );
    assert_eq!(responses[0]["result"]["isError"], true);
    assert_eq!(
        responses[0]["result"]["structuredContent"]["code"],
        "path-outside-workspace"
    );
}

#[test]
fn s_expression_imports_reject_absolute_paths_and_allow_project_imports() {
    let parent = tempfile::tempdir().unwrap();
    let workspace = parent.path().join("project");
    std::fs::create_dir_all(&workspace).unwrap();
    write_project(&workspace);
    let secret = parent.path().join("secret.vibra");
    std::fs::write(&secret, "(const secret str \"hidden\")\n").unwrap();
    let absolute = secret.to_string_lossy().replace('\\', "\\\\");
    std::fs::write(
        workspace.join("src/main.vibra"),
        format!("(import secret \"{absolute}\")\n(fn main () void (do))\n"),
    )
    .unwrap();
    let responses = run_mcp(
        &workspace,
        &[],
        &[call(1, "vibra.lint", json!({"paths": ["src/main.vibra"]}))],
    );
    assert_eq!(responses[0]["result"]["isError"], true);
    assert_eq!(
        responses[0]["result"]["structuredContent"]["code"],
        "path-outside-workspace"
    );

    std::fs::write(workspace.join("src/main.vibra"), "(fn main () void (do))\n").unwrap();
    std::fs::write(
        workspace.join("src/feature.vibra"),
        "(import app \"@app/main.vibra\")\n(fn feature () void (do))\n",
    )
    .unwrap();
    let responses = run_mcp(
        &workspace,
        &[],
        &[call(
            2,
            "vibra.lint",
            json!({"paths": ["src/feature.vibra"]}),
        )],
    );
    assert_eq!(responses[0]["result"]["isError"], false);
}

#[test]
fn write_and_test_gates_are_explicit() {
    let workspace = tempfile::tempdir().unwrap();
    write_project(workspace.path());
    let denied = run_mcp(
        workspace.path(),
        &[],
        &[
            call(
                1,
                "vibra.fmt",
                json!({"paths": ["src/main.vibra"], "write": true}),
            ),
            call(2, "vibra.test", json!({})),
        ],
    );
    assert_eq!(
        denied[0]["result"]["structuredContent"]["code"],
        "permission-denied"
    );
    assert_eq!(
        denied[1]["result"]["structuredContent"]["code"],
        "permission-denied"
    );

    let allowed = run_mcp(
        workspace.path(),
        &["--allow-test"],
        &[call(3, "vibra.test", json!({}))],
    );
    assert_eq!(allowed[0]["result"]["isError"], false);
    assert_eq!(
        allowed[0]["result"]["structuredContent"]["output"]["passed"],
        1
    );
}

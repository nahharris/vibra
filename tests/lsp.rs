use serde_json::{json, Value};
use std::io::Cursor;

fn frame(value: Value) -> Vec<u8> {
    let body = serde_json::to_vec(&value).unwrap();
    let mut framed = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    framed.extend(body);
    framed
}

fn messages(bytes: &[u8]) -> Vec<Value> {
    let mut rest = bytes;
    let mut result = Vec::new();
    while !rest.is_empty() {
        let split = rest.windows(4).position(|v| v == b"\r\n\r\n").unwrap();
        let header = std::str::from_utf8(&rest[..split]).unwrap();
        let len: usize = header
            .strip_prefix("Content-Length: ")
            .unwrap()
            .parse()
            .unwrap();
        let body_start = split + 4;
        result.push(serde_json::from_slice(&rest[body_start..body_start + len]).unwrap());
        rest = &rest[body_start + len..];
    }
    result
}

#[test]
fn lifecycle_and_capabilities_use_lsp_framing() {
    let mut input = Vec::new();
    for value in [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"rootUri":"file:///tmp/demo"}}),
        json!({"jsonrpc":"2.0","id":2,"method":"shutdown"}),
        json!({"jsonrpc":"2.0","method":"exit"}),
    ] {
        input.extend(frame(value));
    }
    let mut output = Vec::new();
    vibra::lsp::serve(Cursor::new(input), &mut output).unwrap();
    let output = messages(&output);
    assert_eq!(output.len(), 2);
    assert_eq!(output[0]["id"], 1);
    assert_eq!(output[0]["result"]["capabilities"]["textDocumentSync"], 1);
    assert_eq!(output[1]["result"], Value::Null);
}

#[test]
fn open_document_publishes_diagnostics_and_semantic_requests_work() {
    let workspace = tempfile::tempdir().unwrap();
    let uri = format!(
        "file:///{}",
        workspace
            .path()
            .join("main.vibra")
            .to_string_lossy()
            .replace('\\', "/")
    );
    let root_uri = format!(
        "file:///{}",
        workspace.path().to_string_lossy().replace('\\', "/")
    );
    let source = "greet:\n  $function: null\n  =doc: Greets the caller\n  args: {}\n  return: string\n  do:\n    - $return: hi\nmain:\n  $function: null\n  args: {}\n  return: string\n  do:\n    - $return: $greet\n";
    let mut input = Vec::new();
    for value in [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"rootUri":root_uri}}),
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"languageId":"vibra","version":1,"text":source}}}),
        json!({"jsonrpc":"2.0","id":2,"method":"textDocument/hover","params":{"textDocument":{"uri":uri},"position":{"line":12,"character":17}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"textDocument/definition","params":{"textDocument":{"uri":uri},"position":{"line":12,"character":17}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"textDocument/references","params":{"textDocument":{"uri":uri},"position":{"line":0,"character":2}}}),
        json!({"jsonrpc":"2.0","id":5,"method":"textDocument/completion","params":{"textDocument":{"uri":uri},"position":{"line":12,"character":17}}}),
        json!({"jsonrpc":"2.0","id":6,"method":"shutdown"}),
        json!({"jsonrpc":"2.0","method":"exit"}),
    ] {
        input.extend(frame(value));
    }
    let mut output = Vec::new();
    vibra::lsp::serve(Cursor::new(input), &mut output).unwrap();
    let output = messages(&output);
    assert_eq!(output[1]["method"], "textDocument/publishDiagnostics");
    assert!(output[2]["result"]["contents"]["value"]
        .as_str()
        .unwrap()
        .contains("Greets"));
    assert_eq!(output[3]["result"]["range"]["start"]["line"], 0);
    assert_eq!(output[4]["result"].as_array().unwrap().len(), 1);
    assert!(output[5]["result"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["label"] == "greet"));
}

#[test]
fn workspace_navigation_resolves_imported_package_symbols_and_open_overlays() {
    let workspace = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(workspace.path().join("packages")).unwrap();
    let main_path = workspace.path().join("main.vibra");
    let util_path = workspace.path().join("packages/util.vibra");
    let main = "util:\n  $import: packages/util.vibra\nmain:\n  $function: null\n  args: {}\n  return: string\n  do:\n    - $return: $util.greet\n";
    let util_disk = "greet:\n  $function: null\n  =doc: Old docs\n  args: {}\n  return: string\n  do:\n    - $return: hello\n";
    let util_overlay = util_disk.replace("Old docs", "Workspace greeting");
    std::fs::write(&main_path, main).unwrap();
    std::fs::write(&util_path, util_disk).unwrap();
    let uri = |path: &std::path::Path| {
        let value = path.to_string_lossy().replace('\\', "/");
        if value.starts_with('/') {
            format!("file://{value}")
        } else {
            format!("file:///{value}")
        }
    };
    let main_uri = uri(&main_path);
    let util_uri = uri(&util_path);
    let root_uri = uri(workspace.path());
    let mut input = Vec::new();
    for value in [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"rootUri":root_uri}}),
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":main_uri,"text":main}}}),
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":util_uri,"text":util_overlay}}}),
        json!({"jsonrpc":"2.0","id":2,"method":"textDocument/definition","params":{"textDocument":{"uri":main_uri},"position":{"line":7,"character":22}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"textDocument/hover","params":{"textDocument":{"uri":main_uri},"position":{"line":7,"character":22}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"textDocument/references","params":{"textDocument":{"uri":util_uri},"position":{"line":0,"character":2}}}),
        json!({"jsonrpc":"2.0","id":5,"method":"textDocument/completion","params":{"textDocument":{"uri":main_uri},"position":{"line":7,"character":22}}}),
        json!({"jsonrpc":"2.0","id":6,"method":"shutdown"}),
        json!({"jsonrpc":"2.0","method":"exit"}),
    ] {
        input.extend(frame(value));
    }
    let mut output = Vec::new();
    vibra::lsp::serve(Cursor::new(input), &mut output).unwrap();
    let output = messages(&output);
    assert_eq!(output[3]["result"]["uri"], util_uri);
    assert_eq!(output[3]["result"]["range"]["start"]["line"], 0);
    assert!(output[4]["result"]["contents"]["value"]
        .as_str()
        .unwrap()
        .contains("Workspace greeting"));
    assert_eq!(output[5]["result"].as_array().unwrap().len(), 1);
    assert!(output[6]["result"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["label"] == "util.greet"));
}

#[test]
fn formatting_returns_a_whole_document_edit() {
    let uri = "file:///tmp/main.vibra";
    // Canonical output always uses LF, so CRLF input guarantees an edit on
    // every host without relying on emitter choices around YAML flow style.
    let source = "main:\r\n  $literal: 1\r\n";
    let mut input = Vec::new();
    for value in [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"text":source}}}),
        json!({"jsonrpc":"2.0","id":2,"method":"textDocument/formatting","params":{"textDocument":{"uri":uri},"options":{"tabSize":2,"insertSpaces":true}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"shutdown"}),
        json!({"jsonrpc":"2.0","method":"exit"}),
    ] {
        input.extend(frame(value));
    }
    let mut output = Vec::new();
    vibra::lsp::serve(Cursor::new(input), &mut output).unwrap();
    let output = messages(&output);
    assert!(output[2]["result"][0]["newText"]
        .as_str()
        .unwrap()
        .contains("$literal"));
}

#[test]
fn compile_diagnostics_follow_unsaved_project_overlays_without_writing_disk() {
    let workspace = tempfile::tempdir().unwrap();
    let manifest = "manifest-version: 1\npackage:\n  name: overlay-test\n  version: 0.1.0\ntargets:\n  bins:\n    - name: app\n      root: .\n      entry: main.vibra\ndependencies: {}\n";
    let main = "helper:\n  $import: helper.vibra\nmain:\n  $function: $void\n  return: $void\n  do:\n    - $helper.run: null\n";
    let valid = "run:\n  $function: $void\n  return: $void\n  do:\n    - $let:\n        value: 1\n";
    let broken = valid.replace("    - $let:", "    - $missing: null\n    - $let:");
    std::fs::write(workspace.path().join("project.vibra"), manifest).unwrap();
    std::fs::write(workspace.path().join("main.vibra"), main).unwrap();
    std::fs::write(workspace.path().join("helper.vibra"), valid).unwrap();
    let uri = |path: &std::path::Path| {
        let value = path.to_string_lossy().replace('\\', "/");
        if value.starts_with('/') {
            format!("file://{value}")
        } else {
            format!("file:///{value}")
        }
    };
    let root_uri = uri(workspace.path());
    let helper_uri = uri(&workspace.path().join("helper.vibra"));
    let mut input = Vec::new();
    for value in [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"rootUri":root_uri}}),
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":helper_uri,"text":broken}}}),
        json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":helper_uri},"contentChanges":[{"text":valid}]}}),
        json!({"jsonrpc":"2.0","id":2,"method":"shutdown"}),
        json!({"jsonrpc":"2.0","method":"exit"}),
    ] {
        input.extend(frame(value));
    }
    let mut output = Vec::new();
    vibra::lsp::serve(Cursor::new(input), &mut output).unwrap();
    let output = messages(&output);
    assert!(!output[1]["params"]["diagnostics"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(output[2]["params"]["diagnostics"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        std::fs::read_to_string(workspace.path().join("helper.vibra")).unwrap(),
        valid
    );
}

#[test]
fn semantic_navigation_resolves_transitive_import_aliases() {
    let workspace = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(workspace.path().join("pkg")).unwrap();
    let main="mid:\n  $import: pkg/mid.vibra\nmain:\n  $function: null\n  args: {}\n  return: string\n  do:\n    - $return: $mid.leaf.greet\n";
    let mid = "leaf:\n  $import: leaf.vibra\n";
    let leaf="greet:\n  $function: null\n  =doc: Transitive greeting\n  args: {}\n  return: string\n  do:\n    - $return: hello\n";
    let main_path = workspace.path().join("main.vibra");
    let leaf_path = workspace.path().join("pkg/leaf.vibra");
    std::fs::write(&main_path, main).unwrap();
    std::fs::write(workspace.path().join("pkg/mid.vibra"), mid).unwrap();
    std::fs::write(&leaf_path, leaf).unwrap();
    let uri = |path: &std::path::Path| {
        let value = path.to_string_lossy().replace('\\', "/");
        if value.starts_with('/') {
            format!("file://{value}")
        } else {
            format!("file:///{value}")
        }
    };
    let root_uri = uri(workspace.path());
    let main_uri = uri(&main_path);
    let leaf_uri = uri(&leaf_path);
    let mut input = Vec::new();
    for value in [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"rootUri":root_uri}}),
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":main_uri,"text":main}}}),
        json!({"jsonrpc":"2.0","id":2,"method":"textDocument/definition","params":{"textDocument":{"uri":main_uri},"position":{"line":7,"character":27}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"textDocument/completion","params":{"textDocument":{"uri":main_uri},"position":{"line":7,"character":27}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"shutdown"}),
        json!({"jsonrpc":"2.0","method":"exit"}),
    ] {
        input.extend(frame(value));
    }
    let mut output = Vec::new();
    vibra::lsp::serve(Cursor::new(input), &mut output).unwrap();
    let output = messages(&output);
    assert_eq!(output[2]["result"]["uri"], leaf_uri);
    assert!(output[3]["result"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["label"] == "mid.leaf.greet"));
}

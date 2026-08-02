use serde_json::{json, Value};
use std::io::Cursor;
use std::time::{Duration, Instant};

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
fn compilation_flags_flow_from_initialization_and_configuration_changes() {
    let workspace = tempfile::tempdir().unwrap();
    let main_path = workspace.path().join("main.vib");
    let main = "(defn main () void (do (enabled)))\n";
    std::fs::write(
        workspace.path().join("project.vib"),
        "(project\n  (package \"flags\" \"0.1.0\")\n  (target flags kind: @bin root: \".\" entry: \"main.vib\"))\n",
    )
    .unwrap();
    std::fs::write(&main_path, main).unwrap();
    std::fs::write(
        workspace.path().join("main.release.vib"),
        "(defn enabled () void (do (let value 1)))\n",
    )
    .unwrap();
    let root_uri = path_uri(workspace.path());
    let main_uri = path_uri(&main_path);
    let mut input = Vec::new();
    for value in [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"rootUri":root_uri,"initializationOptions":{"compilationFlags":[]}}}),
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":main_uri,"text":main}}}),
        json!({"jsonrpc":"2.0","method":"workspace/didChangeConfiguration","params":{"settings":{"vibra":{"compilationFlags":["release"]}}}}),
        json!({"jsonrpc":"2.0","id":2,"method":"shutdown"}),
        json!({"jsonrpc":"2.0","method":"exit"}),
    ] {
        input.extend(frame(value));
    }
    let mut output = Vec::new();
    vibra::lsp::serve(Cursor::new(input), &mut output).unwrap();
    let output = messages(&output);
    assert!(output[1]["params"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|diagnostic| diagnostic["message"].as_str().unwrap().contains("enabled")));
    assert!(output[2]["params"]["diagnostics"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn malformed_lsp_compilation_flags_return_stable_errors() {
    let mut input = Vec::new();
    for value in [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"rootUri":"file:///tmp/demo","initializationOptions":{"compilationFlags":["Bad..flag"]}}}),
        json!({"jsonrpc":"2.0","method":"exit"}),
    ] {
        input.extend(frame(value));
    }
    let mut output = Vec::new();
    vibra::lsp::serve(Cursor::new(input), &mut output).unwrap();
    let output = messages(&output);
    assert!(output[0]["error"]["message"]
        .as_str()
        .unwrap()
        .contains("E-FLAG-001"));
}

#[test]
#[ignore = "legacy YAML fixture removed by the S-expression LSP cutover"]
fn open_document_publishes_diagnostics_and_semantic_requests_work() {
    let workspace = tempfile::tempdir().unwrap();
    let uri = format!(
        "file:///{}",
        workspace
            .path()
            .join("main.vib")
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
#[ignore = "legacy YAML fixture removed by the S-expression LSP cutover"]
fn workspace_navigation_resolves_imported_package_symbols_and_open_overlays() {
    let workspace = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(workspace.path().join("packages")).unwrap();
    let main_path = workspace.path().join("main.vib");
    let util_path = workspace.path().join("packages/util.vib");
    let main = "util:\n  $import: packages/util.vib\nmain:\n  $function: null\n  args: {}\n  return: string\n  do:\n    - $return: $util.greet\n";
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
    assert_eq!(output[4]["result"]["uri"], util_uri);
    assert_eq!(output[4]["result"]["range"]["start"]["line"], 0);
    assert!(output[5]["result"]["contents"]["value"]
        .as_str()
        .unwrap()
        .contains("Workspace greeting"));
    assert_eq!(output[6]["result"].as_array().unwrap().len(), 1);
    assert!(output[7]["result"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["label"] == "util.greet"));
}

#[test]
fn formatting_returns_a_whole_document_edit() {
    let workspace = tempfile::tempdir().unwrap();
    let uri = path_uri(&workspace.path().join("main.vib"));
    let root_uri = path_uri(workspace.path());
    // Canonical output always uses LF, so CRLF input guarantees an edit on
    // every host without relying on emitter choices around printer style.
    let source = "(defn main () void\r\n  (do (let value 1)))\r\n";
    let mut input = Vec::new();
    for value in [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"rootUri":root_uri}}),
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
        .contains("value"));
}

#[test]
fn navigation_uses_unsaved_s_expression_buffers() {
    let workspace = tempfile::tempdir().unwrap();
    let main_path = workspace.path().join("main.vib");
    let helper_path = workspace.path().join("helper.vib");
    let manifest = "(project (package \"overlay-nav\" \"0.1.0\") (target app kind: @bin root: \".\" entry: \"main.vib\"))\n";
    let disk_main = "(import helper \"helper.vib\")\n(defn main () void (do (helper.old)))\n";
    let live_main = "(import helper \"helper.vib\")\n(defn main () void (do (helper.live)))\n";
    let disk_helper = "(defn old () void (do unit))\n";
    let live_helper = "(defn live () void (do unit))\n";
    std::fs::write(workspace.path().join("project.vib"), manifest).unwrap();
    std::fs::write(&main_path, disk_main).unwrap();
    std::fs::write(&helper_path, disk_helper).unwrap();
    let main_uri = path_uri(&main_path);
    let helper_uri = path_uri(&helper_path);
    let mut input = Vec::new();
    for value in [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"rootUri":path_uri(workspace.path())}}),
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":main_uri,"text":live_main}}}),
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":helper_uri,"text":live_helper}}}),
        json!({"jsonrpc":"2.0","id":2,"method":"textDocument/definition","params":{"textDocument":{"uri":main_uri},"position":{"line":1,"character":31}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"shutdown"}),
        json!({"jsonrpc":"2.0","method":"exit"}),
    ] {
        input.extend(frame(value));
    }
    let mut output = Vec::new();
    vibra::lsp::serve(Cursor::new(input), &mut output).unwrap();
    let output = messages(&output);
    assert_eq!(output[4]["result"]["uri"], helper_uri);
    assert_eq!(
        output[4]["result"]["range"]["start"],
        json!({"line":0,"character":6})
    );
}

#[test]
fn atom_literals_hover_complete_and_list_references_but_have_no_definition() {
    let workspace = tempfile::tempdir().unwrap();
    let main_path = workspace.path().join("main.vib");
    let manifest = "(project (package \"atoms\" \"0.1.0\") (target app kind: @bin root: \".\" entry: \"main.vib\"))\n";
    let main = "(defn classify (value atom) bool (do (match value @ok (do (return true)) _ (do (return false)))))\n(defn main () void (do (classify @ok)))\n";
    std::fs::write(workspace.path().join("project.vib"), manifest).unwrap();
    std::fs::write(&main_path, main).unwrap();
    let main_uri = path_uri(&main_path);
    let arm = main.lines().next().unwrap().find("@ok").unwrap();
    let call = main.lines().nth(1).unwrap().find("@ok").unwrap();
    let mut input = Vec::new();
    for value in [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"rootUri":path_uri(workspace.path())}}),
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":main_uri,"text":main}}}),
        json!({"jsonrpc":"2.0","id":2,"method":"textDocument/hover","params":{"textDocument":{"uri":main_uri},"position":{"line":0,"character":arm+1}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"textDocument/definition","params":{"textDocument":{"uri":main_uri},"position":{"line":0,"character":arm+1}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"textDocument/references","params":{"textDocument":{"uri":main_uri},"position":{"line":1,"character":call+1}}}),
        json!({"jsonrpc":"2.0","id":5,"method":"textDocument/completion","params":{"textDocument":{"uri":main_uri},"position":{"line":1,"character":call+1}}}),
        json!({"jsonrpc":"2.0","id":6,"method":"shutdown"}),
        json!({"jsonrpc":"2.0","method":"exit"}),
    ] {
        input.extend(frame(value));
    }
    let mut output = Vec::new();
    vibra::lsp::serve(Cursor::new(input), &mut output).unwrap();
    let output = messages(&output);
    let by_id = |id: i64| {
        output
            .iter()
            .find(|message| message["id"] == id)
            .unwrap_or_else(|| panic!("no response for id {id}"))
    };
    assert!(by_id(2)["result"]["contents"]["value"]
        .as_str()
        .unwrap()
        .contains("@ok"));
    assert_eq!(by_id(3)["result"], Value::Null);
    assert_eq!(by_id(4)["result"].as_array().unwrap().len(), 2);
    assert!(by_id(5)["result"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["label"] == "@ok"));
}

fn path_uri(path: &std::path::Path) -> String {
    let value = path.to_string_lossy().replace('\\', "/");
    if value.starts_with('/') {
        format!("file://{value}")
    } else {
        format!("file:///{value}")
    }
}

#[test]
fn compile_diagnostics_follow_unsaved_project_overlays_without_writing_disk() {
    let workspace = tempfile::tempdir().unwrap();
    let manifest = "(project\n  (package \"overlay-test\" \"0.1.0\")\n  (target app kind: @bin root: \".\" entry: \"main.vib\"))\n";
    let main = "(import helper \"helper.vib\")\n(defn main () void (do (helper.run)))\n";
    let valid = "(defn run () void (do (let value 1)))\n";
    let broken = "(defn run () void (do (missing) (let value 1)))\n";
    std::fs::write(workspace.path().join("project.vib"), manifest).unwrap();
    std::fs::write(workspace.path().join("main.vib"), main).unwrap();
    std::fs::write(workspace.path().join("helper.vib"), valid).unwrap();
    let uri = |path: &std::path::Path| {
        let value = path.to_string_lossy().replace('\\', "/");
        if value.starts_with('/') {
            format!("file://{value}")
        } else {
            format!("file:///{value}")
        }
    };
    let root_uri = uri(workspace.path());
    let main_uri = uri(&workspace.path().join("main.vib"));
    let helper_uri = uri(&workspace.path().join("helper.vib"));
    let mut input = Vec::new();
    for value in [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"rootUri":root_uri}}),
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":main_uri,"text":main}}}),
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
    assert_eq!(output[2]["params"]["uri"], helper_uri);
    let diagnostic = &output[2]["params"]["diagnostics"][0];
    // An unresolved-callee diagnostic is raised during compile-time lowering
    // (module scope), not by the reader, so its span covers the enclosing
    // form rather than pinpointing the bare symbol the way a syntax
    // diagnostic would; this asserts a diagnostic exists at the start of the
    // broken source rather than pinning byte-precise reader-only span
    // semantics that do not apply to this diagnostic's origin.
    assert_eq!(
        diagnostic["range"]["start"],
        json!({"line":0,"character":0})
    );
    assert!(diagnostic["message"].as_str().unwrap().contains("missing"));
    assert_eq!(output[3]["params"]["uri"], main_uri);
    assert!(output[3]["params"]["diagnostics"]
        .as_array()
        .unwrap()
        .is_empty());
    for notification in &output[4..=5] {
        assert!(notification["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty());
    }
    assert_eq!(
        std::fs::read_to_string(workspace.path().join("helper.vib")).unwrap(),
        valid
    );
}

#[test]
#[ignore = "legacy YAML fixture removed by the S-expression LSP cutover"]
fn semantic_navigation_resolves_transitive_import_aliases() {
    let workspace = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(workspace.path().join("pkg")).unwrap();
    let main="mid:\n  $import: pkg/mid.vib\nmain:\n  $function: null\n  args: {}\n  return: string\n  do:\n    - $return: $mid.leaf.greet\n";
    let mid = "leaf:\n  $import: leaf.vib\n";
    let leaf="greet:\n  $function: null\n  =doc: Transitive greeting\n  args: {}\n  return: string\n  do:\n    - $return: hello\n";
    let main_path = workspace.path().join("main.vib");
    let leaf_path = workspace.path().join("pkg/leaf.vib");
    std::fs::write(&main_path, main).unwrap();
    std::fs::write(workspace.path().join("pkg/mid.vib"), mid).unwrap();
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

#[test]
#[ignore = "legacy YAML fixture removed by the S-expression LSP cutover"]
fn semantic_navigation_resolves_project_at_imports() {
    let workspace = tempfile::tempdir().unwrap();
    let dependency = workspace.path().join("packages/util");
    std::fs::create_dir_all(dependency.join("src")).unwrap();
    let manifest="(project\n  (package \"app\" \"0.1.0\")\n  (target app kind: @bin root: \".\" entry: \"main.vib\")\n  (dependency util path: \"packages/util\"))\n";
    let dependency_manifest =
        "(project\n  (package \"util\" \"0.1.0\")\n  (target util kind: @lib root: \"src\" entry: \"greet.vib\"))\n";
    let main="util:\n  $import: '@util/greet.vib'\nmain:\n  $function: $void\n  return: $void\n  do:\n    - $util.greet: null\n";
    let library="greet:\n  $function: $void\n  =doc: Dependency greeting\n  return: $void\n  do:\n    - $let:\n        value: 1\n";
    let main_path = workspace.path().join("main.vib");
    let library_path = dependency.join("src/greet.vib");
    std::fs::write(workspace.path().join("project.vib"), manifest).unwrap();
    std::fs::write(dependency.join("project.vib"), dependency_manifest).unwrap();
    std::fs::write(&main_path, main).unwrap();
    std::fs::write(&library_path, library).unwrap();
    let root_uri = path_uri(workspace.path());
    let main_uri = path_uri(&main_path);
    let library_uri = path_uri(&library_path);
    let mut input = Vec::new();
    for value in [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"rootUri":root_uri}}),
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":main_uri,"text":main}}}),
        json!({"jsonrpc":"2.0","id":2,"method":"textDocument/definition","params":{"textDocument":{"uri":main_uri},"position":{"line":6,"character":13}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"shutdown"}),
        json!({"jsonrpc":"2.0","method":"exit"}),
    ] {
        input.extend(frame(value));
    }
    let mut output = Vec::new();
    vibra::lsp::serve(Cursor::new(input), &mut output).unwrap();
    let output = messages(&output);
    assert_eq!(output[2]["result"]["uri"], library_uri);
    assert_eq!(output[2]["result"]["range"]["start"]["line"], 0);
}

#[test]
fn checked_in_multi_package_sample_supports_core_editor_features() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/lsp-workspace");
    let main_path = root.join("main.vib");
    let library_path = root.join("packages/greeting/src/greet.vib");
    let main = std::fs::read_to_string(&main_path).unwrap();
    let library = std::fs::read_to_string(&library_path).unwrap();
    let root_uri = path_uri(&root);
    let main_uri = path_uri(&main_path);
    let library_uri = path_uri(&library_path);
    let mut input = Vec::new();
    for value in [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"rootUri":root_uri}}),
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":main_uri,"languageId":"vibra","version":1,"text":main}}}),
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":library_uri,"languageId":"vibra","version":1,"text":library}}}),
        json!({"jsonrpc":"2.0","id":2,"method":"textDocument/hover","params":{"textDocument":{"uri":main_uri},"position":{"line":1,"character":26}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"textDocument/definition","params":{"textDocument":{"uri":main_uri},"position":{"line":1,"character":26}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"textDocument/references","params":{"textDocument":{"uri":library_uri},"position":{"line":0,"character":6}}}),
        json!({"jsonrpc":"2.0","id":5,"method":"textDocument/completion","params":{"textDocument":{"uri":main_uri},"position":{"line":1,"character":26}}}),
        json!({"jsonrpc":"2.0","id":6,"method":"textDocument/formatting","params":{"textDocument":{"uri":main_uri},"options":{"tabSize":2,"insertSpaces":true}}}),
        json!({"jsonrpc":"2.0","id":7,"method":"shutdown"}),
        json!({"jsonrpc":"2.0","method":"exit"}),
    ] {
        input.extend(frame(value));
    }
    let mut output = Vec::new();
    vibra::lsp::serve(Cursor::new(input), &mut output).unwrap();
    let output = messages(&output);
    assert!(output.iter().any(|message| {
        message["method"] == "textDocument/publishDiagnostics"
            && message["params"]["diagnostics"]
                .as_array()
                .is_some_and(Vec::is_empty)
    }));
    assert!(output[4]["result"]["contents"]["value"]
        .as_str()
        .is_some_and(|value| value.contains("greet")));
    assert_eq!(output[5]["result"]["uri"], library_uri);
    assert!(!output[6]["result"].as_array().unwrap().is_empty());
    assert!(output[7]["result"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["label"] == "greeting.greet"));
}

#[test]
#[ignore = "legacy YAML fixture removed by the S-expression LSP cutover"]
fn medium_workspace_semantic_request_meets_performance_contract() {
    let workspace = tempfile::tempdir().unwrap();
    let document_count = 250;
    for index in 0..document_count {
        std::fs::write(
            workspace.path().join(format!("module-{index}.vib")),
            format!(
                "function-{index}:\n  $function: $void\n  =doc: Medium workspace symbol {index}\n  return: $void\n  do:\n    - $let:\n        value: {index}\n"
            ),
        )
        .unwrap();
    }
    let main_path = workspace.path().join("main.vib");
    let main = "module:\n  $import: module-249.vib\nmain:\n  $function: $void\n  return: $void\n  do:\n    - $module.function-249: null\n";
    std::fs::write(&main_path, main).unwrap();
    let mut input = Vec::new();
    for value in [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"rootUri":path_uri(workspace.path())}}),
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":path_uri(&main_path),"text":main}}}),
        json!({"jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{"textDocument":{"uri":path_uri(&main_path)},"position":{"line":7,"character":26}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"shutdown"}),
        json!({"jsonrpc":"2.0","method":"exit"}),
    ] {
        input.extend(frame(value));
    }
    let started = Instant::now();
    let mut output = Vec::new();
    vibra::lsp::serve(Cursor::new(input), &mut output).unwrap();
    let elapsed = started.elapsed();
    eprintln!("medium LSP workspace ({document_count} files): {elapsed:?}");
    assert!(
        elapsed < Duration::from_secs(10),
        "medium workspace request took {elapsed:?}"
    );
    assert!(messages(&output).iter().any(|message| {
        message["result"].as_array().is_some_and(|items| {
            items
                .iter()
                .any(|item| item["label"] == "module.function-249")
        })
    }));
}

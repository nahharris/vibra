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
    let uri = "file:///tmp/main.vibra";
    let source = "=doc: Greets the caller\ngreet:\n  $function: null\n  args: {}\n  return: string\n  do:\n    - $return: hi\nmain:\n  $function: null\n  args: {}\n  return: string\n  do:\n    - $return: $greet\n";
    let mut input = Vec::new();
    for value in [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"languageId":"vibra","version":1,"text":source}}}),
        json!({"jsonrpc":"2.0","id":2,"method":"textDocument/hover","params":{"textDocument":{"uri":uri},"position":{"line":12,"character":17}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"textDocument/definition","params":{"textDocument":{"uri":uri},"position":{"line":12,"character":17}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"textDocument/references","params":{"textDocument":{"uri":uri},"position":{"line":1,"character":2}}}),
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
    assert_eq!(output[3]["result"]["range"]["start"]["line"], 1);
    assert_eq!(output[4]["result"].as_array().unwrap().len(), 1);
    assert!(output[5]["result"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["label"] == "greet"));
}

#[test]
fn formatting_returns_a_whole_document_edit() {
    let uri = "file:///tmp/main.vibra";
    let source = "main:\n    $literal: 1\n";
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

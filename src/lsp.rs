//! Minimal dependency-free Language Server Protocol implementation.

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;

#[derive(Default)]
struct Server {
    root: Option<PathBuf>,
    documents: BTreeMap<String, String>,
    shutdown: bool,
}

/// Serve LSP over stdin/stdout until an `exit` notification is received.
pub fn run_stdio() -> Result<()> {
    serve(std::io::stdin().lock(), std::io::stdout().lock())
}

pub fn serve<R: Read, W: Write>(input: R, mut output: W) -> Result<()> {
    let mut input = BufReader::new(input);
    let mut server = Server::default();
    while let Some(message) = read_message(&mut input)? {
        let method = message.get("method").and_then(Value::as_str).unwrap_or("");
        let id = message.get("id").cloned();
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        if method == "exit" {
            break;
        }
        let result = server.handle(method, &params);
        if let Some(id) = id {
            let response = match result {
                Ok(result) => json!({"jsonrpc":"2.0", "id":id, "result":result}),
                Err(error) => {
                    json!({"jsonrpc":"2.0", "id":id, "error":{"code":-32603, "message":error.to_string()}})
                }
            };
            write_message(&mut output, &response)?;
        } else if let Ok(notifications) = result {
            if let Some(items) = notifications.as_array() {
                for notification in items {
                    write_message(&mut output, notification)?;
                }
            }
        }
    }
    Ok(())
}

impl Server {
    fn handle(&mut self, method: &str, params: &Value) -> Result<Value> {
        if self.shutdown && method != "exit" {
            bail!("server has shut down");
        }
        match method {
            "initialize" => {
                self.root = params
                    .get("rootUri")
                    .and_then(Value::as_str)
                    .and_then(uri_path);
                Ok(json!({
                    "serverInfo":{"name":"vibra","version":env!("CARGO_PKG_VERSION")},
                    "capabilities":{
                        "textDocumentSync":1,
                        "documentFormattingProvider":true,
                        "hoverProvider":true,
                        "definitionProvider":true,
                        "referencesProvider":true,
                        "completionProvider":{"triggerCharacters":["$", "."]}
                    }
                }))
            }
            "initialized" => Ok(json!([])),
            "shutdown" => {
                self.shutdown = true;
                Ok(Value::Null)
            }
            "textDocument/didOpen" => {
                let doc = &params["textDocument"];
                self.documents
                    .insert(string(&doc["uri"])?, string(&doc["text"])?);
                self.diagnostic_notifications(string(&doc["uri"])?)
            }
            "textDocument/didChange" => {
                let uri = string(&params["textDocument"]["uri"])?;
                let text = params["contentChanges"]
                    .as_array()
                    .and_then(|v| v.last())
                    .and_then(|v| v.get("text"))
                    .and_then(Value::as_str)
                    .context("full document change requires text")?;
                self.documents.insert(uri.clone(), text.to_string());
                self.diagnostic_notifications(uri)
            }
            "textDocument/didClose" => {
                let uri = string(&params["textDocument"]["uri"])?;
                self.documents.remove(&uri);
                Ok(
                    json!([{"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{"uri":uri,"diagnostics":[]}}]),
                )
            }
            "textDocument/formatting" => {
                let (_uri, source) = self.source(params)?;
                let formatted = crate::tooling::format_source(&source)?;
                if formatted == source {
                    return Ok(json!([]));
                }
                Ok(
                    json!([{"range":{"start":{"line":0,"character":0},"end":end_position(&source)},"newText":formatted}]),
                )
            }
            "textDocument/completion" => {
                let (_, source) = self.source(params)?;
                let symbols = definitions(&source).into_iter().map(|(name, line, _)|
                    json!({"label":name,"kind":6,"detail":format!("Vibra symbol (line {})",line+1)})).collect::<Vec<_>>();
                Ok(Value::Array(symbols))
            }
            "textDocument/hover" => self.hover(params),
            "textDocument/definition" => self.definition(params),
            "textDocument/references" => self.references(params),
            _ => {
                if params.is_null() {
                    Ok(Value::Null)
                } else {
                    Ok(Value::Null)
                }
            }
        }
    }

    fn source(&self, params: &Value) -> Result<(String, String)> {
        let uri = string(&params["textDocument"]["uri"])?;
        let source = self
            .documents
            .get(&uri)
            .cloned()
            .or_else(|| uri_path(&uri).and_then(|p| fs::read_to_string(p).ok()))
            .context("document is not open and cannot be read")?;
        Ok((uri, source))
    }

    fn diagnostic_notifications(&self, uri: String) -> Result<Value> {
        let source = self.documents.get(&uri).context("open document missing")?;
        let path = uri_path(&uri).unwrap_or_else(|| PathBuf::from("document.vibra"));
        let diagnostics = crate::tooling::diagnostics_for_source(&path, source).into_iter().map(|d| json!({
            "range":{"start":{"line":d.span.start.line,"character":d.span.start.column},"end":{"line":d.span.end.line,"character":d.span.end.column}},
            "severity":match d.severity { crate::tooling::Severity::Error=>1, crate::tooling::Severity::Warning=>2, crate::tooling::Severity::Info=>3, crate::tooling::Severity::Hint=>4 },
            "code":d.code,"source":"vibra","message":d.message
        })).collect::<Vec<_>>();
        Ok(
            json!([{"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{"uri":uri,"diagnostics":diagnostics}}]),
        )
    }

    fn hover(&self, params: &Value) -> Result<Value> {
        let (_, source) = self.source(params)?;
        let pos = position(params)?;
        let Some(word) = word_at(&source, pos.0, pos.1) else {
            return Ok(Value::Null);
        };
        let name = word
            .trim_start_matches('$')
            .split('.')
            .next_back()
            .unwrap_or(&word);
        let Some((_, _, doc)) = definitions(&source)
            .into_iter()
            .find(|(candidate, _, _)| candidate == name)
        else {
            return Ok(Value::Null);
        };
        Ok(
            json!({"contents":{"kind":"markdown","value":if doc.is_empty(){format!("`{name}` — Vibra symbol")}else{doc}}}),
        )
    }

    fn definition(&self, params: &Value) -> Result<Value> {
        let (uri, source) = self.source(params)?;
        let pos = position(params)?;
        let Some(word) = word_at(&source, pos.0, pos.1) else {
            return Ok(Value::Null);
        };
        let name = word
            .trim_start_matches('$')
            .split('.')
            .next_back()
            .unwrap_or(&word);
        match definitions(&source)
            .into_iter()
            .find(|(candidate, _, _)| candidate == name)
        {
            Some((_, line, _)) => Ok(location(&uri, line, 0, name.len())),
            None => Ok(Value::Null),
        }
    }

    fn references(&self, params: &Value) -> Result<Value> {
        let (uri, source) = self.source(params)?;
        let pos = position(params)?;
        let Some(word) = word_at(&source, pos.0, pos.1) else {
            return Ok(json!([]));
        };
        let name = word
            .trim_start_matches('$')
            .split('.')
            .next_back()
            .unwrap_or(&word);
        let needle = format!("${name}");
        let mut result = Vec::new();
        for (line, text) in source.lines().enumerate() {
            let mut start = 0;
            while let Some(found) = text[start..].find(&needle) {
                let column = start + found;
                result.push(location(&uri, line, column, needle.len()));
                start = column + needle.len();
            }
        }
        Ok(Value::Array(result))
    }
}

fn definitions(source: &str) -> Vec<(String, usize, String)> {
    let mut result = Vec::new();
    let mut pending_doc = String::new();
    for (line, text) in source.lines().enumerate() {
        let trimmed = text.trim();
        if let Some(doc) = trimmed.strip_prefix("=doc:") {
            pending_doc = doc.trim().trim_matches('"').to_string();
            continue;
        }
        if text.starts_with(|c: char| !c.is_whitespace()) {
            if let Some((key, _)) = text.split_once(':') {
                if !key.starts_with(['=', '$']) && !key.is_empty() {
                    result.push((key.to_string(), line, std::mem::take(&mut pending_doc)));
                }
            }
        }
    }
    result
}
fn position(params: &Value) -> Result<(usize, usize)> {
    Ok((
        params["position"]["line"]
            .as_u64()
            .context("position.line")? as usize,
        params["position"]["character"]
            .as_u64()
            .context("position.character")? as usize,
    ))
}
fn word_at(source: &str, line: usize, column: usize) -> Option<String> {
    let text = source.lines().nth(line)?;
    let bytes = text.as_bytes();
    let mut a = column.min(bytes.len());
    let mut b = a;
    while a > 0 && is_word(bytes[a - 1]) {
        a -= 1
    }
    while b < bytes.len() && is_word(bytes[b]) {
        b += 1
    }
    (a < b).then(|| text[a..b].to_string())
}
fn is_word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'$' | b'-' | b'_' | b'.')
}
fn location(uri: &str, line: usize, column: usize, len: usize) -> Value {
    json!({"uri":uri,"range":{"start":{"line":line,"character":column},"end":{"line":line,"character":column+len}}})
}
fn end_position(source: &str) -> Value {
    let line = source.lines().count().saturating_sub(1);
    let character = source.lines().last().map(str::len).unwrap_or(0);
    json!({"line":line,"character":character})
}
fn string(value: &Value) -> Result<String> {
    value
        .as_str()
        .map(str::to_string)
        .context("expected string")
}
fn uri_path(uri: &str) -> Option<PathBuf> {
    let value = uri.strip_prefix("file://")?;
    let value = value
        .strip_prefix('/')
        .filter(|_| cfg!(windows))
        .unwrap_or(value);
    Some(PathBuf::from(value.replace("%20", " ")))
}
fn read_message<R: BufRead>(input: &mut R) -> Result<Option<Value>> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = Some(value.trim().parse::<usize>()?)
        }
    }
    let mut body = vec![0; length.context("missing Content-Length")?];
    input.read_exact(&mut body)?;
    Ok(Some(serde_json::from_slice(&body)?))
}
fn write_message<W: Write>(output: &mut W, value: &Value) -> Result<()> {
    let body = serde_json::to_vec(value)?;
    write!(output, "Content-Length: {}\r\n\r\n", body.len())?;
    output.write_all(&body)?;
    output.flush()?;
    Ok(())
}

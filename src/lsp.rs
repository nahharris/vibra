//! Minimal dependency-free Language Server Protocol implementation.

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;

use crate::sexpr_semantic::{SemanticIndex, SemanticKind};

struct Server {
    root: Option<PathBuf>,
    documents: BTreeMap<String, String>,
    compilation_flags: crate::load::CompilationFlags,
    shutdown: bool,
}

/// Serve LSP over stdin/stdout until an `exit` notification is received.
pub fn run_stdio() -> Result<()> {
    run_stdio_with_flags(crate::load::CompilationFlags::default())
}

pub fn run_stdio_with_flags(flags: crate::load::CompilationFlags) -> Result<()> {
    serve_with_flags(std::io::stdin().lock(), std::io::stdout().lock(), flags)
}

pub fn serve<R: Read, W: Write>(input: R, mut output: W) -> Result<()> {
    serve_with_flags(input, &mut output, crate::load::CompilationFlags::default())
}

pub fn serve_with_flags<R: Read, W: Write>(
    input: R,
    mut output: W,
    flags: crate::load::CompilationFlags,
) -> Result<()> {
    let mut input = BufReader::new(input);
    let mut server = Server {
        root: None,
        documents: BTreeMap::new(),
        compilation_flags: flags,
        shutdown: false,
    };
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
                if let Some(flags) = params
                    .pointer("/initializationOptions/compilationFlags")
                    .filter(|value| !value.is_null())
                {
                    self.compilation_flags = compilation_flags(flags)?;
                }
                Ok(json!({
                    "serverInfo":{"name":"vibra","version":env!("CARGO_PKG_VERSION")},
                    "capabilities":{
                        "textDocumentSync":1,
                        "documentFormattingProvider":true,
                        "hoverProvider":true,
                        "definitionProvider":true,
                        "referencesProvider":true,
                        "documentSymbolProvider":true,
                        "completionProvider":{"triggerCharacters":["$", "."]}
                    }
                }))
            }
            "initialized" => Ok(json!([])),
            "workspace/didChangeConfiguration" => {
                let flags = params
                    .pointer("/settings/vibra/compilationFlags")
                    .or_else(|| params.pointer("/settings/compilationFlags"))
                    .context("E-FLAG-004: LSP settings require `compilationFlags`")?;
                self.compilation_flags = compilation_flags(flags)?;
                match self.documents.keys().next().cloned() {
                    Some(uri) => self.diagnostic_notifications(uri),
                    None => Ok(json!([])),
                }
            }
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
                let (uri, source) = self.source(params)?;
                let path = uri_path(&uri).unwrap_or_else(|| PathBuf::from("document.vibra"));
                let formatted = crate::tooling::format_source(&path, &source)?;
                if formatted == source {
                    return Ok(json!([]));
                }
                Ok(
                    json!([{"range":{"start":{"line":0,"character":0},"end":end_position(&source)},"newText":formatted}]),
                )
            }
            "textDocument/completion" => {
                let (uri, _) = self.source(params)?;
                let workspace = self.workspace()?;
                let document = uri_path(&uri).context("completion requires a file URI")?;
                let mut names = workspace.visible_symbols(&document);
                names.sort();
                names.dedup();
                let mut symbols = names
                    .into_iter()
                    .map(|name| json!({"label":name,"kind":6,"detail":"Vibra workspace symbol"}))
                    .collect::<Vec<_>>();
                let mut atoms = workspace
                    .index
                    .facts()
                    .iter()
                    .filter(|fact| fact.kind == SemanticKind::Atom)
                    .map(|fact| fact.symbol.clone())
                    .collect::<Vec<_>>();
                atoms.sort();
                atoms.dedup();
                symbols.extend(atoms.into_iter().map(
                    |atom| json!({"label":format!("@{atom}"),"kind":21,"detail":"Vibra atom"}),
                ));
                Ok(Value::Array(symbols))
            }
            "textDocument/hover" => self.hover(params),
            "textDocument/definition" => self.definition(params),
            "textDocument/references" => self.references(params),
            "textDocument/documentSymbol" => self.document_symbols(params),
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
        let path = uri_path(&uri).context("diagnostics require a file URI")?;
        let mut reports = BTreeMap::new();
        let mut syntax_error = false;
        for (open_uri, source) in &self.documents {
            let open_path = uri_path(open_uri).unwrap_or_else(|| PathBuf::from("document.vibra"));
            let diagnostics = crate::tooling::diagnostics_for_source(&open_path, source);
            syntax_error |= diagnostics
                .iter()
                .any(|item| item.severity == crate::tooling::Severity::Error);
            reports.insert(open_uri.clone(), diagnostics);
        }
        if !syntax_error {
            for mut diagnostic in self.overlay_compile_diagnostics(&path)? {
                let target = best_diagnostic_document(&diagnostic.message, &self.documents)
                    .unwrap_or_else(|| uri.clone());
                if let Some(source) = self.documents.get(&target) {
                    if let Some((line, column, length)) =
                        locate_diagnostic(&diagnostic.message, source)
                    {
                        diagnostic.span.start.line = line;
                        diagnostic.span.start.column = column;
                        diagnostic.span.end.line = line;
                        diagnostic.span.end.column = column + length;
                    }
                }
                reports.entry(target).or_default().push(diagnostic);
            }
        }
        Ok(Value::Array(reports.into_iter().map(|(uri, diagnostics)| {
            json!({"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{"uri":uri,"diagnostics":diagnostics.into_iter().map(lsp_diagnostic).collect::<Vec<_>>()}})
        }).collect()))
    }

    fn overlay_compile_diagnostics(
        &self,
        document: &std::path::Path,
    ) -> Result<Vec<crate::tooling::Diagnostic>> {
        let root = self
            .root
            .clone()
            .or_else(|| discover_root(document))
            .context("workspace root is not available")?;
        if !document.starts_with(&root) {
            return Ok(Vec::new());
        }
        let mirror = tempfile::tempdir().context("create LSP compile workspace")?;
        mirror_workspace(&root, mirror.path())?;
        for (uri, source) in &self.documents {
            let Some(path) = uri_path(uri) else { continue };
            let Ok(relative) = path.strip_prefix(&root) else {
                continue;
            };
            let target = mirror.path().join(relative);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&target, source)
                .with_context(|| format!("write overlay {}", relative.display()))?;
        }
        let mirrored_document = mirror.path().join(document.strip_prefix(&root)?);
        let entry = mirrored_project_entry(mirror.path()).unwrap_or(mirrored_document);
        let mirror_text = mirror.path().to_string_lossy();
        let root_text = root.to_string_lossy();
        let mut diagnostics =
            crate::tooling::compile_diagnostics_with_flags(&entry, &self.compilation_flags);
        for diagnostic in &mut diagnostics {
            diagnostic.message = diagnostic
                .message
                .replace(mirror_text.as_ref(), root_text.as_ref());
        }
        Ok(diagnostics)
    }

    fn hover(&self, params: &Value) -> Result<Value> {
        let (uri, source) = self.source(params)?;
        let pos = position(params)?;
        let Some(word) = word_at(&source, pos.0, pos.1) else {
            return Ok(Value::Null);
        };
        if let Some(atom) = word.strip_prefix('@') {
            return Ok(
                json!({"contents":{"kind":"markdown","value":format!("`@{atom}` — Vibra atom (singleton type `@{atom}`, widens to `atom`)")}}),
            );
        }
        let workspace = self.workspace()?;
        let document = uri_path(&uri).context("hover requires a file URI")?;
        let Some((_target, name)) = workspace.resolve(&document, &word) else {
            return Ok(Value::Null);
        };
        let doc = String::new();
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
        // Atom literals have no declaration site.
        if word.starts_with('@') {
            return Ok(Value::Null);
        }
        let workspace = self.workspace()?;
        let document = uri_path(&uri).context("definition requires a file URI")?;
        let Some((target, name)) = workspace.resolve(&document, &word) else {
            return Ok(Value::Null);
        };
        match workspace.definition(&target, &name) {
            Some(definition) => Ok(location_span(
                &path_uri(&workspace.original_path(&definition.document)),
                workspace
                    .sources
                    .get(&workspace.original_path(&definition.document))
                    .map(String::as_str)
                    .unwrap_or_default(),
                definition.span,
            )),
            None => Ok(Value::Null),
        }
    }

    fn references(&self, params: &Value) -> Result<Value> {
        let (uri, source) = self.source(params)?;
        let pos = position(params)?;
        let Some(word) = word_at(&source, pos.0, pos.1) else {
            return Ok(json!([]));
        };
        let workspace = self.workspace()?;
        let document = uri_path(&uri).context("references require a file URI")?;
        if let Some(atom) = word.strip_prefix('@') {
            return Ok(Value::Array(self.atom_references(&workspace, atom)));
        }
        let Some((target, name)) = workspace.resolve(&document, &word) else {
            return Ok(json!([]));
        };
        let mut result = workspace
            .index
            .facts()
            .iter()
            .filter(|fact| matches!(fact.kind, SemanticKind::Reference | SemanticKind::Call))
            .filter(|fact| {
                fact.target_document.as_ref().unwrap_or(&fact.document) == &target
                    && fact.symbol.rsplit('.').next() == Some(name.as_str())
            })
            .filter_map(|fact| {
                workspace
                    .sources
                    .get(&workspace.original_path(&fact.document))
                    .map(|source| {
                        location_span(
                            &path_uri(&workspace.original_path(&fact.document)),
                            source,
                            fact.span,
                        )
                    })
            })
            .collect::<Vec<_>>();
        result.sort_by_key(|item| item.to_string());
        result.dedup();
        Ok(Value::Array(result))
    }

    fn atom_references(&self, workspace: &Workspace, atom: &str) -> Vec<Value> {
        let mut result = workspace
            .index
            .facts()
            .iter()
            .filter(|fact| fact.kind == SemanticKind::Atom && fact.symbol == atom)
            .filter_map(|fact| {
                workspace
                    .sources
                    .get(&workspace.original_path(&fact.document))
                    .map(|source| {
                        location_span(
                            &path_uri(&workspace.original_path(&fact.document)),
                            source,
                            fact.span,
                        )
                    })
            })
            .collect::<Vec<_>>();
        result.sort_by_key(|item| item.to_string());
        result.dedup();
        result
    }

    fn document_symbols(&self, params: &Value) -> Result<Value> {
        let (uri, source) = self.source(params)?;
        let document = uri_path(&uri).context("document symbols require a file URI")?;
        let workspace = self.workspace()?;
        Ok(Value::Array(
            workspace
                .index
                .query(Some(SemanticKind::Definition), None)
                .into_iter()
                .filter(|fact| fact.document == document)
                .map(|fact| {
                    let range = location_span(&uri, &source, fact.span)["range"].clone();
                    json!({"name":fact.symbol,"kind":12,"range":range,"selectionRange":range})
                })
                .collect(),
        ))
    }

    fn workspace(&self) -> Result<Workspace> {
        let root = self
            .root
            .clone()
            .or_else(|| {
                self.documents
                    .keys()
                    .next()
                    .and_then(|u| uri_path(u))
                    .and_then(|p| discover_root(&p))
            })
            .context("workspace root is not available")?;
        let mut sources = BTreeMap::new();
        discover_workspace_sources(&root, &mut sources)?;
        for (uri, source) in &self.documents {
            if let Some(path) = uri_path(uri) {
                sources.insert(normalize(path), source.clone());
            }
        }
        Workspace::build(root, sources)
    }
}

struct Workspace {
    index: SemanticIndex,
    sources: BTreeMap<PathBuf, String>,
    originals: BTreeMap<PathBuf, PathBuf>,
    mirrors: BTreeMap<PathBuf, PathBuf>,
}

impl Workspace {
    fn build(root: PathBuf, sources: BTreeMap<PathBuf, String>) -> Result<Self> {
        let root = fs::canonicalize(root)?;
        let sources = sources
            .into_iter()
            .map(|(path, source)| Ok((fs::canonicalize(path)?, source)))
            .collect::<Result<BTreeMap<_, _>>>()?;
        let mirror = tempfile::tempdir().context("create LSP semantic workspace")?;
        mirror_workspace(&root, mirror.path())?;
        let mut originals = BTreeMap::new();
        let mut mirrors = BTreeMap::new();
        for (path, source) in &sources {
            let relative = path.strip_prefix(&root).with_context(|| {
                format!("open document {} is outside workspace", path.display())
            })?;
            let target = mirror.path().join(relative);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&target, source)?;
            let target = fs::canonicalize(target)?;
            originals.insert(target.clone(), path.clone());
            mirrors.insert(path.clone(), target);
        }
        let roots = mirrors.values().cloned().collect::<Vec<_>>();
        let program = crate::frontend::load_surface_program_multi_root(
            &roots,
            &crate::load::CompilationFlags::default(),
        )?;
        let index = SemanticIndex::from_surface_program(&program);
        Ok(Self {
            index,
            sources,
            originals,
            mirrors,
        })
    }
    fn original_path(&self, path: &std::path::Path) -> PathBuf {
        self.originals
            .get(path)
            .cloned()
            .unwrap_or_else(|| path.to_path_buf())
    }
    fn resolve(&self, document: &PathBuf, word: &str) -> Option<(PathBuf, String)> {
        let symbol = word.trim_start_matches('$');
        let document = fs::canonicalize(document).unwrap_or_else(|_| normalize(document.clone()));
        let document = self.mirrors.get(&document).cloned().unwrap_or(document);
        if let Some((alias, name)) = symbol.split_once('.') {
            let import = self
                .index
                .query(Some(SemanticKind::Import), Some(alias))
                .into_iter()
                .find(|fact| fact.document == document)?;
            let target = import.target_document.clone()?;
            if name.contains('.') {
                return self.resolve(&target, &format!("${name}"));
            }
            return self
                .definition(&target, name)
                .map(|_| (target, name.to_string()));
        }
        let name = symbol.to_string();
        self.definition(&document, &name).map(|_| (document, name))
    }
    fn definition(
        &self,
        document: &PathBuf,
        name: &str,
    ) -> Option<&crate::sexpr_semantic::SemanticFact> {
        self.index
            .query(Some(SemanticKind::Definition), Some(name))
            .into_iter()
            .find(|fact| fact.document == *document)
    }
    fn visible_symbols(&self, document: &PathBuf) -> Vec<String> {
        let document = fs::canonicalize(document).unwrap_or_else(|_| normalize(document.clone()));
        let document = self.mirrors.get(&document).cloned().unwrap_or(document);
        let mut result = self
            .index
            .query(Some(SemanticKind::Definition), None)
            .into_iter()
            .filter(|fact| fact.document == document)
            .map(|fact| fact.symbol.clone())
            .collect::<Vec<_>>();
        for import in self
            .index
            .query(Some(SemanticKind::Import), None)
            .into_iter()
            .filter(|f| f.document == document)
        {
            if let Some(target) = import.target_document.clone() {
                result.extend(
                    self.index
                        .query(Some(SemanticKind::Definition), None)
                        .into_iter()
                        .filter(|fact| fact.document == target)
                        .map(|fact| format!("{}.{}", import.symbol, fact.symbol)),
                );
                result.extend(self.visible_imports(&target, &import.symbol, 0));
            }
        }
        result
    }
    fn visible_imports(&self, document: &PathBuf, prefix: &str, depth: usize) -> Vec<String> {
        if depth >= 16 {
            return Vec::new();
        }
        let mut result = Vec::new();
        for import in self
            .index
            .query(Some(SemanticKind::Import), None)
            .into_iter()
            .filter(|fact| fact.document == *document)
        {
            let Some(target) = import.target_document.clone() else {
                continue;
            };
            let qualified = format!("{prefix}.{}", import.symbol);
            result.extend(
                self.index
                    .query(Some(SemanticKind::Definition), None)
                    .into_iter()
                    .filter(|fact| fact.document == target)
                    .map(|fact| format!("{qualified}.{}", fact.symbol)),
            );
            result.extend(self.visible_imports(&target, &qualified, depth + 1));
        }
        result
    }
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
    // `@` only ever leads a token, so it is picked up here rather than in
    // `is_word` where it would also match mid-word.
    if a > 0 && bytes[a - 1] == b'@' {
        a -= 1;
    }
    (a < b).then(|| text[a..b].to_string())
}
fn is_word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'$' | b'-' | b'_' | b'.')
}
fn location_span(uri: &str, source: &str, span: crate::syntax::Span) -> Value {
    let index = crate::syntax::LineIndex::new(source);
    let start = index.position(source, span.start);
    let end = index.position(source, span.end);
    json!({"uri":uri,"range":{"start":{"line":start.line-1,"character":start.offset},"end":{"line":end.line-1,"character":end.offset}}})
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

fn compilation_flags(value: &Value) -> Result<crate::load::CompilationFlags> {
    let values = value
        .as_array()
        .context("E-FLAG-004: `compilationFlags` must be an array of strings")?;
    let flags = values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .context("E-FLAG-004: every compilation flag must be a string")
        })
        .collect::<Result<Vec<_>>>()?;
    crate::load::CompilationFlags::try_new(flags)
}

fn uri_path(uri: &str) -> Option<PathBuf> {
    let value = uri.strip_prefix("file://")?;
    let value = value
        .strip_prefix('/')
        .filter(|_| cfg!(windows))
        .unwrap_or(value);
    Some(PathBuf::from(value.replace("%20", " ")))
}
fn path_uri(path: &std::path::Path) -> String {
    let value = path
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .replace('\\', "/")
        .replace(' ', "%20");
    if value.starts_with('/') {
        format!("file://{value}")
    } else {
        format!("file:///{value}")
    }
}
fn normalize(path: PathBuf) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                result.pop();
            }
            other => result.push(other.as_os_str()),
        }
    }
    result
}
fn discover_root(path: &std::path::Path) -> Option<PathBuf> {
    let mut current = if path.is_dir() { path } else { path.parent()? };
    loop {
        if current.join(crate::project::MANIFEST_FILE).is_file() {
            return Some(current.to_path_buf());
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => return path.parent().map(PathBuf::from),
        }
    }
}
fn discover_workspace_sources(
    root: &std::path::Path,
    output: &mut BTreeMap<PathBuf, String>,
) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)
            .with_context(|| format!("read workspace {}", directory.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                if !matches!(
                    path.file_name().and_then(|v| v.to_str()),
                    Some(".git" | "target")
                ) {
                    pending.push(path);
                }
            } else if path.extension().and_then(|v| v.to_str()) == Some("vibra")
                && path.file_name().and_then(|v| v.to_str()) != Some(crate::project::MANIFEST_FILE)
            {
                output.insert(
                    normalize(path.clone()),
                    fs::read_to_string(&path)
                        .with_context(|| format!("read {}", path.display()))?,
                );
            }
        }
    }
    Ok(())
}
fn mirror_workspace(source: &std::path::Path, destination: &std::path::Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in
        fs::read_dir(source).with_context(|| format!("read workspace {}", source.display()))?
    {
        let entry = entry?;
        let name = entry.file_name();
        if matches!(name.to_str(), Some(".git" | "target")) {
            continue;
        }
        let from = entry.path();
        let to = destination.join(&name);
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            mirror_workspace(&from, &to)?;
        } else {
            fs::copy(&from, &to).with_context(|| format!("mirror {}", from.display()))?;
        }
    }
    Ok(())
}
fn mirrored_project_entry(root: &std::path::Path) -> Option<PathBuf> {
    let project = crate::project::load_project(root).ok()?;
    let target = project
        .manifest
        .targets
        .bins
        .first()
        .or_else(|| project.manifest.targets.libs.first())?;
    Some(project.root.join(&target.root).join(&target.entry))
}
fn lsp_diagnostic(diagnostic: crate::tooling::Diagnostic) -> Value {
    json!({
        "range":{"start":{"line":diagnostic.span.start.line,"character":diagnostic.span.start.column},"end":{"line":diagnostic.span.end.line,"character":diagnostic.span.end.column}},
        "severity":match diagnostic.severity { crate::tooling::Severity::Error=>1, crate::tooling::Severity::Warning=>2, crate::tooling::Severity::Info=>3, crate::tooling::Severity::Hint=>4 },
        "code":diagnostic.code,"source":"vibra","message":diagnostic.message
    })
}
fn diagnostic_candidates(message: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let mut rest = message;
    while let Some(start) = rest.find('`') {
        rest = &rest[start + 1..];
        let Some(end) = rest.find('`') else { break };
        let value = rest[..end].trim_start_matches('$');
        if !value.is_empty() {
            candidates.push(value.to_string());
        }
        rest = &rest[end + 1..];
    }
    if let Some(prefix) = message.split(':').next() {
        candidates.extend(
            prefix
                .split('.')
                .rev()
                .filter(|part| {
                    !part.is_empty() && part.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
                })
                .map(str::to_string),
        );
    }
    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.len()));
    candidates.dedup();
    candidates
}
fn locate_diagnostic(message: &str, source: &str) -> Option<(usize, usize, usize)> {
    for candidate in diagnostic_candidates(message) {
        for needle in [format!("${candidate}"), format!("{candidate}:")] {
            for (line, row) in source.lines().enumerate() {
                if let Some(column) = row.find(&needle) {
                    return Some((line, column, needle.len()));
                }
            }
        }
    }
    None
}
fn best_diagnostic_document(message: &str, documents: &BTreeMap<String, String>) -> Option<String> {
    documents
        .iter()
        .filter_map(|(uri, source)| {
            locate_diagnostic(message, source).map(|(_, _, length)| (length, uri.clone()))
        })
        .max_by(|left, right| left.0.cmp(&right.0).then_with(|| right.1.cmp(&left.1)))
        .map(|(_, uri)| uri)
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

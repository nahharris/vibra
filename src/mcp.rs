//! Capability-safe Model Context Protocol server for Vibra tooling.
//!
//! The server deliberately exposes a closed set of CLI invocations. It never
//! accepts a command name, environment mutation, or arbitrary argument vector
//! from a client.

use crate::{ast::TopLevel, frontend, project, project_context, test_runner};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const PROTOCOL_VERSION: &str = "2025-06-18";

#[derive(Debug, Clone)]
pub struct ServerOptions {
    pub workspace: PathBuf,
    pub allow_write: bool,
    pub allow_test: bool,
}

#[derive(Debug)]
struct Server {
    root: PathBuf,
    executable: PathBuf,
    allow_write: bool,
    allow_test: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolFailure {
    code: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

pub fn run_stdio(options: ServerOptions) -> Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    serve(
        stdin.lock(),
        stdout.lock(),
        options,
        std::env::current_exe().context("resolve vibra executable")?,
    )
}

pub fn serve<R: Read, W: Write>(
    input: R,
    mut output: W,
    options: ServerOptions,
    executable: PathBuf,
) -> Result<()> {
    let root = fs::canonicalize(&options.workspace)
        .with_context(|| format!("resolve MCP workspace {}", options.workspace.display()))?;
    if !root.is_dir() {
        bail!("MCP workspace `{}` is not a directory", root.display());
    }
    let server = Server {
        root,
        executable,
        allow_write: options.allow_write,
        allow_test: options.allow_test,
    };
    for line in BufReader::new(input).lines() {
        let line = line.context("read MCP request")?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Value>(&line) {
            Ok(request) => server.handle(request),
            Err(error) => Some(protocol_error(
                Value::Null,
                -32700,
                "Parse error",
                json!({
                    "detail": error.to_string()
                }),
            )),
        };
        if let Some(response) = response {
            serde_json::to_writer(&mut output, &response).context("write MCP response")?;
            output.write_all(b"\n").context("write MCP delimiter")?;
            output.flush().context("flush MCP response")?;
        }
    }
    Ok(())
}

impl Server {
    fn handle(&self, request: Value) -> Option<Value> {
        let Some(object) = request.as_object() else {
            return Some(protocol_error(
                Value::Null,
                -32600,
                "Invalid Request",
                json!({"detail": "request must be an object"}),
            ));
        };
        let id = object.get("id").cloned();
        let request_id = id.clone().unwrap_or(Value::Null);
        if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return Some(protocol_error(
                request_id,
                -32600,
                "Invalid Request",
                json!({"detail": "jsonrpc must be \"2.0\""}),
            ));
        }
        let Some(method) = object.get("method").and_then(Value::as_str) else {
            return Some(protocol_error(
                request_id,
                -32600,
                "Invalid Request",
                json!({"detail": "method must be a string"}),
            ));
        };
        if id.is_none() {
            return None;
        }
        let params = object.get("params").cloned().unwrap_or_else(|| json!({}));
        let result = match method {
            "initialize" => Ok(json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "vibra", "version": env!("CARGO_PKG_VERSION")}
            })),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({"tools": tool_definitions()})),
            "tools/call" => self.call_tool(&params),
            _ => {
                return Some(protocol_error(
                    request_id,
                    -32601,
                    "Method not found",
                    json!({"method": method}),
                ));
            }
        };
        Some(match result {
            Ok(result) => json!({"jsonrpc": "2.0", "id": request_id, "result": result}),
            Err(error) => tool_error_response(request_id, error),
        })
    }

    fn call_tool(&self, params: &Value) -> std::result::Result<Value, ToolFailure> {
        let object = params
            .as_object()
            .ok_or_else(|| invalid_arguments("tools/call params must be an object"))?;
        let name = object
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid_arguments("tool name must be a string"))?;
        let arguments = object
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let args = arguments
            .as_object()
            .ok_or_else(|| invalid_arguments("tool arguments must be an object"))?;
        let result = match name {
            "vibra.project.inspect" => self.inspect_project(args),
            "vibra.tests.list" => self.list_tests(args),
            "vibra.check" => self.run_check(args),
            "vibra.docs" => self.run_docs(args),
            "vibra.fmt" => self.run_fmt(args),
            "vibra.lint" => self.run_lint(args),
            "vibra.test" => self.run_test(args),
            "vibra.build" => self.run_build(args),
            _ => Err(ToolFailure {
                code: "tool-not-found",
                message: format!("unknown tool `{name}`"),
                data: None,
            }),
        }?;
        Ok(tool_success(result))
    }

    fn inspect_project(&self, args: &Map<String, Value>) -> ToolResult {
        reject_unknown(args, &["path"])?;
        let path = self.existing_path(optional_string(args, "path")?.unwrap_or("."))?;
        let loaded = project::load_project(&path).map_err(operation_error)?;
        let relative_root = relative_slash(&self.root, &loaded.root)?;
        let mut dependencies = loaded
            .manifest
            .dependencies
            .iter()
            .map(|(name, dependency)| {
                json!({
                    "name": name,
                    "path": dependency.path.as_ref().map(path_slash),
                    "git": dependency.git,
                    "rev": dependency.rev,
                    "wasm": dependency.wasm.as_ref().map(path_slash)
                })
            })
            .collect::<Vec<_>>();
        dependencies.sort_by(|a, b| {
            a["name"]
                .as_str()
                .unwrap_or_default()
                .cmp(b["name"].as_str().unwrap_or_default())
        });
        let targets = loaded
            .manifest
            .targets
            .libs
            .iter()
            .map(|target| target_json("lib", target))
            .chain(
                loaded
                    .manifest
                    .targets
                    .bins
                    .iter()
                    .map(|target| target_json("bin", target)),
            )
            .collect::<Vec<_>>();
        Ok(json!({
            "manifestVersion": loaded.manifest.manifest_version,
            "package": {
                "name": loaded.manifest.package.name,
                "version": loaded.manifest.package.version,
                "doc": loaded.manifest.package.doc
            },
            "root": relative_root,
            "targets": targets,
            "dependencies": dependencies
        }))
    }

    fn list_tests(&self, args: &Map<String, Value>) -> ToolResult {
        reject_unknown(args, &["path", "filter", "profiles", "tags"])?;
        let path = self.existing_path(optional_string(args, "path")?.unwrap_or("."))?;
        self.validate_read_graph()?;
        let filter = optional_string(args, "filter")?;
        let profiles = string_array(args, "profiles")?;
        let tags = string_array(args, "tags")?;
        let tests =
            test_runner::list_tests(&path, filter, &profiles, &tags).map_err(operation_error)?;
        Ok(json!({"tests": tests}))
    }

    fn run_check(&self, args: &Map<String, Value>) -> ToolResult {
        reject_unknown(args, &["path"])?;
        let path = self.existing_path(optional_string(args, "path")?.unwrap_or("."))?;
        self.validate_read_graph()?;
        self.cli_json_owned(&[
            "check".to_owned(),
            path_arg(&path)?,
            "--format".to_owned(),
            "json".to_owned(),
        ])
    }

    fn run_docs(&self, args: &Map<String, Value>) -> ToolResult {
        reject_unknown(args, &["path", "symbol", "target"])?;
        let path = self.existing_path(optional_string(args, "path")?.unwrap_or("."))?;
        self.validate_read_graph()?;
        let mut command = vec!["docs".to_owned(), path_arg(&path)?];
        if let Some(symbol) = optional_string(args, "symbol")? {
            command.push(symbol.to_owned());
        }
        if let Some(target) = optional_string(args, "target")? {
            command.extend(["--target".to_owned(), target.to_owned()]);
        }
        command.extend(["--format".to_owned(), "json".to_owned()]);
        self.cli_json_owned(&command)
    }

    fn run_fmt(&self, args: &Map<String, Value>) -> ToolResult {
        reject_unknown(args, &["paths", "write"])?;
        let write = optional_bool(args, "write")?.unwrap_or(false);
        if write && !self.allow_write {
            return Err(permission_denied(
                "vibra.fmt write mode requires starting `vibra mcp` with --allow-write",
            ));
        }
        let paths = path_array(args, "paths")?;
        let mut command = vec!["fmt".to_owned()];
        if paths.is_empty() {
            command.push(path_arg(&self.root)?);
        } else {
            for path in paths {
                command.push(path_arg(&self.existing_path(&path)?)?);
            }
        }
        if write {
            command.push("--write".to_owned());
        }
        command.extend(["--format".to_owned(), "json".to_owned()]);
        self.cli_json_owned(&command)
    }

    fn run_lint(&self, args: &Map<String, Value>) -> ToolResult {
        reject_unknown(args, &["paths", "categories", "severity", "denyWarnings"])?;
        self.validate_read_graph()?;
        let paths = path_array(args, "paths")?;
        let categories = string_array(args, "categories")?;
        for category in &categories {
            if !matches!(category.as_str(), "style" | "syntax" | "compile") {
                return Err(invalid_arguments(
                    "categories must contain only style, syntax, or compile",
                ));
            }
        }
        let severity = optional_string(args, "severity")?;
        if severity.is_some_and(|value| !matches!(value, "error" | "warning" | "info" | "hint")) {
            return Err(invalid_arguments(
                "severity must be error, warning, info, or hint",
            ));
        }
        let mut command = vec!["lint".to_owned()];
        if paths.is_empty() {
            command.push(path_arg(&self.root)?);
        } else {
            for path in paths {
                command.push(path_arg(&self.existing_path(&path)?)?);
            }
        }
        for category in categories {
            command.extend(["--category".to_owned(), category]);
        }
        if let Some(severity) = severity {
            command.extend(["--severity".to_owned(), severity.to_owned()]);
        }
        if optional_bool(args, "denyWarnings")?.unwrap_or(false) {
            command.push("--deny-warnings".to_owned());
        }
        command.extend(["--format".to_owned(), "json".to_owned()]);
        self.cli_json_owned(&command)
    }

    fn run_test(&self, args: &Map<String, Value>) -> ToolResult {
        reject_unknown(
            args,
            &[
                "path",
                "filter",
                "profiles",
                "tags",
                "denySkips",
                "denyWarnings",
                "timeoutMs",
            ],
        )?;
        if !self.allow_test {
            return Err(permission_denied(
                "vibra.test requires starting `vibra mcp` with --allow-test",
            ));
        }
        self.validate_read_graph()?;
        let path = self.existing_path(optional_string(args, "path")?.unwrap_or("."))?;
        let mut command = vec!["test".to_owned(), path_arg(&path)?];
        if let Some(filter) = optional_string(args, "filter")? {
            command.extend(["--filter".to_owned(), filter.to_owned()]);
        }
        for profile in string_array(args, "profiles")? {
            command.extend(["--profile".to_owned(), profile]);
        }
        for tag in string_array(args, "tags")? {
            command.extend(["--tag".to_owned(), tag]);
        }
        if optional_bool(args, "denySkips")?.unwrap_or(false) {
            command.push("--deny-skips".to_owned());
        }
        if optional_bool(args, "denyWarnings")?.unwrap_or(false) {
            command.push("--deny-warnings".to_owned());
        }
        if let Some(timeout) = optional_u64(args, "timeoutMs")? {
            command.extend(["--timeout-ms".to_owned(), timeout.to_string()]);
        }
        command.extend([
            "--jobs".to_owned(),
            "1".to_owned(),
            "--format".to_owned(),
            "json".to_owned(),
        ]);
        self.cli_json_owned(&command)
    }

    fn run_build(&self, args: &Map<String, Value>) -> ToolResult {
        reject_unknown(args, &["path", "bin", "output"])?;
        if !self.allow_write {
            return Err(permission_denied(
                "vibra.build requires starting `vibra mcp` with --allow-write",
            ));
        }
        self.validate_read_graph()?;
        let path = self.existing_path(optional_string(args, "path")?.unwrap_or("."))?;
        let output = required_string(args, "output")?;
        let output = self.output_path(output)?;
        let mut command = vec![
            "build".to_owned(),
            path_arg(&path)?,
            "--output".to_owned(),
            path_arg(&output)?,
        ];
        if let Some(bin) = optional_string(args, "bin")? {
            command.extend(["--bin".to_owned(), bin.to_owned()]);
        }
        command.extend(["--format".to_owned(), "json".to_owned()]);
        self.cli_json_owned(&command)
    }

    fn cli_json_owned(&self, args: &[String]) -> ToolResult {
        let output = Command::new(&self.executable)
            .args(args)
            .current_dir(&self.root)
            .output()
            .map_err(|error| ToolFailure {
                code: "command-unavailable",
                message: format!("start Vibra CLI: {error}"),
                data: None,
            })?;
        command_result(args, output)
    }

    fn existing_path(&self, value: &str) -> std::result::Result<PathBuf, ToolFailure> {
        let requested = self.requested_path(value)?;
        let canonical = fs::canonicalize(&requested).map_err(|error| ToolFailure {
            code: "path-not-found",
            message: format!("resolve `{value}`: {error}"),
            data: None,
        })?;
        self.ensure_confined(canonical)
    }

    fn output_path(&self, value: &str) -> std::result::Result<PathBuf, ToolFailure> {
        let requested = self.requested_path(value)?;
        if requested.exists() {
            let metadata = fs::symlink_metadata(&requested).map_err(|error| ToolFailure {
                code: "operation-failed",
                message: format!("inspect output `{}`: {error}", requested.display()),
                data: None,
            })?;
            if metadata.file_type().is_symlink() {
                return Err(ToolFailure {
                    code: "path-symlink-unsupported",
                    message: format!("output `{}` must not be a symlink", requested.display()),
                    data: None,
                });
            }
            let canonical = fs::canonicalize(&requested).map_err(|error| ToolFailure {
                code: "operation-failed",
                message: format!("resolve output `{}`: {error}", requested.display()),
                data: None,
            })?;
            self.ensure_confined(canonical)?;
        }
        let file_name = requested
            .file_name()
            .ok_or_else(|| invalid_arguments("output must name a file inside the MCP workspace"))?;
        let parent = requested.parent().unwrap_or(&self.root);
        let parent = fs::canonicalize(parent).map_err(|error| ToolFailure {
            code: "path-not-found",
            message: format!("resolve output parent `{}`: {error}", parent.display()),
            data: None,
        })?;
        let parent = self.ensure_confined(parent)?;
        Ok(parent.join(file_name))
    }

    fn requested_path(&self, value: &str) -> std::result::Result<PathBuf, ToolFailure> {
        if value.is_empty() {
            return Err(invalid_arguments("paths must not be empty"));
        }
        let path = Path::new(value);
        Ok(if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.root.join(path)
        })
    }

    fn ensure_confined(&self, path: PathBuf) -> std::result::Result<PathBuf, ToolFailure> {
        if path == self.root || path.starts_with(&self.root) {
            Ok(path)
        } else {
            Err(ToolFailure {
                code: "path-outside-workspace",
                message: format!(
                    "path `{}` is outside MCP workspace `{}`",
                    path.display(),
                    self.root.display()
                ),
                data: None,
            })
        }
    }

    fn validate_read_graph(&self) -> std::result::Result<(), ToolFailure> {
        let mut files = Vec::new();
        collect_workspace_files(&self.root, &self.root, &mut files)?;
        for path in files {
            if path.file_name().and_then(|name| name.to_str()) == Some(project::MANIFEST_FILE) {
                self.validate_manifest_paths(&path)?;
            }
            if path.file_name().and_then(|name| name.to_str()) == Some(project::MANIFEST_FILE)
                || path.extension().and_then(|extension| extension.to_str()) != Some("vibra")
            {
                continue;
            }
            let (_, module) =
                frontend::load_surface_entry_for_test_discovery(&path).map_err(operation_error)?;
            self.validate_module_imports(&module)?;
        }
        Ok(())
    }

    fn validate_manifest_paths(&self, path: &Path) -> std::result::Result<(), ToolFailure> {
        let loaded = project::load_project(path).map_err(operation_error)?;
        for dependency in loaded.manifest.dependencies.values() {
            if let Some(relative) = &dependency.path {
                self.confine_declared_path(&loaded.root, relative, "dependency path")?;
            }
        }
        for target in loaded
            .manifest
            .targets
            .bins
            .iter()
            .chain(&loaded.manifest.targets.libs)
        {
            self.confine_declared_path(&loaded.root, &target.root, "target root")?;
        }
        Ok(())
    }

    fn validate_module_imports(
        &self,
        module: &frontend::SourceModule,
    ) -> std::result::Result<(), ToolFailure> {
        let parent = module.path.parent().unwrap_or(&self.root);
        let project = project_context::discover_project_import_context(&module.path)
            .map_err(operation_error)?;
        for form in module.forms() {
            let TopLevel::Import(import) = form else {
                continue;
            };
            let path = if import.path.value.starts_with('@') {
                let project = project.as_ref().ok_or_else(|| {
                    operation_error(anyhow::anyhow!(
                        "{}: @ import `{}` requires project.vibra",
                        module.path.display(),
                        import.path.value
                    ))
                })?;
                project_context::resolve_project_import(project, &import.path.value)
                    .map_err(operation_error)?
            } else {
                parent.join(&import.path.value)
            };
            self.confine_declared_path(&self.root, &path, "source import")?;
        }
        Ok(())
    }

    fn confine_declared_path(
        &self,
        base: &Path,
        relative: &Path,
        kind: &'static str,
    ) -> std::result::Result<(), ToolFailure> {
        let candidate = if relative.is_absolute() {
            relative.to_path_buf()
        } else {
            base.join(relative)
        };
        if !candidate.exists() {
            return Ok(());
        }
        let canonical = fs::canonicalize(&candidate).map_err(|error| ToolFailure {
            code: "path-not-found",
            message: format!("resolve {kind} `{}`: {error}", candidate.display()),
            data: None,
        })?;
        self.ensure_confined(canonical).map(|_| ())
    }
}

type ToolResult = std::result::Result<Value, ToolFailure>;

fn command_result(args: &[String], output: Output) -> ToolResult {
    let stdout = String::from_utf8(output.stdout).map_err(|error| ToolFailure {
        code: "invalid-command-output",
        message: format!("Vibra CLI emitted non-UTF-8 stdout: {error}"),
        data: None,
    })?;
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let parsed = if stdout.trim().is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&stdout).map_err(|error| ToolFailure {
            code: "invalid-command-output",
            message: format!("Vibra CLI emitted invalid JSON: {error}"),
            data: Some(json!({"stdout": stdout, "stderr": stderr})),
        })?
    };
    if output.status.success() {
        Ok(json!({"output": parsed}))
    } else {
        Err(ToolFailure {
            code: "command-failed",
            message: if stderr.is_empty() {
                format!("Vibra CLI command `{}` failed", args.join(" "))
            } else {
                stderr
            },
            data: Some(json!({
                "exitCode": output.status.code(),
                "output": parsed
            })),
        })
    }
}

fn tool_success(value: Value) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&value).expect("JSON value serializes")
        }],
        "structuredContent": value,
        "isError": false
    })
}

fn tool_error_response(id: Value, failure: ToolFailure) -> Value {
    let structured = serde_json::to_value(&failure).expect("tool failure serializes");
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&structured).expect("JSON value serializes")
            }],
            "structuredContent": structured,
            "isError": true
        }
    })
}

fn protocol_error(id: Value, code: i64, message: &str, data: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": code, "message": message, "data": data}
    })
}

fn operation_error(error: anyhow::Error) -> ToolFailure {
    ToolFailure {
        code: "operation-failed",
        message: format!("{error:#}"),
        data: None,
    }
}

fn invalid_arguments(message: impl Into<String>) -> ToolFailure {
    ToolFailure {
        code: "invalid-arguments",
        message: message.into(),
        data: None,
    }
}

fn permission_denied(message: impl Into<String>) -> ToolFailure {
    ToolFailure {
        code: "permission-denied",
        message: message.into(),
        data: None,
    }
}

fn reject_unknown(
    args: &Map<String, Value>,
    allowed: &[&str],
) -> std::result::Result<(), ToolFailure> {
    if let Some(name) = args.keys().find(|name| !allowed.contains(&name.as_str())) {
        return Err(invalid_arguments(format!("unknown argument `{name}`")));
    }
    Ok(())
}

fn optional_string<'a>(
    args: &'a Map<String, Value>,
    name: &str,
) -> std::result::Result<Option<&'a str>, ToolFailure> {
    args.get(name)
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| invalid_arguments(format!("`{name}` must be a string")))
        })
        .transpose()
}

fn required_string<'a>(
    args: &'a Map<String, Value>,
    name: &str,
) -> std::result::Result<&'a str, ToolFailure> {
    optional_string(args, name)?.ok_or_else(|| invalid_arguments(format!("`{name}` is required")))
}

fn optional_bool(
    args: &Map<String, Value>,
    name: &str,
) -> std::result::Result<Option<bool>, ToolFailure> {
    args.get(name)
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| invalid_arguments(format!("`{name}` must be a boolean")))
        })
        .transpose()
}

fn optional_u64(
    args: &Map<String, Value>,
    name: &str,
) -> std::result::Result<Option<u64>, ToolFailure> {
    args.get(name)
        .map(|value| {
            value.as_u64().ok_or_else(|| {
                invalid_arguments(format!("`{name}` must be a non-negative integer"))
            })
        })
        .transpose()
}

fn string_array(
    args: &Map<String, Value>,
    name: &str,
) -> std::result::Result<Vec<String>, ToolFailure> {
    let Some(value) = args.get(name) else {
        return Ok(Vec::new());
    };
    value
        .as_array()
        .ok_or_else(|| invalid_arguments(format!("`{name}` must be an array")))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| invalid_arguments(format!("`{name}` must contain only strings")))
        })
        .collect()
}

fn path_array(
    args: &Map<String, Value>,
    name: &str,
) -> std::result::Result<Vec<String>, ToolFailure> {
    string_array(args, name)
}

fn path_arg(path: &Path) -> std::result::Result<String, ToolFailure> {
    path.to_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| invalid_arguments("paths must be valid UTF-8"))
}

fn path_slash(path: &PathBuf) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn collect_workspace_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<PathBuf>,
) -> std::result::Result<(), ToolFailure> {
    let entries = fs::read_dir(directory).map_err(|error| ToolFailure {
        code: "operation-failed",
        message: format!(
            "read workspace directory `{}`: {error}",
            directory.display()
        ),
        data: None,
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| ToolFailure {
            code: "operation-failed",
            message: format!("read workspace entry: {error}"),
            data: None,
        })?;
        let path = entry.path();
        let name = entry.file_name();
        if matches!(name.to_str(), Some(".git" | "target")) {
            continue;
        }
        let metadata = fs::symlink_metadata(&path).map_err(|error| ToolFailure {
            code: "operation-failed",
            message: format!("inspect workspace path `{}`: {error}", path.display()),
            data: None,
        })?;
        if metadata.file_type().is_symlink() {
            return Err(ToolFailure {
                code: "path-symlink-unsupported",
                message: format!(
                    "workspace path `{}` is a symlink; MCP recursive tools require a physical tree",
                    path.display()
                ),
                data: None,
            });
        }
        if metadata.is_dir() {
            collect_workspace_files(root, &path, files)?;
        } else if metadata.is_file() {
            let canonical = fs::canonicalize(&path).map_err(|error| ToolFailure {
                code: "operation-failed",
                message: format!("resolve workspace file `{}`: {error}", path.display()),
                data: None,
            })?;
            if canonical == root || canonical.starts_with(root) {
                files.push(canonical);
            }
        }
    }
    files.sort();
    Ok(())
}

fn relative_slash(root: &Path, path: &Path) -> std::result::Result<String, ToolFailure> {
    let relative = path.strip_prefix(root).map_err(|_| ToolFailure {
        code: "path-outside-workspace",
        message: format!(
            "project root `{}` is outside the MCP workspace",
            path.display()
        ),
        data: None,
    })?;
    let rendered = relative.to_string_lossy().replace('\\', "/");
    Ok(if rendered.is_empty() {
        ".".to_owned()
    } else {
        rendered
    })
}

fn target_json(kind: &str, target: &project::Target) -> Value {
    json!({
        "kind": kind,
        "name": target.name,
        "root": path_slash(&target.root),
        "entry": path_slash(&target.entry)
    })
}

fn object_schema(properties: Value) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "additionalProperties": false
    })
}

fn tool_definitions() -> Vec<Value> {
    let path = json!({"type": "string", "description": "Workspace-relative path"});
    let paths = json!({"type": "array", "items": path});
    vec![
        tool("vibra.project.inspect", "Read the project manifest, targets, and dependencies without executing code.", object_schema(json!({"path": path}))),
        tool("vibra.tests.list", "Discover Vibra tests without executing them.", object_schema(json!({
            "path": path, "filter": {"type": "string"},
            "profiles": {"type": "array", "items": {"type": "string"}},
            "tags": {"type": "array", "items": {"type": "string"}}
        }))),
        tool("vibra.check", "Validate a project exactly as `vibra check --format json`.", object_schema(json!({"path": path}))),
        tool("vibra.docs", "Read source documentation exactly as `vibra docs --format json`.", object_schema(json!({
            "path": path, "symbol": {"type": "string"}, "target": {"type": "string"}
        }))),
        tool("vibra.fmt", "Check canonical formatting, or rewrite it when the server was granted --allow-write.", object_schema(json!({
            "paths": paths, "write": {"type": "boolean", "default": false}
        }))),
        tool("vibra.lint", "Emit diagnostics exactly as `vibra lint --format json`.", object_schema(json!({
            "paths": paths,
            "categories": {"type": "array", "items": {"enum": ["style", "syntax", "compile"]}},
            "severity": {"enum": ["error", "warning", "info", "hint"]},
            "denyWarnings": {"type": "boolean", "default": false}
        }))),
        tool("vibra.test", "Run capability-free tests serially when the server was granted --allow-test.", object_schema(json!({
            "path": path, "filter": {"type": "string"},
            "profiles": {"type": "array", "items": {"type": "string"}},
            "tags": {"type": "array", "items": {"type": "string"}},
            "denySkips": {"type": "boolean", "default": false},
            "denyWarnings": {"type": "boolean", "default": false},
            "timeoutMs": {"type": "integer", "minimum": 1}
        }))),
        tool_with_required("vibra.build", "Build a deterministic .vapp inside the workspace when the server was granted --allow-write.", object_schema(json!({
            "path": path, "bin": {"type": "string"}, "output": path
        })), &["output"]),
    ]
}

fn tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema,
        "outputSchema": {"type": "object"}
    })
}

fn tool_with_required(
    name: &str,
    description: &str,
    mut input_schema: Value,
    required: &[&str],
) -> Value {
    input_schema["required"] = json!(required);
    tool(name, description, input_schema)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn request(id: u64, method: &str, params: Value) -> String {
        json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string() + "\n"
    }

    #[test]
    fn lifecycle_and_tools_are_deterministic() {
        let root = tempfile::tempdir().unwrap();
        let input = request(1, "initialize", json!({}))
            + &request(2, "tools/list", json!({}))
            + &request(3, "ping", json!({}));
        let mut first = Vec::new();
        serve(
            Cursor::new(&input),
            &mut first,
            ServerOptions {
                workspace: root.path().to_path_buf(),
                allow_write: false,
                allow_test: false,
            },
            PathBuf::from("unused"),
        )
        .unwrap();
        let mut second = Vec::new();
        serve(
            Cursor::new(input),
            &mut second,
            ServerOptions {
                workspace: root.path().to_path_buf(),
                allow_write: false,
                allow_test: false,
            },
            PathBuf::from("unused"),
        )
        .unwrap();
        assert_eq!(first, second);
        let lines = String::from_utf8(first).unwrap();
        assert!(lines.contains("\"protocolVersion\":\"2025-06-18\""));
        assert!(lines.contains("\"name\":\"vibra.project.inspect\""));
        assert!(lines.contains("\"name\":\"vibra.build\""));
    }

    #[test]
    fn default_server_denies_mutating_and_executing_tools() {
        let root = tempfile::tempdir().unwrap();
        let input = request(
            1,
            "tools/call",
            json!({"name": "vibra.build", "arguments": {"output": "app.vapp"}}),
        ) + &request(
            2,
            "tools/call",
            json!({"name": "vibra.test", "arguments": {}}),
        );
        let mut output = Vec::new();
        serve(
            Cursor::new(input),
            &mut output,
            ServerOptions {
                workspace: root.path().to_path_buf(),
                allow_write: false,
                allow_test: false,
            },
            PathBuf::from("unused"),
        )
        .unwrap();
        let text = String::from_utf8(output).unwrap();
        assert_eq!(text.matches("\"code\":\"permission-denied\"").count(), 2);
        assert_eq!(text.matches("\"isError\":true").count(), 2);
    }

    #[test]
    fn paths_cannot_escape_through_parent_or_symlink() {
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let server = Server {
            root: fs::canonicalize(workspace.path()).unwrap(),
            executable: PathBuf::from("unused"),
            allow_write: false,
            allow_test: false,
        };
        let error = server
            .existing_path(outside.path().to_str().unwrap())
            .unwrap_err();
        assert_eq!(error.code, "path-outside-workspace");
        let error = server.existing_path("../").unwrap_err();
        assert_eq!(error.code, "path-outside-workspace");
    }

    #[test]
    fn notifications_do_not_receive_responses() {
        let root = tempfile::tempdir().unwrap();
        let input = json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        })
        .to_string()
            + "\n";
        let mut output = Vec::new();
        serve(
            Cursor::new(input),
            &mut output,
            ServerOptions {
                workspace: root.path().to_path_buf(),
                allow_write: false,
                allow_test: false,
            },
            PathBuf::from("unused"),
        )
        .unwrap();
        assert!(output.is_empty());
    }
}

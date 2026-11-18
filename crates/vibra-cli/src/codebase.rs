//! CLI parsing and adaptation for the M1 addressed codebase service.

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode, LineIndex};
use vibra_schema::{DiagnosticDocument, SCHEMA_VERSION};
use vibra_workspace::codebase::import::prepare_import;
use vibra_workspace::codebase::store::{
    CodebaseStore, HeadExpectation, ReadTarget, StoreAccess, StoreError,
};

use crate::{
    CommandEnvelope, CommandResult, Invocation, Payload,
    render_diagnostics_with_sources,
};

const DEFAULT_STORE: &str = ".vibra/codebase.sqlite";

/// One syntactically valid codebase request.
#[derive(Clone, Debug)]
pub(crate) enum Request {
    Init {
        store: PathBuf,
    },
    Head {
        store: PathBuf,
        name: Option<String>,
    },
    Import {
        store: PathBuf,
        head: String,
        expected: String,
    },
    Lookup {
        store: PathBuf,
        selector: Selector,
        entity: String,
    },
    Read {
        store: PathBuf,
        selector: Selector,
        object: String,
    },
    Export {
        store: PathBuf,
        selector: Selector,
        destination: PathBuf,
        write: bool,
        plan: Option<String>,
    },
    Verify {
        store: PathBuf,
    },
    Reindex {
        store: PathBuf,
    },
    Backup {
        store: PathBuf,
        destination: PathBuf,
    },
    Restore {
        store: PathBuf,
        source: PathBuf,
    },
}

#[derive(Clone, Debug)]
pub(crate) enum Selector {
    Head(String),
    Revision(String),
}

#[derive(Clone, Debug)]
pub(crate) struct ParseError {
    pub message: String,
}

/// Parses the closed M1 command grammar after `codebase`.
pub(crate) fn parse(
    subcommand: &str,
    arguments: &[OsString],
) -> Result<Request, ParseError> {
    let (positionals, options) = match parse_options(arguments) {
        Ok(parsed) => parsed,
        Err(message) => return Err(ParseError { message }),
    };
    let store = options
        .get("--store")
        .map_or_else(|| PathBuf::from(DEFAULT_STORE), PathBuf::from);

    let request = match subcommand {
        "init" => {
            ensure_options(subcommand, &options, &["--store"])?;
            require_arity(subcommand, positionals.as_slice(), 0)?;
            Request::Init { store }
        }
        "head" => {
            ensure_options(subcommand, &options, &["--store"])?;
            require_arity(subcommand, positionals.as_slice(), 0..=1)?;
            Request::Head {
                store,
                name: positionals.first().cloned(),
            }
        }
        "import" => {
            ensure_options(subcommand, &options, &["--store", "--head", "--expect"])?;
            require_arity(subcommand, positionals.as_slice(), 0)?;
            let head = options
                .get("--head")
                .cloned()
                .unwrap_or_else(|| "main".to_owned());
            let expected =
                options.get("--expect").cloned().ok_or_else(|| ParseError {
                    message: "`codebase import` requires `--expect absent|REVISION`"
                        .to_owned(),
                })?;
            Request::Import {
                store,
                head,
                expected,
            }
        }
        "lookup" | "read" => {
            ensure_options(subcommand, &options, &["--store", "--head", "--revision"])?;
            let operand = single_operand(subcommand, &positionals)?.to_owned();
            let selector = parse_selector(&options)?;
            if subcommand == "lookup" {
                Request::Lookup {
                    store,
                    selector,
                    entity: operand,
                }
            } else {
                Request::Read {
                    store,
                    selector,
                    object: operand,
                }
            }
        }
        "export" => {
            ensure_options(
                subcommand,
                &options,
                &[
                    "--store",
                    "--head",
                    "--revision",
                    "--destination",
                    "--write",
                    "--plan",
                ],
            )?;
            require_arity(subcommand, positionals.as_slice(), 0)?;
            let selector = parse_selector(&options)?;
            let destination = options
                .get("--destination")
                .map(PathBuf::from)
                .ok_or_else(|| ParseError {
                    message: "`codebase export` requires `--destination PATH`"
                        .to_owned(),
                })?;
            let write = options.contains_key("--write");
            let plan = options.get("--plan").cloned();
            if write != plan.is_some() {
                return Err(ParseError {
                    message: "`codebase export` requires `--write` and `--plan PLAN` together".to_owned(),
                });
            }
            Request::Export {
                store,
                selector,
                destination,
                write,
                plan,
            }
        }
        "verify" => {
            ensure_options(subcommand, &options, &["--store"])?;
            require_arity(subcommand, positionals.as_slice(), 0)?;
            Request::Verify { store }
        }
        "reindex" => {
            ensure_options(subcommand, &options, &["--store"])?;
            require_arity(subcommand, positionals.as_slice(), 0)?;
            Request::Reindex { store }
        }
        "backup" => {
            ensure_options(subcommand, &options, &["--store"])?;
            Request::Backup {
                store,
                destination: PathBuf::from(single_operand(subcommand, &positionals)?),
            }
        }
        "restore" => {
            ensure_options(subcommand, &options, &["--store"])?;
            Request::Restore {
                store,
                source: PathBuf::from(single_operand(subcommand, &positionals)?),
            }
        }
        _ => {
            return Err(ParseError {
                message: format!("unknown codebase command `{subcommand}`"),
            });
        }
    };
    Ok(request)
}

fn ensure_options(
    command: &str,
    options: &std::collections::BTreeMap<String, String>,
    allowed: &[&str],
) -> Result<(), ParseError> {
    if let Some(unknown) = options
        .keys()
        .find(|option| !allowed.contains(&option.as_str()))
    {
        Err(ParseError {
            message: format!("`{unknown}` is not valid for `codebase {command}`"),
        })
    } else {
        Ok(())
    }
}

fn parse_options(
    arguments: &[OsString],
) -> Result<(Vec<String>, std::collections::BTreeMap<String, String>), String> {
    let mut positionals = Vec::new();
    let mut options = std::collections::BTreeMap::new();
    let mut cursor = 0;
    while cursor < arguments.len() {
        let argument = arguments
            .get(cursor)
            .ok_or_else(|| "missing argument".to_owned())?;
        let Some(value) = argument.to_str() else {
            return Err("codebase command arguments must be valid UTF-8".to_owned());
        };
        match value {
            "--store" | "--head" | "--revision" | "--expect" | "--destination"
            | "--plan" => {
                if options.contains_key(value) {
                    return Err(format!("`{value}` may occur only once"));
                }
                let next = arguments
                    .get(cursor.saturating_add(1))
                    .ok_or_else(|| format!("`{value}` requires a value"))?;
                let next = next
                    .to_str()
                    .ok_or_else(|| format!("`{value}` value must be valid UTF-8"))?;
                if next.is_empty() || next.starts_with('-') {
                    return Err(format!("`{value}` requires a non-option value"));
                }
                options.insert(value.to_owned(), next.to_owned());
                cursor = cursor.saturating_add(2);
            }
            "--write" => {
                if options.insert(value.to_owned(), String::new()).is_some() {
                    return Err("`--write` may occur only once".to_owned());
                }
                cursor = cursor.saturating_add(1);
            }
            _ if value.starts_with('-') => {
                return Err(format!("unknown codebase option `{value}`"));
            }
            _ => {
                positionals.push(value.to_owned());
                cursor = cursor.saturating_add(1);
            }
        }
    }
    Ok((positionals, options))
}

fn parse_selector(
    options: &std::collections::BTreeMap<String, String>,
) -> Result<Selector, ParseError> {
    match (options.get("--head"), options.get("--revision")) {
        (Some(head), None) => Ok(Selector::Head(head.clone())),
        (None, Some(revision)) => Ok(Selector::Revision(revision.clone())),
        (Some(_), Some(_)) => Err(ParseError {
            message: "select exactly one of `--head NAME` and `--revision REVISION`"
                .to_owned(),
        }),
        (None, None) => Err(ParseError {
            message: "select exactly one of `--head NAME` and `--revision REVISION`"
                .to_owned(),
        }),
    }
}

trait Arity {
    fn accepts(self, value: usize) -> bool;
}

impl Arity for usize {
    fn accepts(self, value: usize) -> bool {
        self == value
    }
}

impl Arity for std::ops::RangeInclusive<usize> {
    fn accepts(self, value: usize) -> bool {
        self.contains(&value)
    }
}

fn require_arity<A: Arity>(
    command: &str,
    positionals: &[String],
    arity: A,
) -> Result<(), ParseError> {
    if arity.accepts(positionals.len()) {
        Ok(())
    } else {
        Err(arity_error(command))
    }
}

fn single_operand<'a>(
    command: &str,
    positionals: &'a [String],
) -> Result<&'a str, ParseError> {
    match positionals {
        [operand] => Ok(operand),
        _ => Err(arity_error(command)),
    }
}

fn arity_error(command: &str) -> ParseError {
    ParseError {
        message: format!(
            "`codebase {command}` has an invalid number of positional arguments"
        ),
    }
}

/// Executes one parsed codebase command through workspace-owned services.
pub(crate) fn execute<W: Write, E: Write>(
    invocation: &Invocation,
    request: Request,
    stdout: &mut W,
    stderr: &mut E,
) -> CommandEnvelope {
    match execute_request(invocation, request) {
        Ok((payload, diagnostics, source_texts)) => {
            let documents =
                match render_diagnostics_with_sources(&diagnostics, &source_texts) {
                    Ok(documents) => documents,
                    Err(message) => {
                        return error_envelope(
                            invocation,
                            CommandResult::OperationalFailure,
                            vec![unlocated_diagnostic(
                                DiagnosticCode::StoreIoError,
                                message,
                            )],
                            stderr,
                        );
                    }
                };
            if invocation.output_format == crate::OutputFormat::Human
                && let Err(error) =
                    writeln!(stdout, "{}", human_summary(&invocation.command, &payload))
            {
                return error_envelope(
                    invocation,
                    CommandResult::OperationalFailure,
                    vec![unlocated_diagnostic(
                        DiagnosticCode::StoreIoError,
                        format!("cannot write codebase command output: {error}"),
                    )],
                    stderr,
                );
            }
            CommandEnvelope {
                schema_version: SCHEMA_VERSION,
                command: invocation.command.clone(),
                result: CommandResult::Ok,
                diagnostics: documents,
                payload: Payload::Codebase(payload),
            }
        }
        Err(ExecutionError::Store(error)) => error_envelope(
            invocation,
            CommandResult::OperationalFailure,
            vec![error.diagnostic()],
            stderr,
        ),
        Err(ExecutionError::Import(error)) => {
            let result = if error.source_rejected() {
                CommandResult::Diagnostics
            } else {
                CommandResult::OperationalFailure
            };
            let diagnostics = error.diagnostics().to_vec();
            let documents =
                render_diagnostics_with_sources(&diagnostics, error.source_texts())
                    .unwrap_or_else(|_| render_unlocated(&diagnostics));
            CommandEnvelope {
                schema_version: SCHEMA_VERSION,
                command: invocation.command.clone(),
                result,
                diagnostics: documents,
                payload: Payload::Codebase(json!({})),
            }
        }
    }
}

enum ExecutionError {
    Store(StoreError),
    Import(vibra_workspace::codebase::import::ImportError),
}

impl From<StoreError> for ExecutionError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

type ExecutionSuccess = (
    Value,
    Vec<Diagnostic>,
    std::collections::BTreeMap<String, String>,
);

fn execute_request(
    invocation: &Invocation,
    request: Request,
) -> Result<ExecutionSuccess, ExecutionError> {
    let workspace = &invocation.workspace;
    match request {
        Request::Init { store } => {
            let store = CodebaseStore::init(workspace, &store)?;
            Ok(success(
                json!({ "store": display_path(store.relative_path()) }),
                Vec::new(),
            ))
        }
        Request::Head { store, name } => {
            if name.as_deref().is_some_and(|value| !valid_head_name(value)) {
                return Err(StoreError::InvalidInput(
                    "head names must be lowercase kebab-case".to_owned(),
                )
                .into());
            }
            let store = open(workspace, &store, StoreAccess::ReadOnly)?;
            let revision = store.head(name.as_deref())?;
            Ok(success(
                json!({
                    "head": name.as_deref().unwrap_or("main"),
                    "revision": revision,
                }),
                Vec::new(),
            ))
        }
        Request::Import {
            store,
            head,
            expected,
        } => {
            let expectation = if expected == "absent" {
                HeadExpectation::Absent
            } else if valid_address(&expected) {
                HeadExpectation::Revision(expected)
            } else {
                return Err(StoreError::InvalidInput(
                    "`--expect` must be `absent` or a canonical sha256 address"
                        .to_owned(),
                )
                .into());
            };
            if !valid_head_name(&head) {
                return Err(StoreError::InvalidInput(
                    "head names must be lowercase kebab-case".to_owned(),
                )
                .into());
            }
            let store = open(workspace, &store, StoreAccess::ReadWrite)?;
            let prepared = prepare_import(workspace).map_err(ExecutionError::Import)?;
            let receipt =
                store.publish(&head, expectation, &prepared.graph, &prepared.index)?;
            let source_texts = prepared.source_texts;
            Ok((
                json!({
                    "head": head,
                    "revision": receipt.revision,
                    "published": receipt.published,
                    "objectCount": receipt.object_count,
                    "sourceCount": receipt.source_count,
                }),
                prepared.diagnostics,
                source_texts,
            ))
        }
        Request::Lookup {
            store,
            selector,
            entity,
        } => {
            if !valid_entity_key(&entity) {
                return Err(StoreError::InvalidInput(
                    "entity must be one canonical VIBON entity-key record".to_owned(),
                )
                .into());
            }
            let revision = select_revision(workspace, &store, &selector)?;
            let opened = open(workspace, &store, StoreAccess::ReadOnly)?;
            let result = opened.lookup(&revision, &entity)?;
            Ok(success(
                json!({
                    "revision": result.revision,
                    "entity": result.entity,
                    "object": result.object,
                    "part": result.part,
                }),
                Vec::new(),
            ))
        }
        Request::Read {
            store,
            selector,
            object,
        } => {
            if !valid_address(&object) {
                return Err(StoreError::InvalidInput(
                    "object must be a canonical sha256 address".to_owned(),
                )
                .into());
            }
            let revision = select_revision(workspace, &store, &selector)?;
            let opened = open(workspace, &store, StoreAccess::ReadOnly)?;
            let result = opened.read(&revision, &object)?;
            let references = result.references.into_iter().map(|reference| {
                let target = match reference.target {
                    ReadTarget::Object(address) => json!(address),
                    ReadTarget::InternalMember(index) => json!(index),
                    ReadTarget::TrustedExternal { identity, registry } => json!({
                        "kind": "@trusted-external",
                        "identity": identity,
                        "registry": registry,
                    }),
                };
                json!({ "part": reference.part, "kind": reference.kind, "target": target })
            }).collect::<Vec<_>>();
            Ok(success(
                json!({
                    "revision": result.revision,
                    "object": result.object,
                    "component": result.component,
                    "memberIndex": result.member_index,
                    "payload": result.payload,
                    "references": references,
                }),
                Vec::new(),
            ))
        }
        Request::Export {
            store,
            selector,
            destination,
            write,
            plan,
        } => {
            if write && !plan.as_deref().is_some_and(valid_address) {
                return Err(StoreError::InvalidInput(
                    "export `--plan` must be a canonical sha256 address".to_owned(),
                )
                .into());
            }
            let revision = select_revision(workspace, &store, &selector)?;
            let opened = open(workspace, &store, StoreAccess::ReadOnly)?;
            let payload = if write {
                let result = opened.apply_export(
                    &revision,
                    &destination,
                    plan.as_deref().unwrap_or_default(),
                )?;
                json!({
                    "revision": result.revision,
                    "destination": result.destination,
                    "plan": result.plan,
                    "files": result.files.iter().map(|file| json!({
                        "path": file.path,
                        "sourceHash": file.source_hash,
                        "byteLength": file.byte_length,
                    })).collect::<Vec<_>>(),
                    "written": result.written,
                })
            } else {
                let result = opened.preview_export(&revision, &destination)?;
                json!({
                    "revision": result.revision,
                    "destination": result.destination,
                    "plan": result.plan,
                    "files": result.files.iter().map(|file| json!({
                        "path": file.path,
                        "sourceHash": file.source_hash,
                        "byteLength": file.byte_length,
                    })).collect::<Vec<_>>(),
                    "written": false,
                })
            };
            Ok(success(payload, Vec::new()))
        }
        Request::Verify { store } => {
            let opened = open(workspace, &store, StoreAccess::ReadOnly)?;
            let report = opened.verify()?;
            Ok(success(
                json!({
                    "valid": true,
                    "revisionCount": report.revision_count,
                    "objectCount": report.object_count,
                    "sourceCount": report.source_count,
                }),
                Vec::new(),
            ))
        }
        Request::Reindex { store } => {
            let opened = open(workspace, &store, StoreAccess::ReadWrite)?;
            let report = opened.reindex()?;
            Ok(success(
                json!({
                    "revisionCount": report.revision_count,
                    "entityCount": report.entity_count,
                    "edgeCount": report.edge_count,
                }),
                Vec::new(),
            ))
        }
        Request::Backup { store, destination } => {
            let opened = open(workspace, &store, StoreAccess::ReadOnly)?;
            let report = opened.backup(&destination)?;
            Ok(success(
                json!({
                    "destination": display_path(&report.destination),
                    "revisionCount": report.revision_count,
                }),
                Vec::new(),
            ))
        }
        Request::Restore { store, source } => {
            let report = CodebaseStore::restore(workspace, &source, &store)?;
            Ok(success(
                json!({
                    "store": display_path(&report.store),
                    "revisionCount": report.revision_count,
                    "objectCount": report.object_count,
                    "sourceCount": report.source_count,
                }),
                Vec::new(),
            ))
        }
    }
}

fn success(payload: Value, diagnostics: Vec<Diagnostic>) -> ExecutionSuccess {
    (payload, diagnostics, std::collections::BTreeMap::new())
}

fn select_revision(
    workspace: &Path,
    store_path: &Path,
    selector: &Selector,
) -> Result<String, StoreError> {
    match selector {
        Selector::Head(name) => {
            if !valid_head_name(name) {
                return Err(StoreError::InvalidInput(
                    "head names must be lowercase kebab-case".to_owned(),
                ));
            }
            let opened = open(workspace, store_path, StoreAccess::ReadOnly)?;
            opened.head(Some(name))
        }
        Selector::Revision(revision) if valid_address(revision) => Ok(revision.clone()),
        Selector::Revision(_) => Err(StoreError::InvalidInput(
            "revision must be a canonical sha256 address".to_owned(),
        )),
    }
}

fn open(
    workspace: &Path,
    store: &Path,
    access: StoreAccess,
) -> Result<CodebaseStore, StoreError> {
    CodebaseStore::open(workspace, store, access)
}

fn valid_address(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return false;
    };
    hex.len() == 64
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_head_name(value: &str) -> bool {
    vibra_workspace::codebase::is_valid_head_name(value)
}

fn valid_entity_key(value: &str) -> bool {
    vibra_workspace::codebase::is_valid_entity_key(value)
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn human_summary(command: &str, payload: &Value) -> String {
    match command {
        "codebase-init" => format!(
            "Initialized codebase store {}",
            payload["store"].as_str().unwrap_or_default()
        ),
        "codebase-head" => format!(
            "{} -> {}",
            payload["head"].as_str().unwrap_or_default(),
            payload["revision"].as_str().unwrap_or_default()
        ),
        "codebase-import" => format!(
            "Imported revision {}",
            payload["revision"].as_str().unwrap_or_default()
        ),
        "codebase-lookup" => format!(
            "{} -> {}",
            payload["entity"].as_str().unwrap_or_default(),
            payload["object"].as_str().unwrap_or_default()
        ),
        "codebase-read" => format!(
            "Read object {}",
            payload["object"].as_str().unwrap_or_default()
        ),
        "codebase-export" => format!(
            "Export {}{}",
            payload["destination"].as_str().unwrap_or_default(),
            if payload["written"] == true {
                " written"
            } else {
                " previewed"
            }
        ),
        "codebase-verify" => "Codebase integrity verified".to_owned(),
        "codebase-reindex" => "Rebuilt codebase indexes".to_owned(),
        "codebase-backup" => format!(
            "Backed up codebase to {}",
            payload["destination"].as_str().unwrap_or_default()
        ),
        "codebase-restore" => format!(
            "Restored codebase to {}",
            payload["store"].as_str().unwrap_or_default()
        ),
        _ => "Codebase command completed".to_owned(),
    }
}

fn error_envelope<E: Write>(
    invocation: &Invocation,
    result: CommandResult,
    diagnostics: Vec<Diagnostic>,
    stderr: &mut E,
) -> CommandEnvelope {
    if invocation.output_format == crate::OutputFormat::Human {
        for diagnostic in &diagnostics {
            let _ = writeln!(stderr, "{}: {}", diagnostic.code(), diagnostic.message());
        }
    }
    CommandEnvelope {
        schema_version: SCHEMA_VERSION,
        command: invocation.command.clone(),
        result,
        diagnostics: render_unlocated(&diagnostics),
        payload: Payload::Codebase(json!({})),
    }
}

fn unlocated_diagnostic(code: DiagnosticCode, message: String) -> Diagnostic {
    Diagnostic::new(code, ByteSpan::empty_at(0), message)
}

fn render_unlocated(diagnostics: &[Diagnostic]) -> Vec<DiagnosticDocument> {
    let index = LineIndex::new("");
    diagnostics
        .iter()
        .filter_map(|diagnostic| DiagnosticDocument::render(diagnostic, &index).ok())
        .collect()
}

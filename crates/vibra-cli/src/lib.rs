//! Library-owned command parsing, planning, result mapping, and rendering.

pub mod init;

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode, Level, LineIndex};
use vibra_schema::{DiagnosticDocument, SCHEMA_VERSION};
use vibra_workspace::format_plan::{
    FormatPlan, FormatPlanError, apply_format, plan_format,
};

pub use init::{InitError, InitPlan, apply_init, plan_init};

/// The global output mode accepted before a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    /// Human-readable terminal output.
    Human,
    /// One versioned JSON envelope on standard output.
    Json,
}

/// Stable command outcome and process exit-code mapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum CommandResult {
    /// The operation completed successfully.
    #[serde(rename = "@command.ok")]
    Ok,
    /// The operation found an error-level diagnostic.
    #[serde(rename = "@command.diagnostics")]
    Diagnostics,
    /// A selected test failed.
    #[serde(rename = "@command.test-failed")]
    TestFailed,
    /// The command or its options are invalid.
    #[serde(rename = "@command.invalid-input")]
    InvalidInput,
    /// A filesystem or host operation failed.
    #[serde(rename = "@command.operational-failure")]
    OperationalFailure,
    /// The selected valid command is not implemented in this step.
    #[serde(rename = "@command.unavailable")]
    Unavailable,
    /// A future execution command trapped while running a program.
    #[serde(rename = "@command.trap")]
    Trap,
}

impl CommandResult {
    const fn exit_code(self) -> i32 {
        match self {
            Self::Ok => 0,
            Self::Diagnostics | Self::TestFailed => 1,
            Self::InvalidInput => 2,
            Self::OperationalFailure => 3,
            Self::Unavailable => 4,
            Self::Trap => 5,
        }
    }
}

impl fmt::Display for CommandResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Ok => "@command.ok",
            Self::Diagnostics => "@command.diagnostics",
            Self::TestFailed => "@command.test-failed",
            Self::InvalidInput => "@command.invalid-input",
            Self::OperationalFailure => "@command.operational-failure",
            Self::Unavailable => "@command.unavailable",
            Self::Trap => "@command.trap",
        })
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CommandEnvelope {
    schema_version: u32,
    command: String,
    result: CommandResult,
    diagnostics: Vec<DiagnosticDocument>,
    payload: Payload,
}

#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
enum Payload {
    Init(InitPayload),
    Fmt(FmtPayload),
    Check(CheckPayload),
    Run(RunPayload),
    Test(TestPayload),
    Empty(EmptyPayload),
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InitPayload {
    workspace: String,
    created: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FmtPayload {
    path: String,
    changed: bool,
    written: bool,
    text: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CheckPayload {
    accepted: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RunPayload {
    target: String,
    program_result: Option<String>,
    stdout: String,
    stderr: String,
    audit_trace: Vec<String>,
    trap: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TestPayload {
    selected: usize,
    passed: usize,
    failed: usize,
    tests: Vec<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct EmptyPayload {}

#[derive(Clone, Debug)]
struct Invocation {
    output_format: OutputFormat,
    workspace: PathBuf,
    command: String,
    action: Action,
}

#[derive(Clone, Debug)]
enum Action {
    Init(Option<PathBuf>),
    Fmt {
        path: PathBuf,
        write: bool,
    },
    Check {
        target: Option<PathBuf>,
    },
    Run {
        target: PathBuf,
    },
    Unavailable {
        arguments: Vec<OsString>,
    },
    Invalid {
        message: String,
        path: Option<PathBuf>,
    },
}

/// Runs one command and returns its stable process exit code.
///
/// Parsing, service calls, and output routing remain in this library so the
/// process binary contains no duplicate command behavior.
pub fn run<W: Write, E: Write>(
    arguments: impl IntoIterator<Item = OsString>,
    mut stdout: W,
    mut stderr: E,
) -> i32 {
    let current_dir = match std::env::current_dir() {
        Ok(path) => path,
        Err(error) => {
            let _ = writeln!(stderr, "cannot read current directory: {error}");
            return CommandResult::OperationalFailure.exit_code();
        }
    };
    let invocation = parse_invocation(arguments, &current_dir);
    let output_format = invocation.output_format;
    let envelope = execute(invocation, &mut stdout, &mut stderr);
    if let Err(error) = render_diagnostics(&envelope.diagnostics, &mut stderr) {
        let _ = writeln!(stderr, "cannot render diagnostics: {error}");
        return CommandResult::OperationalFailure.exit_code();
    }
    if output_format == OutputFormat::Json
        && (serde_json::to_writer(&mut stdout, &envelope).is_err()
            || stdout.write_all(b"\n").is_err())
    {
        let _ = writeln!(stderr, "cannot write command JSON envelope");
        return CommandResult::OperationalFailure.exit_code();
    }
    envelope.result.exit_code()
}

fn execute<W: Write, E: Write>(
    invocation: Invocation,
    stdout: &mut W,
    stderr: &mut E,
) -> CommandEnvelope {
    match invocation.action.clone() {
        Action::Init(destination) => {
            match plan_init(&invocation.workspace, destination.as_deref()) {
                Ok(plan) => execute_init(plan, invocation, stdout, stderr),
                Err(error) => init_error_envelope(&invocation, error, stderr),
            }
        }
        Action::Fmt { path, write } => {
            match plan_format(&invocation.workspace, &path) {
                Ok(plan) => execute_format(plan, write, invocation, stdout, stderr),
                Err(error) => fmt_error_envelope(&invocation, &path, error, stderr),
            }
        }
        Action::Check { target } => {
            execute_check(&invocation, target.as_deref(), stdout, stderr)
        }
        Action::Run { target } => execute_run(&invocation, &target, stderr),
        Action::Unavailable { arguments } => {
            let message =
                format!("`{}` is not available in M2 Step 12", invocation.command);
            let diagnostic = Diagnostic::new(
                DiagnosticCode::ToolUnavailable,
                ByteSpan::empty_at(0),
                message.clone(),
            );
            unavailable_envelope(&invocation, arguments, diagnostic)
        }
        Action::Invalid { message, path } => {
            if invocation.output_format == OutputFormat::Human {
                let _ = writeln!(stderr, "invalid input: {message}");
            }
            invalid_envelope(&invocation, path)
        }
    }
}

fn execute_init<W: Write, E: Write>(
    plan: InitPlan,
    invocation: Invocation,
    stdout: &mut W,
    stderr: &mut E,
) -> CommandEnvelope {
    if invocation.output_format == OutputFormat::Human {
        if writeln!(
            stdout,
            "Planned project destinations in {}:",
            plan.workspace().display()
        )
        .is_err()
        {
            return operational_init_envelope(
                &invocation,
                plan.workspace(),
                "cannot show the initialization plan".to_owned(),
            );
        }
        for path in plan.created_paths() {
            if writeln!(stdout, "  {path}").is_err() {
                return operational_init_envelope(
                    &invocation,
                    plan.workspace(),
                    "cannot show the initialization plan".to_owned(),
                );
            }
        }
    }
    match apply_init(&plan) {
        Ok(()) => {
            if invocation.output_format == OutputFormat::Human {
                let _ = writeln!(stdout, "Initialized {}", plan.workspace().display());
            }
            CommandEnvelope {
                schema_version: SCHEMA_VERSION,
                command: invocation.command,
                result: CommandResult::Ok,
                diagnostics: Vec::new(),
                payload: Payload::Init(InitPayload {
                    workspace: plan.workspace().display().to_string(),
                    created: plan.created_paths().to_vec(),
                }),
            }
        }
        Err(error) => init_error_envelope(&invocation, error, stderr),
    }
}

fn execute_format<W: Write, E: Write>(
    plan: FormatPlan,
    write: bool,
    invocation: Invocation,
    stdout: &mut W,
    stderr: &mut E,
) -> CommandEnvelope {
    let diagnostic_documents = match render_plan_diagnostics(&plan) {
        Ok(documents) => documents,
        Err(error) => {
            return fmt_error_envelope(
                &invocation,
                Path::new(plan.relative_path()),
                FormatPlanError::Postcondition(error),
                stderr,
            );
        }
    };
    let diagnostics_found = plan
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.level() == Level::Error);
    let mut result = if diagnostics_found {
        CommandResult::Diagnostics
    } else {
        CommandResult::Ok
    };
    let mut written = false;
    if write && result == CommandResult::Ok {
        match apply_format(&plan) {
            Ok(()) => written = plan.changed(),
            Err(error) => {
                let message = error.to_string();
                let diagnostic = operational_diagnostic(message);
                return CommandEnvelope {
                    schema_version: SCHEMA_VERSION,
                    command: invocation.command,
                    result: CommandResult::OperationalFailure,
                    diagnostics: render_unlocated_diagnostics(&[diagnostic]),
                    payload: Payload::Fmt(FmtPayload {
                        path: plan.relative_path().to_owned(),
                        changed: plan.changed(),
                        written: false,
                        text: Some(plan.formatted_text().to_owned()),
                    }),
                };
            }
        }
    } else if !write
        && invocation.output_format == OutputFormat::Human
        && stdout.write_all(plan.formatted_text().as_bytes()).is_err()
    {
        result = CommandResult::OperationalFailure;
        let _ = writeln!(stderr, "cannot write formatter preview");
    }
    if write
        && invocation.output_format == OutputFormat::Human
        && result == CommandResult::Ok
    {
        let message = if written {
            format!("Formatted {}", plan.relative_path())
        } else {
            format!("Already formatted: {}", plan.relative_path())
        };
        let _ = writeln!(stdout, "{message}");
    }
    CommandEnvelope {
        schema_version: SCHEMA_VERSION,
        command: invocation.command,
        result,
        diagnostics: diagnostic_documents,
        payload: Payload::Fmt(FmtPayload {
            path: plan.relative_path().to_owned(),
            changed: plan.changed(),
            written,
            text: if !write || diagnostics_found {
                Some(plan.formatted_text().to_owned())
            } else {
                None
            },
        }),
    }
}

fn execute_check<W: Write, E: Write>(
    invocation: &Invocation,
    target_path: Option<&Path>,
    stdout: &mut W,
    stderr: &mut E,
) -> CommandEnvelope {
    let snapshot = match vibra_workspace::WorkspaceSnapshot::load(&invocation.workspace)
    {
        Ok(snapshot) => snapshot,
        Err(error) => return workspace_load_error_envelope(invocation, error, stderr),
    };
    let target = match target_path {
        Some(path) => match select_target(&snapshot, path) {
            Ok(target) => Some(target),
            Err(()) => {
                if invocation.output_format == OutputFormat::Human {
                    let _ = writeln!(
                        stderr,
                        "invalid input: target root does not identify a local target"
                    );
                }
                return invalid_envelope(
                    invocation,
                    target_path.map(Path::to_path_buf),
                );
            }
        },
        None => None,
    };
    let verification = match verify_workspace_bootstrap(&snapshot) {
        Ok(verification) => verification,
        Err(message) => {
            return operational_check_envelope(invocation, target_path, message);
        }
    };
    let checked = match (target, verification.as_ref()) {
        (Some(target), Some(verification)) => {
            vibra_workspace::semantic::check_target_with_bootstrap(
                &snapshot,
                target,
                Some(verification),
            )
        }
        (Some(target), None) => {
            vibra_workspace::semantic::check_target(&snapshot, target)
        }
        (None, Some(verification)) => {
            vibra_workspace::semantic::check_all_with_bootstrap(
                &snapshot,
                Some(verification),
            )
        }
        (None, None) => vibra_workspace::semantic::check_all(&snapshot),
    };
    let diagnostics = match render_workspace_diagnostics(
        &snapshot,
        verification.as_ref(),
        checked.diagnostics(),
    ) {
        Ok(diagnostics) => diagnostics,
        Err(message) => {
            return operational_check_envelope(invocation, target_path, message);
        }
    };
    let result = match checked.status() {
        vibra_workspace::semantic::CheckStatus::Accepted => CommandResult::Ok,
        vibra_workspace::semantic::CheckStatus::Diagnostics => {
            CommandResult::Diagnostics
        }
        vibra_workspace::semantic::CheckStatus::Unavailable => {
            CommandResult::Unavailable
        }
    };
    if result == CommandResult::Ok && invocation.output_format == OutputFormat::Human {
        let _ = writeln!(stdout, "check accepted");
    }
    CommandEnvelope {
        schema_version: SCHEMA_VERSION,
        command: invocation.command.clone(),
        result,
        diagnostics,
        payload: Payload::Check(CheckPayload {
            accepted: result == CommandResult::Ok,
        }),
    }
}

fn execute_run<E: Write>(
    invocation: &Invocation,
    target_path: &Path,
    stderr: &mut E,
) -> CommandEnvelope {
    let snapshot = match vibra_workspace::WorkspaceSnapshot::load(&invocation.workspace)
    {
        Ok(snapshot) => snapshot,
        Err(error) => return workspace_load_error_envelope(invocation, error, stderr),
    };
    let target = match select_target(&snapshot, target_path) {
        Ok(target) => target,
        Err(()) => {
            if invocation.output_format == OutputFormat::Human {
                let _ = writeln!(
                    stderr,
                    "invalid input: target root does not identify a local binary target"
                );
            }
            return invalid_envelope(invocation, Some(target_path.to_path_buf()));
        }
    };
    if target.kind() != vibra_workspace::project::TargetKind::Bin {
        if invocation.output_format == OutputFormat::Human {
            let _ = writeln!(stderr, "invalid input: `run` requires a binary target");
        }
        return invalid_envelope(invocation, Some(target_path.to_path_buf()));
    }
    let target_root = canonical_target_root(&snapshot, target);
    let verification = match verify_workspace_bootstrap(&snapshot) {
        Ok(verification) => verification,
        Err(message) => {
            return operational_run_envelope(invocation, &target_root, message, stderr);
        }
    };
    let outcome = match verification.as_ref() {
        Some(verification) => vibra_workspace::semantic::run_target_with_bootstrap(
            &snapshot,
            target,
            Some(verification),
        ),
        None => vibra_workspace::semantic::run_target(&snapshot, target),
    };
    let diagnostics = match render_workspace_diagnostics(
        &snapshot,
        verification.as_ref(),
        outcome.check().diagnostics(),
    ) {
        Ok(diagnostics) => diagnostics,
        Err(message) => {
            return operational_run_envelope(invocation, &target_root, message, stderr);
        }
    };
    let result = match outcome.check().status() {
        vibra_workspace::semantic::CheckStatus::Diagnostics => {
            CommandResult::Diagnostics
        }
        vibra_workspace::semantic::CheckStatus::Unavailable => {
            CommandResult::Unavailable
        }
        vibra_workspace::semantic::CheckStatus::Accepted => match outcome.outcome() {
            Some(vibra_workspace::semantic::RunOutcome::Program(_)) => {
                CommandResult::Ok
            }
            Some(vibra_workspace::semantic::RunOutcome::InterpreterFailure(_)) => {
                CommandResult::OperationalFailure
            }
            None => CommandResult::OperationalFailure,
        },
    };
    let (program_result, stdout, program_stderr, audit_trace) = match outcome.outcome()
    {
        Some(vibra_workspace::semantic::RunOutcome::Program(execution)) => (
            Some(execution.canonical_result()),
            String::new(),
            String::new(),
            execution.audit_trace().to_vec(),
        ),
        Some(vibra_workspace::semantic::RunOutcome::InterpreterFailure(error)) => {
            let _ = writeln!(stderr, "interpreter invariant failure: {error}");
            (None, String::new(), String::new(), Vec::new())
        }
        None => {
            if result == CommandResult::OperationalFailure {
                let _ = writeln!(
                    stderr,
                    "accepted target did not produce an executable program"
                );
            }
            (None, String::new(), String::new(), Vec::new())
        }
    };
    CommandEnvelope {
        schema_version: SCHEMA_VERSION,
        command: invocation.command.clone(),
        result,
        diagnostics,
        payload: Payload::Run(RunPayload {
            target: target_root,
            program_result,
            stdout,
            stderr: program_stderr,
            audit_trace,
            trap: None,
        }),
    }
}

fn select_target<'a>(
    snapshot: &'a vibra_workspace::WorkspaceSnapshot,
    path: &Path,
) -> Result<&'a vibra_workspace::project::Target, ()> {
    let root = snapshot.project().root();
    let selected = root.join(path).canonicalize().map_err(|_| ())?;
    if !selected.starts_with(root) {
        return Err(());
    }
    let unit = snapshot
        .source()
        .units()
        .iter()
        .find(|unit| unit.root() == selected)
        .ok_or(())?;
    snapshot
        .project()
        .project()
        .targets()
        .iter()
        .find(|target| target.name().atom().value() == unit.name())
        .ok_or(())
}

fn canonical_target_root(
    snapshot: &vibra_workspace::WorkspaceSnapshot,
    target: &vibra_workspace::project::Target,
) -> String {
    let Some(unit) = snapshot
        .source()
        .units()
        .iter()
        .find(|unit| unit.name() == target.name().atom().value())
    else {
        return target.root().value().replace('\\', "/");
    };
    unit.root()
        .strip_prefix(snapshot.source().project_root())
        .map_or_else(
            |_| target.root().value().replace('\\', "/"),
            |relative| relative.to_string_lossy().replace('\\', "/"),
        )
}

fn verify_toolchain_bootstrap() -> Result<vibra_types::BootstrapVerification, String> {
    let repository_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    vibra_types::verify_bootstrap(repository_root).map_err(|error| error.to_string())
}

fn verify_workspace_bootstrap(
    snapshot: &vibra_workspace::WorkspaceSnapshot,
) -> Result<Option<vibra_types::BootstrapVerification>, String> {
    let requires_verification = snapshot
        .requires_bootstrap_verification()
        .map_err(|error| error.to_string())?;
    if requires_verification {
        verify_toolchain_bootstrap().map(Some)
    } else {
        Ok(None)
    }
}

fn render_workspace_diagnostics(
    snapshot: &vibra_workspace::WorkspaceSnapshot,
    verification: Option<&vibra_types::BootstrapVerification>,
    diagnostics: &[Diagnostic],
) -> Result<Vec<DiagnosticDocument>, String> {
    let mut sources = std::collections::BTreeMap::<String, String>::new();
    sources.insert(
        snapshot.project().project_source_id().to_owned(),
        String::from_utf8_lossy(snapshot.project().project_bytes()).into_owned(),
    );
    for document in snapshot.source().documents() {
        sources.insert(
            document.source_id().to_owned(),
            String::from_utf8_lossy(document.bytes()).into_owned(),
        );
    }
    if let Some(verification) = verification {
        let (_, bootstrap_modules) = verification.resolver_overlay();
        for module in bootstrap_modules {
            sources
                .entry(module.source_id().to_owned())
                .or_insert_with(|| {
                    String::from_utf8_lossy(module.bytes()).into_owned()
                });
        }
    }
    render_diagnostics_with_sources(diagnostics, &sources)
}

fn render_diagnostics_with_sources(
    diagnostics: &[Diagnostic],
    sources: &std::collections::BTreeMap<String, String>,
) -> Result<Vec<DiagnosticDocument>, String> {
    let indexes = sources
        .iter()
        .map(|(source_id, source)| (source_id.clone(), LineIndex::new(source)))
        .collect::<std::collections::BTreeMap<_, _>>();
    let fallback = LineIndex::new("");
    diagnostics
        .iter()
        .map(|diagnostic| {
            let primary = diagnostic
                .source_id()
                .and_then(|source_id| indexes.get(source_id))
                .unwrap_or(&fallback);
            DiagnosticDocument::render_with_source_indexes(
                diagnostic, primary, &indexes,
            )
            .map_err(|error| error.to_string())
        })
        .collect()
}

fn workspace_load_error_envelope<E: Write>(
    invocation: &Invocation,
    error: vibra_workspace::WorkspaceError,
    stderr: &mut E,
) -> CommandEnvelope {
    let mut diagnostics = render_workspace_error_diagnostics(&error)
        .unwrap_or_else(|_| render_unlocated_diagnostics(error.diagnostics()));
    let result = if error.diagnostics().is_empty() {
        let message = error.to_string();
        let _ = writeln!(stderr, "{message}");
        diagnostics = render_unlocated_diagnostics(&[operational_diagnostic(message)]);
        CommandResult::OperationalFailure
    } else {
        classify_workspace_load_error(&error)
    };
    check_or_run_error_envelope(invocation, result, diagnostics, None)
}

fn render_workspace_error_diagnostics(
    error: &vibra_workspace::WorkspaceError,
) -> Result<Vec<DiagnosticDocument>, String> {
    render_diagnostics_with_sources(error.diagnostics(), error.source_texts())
}

fn classify_workspace_load_error(
    error: &vibra_workspace::WorkspaceError,
) -> CommandResult {
    if error.diagnostics().is_empty()
        || error.diagnostics().iter().any(|diagnostic| {
            matches!(
                diagnostic.code(),
                DiagnosticCode::ProjectIoError | DiagnosticCode::ModuleIoError
            )
        })
    {
        CommandResult::OperationalFailure
    } else {
        CommandResult::Diagnostics
    }
}

fn operational_check_envelope(
    invocation: &Invocation,
    target: Option<&Path>,
    message: String,
) -> CommandEnvelope {
    let diagnostics = render_unlocated_diagnostics(&[operational_diagnostic(message)]);
    check_or_run_error_envelope(
        invocation,
        CommandResult::OperationalFailure,
        diagnostics,
        target,
    )
}

fn operational_run_envelope<E: Write>(
    invocation: &Invocation,
    target: &str,
    message: String,
    stderr: &mut E,
) -> CommandEnvelope {
    let _ = writeln!(stderr, "{message}");
    let diagnostics = render_unlocated_diagnostics(&[operational_diagnostic(message)]);
    CommandEnvelope {
        schema_version: SCHEMA_VERSION,
        command: invocation.command.clone(),
        result: CommandResult::OperationalFailure,
        diagnostics,
        payload: Payload::Run(RunPayload {
            target: target.to_owned(),
            program_result: None,
            stdout: String::new(),
            stderr: String::new(),
            audit_trace: Vec::new(),
            trap: None,
        }),
    }
}

fn check_or_run_error_envelope(
    invocation: &Invocation,
    result: CommandResult,
    diagnostics: Vec<DiagnosticDocument>,
    target: Option<&Path>,
) -> CommandEnvelope {
    let payload = match invocation.command.as_str() {
        "check" => Payload::Check(CheckPayload { accepted: false }),
        "run" => Payload::Run(RunPayload {
            target: target.map_or_else(String::new, |path| {
                path.to_string_lossy().replace('\\', "/")
            }),
            program_result: None,
            stdout: String::new(),
            stderr: String::new(),
            audit_trace: Vec::new(),
            trap: None,
        }),
        _ => Payload::Empty(EmptyPayload {}),
    };
    CommandEnvelope {
        schema_version: SCHEMA_VERSION,
        command: invocation.command.clone(),
        result,
        diagnostics,
        payload,
    }
}

fn parse_invocation(
    arguments: impl IntoIterator<Item = OsString>,
    current_dir: &Path,
) -> Invocation {
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    let mut output_format = OutputFormat::Human;
    let mut workspace = current_dir.to_path_buf();
    let mut seen_format = false;
    let mut seen_workspace = false;
    let mut cursor = 0;
    while let Some(argument) = arguments.get(cursor) {
        if argument == OsStr::new("--format") {
            if seen_format {
                return invalid_invocation(
                    output_format,
                    workspace,
                    "invalid",
                    "`--format` may occur only once before the command",
                    None,
                );
            }
            let Some(value) = arguments.get(cursor.saturating_add(1)) else {
                return invalid_invocation(
                    output_format,
                    workspace,
                    "invalid",
                    "`--format` requires `human` or `json`",
                    None,
                );
            };
            output_format = match value.to_str() {
                Some("human") => OutputFormat::Human,
                Some("json") => OutputFormat::Json,
                _ => {
                    return invalid_invocation(
                        output_format,
                        workspace,
                        "invalid",
                        "`--format` requires `human` or `json`",
                        None,
                    );
                }
            };
            seen_format = true;
            cursor = cursor.saturating_add(2);
        } else if argument == OsStr::new("--workspace") {
            if seen_workspace {
                return invalid_invocation(
                    output_format,
                    workspace,
                    "invalid",
                    "`--workspace` may occur only once before the command",
                    None,
                );
            }
            let Some(value) = arguments.get(cursor.saturating_add(1)) else {
                return invalid_invocation(
                    output_format,
                    workspace,
                    "invalid",
                    "`--workspace` requires a path",
                    None,
                );
            };
            workspace = PathBuf::from(value);
            seen_workspace = true;
            cursor = cursor.saturating_add(2);
        } else {
            break;
        }
    }
    let Some(command_arg) = arguments.get(cursor) else {
        return invalid_invocation(
            output_format,
            workspace,
            "invalid",
            "a command is required",
            None,
        );
    };
    let Some(command_name) = command_arg.to_str() else {
        return invalid_invocation(
            output_format,
            workspace,
            "invalid",
            "command names must be valid UTF-8",
            None,
        );
    };
    let remaining = arguments
        .get(cursor.saturating_add(1)..)
        .unwrap_or_default();
    match command_name {
        "project" => parse_project_command(output_format, workspace, remaining),
        "fmt" => parse_fmt_command(output_format, workspace, remaining),
        "check" => parse_check_command(output_format, workspace, remaining),
        "run" => parse_run_command(output_format, workspace, remaining),
        "test" => parse_unavailable_target_command(
            output_format,
            workspace,
            command_name,
            remaining,
        ),
        "lint" | "build" | "query" | "edit" | "mcp" => Invocation {
            output_format,
            workspace,
            command: command_name.to_owned(),
            action: Action::Unavailable {
                arguments: remaining.to_vec(),
            },
        },
        _ => invalid_invocation(
            output_format,
            workspace,
            "invalid",
            &format!("unknown command `{command_name}`"),
            None,
        ),
    }
}

fn parse_unavailable_target_command(
    output_format: OutputFormat,
    workspace: PathBuf,
    command_name: &str,
    arguments: &[OsString],
) -> Invocation {
    let (minimum, maximum, usage) = match command_name {
        "test" => (0, 1, "`test` accepts at most one test name"),
        _ => unreachable!("only deferred target commands use this parser"),
    };
    if arguments.len() < minimum
        || arguments.len() > maximum
        || arguments
            .iter()
            .any(|argument| !is_confined_workspace_argument(argument))
    {
        return invalid_invocation(output_format, workspace, command_name, usage, None);
    }
    Invocation {
        output_format,
        workspace,
        command: command_name.to_owned(),
        action: Action::Unavailable {
            arguments: arguments.to_vec(),
        },
    }
}

fn parse_check_command(
    output_format: OutputFormat,
    workspace: PathBuf,
    arguments: &[OsString],
) -> Invocation {
    if arguments.len() > 1
        || arguments
            .iter()
            .any(|argument| !is_confined_workspace_argument(argument))
    {
        return invalid_invocation(
            output_format,
            workspace,
            "check",
            "`check` accepts at most one project-relative target root",
            None,
        );
    }
    Invocation {
        output_format,
        workspace,
        command: "check".to_owned(),
        action: Action::Check {
            target: arguments.first().map(PathBuf::from),
        },
    }
}

fn parse_run_command(
    output_format: OutputFormat,
    workspace: PathBuf,
    arguments: &[OsString],
) -> Invocation {
    let Some(target) = arguments.first() else {
        return invalid_invocation(
            output_format,
            workspace,
            "run",
            "`run` requires exactly one project-relative binary target root",
            None,
        );
    };
    if arguments.len() != 1 || !is_confined_workspace_argument(target) {
        return invalid_invocation(
            output_format,
            workspace,
            "run",
            "`run` requires exactly one project-relative binary target root",
            None,
        );
    }
    Invocation {
        output_format,
        workspace,
        command: "run".to_owned(),
        action: Action::Run {
            target: PathBuf::from(target),
        },
    }
}

fn is_confined_workspace_argument(argument: &OsStr) -> bool {
    let Some(value) = argument.to_str() else {
        return false;
    };
    if value.is_empty() || value.starts_with('-') {
        return false;
    }
    let path = Path::new(value);
    if path.is_absolute() {
        return false;
    }
    let mut has_name = false;
    for component in path.components() {
        match component {
            Component::Normal(_) => has_name = true,
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return false;
            }
        }
    }
    has_name
}

fn parse_project_command(
    output_format: OutputFormat,
    workspace: PathBuf,
    arguments: &[OsString],
) -> Invocation {
    let Some(subcommand) = arguments.first().and_then(|value| value.to_str()) else {
        return invalid_invocation(
            output_format,
            workspace,
            "invalid",
            "`project` requires a subcommand",
            None,
        );
    };
    match subcommand {
        "init" => {
            let destination = match arguments.get(1) {
                Some(value)
                    if arguments.len() == 2
                        && !value.to_string_lossy().starts_with('-') =>
                {
                    Some(PathBuf::from(value))
                }
                None if arguments.len() == 1 => None,
                _ => {
                    return invalid_invocation(
                        output_format,
                        workspace,
                        "init",
                        "`project init` accepts at most one destination and no options",
                        None,
                    );
                }
            };
            Invocation {
                output_format,
                workspace,
                command: "init".to_owned(),
                action: Action::Init(destination),
            }
        }
        "inspect" | "add" | "remove" | "sync" => Invocation {
            output_format,
            workspace,
            command: subcommand.to_owned(),
            action: Action::Unavailable {
                arguments: arguments.get(1..).unwrap_or_default().to_vec(),
            },
        },
        _ => invalid_invocation(
            output_format,
            workspace,
            "invalid",
            &format!("unknown project subcommand `{subcommand}`"),
            None,
        ),
    }
}

fn parse_fmt_command(
    output_format: OutputFormat,
    workspace: PathBuf,
    arguments: &[OsString],
) -> Invocation {
    let Some(path) = arguments.first() else {
        return invalid_invocation(
            output_format,
            workspace,
            "fmt",
            "`fmt` requires one path",
            Some(PathBuf::new()),
        );
    };
    let path = PathBuf::from(path);
    if path.to_str().is_none() {
        return invalid_invocation(
            output_format,
            workspace,
            "fmt",
            "formatter paths must be valid UTF-8",
            Some(path),
        );
    }
    let write = match arguments.get(1) {
        None if arguments.len() == 1 => false,
        Some(option) if option == OsStr::new("--write") && arguments.len() == 2 => true,
        _ => {
            return invalid_invocation(
                output_format,
                workspace,
                "fmt",
                "`fmt` accepts only one path and the optional `--write` flag",
                Some(path),
            );
        }
    };
    Invocation {
        output_format,
        workspace,
        command: "fmt".to_owned(),
        action: Action::Fmt { path, write },
    }
}

fn invalid_invocation(
    output_format: OutputFormat,
    workspace: PathBuf,
    command: &str,
    message: &str,
    path: Option<PathBuf>,
) -> Invocation {
    Invocation {
        output_format,
        workspace,
        command: command.to_owned(),
        action: Action::Invalid {
            message: message.to_owned(),
            path,
        },
    }
}

fn init_error_envelope<E: Write>(
    invocation: &Invocation,
    error: InitError,
    stderr: &mut E,
) -> CommandEnvelope {
    match error {
        InitError::InvalidInput(message) => {
            if invocation.output_format == OutputFormat::Human {
                let _ = writeln!(stderr, "invalid input: {message}");
            }
            CommandEnvelope {
                schema_version: SCHEMA_VERSION,
                command: invocation.command.clone(),
                result: CommandResult::InvalidInput,
                diagnostics: Vec::new(),
                payload: Payload::Init(InitPayload {
                    workspace: invocation.workspace.display().to_string(),
                    created: Vec::new(),
                }),
            }
        }
        InitError::OperationalFailure(message) => {
            operational_init_envelope(invocation, &invocation.workspace, message)
        }
    }
}

fn operational_init_envelope(
    invocation: &Invocation,
    workspace: &Path,
    message: String,
) -> CommandEnvelope {
    CommandEnvelope {
        schema_version: SCHEMA_VERSION,
        command: invocation.command.clone(),
        result: CommandResult::OperationalFailure,
        diagnostics: render_unlocated_diagnostics(&[operational_diagnostic(message)]),
        payload: Payload::Init(InitPayload {
            workspace: workspace.display().to_string(),
            created: Vec::new(),
        }),
    }
}

fn fmt_error_envelope<E: Write>(
    invocation: &Invocation,
    path: &Path,
    error: FormatPlanError,
    stderr: &mut E,
) -> CommandEnvelope {
    let (result, diagnostics) = match error {
        FormatPlanError::InvalidPath(message)
        | FormatPlanError::UnsupportedExtension(message)
        | FormatPlanError::InvalidUtf8(message) => {
            if invocation.output_format == OutputFormat::Human {
                let _ = writeln!(stderr, "invalid input: {message}");
            }
            (CommandResult::InvalidInput, Vec::new())
        }
        error => {
            let message = error.to_string();
            (
                CommandResult::OperationalFailure,
                render_unlocated_diagnostics(&[operational_diagnostic(message)]),
            )
        }
    };
    CommandEnvelope {
        schema_version: SCHEMA_VERSION,
        command: invocation.command.clone(),
        result,
        diagnostics,
        payload: Payload::Fmt(FmtPayload {
            path: path.to_string_lossy().replace('\\', "/"),
            changed: false,
            written: false,
            text: None,
        }),
    }
}

fn invalid_envelope(invocation: &Invocation, path: Option<PathBuf>) -> CommandEnvelope {
    let payload = match invocation.command.as_str() {
        "init" => Payload::Init(InitPayload {
            workspace: invocation.workspace.display().to_string(),
            created: Vec::new(),
        }),
        "fmt" => Payload::Fmt(FmtPayload {
            path: path.map_or_else(String::new, |path| {
                path.to_string_lossy().replace('\\', "/")
            }),
            changed: false,
            written: false,
            text: None,
        }),
        "check" => Payload::Check(CheckPayload { accepted: false }),
        "run" => Payload::Run(RunPayload {
            target: String::new(),
            program_result: None,
            stdout: String::new(),
            stderr: String::new(),
            audit_trace: Vec::new(),
            trap: None,
        }),
        "test" => Payload::Test(TestPayload {
            selected: 0,
            passed: 0,
            failed: 0,
            tests: Vec::new(),
        }),
        _ => Payload::Empty(EmptyPayload {}),
    };
    CommandEnvelope {
        schema_version: SCHEMA_VERSION,
        command: invocation.command.clone(),
        result: CommandResult::InvalidInput,
        diagnostics: Vec::new(),
        payload,
    }
}

fn unavailable_envelope(
    invocation: &Invocation,
    arguments: Vec<OsString>,
    diagnostic: Diagnostic,
) -> CommandEnvelope {
    let arg = arguments
        .first()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_owned();
    let payload = match invocation.command.as_str() {
        "check" => Payload::Check(CheckPayload { accepted: false }),
        "run" => Payload::Run(RunPayload {
            target: arg,
            program_result: None,
            stdout: String::new(),
            stderr: String::new(),
            audit_trace: Vec::new(),
            trap: None,
        }),
        "test" => Payload::Test(TestPayload {
            selected: 0,
            passed: 0,
            failed: 0,
            tests: Vec::new(),
        }),
        _ => Payload::Empty(EmptyPayload {}),
    };
    CommandEnvelope {
        schema_version: SCHEMA_VERSION,
        command: invocation.command.clone(),
        result: CommandResult::Unavailable,
        diagnostics: render_unlocated_diagnostics(&[diagnostic]),
        payload,
    }
}

fn operational_diagnostic(message: String) -> Diagnostic {
    Diagnostic::new(
        DiagnosticCode::ProjectIoError,
        ByteSpan::empty_at(0),
        message,
    )
}

fn render_unlocated_diagnostics(diagnostics: &[Diagnostic]) -> Vec<DiagnosticDocument> {
    let index = LineIndex::new("");
    diagnostics
        .iter()
        .filter_map(|diagnostic| DiagnosticDocument::render(diagnostic, &index).ok())
        .collect()
}

fn render_plan_diagnostics(
    plan: &FormatPlan,
) -> Result<Vec<DiagnosticDocument>, String> {
    let index = LineIndex::new(plan.original_text());
    plan.diagnostics()
        .iter()
        .map(|diagnostic| {
            DiagnosticDocument::render(diagnostic, &index)
                .map_err(|error| error.to_string())
        })
        .collect()
}

fn render_diagnostics<E: Write>(
    diagnostics: &[DiagnosticDocument],
    stderr: &mut E,
) -> Result<(), std::io::Error> {
    for diagnostic in diagnostics {
        writeln!(stderr, "{}: {}", diagnostic.code, diagnostic.message)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used
    )]

    use super::{
        Action, CommandResult, Invocation, OutputFormat, classify_workspace_load_error,
        is_confined_workspace_argument, workspace_load_error_envelope,
    };
    use std::ffi::{OsStr, OsString};
    use std::path::PathBuf;
    use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
    use vibra_workspace::WorkspaceError;

    #[test]
    fn deferred_arguments_are_confined_workspace_paths() {
        for invalid in ["", "-all", "../outside", "/outside"] {
            assert!(
                !is_confined_workspace_argument(OsStr::new(invalid)),
                "{invalid:?}"
            );
        }
        assert!(is_confined_workspace_argument(OsStr::new("app")));
        assert!(is_confined_workspace_argument(OsStr::new("app.main.case")));
        assert!(is_confined_workspace_argument(OsStr::new("./app")));
    }

    #[test]
    fn workspace_io_diagnostics_are_operational_failures() {
        for code in [
            DiagnosticCode::ProjectIoError,
            DiagnosticCode::ModuleIoError,
        ] {
            let error = WorkspaceError::new(
                "workspace I/O failed",
                vec![Diagnostic::new(
                    code,
                    ByteSpan::empty_at(0),
                    "workspace I/O failed",
                )],
            );
            assert_eq!(
                classify_workspace_load_error(&error),
                CommandResult::OperationalFailure
            );
        }
    }

    #[test]
    fn workspace_load_error_renders_nonzero_spans_from_captured_source_text() {
        let error = WorkspaceError::new(
            "project marker could not be loaded",
            vec![
                Diagnostic::new(
                    DiagnosticCode::ProjectIoError,
                    ByteSpan::new(6, 12),
                    "project marker could not be loaded",
                )
                .with_source_id("project.vibon"),
            ],
        )
        .with_source_text("project.vibon", "first\nsecond\n");
        let invocation = Invocation {
            output_format: OutputFormat::Json,
            workspace: PathBuf::from("."),
            command: "check".to_owned(),
            action: Action::Check { target: None },
        };
        let mut stderr = Vec::new();

        let envelope = workspace_load_error_envelope(&invocation, error, &mut stderr);

        assert_eq!(envelope.result, CommandResult::OperationalFailure);
        assert_eq!(envelope.diagnostics.len(), 1);
        assert_eq!(
            envelope.diagnostics[0].primary_span.source_id.as_deref(),
            Some("project.vibon")
        );
        assert_eq!(envelope.diagnostics[0].primary_span.start_position.line, 2);
        assert_eq!(
            envelope.diagnostics[0].primary_span.start_position.column,
            1
        );
        assert!(stderr.is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn deferred_arguments_reject_invalid_unicode() {
        use std::os::windows::ffi::OsStringExt;

        let invalid_unicode = OsString::from_wide(&[0xD800]);
        assert!(!is_confined_workspace_argument(&invalid_unicode));
    }

    #[cfg(unix)]
    #[test]
    fn deferred_arguments_reject_invalid_unicode() {
        use std::os::unix::ffi::OsStringExt;

        let invalid_unicode = OsString::from_vec(vec![0xFF]);
        assert!(!is_confined_workspace_argument(&invalid_unicode));
    }
}

//! Vibra compiler CLI. See [DRAFT.md](../DRAFT.md).

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use serde_yaml::{Mapping, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use vibra::lower::{RuntimeValue, TypeRef};
use vibra::{
    docs, execute, load, lower, lsp, mcp, package, plugin, project, runtime, test_runner, tooling,
};

#[derive(Parser)]
#[command(name = "vibra", version, about = "Vibra language toolchain")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start the Model Context Protocol server over stdin/stdout.
    Mcp {
        /// Workspace root visible to MCP tools.
        #[arg(long, default_value = ".")]
        workspace: PathBuf,
        /// Permit tools that rewrite source or create build artifacts.
        #[arg(long)]
        allow_write: bool,
        /// Permit the test tool to execute untrusted project code.
        #[arg(long)]
        allow_test: bool,
    },
    /// Start the Language Server Protocol server over stdin/stdout.
    Lsp {
        /// Enable a conditional-compilation flag (repeatable).
        #[arg(long = "flag", value_parser = parse_compilation_flag)]
        flag: Vec<String>,
    },
    /// Create a new Vibra project.
    Init {
        /// Project directory name to create.
        name: PathBuf,
        /// Project template to scaffold.
        #[arg(long, value_enum, default_value_t = TemplateArg::Bin)]
        template: TemplateArg,
        /// Output format.
        #[arg(long, value_enum, default_value_t = StatusFormatArg::Json)]
        format: StatusFormatArg,
    },
    /// Clone/fetch pinned git dependencies into dep/.
    Sync {
        /// Project directory or project.vibra path.
        path: Option<PathBuf>,
        /// Output format.
        #[arg(long, value_enum, default_value_t = StatusFormatArg::Json)]
        format: StatusFormatArg,
    },
    /// Build a deterministic executable `.vapp` archive.
    Build {
        /// Project directory or project.vibra path.
        path: Option<PathBuf>,
        /// Bin target name (required when the project has multiple bins).
        #[arg(long = "bin")]
        bin: Option<String>,
        /// Output `.vapp` path.
        #[arg(short, long)]
        output: PathBuf,
        /// Output format.
        #[arg(long, value_enum, default_value_t = StatusFormatArg::Json)]
        format: StatusFormatArg,
        /// Enable a conditional-compilation flag (repeatable).
        #[arg(long = "flag", value_parser = parse_compilation_flag)]
        flag: Vec<String>,
    },
    /// Inspect or verify a `.vapp` archive.
    Package {
        #[command(subcommand)]
        command: PackageCommand,
    },
    /// Validate a Vibra project manifest, dependencies, targets, and imports.
    Check {
        /// Project directory or project.vibra path.
        path: Option<PathBuf>,
        /// Output format.
        #[arg(long, value_enum, default_value_t = StatusFormatArg::Json)]
        format: StatusFormatArg,
        /// Enable a conditional-compilation flag (repeatable).
        #[arg(long = "flag", value_parser = parse_compilation_flag)]
        flag: Vec<String>,
    },
    /// Validate and instantiate a typed local runtime plugin.
    Plugin {
        /// Project directory containing the declared plugin interface.
        project: PathBuf,
        /// Declared interface name under `plugin-interfaces`.
        #[arg(long = "interface")]
        interface: String,
        /// Arbitrary local wasm plugin path.
        #[arg(long = "path")]
        path: PathBuf,
        /// Explicit plugin-load authority for this file or directory.
        #[arg(long = "allow-plugin-load")]
        allow_plugin_load: Vec<PathBuf>,
    },
    /// Show `=doc` documentation for modules and symbols.
    Docs {
        /// Entry module, project directory, or project.vibra. Defaults to the current project.
        path: Option<PathBuf>,
        /// Qualified symbol such as `main` or `io.println`. Omit to browse documented symbols.
        symbol: Option<String>,
        /// Project target to inspect when the project has multiple targets.
        #[arg(long)]
        target: Option<String>,
        /// Documentation output format.
        #[arg(long, value_enum, default_value_t = DocsFormatArg::Plain)]
        format: DocsFormatArg,
        /// Enable a conditional-compilation flag (repeatable).
        #[arg(long = "flag", value_parser = parse_compilation_flag)]
        flag: Vec<String>,
    },
    /// Check or rewrite canonical Vibra/YAML formatting.
    Fmt {
        /// Files, directories, or globs to format. Defaults to the current directory.
        path: Vec<PathBuf>,
        /// Rewrite changed files in place.
        #[arg(long)]
        write: bool,
        /// Structured output format.
        #[arg(long, visible_alias = "output", value_enum, default_value_t = ToolOutputArg::Json)]
        format: ToolOutputArg,
    },
    /// Emit Vibra diagnostics for source files.
    Lint {
        /// Files, directories, or globs to lint. Defaults to the current directory.
        path: Vec<PathBuf>,
        /// Structured output format.
        #[arg(long, value_enum, default_value_t = LintFormatArg::Json)]
        format: LintFormatArg,
        /// Diagnostic category to include. Repeat to include multiple categories.
        #[arg(long = "category", value_enum)]
        category: Vec<LintCategoryArg>,
        /// Minimum diagnostic severity to include.
        #[arg(long, value_enum)]
        severity: Option<LintSeverityArg>,
        /// Treat warnings as CI failures.
        #[arg(long = "deny-warnings")]
        deny_warnings: bool,
    },
    /// Parse, compile (MVP), and run a `.vibra` module via embedded Wasmer.
    Run {
        /// Entry module path (e.g. examples/hello.vibra).
        path: PathBuf,
        /// Preopen a host directory for WASI mapping; does not approve host ABI access.
        #[arg(long = "preopen")]
        preopen: Vec<PathBuf>,
        /// Allow filesystem reads under this host path (repeatable).
        #[arg(long = "allow-read")]
        allow_read: Vec<PathBuf>,
        /// Allow filesystem writes under this host path (repeatable).
        #[arg(long = "allow-write")]
        allow_write: Vec<PathBuf>,
        /// Allow reading from stdin.
        #[arg(long = "allow-stdin")]
        allow_stdin: bool,
        /// Allow reading the named environment variable (repeatable).
        #[arg(long = "allow-env")]
        allow_env: Vec<String>,
        /// Allow writing the named environment variable (repeatable).
        #[arg(long = "allow-env-write")]
        allow_env_write: Vec<String>,
        /// Allow outbound network access to HOST[:PORT] (repeatable).
        #[arg(long = "allow-net")]
        allow_net: Vec<String>,
        /// Allow listening on HOST[:PORT] (repeatable).
        #[arg(long = "allow-net-listen")]
        allow_net_listen: Vec<String>,
        /// Allow running the named command (repeatable).
        #[arg(long = "allow-run")]
        allow_run: Vec<String>,
        /// Allow clock/time access.
        #[arg(long = "allow-clock")]
        allow_clock: bool,
        /// Allow randomness access.
        #[arg(long = "allow-random")]
        allow_random: bool,
        /// Allow system information access.
        #[arg(long = "allow-sys-info")]
        allow_system_info: bool,
        /// Allow every modeled non-filesystem permission and filesystem access under the current directory.
        #[arg(long = "allow-all")]
        allow_all: bool,
        /// Maximum number of concurrently open file handles (0 = unlimited).
        #[arg(long = "max-open-files", default_value_t = 1024)]
        max_open_files: usize,
        /// Enable a conditional-compilation flag (repeatable).
        #[arg(long = "flag", value_parser = parse_compilation_flag)]
        flag: Vec<String>,
    },
    /// Print the statically referenced, typed host effect surface as JSON.
    Effects {
        /// Entry module path.
        path: PathBuf,
        /// Output format.
        #[arg(long, value_enum, default_value_t = StructuredFormatArg::Json)]
        format: StructuredFormatArg,
        /// Enable a conditional-compilation flag (repeatable).
        #[arg(long = "flag", value_parser = parse_compilation_flag)]
        flag: Vec<String>,
    },
    /// Expand compile-time macros and print canonical Vibra source.
    Expand {
        /// Entry module to expand.
        path: PathBuf,
        /// Output format.
        #[arg(long, value_enum, default_value_t = StructuredFormatArg::Json)]
        format: StructuredFormatArg,
        /// Enable a conditional-compilation flag (repeatable).
        #[arg(long = "flag", value_parser = parse_compilation_flag)]
        flag: Vec<String>,
    },
    /// Evaluate one inline Vibra expression for tooling workflows.
    Exec {
        /// Inline Vibra expression encoded as YAML.
        expr: String,
        /// Bind a string local as `name=value` (repeatable).
        #[arg(long = "arg")]
        arg: Vec<String>,
        /// Bind a string local to file contents as `name=path` (repeatable).
        #[arg(long = "arg-file")]
        arg_file: Vec<String>,
        /// Add an import to the exec context as `alias=path` (repeatable).
        #[arg(long = "import")]
        import: Vec<String>,
        /// Output format.
        #[arg(long, value_enum, default_value_t = ExecFormatArg::Json)]
        format: ExecFormatArg,
        /// Preopen a host directory for WASI mapping; does not approve host ABI access.
        #[arg(long = "preopen")]
        preopen: Vec<PathBuf>,
        /// Allow filesystem reads under this host path (repeatable).
        #[arg(long = "allow-read")]
        allow_read: Vec<PathBuf>,
        /// Allow filesystem writes under this host path (repeatable).
        #[arg(long = "allow-write")]
        allow_write: Vec<PathBuf>,
        /// Allow reading from stdin.
        #[arg(long = "allow-stdin")]
        allow_stdin: bool,
        /// Allow reading the named environment variable (repeatable).
        #[arg(long = "allow-env")]
        allow_env: Vec<String>,
        /// Allow writing the named environment variable (repeatable).
        #[arg(long = "allow-env-write")]
        allow_env_write: Vec<String>,
        /// Allow outbound network access to HOST[:PORT] (repeatable).
        #[arg(long = "allow-net")]
        allow_net: Vec<String>,
        /// Allow listening on HOST[:PORT] (repeatable).
        #[arg(long = "allow-net-listen")]
        allow_net_listen: Vec<String>,
        /// Allow running the named command (repeatable).
        #[arg(long = "allow-run")]
        allow_run: Vec<String>,
        /// Allow clock/time access.
        #[arg(long = "allow-clock")]
        allow_clock: bool,
        /// Allow randomness access.
        #[arg(long = "allow-random")]
        allow_random: bool,
        /// Allow system information access.
        #[arg(long = "allow-sys-info")]
        allow_system_info: bool,
        /// Allow every modeled non-filesystem permission and filesystem access under the current directory.
        #[arg(long = "allow-all")]
        allow_all: bool,
        /// Maximum number of concurrently open file handles (0 = unlimited).
        #[arg(long = "max-open-files", default_value_t = 1024)]
        max_open_files: usize,
    },
    /// Discover and run `$test` declarations.
    Test {
        /// Project directory, tests directory, or single `.vibra` test file.
        path: Option<PathBuf>,
        /// Run only tests whose name or path contains this substring.
        #[arg(long)]
        filter: Option<String>,
        /// Select tests from these profiles (repeatable; defaults to `core`).
        #[arg(long = "profile")]
        profiles: Vec<String>,
        /// Require every listed tag (repeatable).
        #[arg(long = "tag")]
        tags: Vec<String>,
        /// Treat selected skipped tests as a failing command.
        #[arg(long = "deny-skips")]
        deny_skips: bool,
        /// Fail tests that produce compiler warnings.
        #[arg(long = "deny-warnings")]
        deny_warnings: bool,
        /// Allow an isolated `workspace: temp` test to access its temporary cwd.
        #[arg(long = "allow-test-workspace", value_enum)]
        allow_test_workspace: Option<TestWorkspaceAccessArg>,
        /// Number of test worker processes.
        #[arg(long)]
        jobs: Option<usize>,
        /// Per-test timeout in milliseconds.
        #[arg(long = "timeout-ms", default_value_t = 30_000)]
        timeout_ms: u64,
        /// Stop scheduling tests after the first failure.
        #[arg(long = "fail-fast")]
        fail_fast: bool,
        /// Measure selected test declarations without changing normal test mode.
        #[arg(long)]
        benchmark: bool,
        /// Number of measured executions per selected benchmark.
        #[arg(long = "benchmark-iterations", default_value_t = 10)]
        benchmark_iterations: usize,
        /// Number of unmeasured warm-up executions per selected benchmark.
        #[arg(long = "benchmark-warmup", default_value_t = 1)]
        benchmark_warmup: usize,
        /// Structured report format.
        #[arg(long, visible_alias = "report", value_enum, default_value_t = ReportArg::Json)]
        format: ReportArg,
        /// Write structured report to this path.
        #[arg(long = "report-file")]
        report_file: Option<PathBuf>,
        /// Preopen a host directory for WASI mapping; does not approve host ABI access.
        #[arg(long = "preopen")]
        preopen: Vec<PathBuf>,
        /// Allow filesystem reads under this host path (repeatable).
        #[arg(long = "allow-read")]
        allow_read: Vec<PathBuf>,
        /// Allow filesystem writes under this host path (repeatable).
        #[arg(long = "allow-write")]
        allow_write: Vec<PathBuf>,
        /// Allow reading from stdin.
        #[arg(long = "allow-stdin")]
        allow_stdin: bool,
        /// Allow reading the named environment variable (repeatable).
        #[arg(long = "allow-env")]
        allow_env: Vec<String>,
        /// Allow writing the named environment variable (repeatable).
        #[arg(long = "allow-env-write")]
        allow_env_write: Vec<String>,
        /// Allow outbound network access to HOST[:PORT] (repeatable).
        #[arg(long = "allow-net")]
        allow_net: Vec<String>,
        /// Allow listening on HOST[:PORT] (repeatable).
        #[arg(long = "allow-net-listen")]
        allow_net_listen: Vec<String>,
        /// Allow running the named command (repeatable).
        #[arg(long = "allow-run")]
        allow_run: Vec<String>,
        /// Allow clock/time access.
        #[arg(long = "allow-clock")]
        allow_clock: bool,
        /// Allow randomness access.
        #[arg(long = "allow-random")]
        allow_random: bool,
        /// Allow system information access.
        #[arg(long = "allow-sys-info")]
        allow_system_info: bool,
        /// Allow every modeled non-filesystem permission and filesystem access under the current directory.
        #[arg(long = "allow-all")]
        allow_all: bool,
        /// Maximum number of concurrently open file handles (0 = unlimited).
        #[arg(long = "max-open-files", default_value_t = 1024)]
        max_open_files: usize,
    },
    #[command(name = "__run-test", hide = true)]
    RunTest {
        path: PathBuf,
        name: String,
        /// Private parent/worker protocol result path.
        #[arg(long = "result-file")]
        result_file: PathBuf,
        #[arg(long = "preopen")]
        preopen: Vec<PathBuf>,
        #[arg(long = "allow-read")]
        allow_read: Vec<PathBuf>,
        #[arg(long = "allow-write")]
        allow_write: Vec<PathBuf>,
        #[arg(long = "allow-stdin")]
        allow_stdin: bool,
        #[arg(long = "allow-env")]
        allow_env: Vec<String>,
        #[arg(long = "allow-env-write")]
        allow_env_write: Vec<String>,
        #[arg(long = "allow-net")]
        allow_net: Vec<String>,
        #[arg(long = "allow-net-listen")]
        allow_net_listen: Vec<String>,
        #[arg(long = "allow-run")]
        allow_run: Vec<String>,
        #[arg(long = "allow-clock")]
        allow_clock: bool,
        #[arg(long = "allow-random")]
        allow_random: bool,
        #[arg(long = "allow-sys-info")]
        allow_system_info: bool,
        /// Private deterministic test-worker random state.
        #[arg(long = "injected-random-state", hide = true)]
        injected_random_state: Option<u64>,
        /// Private deterministic test-worker wall clock.
        #[arg(long = "injected-clock-unix-millis", hide = true)]
        injected_clock_unix_millis: Option<u64>,
        /// Private deterministic test-worker monotonic clock.
        #[arg(long = "injected-clock-monotonic-millis", hide = true)]
        injected_clock_monotonic_millis: Option<u64>,
        #[arg(long = "allow-all")]
        allow_all: bool,
        #[arg(long = "max-open-files", default_value_t = 1024)]
        max_open_files: usize,
    },
}

#[derive(Subcommand)]
enum PackageCommand {
    /// Print canonical package metadata.
    Inspect {
        path: PathBuf,
        #[arg(long, value_enum, default_value_t = StructuredFormatArg::Json)]
        format: StructuredFormatArg,
    },
    /// Verify package structure and content hashes.
    Verify {
        path: PathBuf,
        #[arg(long, value_enum, default_value_t = StatusFormatArg::Json)]
        format: StatusFormatArg,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum TemplateArg {
    Bin,
    Lib,
    Workspace,
}

#[derive(Clone, Copy, ValueEnum)]
enum ReportArg {
    Human,
    Json,
}

#[derive(Clone, Copy, ValueEnum)]
enum StructuredFormatArg {
    Json,
}

#[derive(Clone, Copy, ValueEnum)]
enum StatusFormatArg {
    Json,
    Human,
}

#[derive(Clone, Copy, ValueEnum)]
enum TestWorkspaceAccessArg {
    Read,
    Write,
    ReadWrite,
}

#[derive(Clone, Copy, ValueEnum)]
enum ToolOutputArg {
    Json,
}

#[derive(Clone, Copy, ValueEnum)]
enum LintFormatArg {
    Json,
    Sarif,
}

#[derive(Clone, Copy, ValueEnum)]
enum LintCategoryArg {
    Style,
    Syntax,
    Compile,
}

#[derive(Clone, Copy, ValueEnum)]
enum LintSeverityArg {
    Error,
    Warning,
    Info,
    Hint,
}

#[derive(Clone, Copy, ValueEnum)]
enum ExecFormatArg {
    Raw,
    Json,
}

#[derive(Clone, Copy, ValueEnum)]
enum DocsFormatArg {
    Plain,
    Markdown,
    Json,
}

impl From<ReportArg> for test_runner::ReportFormat {
    fn from(value: ReportArg) -> Self {
        match value {
            ReportArg::Human => test_runner::ReportFormat::Human,
            ReportArg::Json => test_runner::ReportFormat::Json,
        }
    }
}

impl From<TestWorkspaceAccessArg> for test_runner::TestWorkspaceAccess {
    fn from(value: TestWorkspaceAccessArg) -> Self {
        match value {
            TestWorkspaceAccessArg::Read => test_runner::TestWorkspaceAccess::Read,
            TestWorkspaceAccessArg::Write => test_runner::TestWorkspaceAccess::Write,
            TestWorkspaceAccessArg::ReadWrite => test_runner::TestWorkspaceAccess::ReadWrite,
        }
    }
}

impl From<TemplateArg> for project::InitTemplate {
    fn from(value: TemplateArg) -> Self {
        match value {
            TemplateArg::Bin => project::InitTemplate::Bin,
            TemplateArg::Lib => project::InitTemplate::Lib,
            TemplateArg::Workspace => project::InitTemplate::Workspace,
        }
    }
}

impl From<ToolOutputArg> for tooling::ToolOutputFormat {
    fn from(value: ToolOutputArg) -> Self {
        match value {
            ToolOutputArg::Json => tooling::ToolOutputFormat::Json,
        }
    }
}

impl From<LintFormatArg> for tooling::LintOutputFormat {
    fn from(value: LintFormatArg) -> Self {
        match value {
            LintFormatArg::Json => tooling::LintOutputFormat::Json,
            LintFormatArg::Sarif => tooling::LintOutputFormat::Sarif,
        }
    }
}

impl From<LintCategoryArg> for tooling::Category {
    fn from(value: LintCategoryArg) -> Self {
        match value {
            LintCategoryArg::Style => tooling::Category::Style,
            LintCategoryArg::Syntax => tooling::Category::Syntax,
            LintCategoryArg::Compile => tooling::Category::Compile,
        }
    }
}

impl From<LintSeverityArg> for tooling::Severity {
    fn from(value: LintSeverityArg) -> Self {
        match value {
            LintSeverityArg::Error => tooling::Severity::Error,
            LintSeverityArg::Warning => tooling::Severity::Warning,
            LintSeverityArg::Info => tooling::Severity::Info,
            LintSeverityArg::Hint => tooling::Severity::Hint,
        }
    }
}

fn parse_compilation_flag(value: &str) -> std::result::Result<String, String> {
    if load::is_compilation_flag(value) {
        Ok(value.to_string())
    } else {
        Err(format!(
            "E-FLAG-001: invalid compilation flag `{value}`; expected a kebab-case name"
        ))
    }
}

fn compilation_flags(flags: Vec<String>) -> Result<load::CompilationFlags> {
    load::CompilationFlags::try_new(flags)
}

fn main() -> Result<()> {
    std::thread::Builder::new()
        .name("vibra-cli".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(run_cli)?
        .join()
        .map_err(|_| anyhow::anyhow!("vibra CLI thread panicked"))?
}

fn run_cli() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Mcp {
            workspace,
            allow_write,
            allow_test,
        } => mcp::run_stdio(mcp::ServerOptions {
            workspace,
            allow_write,
            allow_test,
        })?,
        Command::Lsp { flag } => {
            lsp::run_stdio_with_flags(compilation_flags(flag)?)?;
        }
        Command::Init {
            name,
            template,
            format,
        } => {
            project::init_project(&name, template.into())?;
            print_status("created", &name, format)?;
        }
        Command::Sync { path, format } => {
            let path = path.unwrap_or_else(|| PathBuf::from("."));
            project::sync_project(&path)?;
            print_status("synced", &path, format)?;
        }
        Command::Build {
            path,
            bin,
            output,
            format,
            flag,
        } => {
            let path = path.unwrap_or_else(|| PathBuf::from("."));
            package::build_with_flags(&path, bin.as_deref(), &output, &compilation_flags(flag)?)?;
            print_status("built", &output, format)?;
        }
        Command::Package { command } => match command {
            PackageCommand::Inspect { path, format } => {
                let metadata: serde_json::Value = serde_json::from_str(&package::inspect(&path)?)?;
                print_structured(&metadata, format)?;
            }
            PackageCommand::Verify { path, format } => {
                package::verify(&path)?;
                print_status("verified", &path, format)?;
            }
        },
        Command::Check { path, format, flag } => {
            let path = path.unwrap_or_else(|| PathBuf::from("."));
            project::check_project_with_flags(&path, &compilation_flags(flag)?)?;
            print_status("checked", &path, format)?;
        }
        Command::Plugin {
            project,
            interface,
            path,
            allow_plugin_load,
        } => {
            print!(
                "{}",
                plugin::load(&project, &interface, &path, &allow_plugin_load)?
            );
        }
        Command::Docs {
            path,
            symbol,
            target,
            format,
            flag,
        } => {
            let path = path.unwrap_or_else(|| PathBuf::from("."));
            let entry = docs::resolve_entry(&path, target.as_deref())?;
            let available = docs::collect_with_flags(&entry, &compilation_flags(flag)?)?;
            print_docs(&available, symbol.as_deref(), format)?;
        }
        Command::Fmt {
            path,
            write,
            format,
        } => {
            let ok = tooling::run_fmt(tooling::FmtOptions {
                inputs: path,
                write,
                output: format.into(),
            })?;
            if !ok {
                std::process::exit(1);
            }
        }
        Command::Lint {
            path,
            format,
            category,
            severity,
            deny_warnings,
        } => {
            let ok = tooling::run_lint(tooling::LintOptions {
                inputs: path,
                format: format.into(),
                categories: category.into_iter().map(Into::into).collect(),
                severity: severity.map(Into::into),
                deny_warnings,
            })?;
            if !ok {
                std::process::exit(1);
            }
        }
        Command::Run {
            path,
            preopen,
            allow_read,
            allow_write,
            allow_stdin,
            allow_env,
            allow_env_write,
            allow_net,
            allow_net_listen,
            allow_run,
            allow_clock,
            allow_random,
            allow_system_info,
            allow_all,
            max_open_files,
            flag,
        } => {
            let config = run_config(
                preopen,
                allow_read,
                allow_write,
                allow_stdin,
                allow_env,
                allow_env_write,
                allow_net,
                allow_net_listen,
                allow_run,
                allow_clock,
                allow_random,
                allow_system_info,
                allow_all,
                max_open_files,
            );
            if path.extension().and_then(|value| value.to_str()) == Some("vapp") {
                if !flag.is_empty() {
                    bail!(
                        "E-FLAG-003: compilation flags cannot override a precompiled .vapp artifact"
                    );
                }
                package::run(&path, &config)?;
            } else {
                let program = load::load_program_with_flags(&path, &compilation_flags(flag)?)?;
                let lowered = lower::lower_program(&program)?;
                for warning in &lowered.warnings {
                    eprintln!("warning: {warning}");
                }
                execute::run_lowered(&lowered, &config)?;
            }
        }
        Command::Effects { path, format, flag } => {
            print_effects(&path, format, &compilation_flags(flag)?)?
        }
        Command::Expand { path, format, flag } => {
            let loaded = load::load_program_with_flags(&path, &compilation_flags(flag)?)?;
            let expanded = loaded
                .modules
                .get(&loaded.entry)
                .context("expanded entry module is missing")?;
            print_structured(expanded, format)?;
        }
        Command::Exec {
            expr,
            arg,
            arg_file,
            import,
            format,
            preopen,
            allow_read,
            allow_write,
            allow_stdin,
            allow_env,
            allow_env_write,
            allow_net,
            allow_net_listen,
            allow_run,
            allow_clock,
            allow_random,
            allow_system_info,
            allow_all,
            max_open_files,
        } => {
            let (bindings, local_types) = exec_bindings(arg, arg_file)?;
            let expr_value: Value = serde_yaml::from_str(&expr).context("parse exec expression")?;
            let root = exec_root(import)?;
            let cwd = std::env::current_dir().context("resolve current directory")?;
            let program = load::load_inline_program(&cwd, Value::Mapping(root))?;
            let lowered = lower::lower_exec_expr(&program, &expr_value, &local_types)?;
            for warning in &lowered.program.warnings {
                eprintln!("warning: {warning}");
            }
            let config = run_config(
                preopen,
                allow_read,
                allow_write,
                allow_stdin,
                allow_env,
                allow_env_write,
                allow_net,
                allow_net_listen,
                allow_run,
                allow_clock,
                allow_random,
                allow_system_info,
                allow_all,
                max_open_files,
            );
            let value = execute::eval_lowered_exec(&lowered, &bindings, &config)?;
            print_exec_value(value, format)?;
        }
        Command::Test {
            path,
            filter,
            profiles,
            tags,
            deny_skips,
            deny_warnings,
            allow_test_workspace,
            jobs,
            timeout_ms,
            fail_fast,
            benchmark,
            benchmark_iterations,
            benchmark_warmup,
            format,
            report_file,
            preopen,
            allow_read,
            allow_write,
            allow_stdin,
            allow_env,
            allow_env_write,
            allow_net,
            allow_net_listen,
            allow_run,
            allow_clock,
            allow_random,
            allow_system_info,
            allow_all,
            max_open_files,
        } => {
            let config = run_config(
                preopen,
                allow_read,
                allow_write,
                allow_stdin,
                allow_env,
                allow_env_write,
                allow_net,
                allow_net_listen,
                allow_run,
                allow_clock,
                allow_random,
                allow_system_info,
                allow_all,
                max_open_files,
            );
            let ok = test_runner::run_tests(test_runner::TestOptions {
                path: path.unwrap_or_else(|| PathBuf::from(".")),
                filter,
                profiles,
                tags,
                deny_skips,
                deny_warnings,
                allow_test_workspace: allow_test_workspace.map(Into::into),
                jobs: jobs
                    .or_else(|| std::thread::available_parallelism().ok().map(usize::from))
                    .unwrap_or(1),
                timeout: Duration::from_millis(timeout_ms),
                fail_fast,
                benchmark,
                benchmark_iterations,
                benchmark_warmup,
                report: format.into(),
                report_file,
                run_config: config,
            })?;
            if !ok {
                std::process::exit(1);
            }
        }
        Command::RunTest {
            path,
            name,
            result_file,
            preopen,
            allow_read,
            allow_write,
            allow_stdin,
            allow_env,
            allow_env_write,
            allow_net,
            allow_net_listen,
            allow_run,
            allow_clock,
            allow_random,
            allow_system_info,
            injected_random_state,
            injected_clock_unix_millis,
            injected_clock_monotonic_millis,
            allow_all,
            max_open_files,
        } => {
            let mut config = run_config(
                preopen,
                allow_read,
                allow_write,
                allow_stdin,
                allow_env,
                allow_env_write,
                allow_net,
                allow_net_listen,
                allow_run,
                allow_clock,
                allow_random,
                allow_system_info,
                allow_all,
                max_open_files,
            );
            if let Some(state) = injected_random_state {
                config.injected_random = Some(std::sync::Arc::new(std::sync::Mutex::new(
                    runtime::InjectedRandom::new(state),
                )));
            }
            match (injected_clock_unix_millis, injected_clock_monotonic_millis) {
                (Some(unix_millis), Some(monotonic_millis)) => {
                    config.injected_clock = Some(std::sync::Arc::new(std::sync::Mutex::new(
                        runtime::InjectedClock {
                            unix_millis,
                            monotonic_millis,
                        },
                    )));
                }
                (None, None) => {}
                _ => anyhow::bail!("injected test clock requires both wall and monotonic values"),
            }
            let outcome = test_runner::run_single_test(&path, &name, &config);
            let json = serde_json::to_string(&outcome)?;
            std::fs::write(&result_file, json)?;
        }
    }
    Ok(())
}

fn exec_bindings(
    args: Vec<String>,
    arg_files: Vec<String>,
) -> Result<(HashMap<String, RuntimeValue>, HashMap<String, TypeRef>)> {
    let mut values = HashMap::new();
    let mut types = HashMap::new();
    for raw in args {
        let (name, value) = split_name_value(&raw, "--arg")?;
        validate_exec_binding_name(name)?;
        insert_exec_binding(name, value.to_string(), &mut values, &mut types)?;
    }
    for raw in arg_files {
        let (name, path) = split_name_value(&raw, "--arg-file")?;
        validate_exec_binding_name(name)?;
        let value = std::fs::read_to_string(path)
            .with_context(|| format!("read --arg-file `{name}` from `{path}`"))?;
        insert_exec_binding(name, value, &mut values, &mut types)?;
    }
    Ok((values, types))
}

fn split_name_value<'a>(raw: &'a str, flag: &str) -> Result<(&'a str, &'a str)> {
    let (name, value) = raw
        .split_once('=')
        .with_context(|| format!("{flag} expects `name=value`"))?;
    Ok((name, value))
}

fn validate_exec_binding_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("exec binding name must not be empty");
    }
    if name.contains('$') || name.contains('.') {
        bail!(
            "exec binding name `{name}` must be referenced as `${name}` and cannot contain `$` or `.`"
        );
    }
    Ok(())
}

fn insert_exec_binding(
    name: &str,
    value: String,
    values: &mut HashMap<String, RuntimeValue>,
    types: &mut HashMap<String, TypeRef>,
) -> Result<()> {
    if values
        .insert(name.to_string(), RuntimeValue::Str(value))
        .is_some()
    {
        bail!("duplicate exec binding `{name}`");
    }
    types.insert(name.to_string(), TypeRef::Str);
    Ok(())
}

fn exec_root(imports: Vec<String>) -> Result<Mapping> {
    let mut root = Mapping::new();
    let code = project::locate_stdlib_source()?.join("code.vibra");
    insert_import(&mut root, "code", &path_str(&code))?;
    for raw in imports {
        let (alias, path) = split_name_value(&raw, "--import")?;
        validate_exec_import_alias(alias)?;
        insert_import(&mut root, alias, path)?;
    }
    Ok(root)
}

fn validate_exec_import_alias(alias: &str) -> Result<()> {
    if alias.is_empty() {
        bail!("exec import alias must not be empty");
    }
    if alias.contains('$') || alias.contains('.') {
        bail!("exec import alias `{alias}` cannot contain `$` or `.`");
    }
    Ok(())
}

fn insert_import(root: &mut Mapping, alias: &str, path: &str) -> Result<()> {
    if root.contains_key(Value::String(alias.to_string())) {
        bail!("duplicate exec import alias `{alias}`");
    }
    let mut import = Mapping::new();
    import.insert(Value::String("$import".into()), Value::String(path.into()));
    root.insert(Value::String(alias.into()), Value::Mapping(import));
    Ok(())
}

fn print_exec_value(value: RuntimeValue, format: ExecFormatArg) -> Result<()> {
    match format {
        ExecFormatArg::Raw => {
            let s = raw_exec_string(value)?;
            print!("{s}");
        }
        ExecFormatArg::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&runtime_value_to_json(value)?)?
            );
        }
    }
    Ok(())
}

#[derive(serde::Serialize)]
struct StatusReport<'a> {
    status: &'a str,
    path: String,
}

fn print_status(status: &str, path: &std::path::Path, format: StatusFormatArg) -> Result<()> {
    let report = StatusReport {
        status,
        path: path.display().to_string(),
    };
    match format {
        StatusFormatArg::Json => println!("{}", serde_json::to_string_pretty(&report)?),
        StatusFormatArg::Human => println!("{status} {}", path.display()),
    }
    Ok(())
}

fn print_structured<T: serde::Serialize>(value: &T, format: StructuredFormatArg) -> Result<()> {
    match format {
        StructuredFormatArg::Json => println!("{}", serde_json::to_string_pretty(value)?),
    }
    Ok(())
}

fn print_docs(
    available: &[docs::Documentation],
    symbol: Option<&str>,
    format: DocsFormatArg,
) -> Result<()> {
    let selected = if let Some(symbol) = symbol {
        let symbol = symbol.strip_prefix('$').unwrap_or(symbol);
        vec![available
            .iter()
            .find(|entry| entry.symbol == symbol)
            .with_context(|| format!("no documentation found for symbol `{symbol}`"))?]
    } else {
        available.iter().collect::<Vec<_>>()
    };

    match format {
        DocsFormatArg::Json => {
            if symbol.is_some() {
                println!("{}", serde_json::to_string_pretty(selected[0])?);
            } else {
                println!("{}", serde_json::to_string_pretty(&selected)?);
            }
        }
        DocsFormatArg::Plain if symbol.is_some() => {
            print!("{}", selected[0].documentation);
            if !selected[0].documentation.ends_with('\n') {
                println!();
            }
        }
        DocsFormatArg::Plain => {
            for entry in selected {
                let summary = entry.documentation.lines().next().unwrap_or_default();
                println!("{}\t{}\t{}", entry.symbol, entry.kind, summary);
            }
        }
        DocsFormatArg::Markdown => {
            for (index, entry) in selected.iter().enumerate() {
                if index > 0 {
                    println!();
                }
                println!(
                    "## `{}`\n\n*{}*\n\n{}",
                    entry.symbol, entry.kind, entry.documentation
                );
            }
        }
    }
    Ok(())
}

fn raw_exec_string(value: RuntimeValue) -> Result<String> {
    match value {
        RuntimeValue::Str(s) => Ok(s),
        RuntimeValue::Typed { value, .. } => match *value {
            RuntimeValue::Str(s) => Ok(s),
            other => bail!("raw output requires a string result, got {other:?}"),
        },
        other => bail!("raw output requires a string result, got {other:?}"),
    }
}

#[derive(serde::Serialize)]
struct EffectsReport {
    effects: Vec<EffectEntry>,
    #[serde(rename = "root-policy")]
    root_policy: std::collections::BTreeMap<String, vibra::lower::PolicyType>,
}

#[derive(serde::Serialize)]
struct EffectEntry {
    source: String,
    module: String,
    name: String,
    params: Vec<EffectParam>,
    #[serde(rename = "return")]
    result: String,
    #[serde(rename = "required-domains")]
    required_domains: Vec<String>,
}

#[derive(serde::Serialize)]
struct EffectParam {
    kind: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    value_type: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    domains: Vec<String>,
}

fn print_effects(
    path: &std::path::Path,
    format: StructuredFormatArg,
    flags: &load::CompilationFlags,
) -> Result<()> {
    let loaded = load::load_program_with_flags(path, flags)?;
    let lowered = lower::lower_program(&loaded)?;
    let reachable = reachable_functions(&lowered);
    let mut functions = lowered
        .functions
        .iter()
        .filter(|(name, _)| reachable.contains(*name))
        .collect::<Vec<_>>();
    functions.sort_by(|(left, _), (right, _)| left.cmp(right));
    let mut effects = Vec::new();
    for (source, signature) in functions {
        let lower::FunctionBody::Wasm { import, .. } = &signature.body else {
            continue;
        };
        let entry = vibra::host_abi::lookup(&import.module, &import.name)
            .context("lowered host import is absent from the ABI registry")?;
        let params = entry
            .params
            .iter()
            .map(|param| match param {
                vibra::host_abi::ParamKind::Value(kind) => EffectParam {
                    kind: "value".into(),
                    value_type: Some(kind.as_str().into()),
                    domains: Vec::new(),
                },
                vibra::host_abi::ParamKind::Capability(domains) => EffectParam {
                    kind: "capability".into(),
                    value_type: None,
                    domains: domains
                        .iter()
                        .map(|domain| domain.as_str().into())
                        .collect(),
                },
            })
            .collect();
        effects.push(EffectEntry {
            source: source.clone(),
            module: import.module.clone(),
            name: import.name.clone(),
            params,
            result: entry.result.as_str().into(),
            required_domains: entry
                .required_domains()
                .iter()
                .map(|domain| domain.as_str().into())
                .collect(),
        });
    }
    let root_policy = lowered
        .main_arg_bindings
        .iter()
        .filter_map(|(name, ty)| match ty {
            TypeRef::Policy(policy) => Some((name.clone(), policy.clone())),
            _ => None,
        })
        .collect();
    print_structured(
        &EffectsReport {
            effects,
            root_policy,
        },
        format,
    )
}

fn reachable_functions(program: &lower::LoweredProgram) -> std::collections::BTreeSet<String> {
    fn visit_expr(expr: &lower::Expr, pending: &mut Vec<String>) {
        use lower::Expr;
        match expr {
            Expr::Call { call, .. } => visit_call(call, pending),
            Expr::Mutable(value)
            | Expr::Cast { from: value, .. }
            | Expr::PolicyNarrow { from: value, .. } => visit_expr(value, pending),
            Expr::Reference { target, .. } => visit_expr(target, pending),
            Expr::EnumConstructor { payload, .. } => {
                if let Some(payload) = payload {
                    visit_expr(payload, pending);
                }
            }
            Expr::Record(fields) => fields.values().for_each(|value| visit_expr(value, pending)),
            Expr::Tuple(values) | Expr::Array(values) => {
                values.iter().for_each(|value| visit_expr(value, pending));
            }
            Expr::Primitive { args, .. } => {
                args.iter().for_each(|value| visit_expr(value, pending));
            }
            Expr::Map(entries) => entries.iter().for_each(|(key, value)| {
                visit_expr(key, pending);
                visit_expr(value, pending);
            }),
            Expr::Range { start, end, step } => {
                visit_expr(start, pending);
                visit_expr(end, pending);
                visit_expr(step, pending);
            }
            Expr::If {
                cond,
                then_e,
                else_e,
            } => {
                visit_expr(cond, pending);
                visit_expr(then_e, pending);
                visit_expr(else_e, pending);
            }
            Expr::Value(_) | Expr::VarRef(_) => {}
        }
    }
    fn visit_call(call: &lower::Call, pending: &mut Vec<String>) {
        pending.push(call.callee_key.clone());
        call.args.iter().for_each(|arg| visit_expr(arg, pending));
    }
    fn visit_statements(statements: &[lower::Statement], pending: &mut Vec<String>) {
        use lower::{LetValue, Statement};
        for statement in statements {
            match statement {
                Statement::Call(call) => visit_call(call, pending),
                Statement::Let { value, .. } => match value {
                    LetValue::Call(call) => visit_call(call, pending),
                    LetValue::Expr(expr) => visit_expr(expr, pending),
                },
                Statement::Set { value, .. }
                | Statement::Return(value)
                | Statement::Eval(value) => {
                    visit_expr(value, pending);
                }
                Statement::Match { target, arms } => {
                    visit_expr(target, pending);
                    arms.iter()
                        .for_each(|arm| visit_statements(&arm.body, pending));
                }
                Statement::If {
                    cond,
                    then_body,
                    else_body,
                } => {
                    visit_expr(cond, pending);
                    visit_statements(then_body, pending);
                    visit_statements(else_body, pending);
                }
                Statement::While { cond, body } => {
                    visit_expr(cond, pending);
                    visit_statements(body, pending);
                }
                Statement::For { source, body, .. } => {
                    visit_expr(source, pending);
                    visit_statements(body, pending);
                }
                Statement::Task { body, .. } => visit_statements(body, pending),
                Statement::Spawn { value, .. } => visit_expr(value, pending),
                Statement::Join { .. } => {}
                Statement::Break | Statement::Continue => {}
            }
        }
    }

    let mut reachable = std::collections::BTreeSet::new();
    let mut pending = Vec::new();
    visit_statements(&program.statements, &mut pending);
    while let Some(name) = pending.pop() {
        if !reachable.insert(name.clone()) {
            continue;
        }
        if let Some(signature) = program.functions.get(&name) {
            if let lower::FunctionBody::User { statements } = &signature.body {
                visit_statements(statements, &mut pending);
            }
        }
    }
    reachable
}

fn runtime_value_to_json(value: RuntimeValue) -> Result<serde_json::Value> {
    let value = vibra::execute::materialize_runtime_value(value);
    Ok(match value {
        RuntimeValue::Bool(b) => serde_json::Value::Bool(b),
        RuntimeValue::Int(i) => serde_json::Value::Number(i.into()),
        RuntimeValue::Float(f) => serde_json::to_value(f)?,
        RuntimeValue::Str(s) => serde_json::Value::String(s),
        RuntimeValue::Array(items) => serde_json::Value::Array(
            items
                .into_iter()
                .map(runtime_value_to_json)
                .collect::<Result<Vec<_>>>()?,
        ),
        RuntimeValue::Record(fields) => {
            let mut map = serde_json::Map::new();
            for (key, value) in fields {
                map.insert(key, runtime_value_to_json(value)?);
            }
            serde_json::Value::Object(map)
        }
        RuntimeValue::Tuple(items) => serde_json::Value::Array(
            items
                .into_iter()
                .map(runtime_value_to_json)
                .collect::<Result<Vec<_>>>()?,
        ),
        RuntimeValue::Map(items) => serde_json::Value::Array(
            items
                .into_iter()
                .map(|(key, value)| {
                    Ok(serde_json::json!({
                        "key": runtime_value_to_json(key)?,
                        "value": runtime_value_to_json(value)?,
                    }))
                })
                .collect::<Result<Vec<_>>>()?,
        ),
        RuntimeValue::Range { start, end, step } => serde_json::json!({
            "$range": {"start": start, "end": end, "step": step}
        }),
        RuntimeValue::Typed { type_ref, value } => serde_json::json!({
            "type": format!("{type_ref:?}"),
            "value": runtime_value_to_json(*value)?,
        }),
        RuntimeValue::Policy(policy) => serde_json::Value::String(format!("{:?}", policy.policy)),
        RuntimeValue::Capability(capability) => {
            serde_json::Value::String(format!("{:?}", capability.capability))
        }
        RuntimeValue::HostHandle(_) => {
            bail!("opaque host handles cannot be rendered as source values")
        }
        RuntimeValue::JoinHandle(_) => {
            bail!("affine task handles cannot be rendered as source values")
        }
        RuntimeValue::Enum {
            enum_key,
            tag,
            payload,
        } => serde_json::json!({
            "enum": enum_key,
            "tag": tag,
            "payload": match payload {
                Some(payload) => runtime_value_to_json(*payload)?,
                None => serde_json::Value::Null,
            },
        }),
        RuntimeValue::Void => serde_json::Value::Null,
        RuntimeValue::Mutable(_) | RuntimeValue::Reference { .. } => {
            unreachable!("runtime place handles are materialized before rendering")
        }
    })
}

fn path_str(path: &std::path::Path) -> String {
    path.display().to_string().replace('\\', "/")
}

#[allow(clippy::too_many_arguments)]
fn run_config(
    preopen: Vec<PathBuf>,
    allow_read: Vec<PathBuf>,
    allow_write: Vec<PathBuf>,
    allow_stdin: bool,
    allow_env: Vec<String>,
    allow_env_write: Vec<String>,
    allow_net: Vec<String>,
    allow_net_listen: Vec<String>,
    allow_run: Vec<String>,
    allow_clock: bool,
    allow_random: bool,
    allow_system_info: bool,
    allow_all: bool,
    max_open_files: usize,
) -> runtime::RunConfig {
    runtime::RunConfig {
        preopen_host_dirs: preopen,
        allow_read: if allow_all {
            vec![PathBuf::from(".")]
        } else {
            allow_read
        },
        allow_write: if allow_all {
            vec![PathBuf::from(".")]
        } else {
            allow_write
        },
        allow_stdin: allow_all || allow_stdin,
        allow_env: if allow_all {
            vec!["*".to_string()]
        } else {
            allow_env
        },
        allow_env_write: if allow_all {
            vec!["*".to_string()]
        } else {
            allow_env_write
        },
        allow_net: if allow_all {
            vec!["*".to_string()]
        } else {
            allow_net
        },
        allow_net_listen: if allow_all {
            vec!["*".to_string()]
        } else {
            allow_net_listen
        },
        allow_run: if allow_all {
            vec!["*".to_string()]
        } else {
            allow_run
        },
        allow_clock: allow_all || allow_clock,
        allow_random: allow_all || allow_random,
        allow_system_info: allow_all || allow_system_info,
        max_open_files,
        ..runtime::RunConfig::default()
    }
}

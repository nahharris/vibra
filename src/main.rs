//! Vibra compiler CLI. See [DRAFT.md](../DRAFT.md).

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use serde_yaml::{Mapping, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use vibra::lower::{RuntimeValue, TypeRef};
use vibra::{execute, load, lower, project, runtime, test_runner, tooling};

#[derive(Parser)]
#[command(name = "vibra", version, about = "Vibra language toolchain")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a new Vibra project.
    Init {
        /// Project directory name to create.
        name: PathBuf,
        /// Project template to scaffold.
        #[arg(long, value_enum, default_value_t = TemplateArg::Bin)]
        template: TemplateArg,
    },
    /// Clone/fetch pinned git dependencies into dep/.
    Sync {
        /// Project directory or project.vibra path.
        path: Option<PathBuf>,
    },
    /// Validate a Vibra project manifest, dependencies, targets, and imports.
    Check {
        /// Project directory or project.vibra path.
        path: Option<PathBuf>,
    },
    /// Check or rewrite canonical Vibra/YAML formatting.
    Fmt {
        /// Files, directories, or globs to format. Defaults to the current directory.
        path: Vec<PathBuf>,
        /// Rewrite changed files in place.
        #[arg(long)]
        write: bool,
        /// Structured output format.
        #[arg(long, value_enum, default_value_t = ToolOutputArg::Yaml)]
        output: ToolOutputArg,
    },
    /// Emit Vibra diagnostics for source files.
    Lint {
        /// Files, directories, or globs to lint. Defaults to the current directory.
        path: Vec<PathBuf>,
        /// Structured output format.
        #[arg(long, value_enum, default_value_t = LintFormatArg::Yaml)]
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
        /// Deprecated: seed both read and write filesystem grants.
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
        #[arg(long, value_enum, default_value_t = ExecFormatArg::Yaml)]
        format: ExecFormatArg,
        /// Deprecated: seed both read and write filesystem grants.
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
        /// Number of test worker processes.
        #[arg(long)]
        jobs: Option<usize>,
        /// Per-test timeout in milliseconds.
        #[arg(long = "timeout-ms", default_value_t = 30_000)]
        timeout_ms: u64,
        /// Stop scheduling tests after the first failure.
        #[arg(long = "fail-fast")]
        fail_fast: bool,
        /// Structured report format.
        #[arg(long, value_enum, default_value_t = ReportArg::Human)]
        report: ReportArg,
        /// Write structured report to this path.
        #[arg(long = "report-file")]
        report_file: Option<PathBuf>,
        /// Deprecated: seed both read and write filesystem grants.
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
        #[arg(long = "allow-all")]
        allow_all: bool,
        #[arg(long = "max-open-files", default_value_t = 1024)]
        max_open_files: usize,
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
    Yaml,
}

#[derive(Clone, Copy, ValueEnum)]
enum ToolOutputArg {
    Yaml,
    Json,
}

#[derive(Clone, Copy, ValueEnum)]
enum LintFormatArg {
    Yaml,
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
    Yaml,
}

impl From<ReportArg> for test_runner::ReportFormat {
    fn from(value: ReportArg) -> Self {
        match value {
            ReportArg::Human => test_runner::ReportFormat::Human,
            ReportArg::Yaml => test_runner::ReportFormat::Yaml,
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
            ToolOutputArg::Yaml => tooling::ToolOutputFormat::Yaml,
            ToolOutputArg::Json => tooling::ToolOutputFormat::Json,
        }
    }
}

impl From<LintFormatArg> for tooling::LintOutputFormat {
    fn from(value: LintFormatArg) -> Self {
        match value {
            LintFormatArg::Yaml => tooling::LintOutputFormat::Yaml,
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

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Init { name, template } => {
            project::init_project(&name, template.into())?;
            println!("created {}", name.display());
        }
        Command::Sync { path } => {
            let path = path.unwrap_or_else(|| PathBuf::from("."));
            project::sync_project(&path)?;
            println!("synced {}", path.display());
        }
        Command::Check { path } => {
            let path = path.unwrap_or_else(|| PathBuf::from("."));
            project::check_project(&path)?;
            println!("checked {}", path.display());
        }
        Command::Fmt {
            path,
            write,
            output,
        } => {
            let ok = tooling::run_fmt(tooling::FmtOptions {
                inputs: path,
                write,
                output: output.into(),
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
        } => {
            let program = load::load_program(&path)?;
            let lowered = lower::lower_program(&program)?;
            for warning in &lowered.warnings {
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
            execute::run_lowered(&lowered, &config)?;
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
            jobs,
            timeout_ms,
            fail_fast,
            report,
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
                jobs: jobs
                    .or_else(|| std::thread::available_parallelism().ok().map(usize::from))
                    .unwrap_or(1),
                timeout: Duration::from_millis(timeout_ms),
                fail_fast,
                report: report.into(),
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
            test_runner::run_single_test(&path, &name, &config)?;
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
        ExecFormatArg::Yaml => {
            let yaml = serde_yaml::to_string(&runtime_value_to_yaml(value)?)?;
            print!("{yaml}");
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

fn runtime_value_to_yaml(value: RuntimeValue) -> Result<Value> {
    let value = vibra::execute::materialize_runtime_value(value);
    Ok(match value {
        RuntimeValue::Bool(b) => Value::Bool(b),
        RuntimeValue::Int(i) => Value::Number(i.into()),
        RuntimeValue::Float(f) => serde_yaml::to_value(f)?,
        RuntimeValue::Str(s) => Value::String(s),
        RuntimeValue::Array(items) => Value::Sequence(
            items
                .into_iter()
                .map(runtime_value_to_yaml)
                .collect::<Result<Vec<_>>>()?,
        ),
        RuntimeValue::Record(fields) => {
            let mut map = Mapping::new();
            for (key, value) in fields {
                map.insert(Value::String(key), runtime_value_to_yaml(value)?);
            }
            Value::Mapping(map)
        }
        RuntimeValue::Tuple(items) => Value::Sequence(
            items
                .into_iter()
                .map(runtime_value_to_yaml)
                .collect::<Result<Vec<_>>>()?,
        ),
        RuntimeValue::Map(items) => Value::Sequence(
            items
                .into_iter()
                .map(|(key, value)| {
                    let mut map = Mapping::new();
                    map.insert(Value::String("key".into()), runtime_value_to_yaml(key)?);
                    map.insert(Value::String("value".into()), runtime_value_to_yaml(value)?);
                    Ok(Value::Mapping(map))
                })
                .collect::<Result<Vec<_>>>()?,
        ),
        RuntimeValue::Typed { type_ref, value } => {
            let mut map = Mapping::new();
            map.insert(
                Value::String("type".into()),
                Value::String(format!("{type_ref:?}")),
            );
            map.insert(
                Value::String("value".into()),
                runtime_value_to_yaml(*value)?,
            );
            Value::Mapping(map)
        }
        RuntimeValue::Capability(grant) => {
            let mut map = Mapping::new();
            map.insert(Value::String("type".into()), Value::String(grant.type_key));
            map.insert(
                Value::String("scopes".into()),
                Value::Sequence(grant.scopes.into_iter().map(Value::String).collect()),
            );
            Value::Mapping(map)
        }
        RuntimeValue::GrantToken(grant) => {
            let mut map = Mapping::new();
            map.insert(Value::String("grant".into()), Value::String(grant.name));
            map.insert(
                Value::String("scopes".into()),
                Value::Sequence(grant.scopes.into_iter().map(Value::String).collect()),
            );
            Value::Mapping(map)
        }
        RuntimeValue::Policy(policy) => Value::String(format!("{:?}", policy.policy)),
        RuntimeValue::Enum {
            enum_key,
            tag,
            payload,
        } => {
            let mut map = Mapping::new();
            map.insert(Value::String("enum".into()), Value::String(enum_key));
            map.insert(Value::String("tag".into()), Value::String(tag));
            map.insert(
                Value::String("payload".into()),
                match payload {
                    Some(payload) => runtime_value_to_yaml(*payload)?,
                    None => Value::Null,
                },
            );
            Value::Mapping(map)
        }
        RuntimeValue::Void => Value::Null,
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

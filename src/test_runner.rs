//! Convention-based `$test` runner.

use crate::{execute, load, lower, project, runtime};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportFormat {
    Human,
    Yaml,
    Json,
}

#[derive(Debug, Clone)]
pub struct TestOptions {
    pub path: PathBuf,
    pub filter: Option<String>,
    pub profiles: Vec<String>,
    pub tags: Vec<String>,
    pub deny_skips: bool,
    pub deny_warnings: bool,
    pub allow_test_workspace: Option<TestWorkspaceAccess>,
    pub jobs: usize,
    pub timeout: Duration,
    pub fail_fast: bool,
    pub report: ReportFormat,
    pub report_file: Option<PathBuf>,
    pub run_config: runtime::RunConfig,
}

/// The filesystem policy approved for an isolated `workspace: temp` test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestWorkspaceAccess {
    Read,
    Write,
    ReadWrite,
}

#[derive(Debug, Clone)]
struct TestPlanItem {
    index: usize,
    path: PathBuf,
    display_path: String,
    name: String,
    profile: String,
    tags: Vec<String>,
    timeout: Duration,
    skip_reason: Option<String>,
    expect_error: Option<lower::ExpectedTestError>,
    workspace: Option<lower::TestWorkspace>,
}

/// Stable discovery metadata used by non-executing tooling such as MCP.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredTest {
    pub path: String,
    pub name: String,
    pub profile: String,
    pub tags: Vec<String>,
    pub timeout_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip_reason: Option<String>,
    pub requires_workspace: bool,
}

/// Discover tests without executing project code or granting capabilities.
pub fn list_tests(
    path: &Path,
    filter: Option<&str>,
    profiles: &[String],
    tags: &[String],
) -> Result<Vec<DiscoveredTest>> {
    discover_tests(path, filter, profiles, tags, Duration::from_secs(30)).map(|items| {
        items
            .into_iter()
            .map(|item| DiscoveredTest {
                path: item.display_path,
                name: item.name,
                profile: item.profile,
                tags: item.tags,
                timeout_ms: item.timeout.as_millis().min(u128::from(u64::MAX)) as u64,
                skip_reason: item.skip_reason,
                requires_workspace: item.workspace.is_some(),
            })
            .collect()
    })
}

/// A machine-readable result emitted by the isolated test worker. The parent
/// consumes this file instead of inferring a failure phase from process status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChildTestOutcome {
    pub phase: Option<ChildFailurePhase>,
    pub code: Option<String>,
    pub message: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChildFailurePhase {
    Load,
    Compile,
    Runtime,
}

#[derive(Debug, Serialize)]
pub struct TestReport {
    total: usize,
    passed: usize,
    failed: usize,
    timed_out: usize,
    skipped: usize,
    duration_ms: u128,
    tests: Vec<TestResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TestResult {
    #[serde(skip)]
    index: usize,
    name: String,
    path: String,
    status: String,
    duration_ms: u128,
    stdout: String,
    stderr: String,
    message: String,
    profile: String,
    tags: Vec<String>,
    skip_reason: Option<String>,
    warnings: Vec<String>,
}

pub fn run_tests(options: TestOptions) -> Result<bool> {
    let started = Instant::now();
    let mut items = discover_tests(
        &options.path,
        options.filter.as_deref(),
        &options.profiles,
        &options.tags,
        options.timeout,
    )?;
    if items.is_empty() {
        bail!("no tests discovered");
    }
    let total = items.len();
    let mut skipped_results = Vec::new();
    items.retain(|item| {
        let reason = item.skip_reason.as_deref().or_else(|| {
            (item.workspace.is_some() && options.allow_test_workspace.is_none())
                .then_some("requires --allow-test-workspace read, write, or read-write")
        });
        if let Some(reason) = reason {
            skipped_results.push(skipped_result(item, reason));
            false
        } else {
            true
        }
    });
    let queue = Arc::new(Mutex::new(VecDeque::from(std::mem::take(&mut items))));
    let results = Arc::new(Mutex::new(Vec::new()));
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let jobs = options.jobs.max(1).min(items.len().max(1));

    let mut handles = Vec::new();
    for _ in 0..jobs {
        let queue = Arc::clone(&queue);
        let results = Arc::clone(&results);
        let stop = Arc::clone(&stop);
        let fail_fast = options.fail_fast;
        let run_config = options.run_config.clone();
        let allow_test_workspace = options.allow_test_workspace;
        let deny_warnings = options.deny_warnings;
        handles.push(thread::spawn(move || -> Result<()> {
            loop {
                if fail_fast && stop.load(std::sync::atomic::Ordering::SeqCst) {
                    return Ok(());
                }
                let item = {
                    let mut q = queue.lock().expect("test queue poisoned");
                    q.pop_front()
                };
                let Some(item) = item else {
                    return Ok(());
                };
                let result = run_one_child(
                    &item,
                    item.timeout,
                    &run_config,
                    allow_test_workspace,
                    deny_warnings,
                )?;
                if fail_fast && result.status != "passed" {
                    stop.store(true, std::sync::atomic::Ordering::SeqCst);
                }
                results.lock().expect("test results poisoned").push(result);
            }
        }));
    }
    for handle in handles {
        handle
            .join()
            .map_err(|_| anyhow::anyhow!("test worker panicked"))??;
    }

    let mut tests = results.lock().expect("test results poisoned").clone();
    tests.extend(skipped_results);
    tests.sort_by_key(|r| r.index);
    let passed = tests.iter().filter(|r| r.status == "passed").count();
    let failed = tests.iter().filter(|r| r.status == "failed").count();
    let timed_out = tests.iter().filter(|r| r.status == "timed_out").count();
    let skipped = tests.iter().filter(|r| r.status == "skipped").count();
    let report = TestReport {
        total,
        passed,
        failed,
        timed_out,
        skipped,
        duration_ms: started.elapsed().as_millis(),
        tests,
    };

    if options.report == ReportFormat::Human {
        print_human_report(&report);
    }
    if options.report != ReportFormat::Human || options.report_file.is_some() {
        let rendered = match options.report {
            ReportFormat::Human | ReportFormat::Yaml => serde_yaml::to_string(&report)?,
            ReportFormat::Json => serde_json::to_string_pretty(&report)? + "\n",
        };
        if let Some(path) = &options.report_file {
            fs::write(path, &rendered).with_context(|| format!("write {}", path.display()))?;
        } else {
            print!("{rendered}");
        }
    }
    Ok(report.failed == 0 && report.timed_out == 0 && (!options.deny_skips || report.skipped == 0))
}

pub fn run_single_test(path: &Path, name: &str, config: &runtime::RunConfig) -> ChildTestOutcome {
    let program = match load::load_program(path) {
        Ok(program) => program,
        Err(error) => return failed_outcome(ChildFailurePhase::Load, error, Vec::new()),
    };
    let test = match lower::lower_named_test(&program, name) {
        Ok(test) => test,
        Err(error) => return failed_outcome(ChildFailurePhase::Compile, error, Vec::new()),
    };
    let warnings = test.program.warnings.clone();
    for warning in &warnings {
        eprintln!("warning: {warning}");
    }
    match execute::run_lowered(&test.program, config) {
        Ok(()) => ChildTestOutcome {
            phase: None,
            code: None,
            message: String::new(),
            warnings,
        },
        Err(error) => failed_outcome(ChildFailurePhase::Runtime, error, warnings),
    }
}

fn failed_outcome(
    phase: ChildFailurePhase,
    error: anyhow::Error,
    warnings: Vec<String>,
) -> ChildTestOutcome {
    let message = format!("{error:#}");
    ChildTestOutcome {
        phase: Some(phase),
        code: diagnostic_code(&message),
        message,
        warnings,
    }
}

fn diagnostic_code(message: &str) -> Option<String> {
    message
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '-'))
        .find(|word| {
            let mut parts = word.split('-');
            matches!(parts.next(), Some("E"))
                && parts.next().is_some_and(|part| !part.is_empty())
                && parts
                    .next()
                    .is_some_and(|part| part.chars().all(|ch| ch.is_ascii_digit()))
                && parts.next().is_none()
        })
        .map(ToOwned::to_owned)
}

fn discover_tests(
    path: &Path,
    filter: Option<&str>,
    profiles: &[String],
    tags: &[String],
    command_timeout: Duration,
) -> Result<Vec<TestPlanItem>> {
    let root = test_root(path)?;
    let files = if root.is_file() {
        vec![root.clone()]
    } else {
        let test_dir = root.join("tests");
        if !test_dir.exists() {
            bail!("test directory `{}` does not exist", test_dir.display());
        }
        let mut files = Vec::new();
        collect_vibra_files(&test_dir, &mut files)?;
        files
    };

    let base = if root.is_file() {
        root.parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf()
    } else {
        root
    };
    let mut items = Vec::new();
    let mut seen_modules = HashSet::new();
    for file in files {
        let display_path = normalize_path(file.strip_prefix(&base).unwrap_or(&file));
        let (entry, entry_module) = load::load_entry_module_for_test_discovery(&file)?;
        if !seen_modules.insert(entry.clone()) {
            continue;
        }
        let entry_map = entry_module
            .as_mapping()
            .context("entry root must be mapping")?;
        let specs = lower::discover_test_specs_in_entry(entry_map, &entry)?;
        for spec in specs {
            let name = spec.name;
            let full_name = format!("{display_path}::{name}");
            if filter.is_some_and(|f| !full_name.contains(f) && !display_path.contains(f)) {
                continue;
            }
            if !profiles.is_empty() && !profiles.iter().any(|profile| profile == &spec.profile) {
                continue;
            }
            if profiles.is_empty() && spec.profile != "core" {
                continue;
            }
            if !tags
                .iter()
                .all(|tag| spec.tags.iter().any(|candidate| candidate == tag))
            {
                continue;
            }
            items.push(TestPlanItem {
                index: items.len(),
                path: file.clone(),
                display_path: display_path.clone(),
                name,
                profile: spec.profile,
                tags: spec.tags,
                timeout: spec
                    .timeout_ms
                    .map(Duration::from_millis)
                    .map(|value| value.min(command_timeout))
                    .unwrap_or(command_timeout),
                skip_reason: spec.skip,
                expect_error: spec.expect_error,
                workspace: spec.workspace,
            });
        }
    }
    items.sort_by(|a, b| {
        a.display_path
            .cmp(&b.display_path)
            .then_with(|| a.name.cmp(&b.name))
    });
    for (idx, item) in items.iter_mut().enumerate() {
        item.index = idx;
    }
    Ok(items)
}

fn skipped_result(item: &TestPlanItem, reason: &str) -> TestResult {
    TestResult {
        index: item.index,
        name: item.name.clone(),
        path: item.display_path.clone(),
        status: "skipped".to_string(),
        duration_ms: 0,
        stdout: String::new(),
        stderr: String::new(),
        message: String::new(),
        profile: item.profile.clone(),
        tags: item.tags.clone(),
        skip_reason: Some(reason.to_string()),
        warnings: Vec::new(),
    }
}

fn test_root(path: &Path) -> Result<PathBuf> {
    if path.is_file() {
        return fs::canonicalize(path).with_context(|| format!("resolve {}", path.display()));
    }
    if path.join(project::MANIFEST_FILE).exists() {
        return fs::canonicalize(path).with_context(|| format!("resolve {}", path.display()));
    }
    if let Ok(project) = project::load_project(path) {
        return Ok(project.root);
    }
    fs::canonicalize(path).with_context(|| format!("resolve {}", path.display()))
}

fn collect_vibra_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect_vibra_files(&path, files)?;
        } else if is_vibra_file(&path) {
            files.push(path);
        }
    }
    files.sort();
    Ok(())
}

fn is_vibra_file(path: &Path) -> bool {
    let s = path.to_string_lossy();
    s.ends_with(".vibra") || s.ends_with(".vibra.yaml")
}

fn run_one_child(
    item: &TestPlanItem,
    timeout: Duration,
    config: &runtime::RunConfig,
    allow_test_workspace: Option<TestWorkspaceAccess>,
    deny_warnings: bool,
) -> Result<TestResult> {
    let started = Instant::now();
    let exe = std::env::current_exe().context("resolve current executable")?;
    let mut cmd = Command::new(exe);
    let result_file = child_result_file()?;
    let result_path: &Path = result_file.as_ref();
    cmd.arg("__run-test")
        .arg(&item.path)
        .arg(&item.name)
        .arg("--result-file")
        .arg(result_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let temp_workspace = item
        .workspace
        .is_some()
        .then(tempfile::tempdir)
        .transpose()?;
    let child_config = match (temp_workspace.as_ref(), allow_test_workspace) {
        (Some(workspace), Some(access)) => {
            isolated_workspace_config(config, workspace.path(), access)
        }
        (Some(_), None) => bail!("workspace test was scheduled without workspace access"),
        (None, _) => config.clone(),
    };
    if let Some(workspace) = &temp_workspace {
        cmd.current_dir(workspace.path());
    }
    append_run_config_args(&mut cmd, &child_config);
    let mut child = cmd
        .spawn()
        .with_context(|| format!("spawn test `{}` from {}", item.name, item.path.display()))?;
    // Drain both pipes while the worker executes. Waiting to read until after
    // `try_wait` can deadlock once a test writes more than an OS pipe buffer.
    let stdout_pipe = child.stdout.take().context("child stdout was not piped")?;
    let stderr_pipe = child.stderr.take().context("child stderr was not piped")?;
    let stdout_reader = thread::spawn(move || -> std::io::Result<String> {
        let mut stdout = String::new();
        let mut pipe = stdout_pipe;
        pipe.read_to_string(&mut stdout)?;
        Ok(stdout)
    });
    let stderr_reader = thread::spawn(move || -> std::io::Result<String> {
        let mut stderr = String::new();
        let mut pipe = stderr_pipe;
        pipe.read_to_string(&mut stderr)?;
        Ok(stderr)
    });
    loop {
        if let Some(status) = child.try_wait()? {
            let stdout = join_output_reader(stdout_reader, "stdout")?;
            let stderr = join_output_reader(stderr_reader, "stderr")?;
            let outcome = fs::read_to_string(result_path)
                .with_context(|| format!("read child result for test `{}`", item.name))
                .and_then(|text| {
                    serde_yaml::from_str::<ChildTestOutcome>(&text)
                        .context("decode child test result")
                });
            let _ = result_file.close();
            let (mut passed, mut message, warnings) =
                match outcome {
                    Ok(outcome) => {
                        let (passed, message) =
                            match_expected_outcome(item.expect_error.as_ref(), &outcome);
                        (passed, message, outcome.warnings)
                    }
                    Err(error) => {
                        (
                            false,
                            format!(
                        "test worker exited {} without a valid structured result: {error:#}",
                        if status.success() { "successfully" } else { "unsuccessfully" }
                    ),
                            Vec::new(),
                        )
                    }
                };
            if passed && deny_warnings && !warnings.is_empty() {
                passed = false;
                message = format!("compiler warnings denied: {}", warnings.join("; "));
            }
            return Ok(TestResult {
                name: item.name.clone(),
                index: item.index,
                path: item.display_path.clone(),
                status: if passed { "passed" } else { "failed" }.to_string(),
                duration_ms: started.elapsed().as_millis(),
                stdout,
                stderr: stderr.clone(),
                message: if passed { String::new() } else { message },
                profile: item.profile.clone(),
                tags: item.tags.clone(),
                skip_reason: None,
                warnings,
            });
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            let _ = result_file.close();
            let stdout = join_output_reader(stdout_reader, "stdout").unwrap_or_default();
            let stderr = join_output_reader(stderr_reader, "stderr").unwrap_or_default();
            return Ok(TestResult {
                name: item.name.clone(),
                index: item.index,
                path: item.display_path.clone(),
                status: "timed_out".to_string(),
                duration_ms: started.elapsed().as_millis(),
                stdout,
                stderr,
                message: format!("timed out after {} ms", timeout.as_millis()),
                profile: item.profile.clone(),
                tags: item.tags.clone(),
                skip_reason: None,
                warnings: Vec::new(),
            });
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn join_output_reader(
    reader: thread::JoinHandle<std::io::Result<String>>,
    stream: &str,
) -> Result<String> {
    reader
        .join()
        .map_err(|_| anyhow::anyhow!("test child {stream} reader panicked"))?
        .with_context(|| format!("read test child {stream}"))
}

fn isolated_workspace_config(
    base: &runtime::RunConfig,
    workspace: &Path,
    access: TestWorkspaceAccess,
) -> runtime::RunConfig {
    let mut config = base.clone();
    config.preopen_host_dirs.clear();
    config.allow_read.clear();
    config.allow_write.clear();
    match access {
        TestWorkspaceAccess::Read => config.allow_read.push(workspace.to_path_buf()),
        TestWorkspaceAccess::Write => config.allow_write.push(workspace.to_path_buf()),
        TestWorkspaceAccess::ReadWrite => {
            config.allow_read.push(workspace.to_path_buf());
            config.allow_write.push(workspace.to_path_buf());
        }
    }
    config
}

fn match_expected_outcome(
    expected: Option<&lower::ExpectedTestError>,
    outcome: &ChildTestOutcome,
) -> (bool, String) {
    let Some(expected) = expected else {
        return match outcome.phase {
            None => (true, String::new()),
            Some(phase) => (false, format!("{}", outcome_message(phase, outcome))),
        };
    };
    let expected_phase = expected_phase_name(expected.phase);
    let Some(actual_phase) = outcome.phase else {
        return (
            false,
            format!("expected {expected_phase} error, but test passed"),
        );
    };
    if !same_phase(expected.phase, actual_phase) {
        return (
            false,
            format!(
                "expected {expected_phase} error, but test failed during {}: {}",
                phase_name(actual_phase),
                outcome.message
            ),
        );
    }
    if let Some(code) = &expected.code {
        if outcome.code.as_deref() != Some(code.as_str()) {
            return (
                false,
                format!(
                    "expected {expected_phase} error code `{code}`, got `{}`: {}",
                    outcome.code.as_deref().unwrap_or("no diagnostic code"),
                    outcome.message
                ),
            );
        }
    }
    if let Some(needle) = &expected.message_contains {
        if !outcome.message.contains(needle) {
            return (
                false,
                format!(
                    "expected {expected_phase} error message to contain `{needle}`, got: {}",
                    outcome.message
                ),
            );
        }
    }
    (true, String::new())
}

fn same_phase(expected: lower::TestErrorPhase, actual: ChildFailurePhase) -> bool {
    matches!(
        (expected, actual),
        (lower::TestErrorPhase::Load, ChildFailurePhase::Load)
            | (lower::TestErrorPhase::Compile, ChildFailurePhase::Compile)
            | (lower::TestErrorPhase::Runtime, ChildFailurePhase::Runtime)
    )
}

fn expected_phase_name(phase: lower::TestErrorPhase) -> &'static str {
    match phase {
        lower::TestErrorPhase::Load => "load",
        lower::TestErrorPhase::Compile => "compile",
        lower::TestErrorPhase::Runtime => "runtime",
    }
}

fn phase_name(phase: ChildFailurePhase) -> &'static str {
    match phase {
        ChildFailurePhase::Load => "load",
        ChildFailurePhase::Compile => "compile",
        ChildFailurePhase::Runtime => "runtime",
    }
}

fn outcome_message(phase: ChildFailurePhase, outcome: &ChildTestOutcome) -> String {
    format!("{} error: {}", phase_name(phase), outcome.message)
}

fn child_result_file() -> Result<tempfile::TempPath> {
    tempfile::NamedTempFile::new_in(std::env::temp_dir())
        .context("reserve child test result file")
        .map(tempfile::NamedTempFile::into_temp_path)
}

fn append_run_config_args(cmd: &mut Command, config: &runtime::RunConfig) {
    for path in &config.preopen_host_dirs {
        cmd.arg("--preopen").arg(path);
    }
    for path in &config.allow_read {
        cmd.arg("--allow-read").arg(path);
    }
    for path in &config.allow_write {
        cmd.arg("--allow-write").arg(path);
    }
    if config.allow_stdin {
        cmd.arg("--allow-stdin");
    }
    for name in &config.allow_env {
        cmd.arg("--allow-env").arg(name);
    }
    for name in &config.allow_env_write {
        cmd.arg("--allow-env-write").arg(name);
    }
    for host in &config.allow_net {
        cmd.arg("--allow-net").arg(host);
    }
    for host in &config.allow_net_listen {
        cmd.arg("--allow-net-listen").arg(host);
    }
    for program in &config.allow_run {
        cmd.arg("--allow-run").arg(program);
    }
    if config.allow_clock {
        cmd.arg("--allow-clock");
    }
    if config.allow_random {
        cmd.arg("--allow-random");
    }
    if config.allow_system_info {
        cmd.arg("--allow-sys-info");
    }
    cmd.arg("--max-open-files")
        .arg(config.max_open_files.to_string());
}

fn print_human_report(report: &TestReport) {
    for test in &report.tests {
        println!("{} {}::{}", test.status, test.path, test.name);
        if test.status == "skipped" {
            if let Some(reason) = &test.skip_reason {
                println!("  {reason}");
            }
        }
        if test.status != "passed" && !test.message.is_empty() {
            eprintln!("{}", test.message.trim());
        }
    }
    println!(
        "{} passed; {} failed; {} timed out; {} skipped; {} total",
        report.passed, report.failed, report.timed_out, report.skipped, report.total
    );
}

fn normalize_path(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

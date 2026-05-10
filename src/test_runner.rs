//! Convention-based `$test` runner.

use crate::{execute, load, lower, project, runtime};
use anyhow::{bail, Context, Result};
use serde::Serialize;
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
}

#[derive(Debug, Clone)]
pub struct TestOptions {
    pub path: PathBuf,
    pub filter: Option<String>,
    pub jobs: usize,
    pub timeout: Duration,
    pub fail_fast: bool,
    pub report: ReportFormat,
    pub report_file: Option<PathBuf>,
    pub run_config: runtime::RunConfig,
}

#[derive(Debug, Clone)]
struct TestPlanItem {
    index: usize,
    path: PathBuf,
    display_path: String,
    name: String,
}

#[derive(Debug, Serialize)]
pub struct TestReport {
    total: usize,
    passed: usize,
    failed: usize,
    timed_out: usize,
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
}

pub fn run_tests(options: TestOptions) -> Result<bool> {
    let started = Instant::now();
    let mut items = discover_tests(&options.path, options.filter.as_deref())?;
    if items.is_empty() {
        bail!("no tests discovered");
    }
    let total = items.len();
    let queue = Arc::new(Mutex::new(VecDeque::from(std::mem::take(&mut items))));
    let results = Arc::new(Mutex::new(Vec::new()));
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let jobs = options.jobs.max(1).min(total);

    let mut handles = Vec::new();
    for _ in 0..jobs {
        let queue = Arc::clone(&queue);
        let results = Arc::clone(&results);
        let stop = Arc::clone(&stop);
        let timeout = options.timeout;
        let fail_fast = options.fail_fast;
        let run_config = options.run_config.clone();
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
                let result = run_one_child(&item, timeout, &run_config)?;
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
    tests.sort_by_key(|r| r.index);
    let passed = tests.iter().filter(|r| r.status == "passed").count();
    let failed = tests.iter().filter(|r| r.status == "failed").count();
    let timed_out = tests.iter().filter(|r| r.status == "timed_out").count();
    let report = TestReport {
        total,
        passed,
        failed,
        timed_out,
        duration_ms: started.elapsed().as_millis(),
        tests,
    };

    if options.report == ReportFormat::Human {
        print_human_report(&report);
    }
    if options.report == ReportFormat::Yaml || options.report_file.is_some() {
        let yaml = serde_yaml::to_string(&report)?;
        if let Some(path) = &options.report_file {
            fs::write(path, &yaml).with_context(|| format!("write {}", path.display()))?;
        } else {
            println!("{yaml}");
        }
    }
    Ok(report.failed == 0 && report.timed_out == 0)
}

pub fn run_single_test(path: &Path, name: &str, config: &runtime::RunConfig) -> Result<()> {
    let program = load::load_program(path)?;
    let tests = lower::lower_tests(&program)?;
    let test = tests
        .into_iter()
        .find(|test| test.name == name)
        .with_context(|| format!("test `{name}` not found in {}", path.display()))?;
    for warning in &test.program.warnings {
        eprintln!("warning: {warning}");
    }
    execute::run_lowered(&test.program, config)
}

fn discover_tests(path: &Path, filter: Option<&str>) -> Result<Vec<TestPlanItem>> {
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
        let program = load::load_program(&file)?;
        if !seen_modules.insert(program.entry.clone()) {
            continue;
        }
        let tests = lower::lower_tests(&program)?;
        for test in tests {
            let full_name = format!("{display_path}::{}", test.name);
            if filter.is_some_and(|f| !full_name.contains(f) && !display_path.contains(f)) {
                continue;
            }
            items.push(TestPlanItem {
                index: items.len(),
                path: file.clone(),
                display_path: display_path.clone(),
                name: test.name,
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
) -> Result<TestResult> {
    let started = Instant::now();
    let exe = std::env::current_exe().context("resolve current executable")?;
    let mut cmd = Command::new(exe);
    cmd.arg("__run-test")
        .arg(&item.path)
        .arg(&item.name)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    append_run_config_args(&mut cmd, config);
    let mut child = cmd
        .spawn()
        .with_context(|| format!("spawn test `{}` from {}", item.name, item.path.display()))?;
    let mut stdout_pipe = child.stdout.take().context("child stdout was not piped")?;
    let mut stderr_pipe = child.stderr.take().context("child stderr was not piped")?;
    loop {
        if let Some(status) = child.try_wait()? {
            let mut stdout = String::new();
            let mut stderr = String::new();
            stdout_pipe.read_to_string(&mut stdout)?;
            stderr_pipe.read_to_string(&mut stderr)?;
            let passed = status.success();
            return Ok(TestResult {
                name: item.name.clone(),
                index: item.index,
                path: item.display_path.clone(),
                status: if passed { "passed" } else { "failed" }.to_string(),
                duration_ms: started.elapsed().as_millis(),
                stdout,
                stderr: stderr.clone(),
                message: if passed { String::new() } else { stderr },
            });
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            let mut stdout = String::new();
            let mut stderr = String::new();
            let _ = stdout_pipe.read_to_string(&mut stdout);
            let _ = stderr_pipe.read_to_string(&mut stderr);
            return Ok(TestResult {
                name: item.name.clone(),
                index: item.index,
                path: item.display_path.clone(),
                status: "timed_out".to_string(),
                duration_ms: started.elapsed().as_millis(),
                stdout,
                stderr,
                message: format!("timed out after {} ms", timeout.as_millis()),
            });
        }
        thread::sleep(Duration::from_millis(10));
    }
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
}

fn print_human_report(report: &TestReport) {
    for test in &report.tests {
        println!("{} {}::{}", test.status, test.path, test.name);
        if test.status != "passed" && !test.message.is_empty() {
            eprintln!("{}", test.message.trim());
        }
    }
    println!(
        "{} passed; {} failed; {} timed out; {} total",
        report.passed, report.failed, report.timed_out, report.total
    );
}

fn normalize_path(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

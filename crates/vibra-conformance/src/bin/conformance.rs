//! The conformance runner's command line:
//! `vibra-conformance [--root <dir>] [--executor <name>]`.
//!
//! It runs every case of the corpus, prints one line per case and one count
//! line per profile, and exits 0 only when every case passed. With
//! `--executor wasm`, every executable case the interpreter accepted also runs
//! on the WebAssembly transition backend, and the executor's column is printed
//! per profile; a case it does not run is unavailable there and does not fail
//! the run. An executor this build or machine does not have is reported
//! missing, and the run fails. It is a CI
//! adapter, not the user-facing `vibra` command, and it has no option that
//! writes an expectation.

use std::path::PathBuf;
use std::process::ExitCode;

use vibra_conformance::{
    CaseStatus, ConformanceRunner, Corpus, WasmExecutor, standard_dispatcher,
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("conformance: {message}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<(), String> {
    let (root, executor) = parse_options(std::env::args_os().skip(1))?;
    let corpus = Corpus::discover(&root).map_err(|error| error.to_string())?;
    if corpus.is_empty() {
        return Err("corpus is empty".to_owned());
    }
    let mut runner = ConformanceRunner::new(standard_dispatcher());
    let mut missing = None;
    match executor.as_deref() {
        None => {}
        Some(WasmExecutor::NAME) => match WasmExecutor::new() {
            Ok(wasm) => runner = runner.with_executor(wasm),
            Err(reason) => missing = Some(format!("{} ({reason})", WasmExecutor::NAME)),
        },
        Some(other) => missing = Some(other.to_owned()),
    }
    let report = runner.run(&corpus);
    for case in report.cases() {
        match &case.status {
            CaseStatus::Passed => println!("PASS {}", case.case_id),
            CaseStatus::Failed { reason } => {
                println!("FAIL {}: {reason}", case.case_id)
            }
            CaseStatus::Unknown { reason } => {
                println!("UNKNOWN {}: {reason}", case.case_id);
            }
            CaseStatus::Unavailable { reason } => {
                println!("UNAVAILABLE {}: {reason}", case.case_id);
            }
        }
        match &case.executor {
            None | Some(CaseStatus::Passed) => {}
            Some(CaseStatus::Failed { reason }) => {
                println!("FAIL {}: {reason}", case.case_id)
            }
            Some(CaseStatus::Unknown { reason }) => {
                println!("UNKNOWN {}: {reason}", case.case_id);
            }
            Some(CaseStatus::Unavailable { reason }) => {
                println!("UNAVAILABLE {}: {reason}", case.case_id);
            }
        }
    }
    for (profile, counts) in report.profile_counts() {
        println!("{profile} conformance: {counts}");
    }
    println!("conformance total: {}", report.total());
    if executor.is_some() && missing.is_none() {
        for (profile, counts) in report.executor_counts() {
            println!("{profile} conformance ({}): {counts}", WasmExecutor::NAME);
        }
        println!(
            "{} executor total: {}",
            WasmExecutor::NAME,
            report.executor_total()
        );
    }
    if let Some(missing) = missing {
        println!("{missing} executor: missing");
        return Err("an executor is missing".to_owned());
    }
    if report.is_success() {
        Ok(())
    } else {
        Err("one or more cases did not pass".to_owned())
    }
}

/// The corpus root, the one `--root <dir>` option or the workspace's corpus,
/// and the executor of the one `--executor <name>` option.
fn parse_options<I>(mut arguments: I) -> Result<(PathBuf, Option<String>), String>
where
    I: Iterator<Item = std::ffi::OsString>,
{
    let mut root = None;
    let mut executor = None;
    while let Some(argument) = arguments.next() {
        if argument == "--root" {
            let Some(path) = arguments.next() else {
                return Err("--root requires a path".to_owned());
            };
            if root.is_some() {
                return Err("--root is given more than once".to_owned());
            }
            root = Some(PathBuf::from(path));
        } else if argument == "--executor" {
            let Some(name) = arguments.next() else {
                return Err("--executor requires a name".to_owned());
            };
            if executor.is_some() {
                return Err("--executor is given more than once".to_owned());
            }
            executor = Some(name.to_string_lossy().into_owned());
        } else if argument.to_string_lossy().starts_with('-') {
            return Err(format!("unknown option `{}`", argument.to_string_lossy()));
        } else {
            return Err(format!(
                "unexpected argument `{}`; the corpus root is given with --root",
                argument.to_string_lossy()
            ));
        }
    }
    Ok((
        root.unwrap_or_else(vibra_conformance::corpus_root),
        executor,
    ))
}

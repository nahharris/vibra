//! Internal reader-v1 conformance entrypoint.
//!
//! This binary is intentionally owned by `vibra-conformance`; it is a CI
//! adapter and is not the user-facing `vibra` command promised for milestone 2.

use std::path::PathBuf;
use std::process::ExitCode;

use vibra_conformance::{
    ConformanceProfile, ConformanceRunner, Corpus, ProfileDispatcher, ReaderV1Handler,
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
    let root = parse_root(std::env::args_os().skip(1))?;
    let corpus = Corpus::discover(&root).map_err(|error| error.to_string())?;
    if corpus.is_empty() {
        return Err("corpus is empty".to_owned());
    }
    let reader_cases = corpus
        .cases()
        .iter()
        .filter(|case| case.manifest().profile() == ConformanceProfile::ReaderV1)
        .count();
    if reader_cases == 0 {
        return Err("corpus contains no reader-v1 cases".to_owned());
    }
    let dispatcher = ProfileDispatcher::new()
        .with_handler(ConformanceProfile::ReaderV1, ReaderV1Handler);
    let report = ConformanceRunner::new(dispatcher).run(&corpus);

    for case in report.cases() {
        match &case.status {
            vibra_conformance::CaseStatus::Passed => {
                println!("PASS {}", case.case_id);
            }
            vibra_conformance::CaseStatus::Failed { reason } => {
                println!("FAIL {}: {reason}", case.case_id);
            }
            vibra_conformance::CaseStatus::Unavailable { reason } => {
                println!("UNAVAILABLE {}: {reason}", case.case_id);
            }
        }
    }
    println!(
        "reader-v1 conformance: {} passed, {} failed, {} unavailable",
        report.passed(),
        report.failed(),
        report.unavailable()
    );

    if report.is_success() {
        Ok(())
    } else {
        Err("one or more cases did not pass".to_owned())
    }
}

fn parse_root<I>(mut arguments: I) -> Result<PathBuf, String>
where
    I: Iterator<Item = std::ffi::OsString>,
{
    let Some(first) = arguments.next() else {
        return Ok(PathBuf::from("conformance/cases"));
    };
    if first == "--root" {
        let Some(root) = arguments.next() else {
            return Err("--root requires a path".to_owned());
        };
        if arguments.next().is_some() {
            return Err("unexpected arguments after --root".to_owned());
        }
        return Ok(PathBuf::from(root));
    }
    if first.to_string_lossy().starts_with('-') {
        return Err(format!("unknown option `{}`", first.to_string_lossy()));
    }
    if arguments.next().is_some() {
        return Err("expected one corpus root path".to_owned());
    }
    Ok(PathBuf::from(first))
}

//! The conformance runner's command line: `vibra-conformance [--root <dir>]`.
//!
//! It runs every case of the corpus, prints one line per case and one count
//! line per profile, and exits 0 only when every case passed. It is a CI
//! adapter, not the user-facing `vibra` command, and it has no option that
//! writes an expectation.

use std::path::PathBuf;
use std::process::ExitCode;

use vibra_conformance::{CaseStatus, ConformanceRunner, Corpus, standard_dispatcher};

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
    let root = parse_options(std::env::args_os().skip(1))?;
    let corpus = Corpus::discover(&root).map_err(|error| error.to_string())?;
    if corpus.is_empty() {
        return Err("corpus is empty".to_owned());
    }
    let report = ConformanceRunner::new(standard_dispatcher()).run(&corpus);
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
    }
    for (profile, counts) in report.profile_counts() {
        println!("{profile} conformance: {counts}");
    }
    println!("conformance total: {}", report.total());
    if report.is_success() {
        Ok(())
    } else {
        Err("one or more cases did not pass".to_owned())
    }
}

/// The corpus root: the one `--root <dir>` option, or the workspace's corpus.
fn parse_options<I>(mut arguments: I) -> Result<PathBuf, String>
where
    I: Iterator<Item = std::ffi::OsString>,
{
    let mut root = None;
    while let Some(argument) = arguments.next() {
        if argument == "--root" {
            let Some(path) = arguments.next() else {
                return Err("--root requires a path".to_owned());
            };
            if root.is_some() {
                return Err("--root is given more than once".to_owned());
            }
            root = Some(PathBuf::from(path));
        } else if argument.to_string_lossy().starts_with('-') {
            return Err(format!("unknown option `{}`", argument.to_string_lossy()));
        } else {
            return Err(format!(
                "unexpected argument `{}`; the corpus root is given with --root",
                argument.to_string_lossy()
            ));
        }
    }
    Ok(root.unwrap_or_else(vibra_conformance::corpus_root))
}

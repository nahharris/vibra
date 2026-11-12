//! Internal conformance entrypoint.
//!
//! This binary is intentionally owned by `vibra-conformance`; it is a CI
//! adapter and is not the user-facing `vibra` command promised for milestone 2.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

use vibra_conformance::{
    ConformanceProfile, ConformanceRunner, Corpus, Inventory, NATIVE_PARITY_FILE,
    standard_dispatcher,
};

/// What the command line asks for.
struct Options {
    root: PathBuf,
    /// Where to write the native parity inventory the current executors
    /// produce, instead of checking the committed one.
    write_native_parity: Option<PathBuf>,
}

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
    let Options {
        root,
        write_native_parity,
    } = parse_options(std::env::args_os().skip(1))?;
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
    let report = ConformanceRunner::new(standard_dispatcher()).run(&corpus);

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
    let mut by_profile = BTreeMap::<ConformanceProfile, (usize, usize, usize)>::new();
    for case in report.cases() {
        let counts = by_profile.entry(case.required_profile).or_default();
        match &case.status {
            vibra_conformance::CaseStatus::Passed => counts.0 += 1,
            vibra_conformance::CaseStatus::Failed { .. } => counts.1 += 1,
            vibra_conformance::CaseStatus::Unavailable { .. } => counts.2 += 1,
        }
    }
    for profile in ConformanceProfile::ALL {
        if let Some((passed, failed, unavailable)) = by_profile.get(profile) {
            println!(
                "{profile} conformance: {passed} passed, {failed} failed, {unavailable} unavailable"
            );
        }
    }
    // One line per backend over the executable cases
    // (`docs/spec/07-diagnostics-and-conformance.md`, "Differential execution").
    let interpreter = report.interpreter_counts();
    println!(
        "interpreter backend: {} passed, {} failed, {} unavailable",
        interpreter.passed, interpreter.failed, interpreter.unavailable
    );
    let wasm = report.wasm_counts();
    println!(
        "wasm backend: {} matched, {} failed",
        wasm.matched, wasm.failed
    );
    // The reference stepper, against the committed inventory when the corpus
    // has one beside it. Pending cells are gaps and do not fail the run; a cell
    // the stepper contradicts does.
    let statuses = report.native_statuses();
    if let Some(path) = write_native_parity {
        let text = Inventory::generated(&statuses).render();
        std::fs::write(&path, text)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        println!("native parity inventory written to {}", path.display());
    }
    let mut disagreements = Vec::new();
    let table = root
        .parent()
        .map(|parent| parent.join(NATIVE_PARITY_FILE))
        .filter(|path| path.is_file());
    if let Some(path) = table {
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let inventory = Inventory::parse(&text)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let executable = statuses
            .iter()
            .map(|(case, _)| case.clone())
            .collect::<Vec<_>>();
        disagreements.extend(inventory.problems(&executable));
        disagreements.extend(inventory.stepper_disagreements(&statuses));
    }
    for disagreement in &disagreements {
        println!("NATIVE {disagreement}");
    }
    let native = report.native_counts();
    println!(
        "native backend: stepper {} matched, {} pending, {} failed",
        native.matched,
        native.pending,
        native.failed.max(disagreements.len())
    );
    println!(
        "conformance total: {} passed, {} failed, {} unavailable",
        report.passed(),
        report.failed(),
        report.unavailable()
    );

    if report.is_success() && disagreements.is_empty() {
        Ok(())
    } else {
        Err("one or more cases did not pass".to_owned())
    }
}

fn parse_options<I>(mut arguments: I) -> Result<Options, String>
where
    I: Iterator<Item = std::ffi::OsString>,
{
    let mut options = Options {
        root: vibra_conformance::corpus_root(),
        write_native_parity: None,
    };
    let mut positional = false;
    let mut named_root = false;
    while let Some(argument) = arguments.next() {
        if argument == "--root" {
            let Some(root) = arguments.next() else {
                return Err("--root requires a path".to_owned());
            };
            if positional || named_root {
                return Err("expected one corpus root path".to_owned());
            }
            named_root = true;
            options.root = PathBuf::from(root);
        } else if argument == "--write-native-parity" {
            let Some(path) = arguments.next() else {
                return Err("--write-native-parity requires a path".to_owned());
            };
            options.write_native_parity = Some(PathBuf::from(path));
        } else if argument.to_string_lossy().starts_with('-') {
            return Err(format!("unknown option `{}`", argument.to_string_lossy()));
        } else {
            if positional || named_root {
                return Err("expected one corpus root path".to_owned());
            }
            positional = true;
            options.root = PathBuf::from(argument);
        }
    }
    Ok(options)
}

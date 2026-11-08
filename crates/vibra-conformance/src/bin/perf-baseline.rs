//! The performance-baseline tool (milestone 4 Step 21).
//!
//! ```text
//! perf-baseline [--write] [--check] [--runs N]
//! ```
//!
//! With no option it measures the exact metrics and the medians and prints the
//! `@perf-baseline.v1` document. `--write` writes it to
//! `docs/roadmap/v0.0.1/m4/performance-baseline.vibon` once every program has
//! completed, and writes nothing if any program fails. `--check` measures only
//! the exact metrics and compares them with the checked-in document, exiting 1
//! with a message for each difference. `--runs N` sets the runs behind each
//! median (11 or more, default 11).

use std::process::ExitCode;

use vibra_conformance::baseline::{
    BASELINE_PATH, Baseline, MIN_RUNS, differences, measure, measure_exact,
};
use vibra_conformance::workspace_root;

fn main() -> ExitCode {
    let mut write = false;
    let mut check = false;
    let mut runs = MIN_RUNS;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--write" => write = true,
            "--check" => check = true,
            "--runs" => match args.next().and_then(|value| value.parse().ok()) {
                Some(value) => runs = value,
                None => return usage("--runs takes a number"),
            },
            other => return usage(&format!("unknown argument `{other}`")),
        }
    }
    let root = workspace_root();
    let path = root.join(BASELINE_PATH);
    if check {
        return check_exact(&root, &path);
    }
    let text = match measure(&root, runs).and_then(|baseline| baseline.render()) {
        Ok(text) => text,
        Err(message) => {
            eprintln!("perf-baseline: {message}");
            return ExitCode::from(1);
        }
    };
    if write {
        if let Err(error) = std::fs::write(&path, &text) {
            eprintln!("perf-baseline: {}: {error}", path.display());
            return ExitCode::from(1);
        }
        println!("wrote {BASELINE_PATH}");
    } else {
        print!("{text}");
    }
    ExitCode::SUCCESS
}

fn check_exact(root: &std::path::Path, path: &std::path::Path) -> ExitCode {
    let recorded = std::fs::read_to_string(path)
        .map_err(|error| format!("{}: {error}", path.display()))
        .and_then(|text| Baseline::parse(&text));
    let recorded = match recorded {
        Ok(baseline) => baseline,
        Err(message) => {
            eprintln!("perf-baseline: {message}");
            return ExitCode::from(1);
        }
    };
    let measured = match measure_exact(root) {
        Ok(measured) => measured,
        Err(message) => {
            eprintln!("perf-baseline: {message}");
            return ExitCode::from(1);
        }
    };
    let found = differences(&recorded.exact, &measured);
    for line in &found {
        eprintln!("perf-baseline: {line}");
    }
    if found.is_empty() {
        println!("the exact metrics match {BASELINE_PATH}");
        ExitCode::SUCCESS
    } else {
        eprintln!(
            "perf-baseline: regenerate {BASELINE_PATH} with `perf-baseline --write` in the same change"
        );
        ExitCode::from(1)
    }
}

fn usage(message: &str) -> ExitCode {
    eprintln!("perf-baseline: {message}");
    eprintln!("usage: perf-baseline [--write] [--check] [--runs N]");
    ExitCode::from(2)
}

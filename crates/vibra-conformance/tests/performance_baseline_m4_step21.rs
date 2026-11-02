//! The performance baseline (milestone 4 Step 21; ledger D29.1).
//!
//! The checked-in `docs/roadmap/milestone-4/performance-baseline.vibon` records
//! the exact metrics of ten programs in both backends. This test regenerates
//! them and fails on any difference, naming the program and the metric, so a
//! change that moves one is reviewed as a baseline change: regenerate the file
//! with `cargo run --release -p vibra-conformance --bin perf-baseline --
//! --write` in the same change. The wall-time medians of the document are
//! information and take no part in any comparison here.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs
)]

use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use vibra_conformance::baseline::{
    BASELINE_PATH, Baseline, Exact, MIN_RUNS, PROGRAMS, Program, differences,
    measure_exact, measure_program, repository_root,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn recorded() -> Baseline {
    let path = repository_root().join(BASELINE_PATH);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    Baseline::parse(&text)
        .unwrap_or_else(|message| panic!("{BASELINE_PATH}: {message}"))
}

/// One measurement of every program, shared by the tests that need one.
fn measured() -> &'static Vec<Exact> {
    static MEASURED: OnceLock<Vec<Exact>> = OnceLock::new();
    MEASURED.get_or_init(|| {
        measure_exact(&repository_root()).unwrap_or_else(|message| panic!("{message}"))
    })
}

#[test]
fn the_checked_in_exact_metrics_equal_a_regeneration() {
    let found = differences(&recorded().exact, measured());
    assert!(
        found.is_empty(),
        "the exact metrics moved:\n  {}\nregenerate {BASELINE_PATH} in the same change with `cargo run --release -p vibra-conformance --bin perf-baseline -- --write`",
        found.join("\n  ")
    );
}

#[test]
fn two_consecutive_measurements_give_identical_exact_metrics() {
    let second = measure_exact(&repository_root()).expect("a second measurement");
    assert_eq!(&second, measured());
}

#[test]
fn the_document_names_the_programs_of_the_list_in_order() {
    let baseline = recorded();
    let listed = PROGRAMS
        .iter()
        .map(|program| program.name)
        .collect::<Vec<_>>();
    let exact = baseline
        .exact
        .iter()
        .map(|entry| entry.program.as_str())
        .collect::<Vec<_>>();
    let medians = baseline
        .informational
        .medians
        .iter()
        .map(|entry| entry.program.as_str())
        .collect::<Vec<_>>();
    assert_eq!(exact, listed);
    assert_eq!(medians, listed);
    assert_eq!(listed.len(), 10);
}

#[test]
fn the_medians_are_over_at_least_eleven_runs_and_name_their_machine() {
    let info = recorded().informational;
    assert!(
        usize::try_from(info.runs).expect("a small count") >= MIN_RUNS,
        "{} runs",
        info.runs
    );
    let env = info.environment;
    for (label, text) in [
        ("os", &env.os),
        ("os-version", &env.os_version),
        ("arch", &env.arch),
        ("cpu", &env.cpu),
        ("rust", &env.rust),
        ("wasmtime", &env.wasmtime),
        ("profile", &env.profile),
    ] {
        assert!(!text.is_empty(), "{label} is empty");
    }
    assert!(env.logical_cpus > 0);
}

#[test]
fn the_document_is_canonical_vibon_and_the_decoder_round_trips_it() {
    let path = repository_root().join(BASELINE_PATH);
    let text = std::fs::read_to_string(&path).expect("the baseline");
    let formatted = vibra_fmt::format_source("performance-baseline.vibon", &text)
        .expect("the baseline formats");
    assert_eq!(formatted, text, "the formatter changes {BASELINE_PATH}");
    let decoded = Baseline::parse(&text).expect("the baseline decodes");
    assert_eq!(decoded.render().expect("a rendering"), text);
}

#[test]
fn a_moved_metric_is_reported_with_the_program_and_the_metric() {
    let mut moved = measured().clone();
    moved[3].wasm.fuel += 1;
    let found = differences(&recorded().exact, &moved);
    assert_eq!(
        found,
        vec![
            "program `tail-loop`: wasm fuel: recorded ".to_owned()
                + &recorded().exact[3].wasm.fuel.to_string()
                + ", measured "
                + &moved[3].wasm.fuel.to_string()
        ]
    );
}

#[test]
fn a_missing_or_extra_program_fails() {
    let mut fewer = measured().clone();
    let gone = fewer.remove(4);
    let found = differences(&recorded().exact, &fewer);
    assert_eq!(
        found,
        vec![format!(
            "program `{}` is recorded but no longer measured",
            gone.program
        )]
    );
    let mut recorded_short = recorded().exact;
    recorded_short.remove(4);
    let found = differences(&recorded_short, measured());
    assert_eq!(
        found,
        vec![format!(
            "program `{}` is measured but missing from the recorded baseline",
            gone.program
        )]
    );
}

/// A root holding one source file, which removes itself.
struct Scratch {
    root: PathBuf,
}

impl Scratch {
    fn with(path: &str, text: &str) -> Self {
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir()
            .join(format!("vibra-step21-{}-{serial}", std::process::id()));
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().expect("a parent")).expect("directory");
        std::fs::write(file, text).expect("source");
        Self { root }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn a_changed_program_moves_its_metrics_and_the_comparison_names_them() {
    // The tail loop at a thousand iterations instead of a million.
    let original = std::fs::read_to_string(
        repository_root().join("examples/m4-baseline/programs/tail-loop.vib"),
    )
    .expect("the tail loop");
    assert!(original.contains("1000000u64"));
    let scratch =
        Scratch::with("tail-loop.vib", &original.replace("1000000u64", "1000u64"));
    let program = Program::source("tail-loop", "perturbed", "tail-loop.vib");
    let changed = measure_program(&scratch.root, &program).expect("the program runs");
    let mut after = measured().clone();
    after[3] = changed;
    let found = differences(&recorded().exact, &after);
    assert!(found.len() >= 6, "{found:?}");
    for metric in [
        "interpreter allocations",
        "interpreter reductions",
        "wasm allocations",
        "wasm fuel",
    ] {
        assert!(
            found
                .iter()
                .any(|line| line.starts_with("program `tail-loop`:")
                    && line.contains(metric)),
            "no difference reports {metric}: {found:?}"
        );
    }
}

#[test]
fn a_program_that_does_not_complete_is_an_error_naming_it() {
    // An entry that returns `err` is a program failure, not a measurement.
    let scratch = Scratch::with(
        "fails.vib",
        "(defn main () (result u64 u64) (result.err 1u64))\n",
    );
    let program = Program::source("fails", "returns err", "fails.vib");
    let error = measure_program(&scratch.root, &program).expect_err("no measurement");
    assert!(error.contains("`fails`"), "{error}");
    // A source that is not accepted fails before any run.
    let scratch = Scratch::with("rejected.vib", "(defn main () u64 nothing)\n");
    let program = Program::source("rejected", "does not check", "rejected.vib");
    let error = measure_program(&scratch.root, &program).expect_err("no measurement");
    assert!(error.contains("`rejected`"), "{error}");
}

#[test]
fn the_largest_programs_complete_in_bounded_time_in_a_debug_build() {
    // The nested value of ledger D14.2 took about seven minutes in a debug
    // build before the interpreter shared the parts of a compound value; it now
    // takes a fraction of a second. The bound is generous so that a slow machine
    // never fails it, and far below the old cost.
    let nested = PROGRAMS
        .iter()
        .find(|program| program.name == "nested-value-5000")
        .expect("the nested value");
    let start = Instant::now();
    let exact = measure_program(&repository_root(), nested).expect("it completes");
    assert!(
        start.elapsed() < Duration::from_secs(120),
        "{:?}",
        start.elapsed()
    );
    // The tail loop of a million iterations completes under the runner's memory
    // limit with a constant live arena.
    let recorded = recorded();
    let tail = recorded
        .exact
        .iter()
        .find(|entry| entry.program == "tail-loop")
        .expect("the tail loop");
    assert!(tail.wasm.peak_live_bytes < 64 * 1024, "{tail:?}");
    assert!(exact.wasm.peak_live_bytes > 0);
}

#[test]
fn a_module_emitted_for_measurement_computes_what_the_plain_module_does() {
    use vibra_wasm::layout::state;
    use vibra_wasm_run::{MemoryLimit, Outcome, Runner, Started};

    let text = std::fs::read_to_string(
        repository_root().join("examples/m4-baseline/programs/dict-insert.vib"),
    )
    .expect("the dict program");
    let checked = vibra_types::check_source("input.vib", &text);
    let program = checked.program().expect("an accepted program");
    let plain = vibra_wasm::emit(program).expect("lowers");
    let counting = vibra_wasm::emit_measured(program).expect("lowers");
    assert_ne!(plain.bytes(), counting.bytes());
    let runner = Runner::new(MemoryLimit::new(64 * 1024 * 1024)).expect("a runner");
    let mut counters = Vec::new();
    let mut results = Vec::new();
    for module in [&plain, &counting] {
        let Started::Ready(mut instance) =
            runner.start(module.bytes()).expect("starts")
        else {
            panic!("the module fits the limit");
        };
        let Outcome::Completed { result, live_size } =
            instance.call_entry().expect("runs")
        else {
            panic!("the entry completes");
        };
        results.push((format!("{result:?}"), live_size));
        let mut read = |offset: u32| {
            let bytes = instance
                .read_memory_for_tests(offset, 8)
                .expect("the state is in memory");
            u64::from_le_bytes(bytes.try_into().expect("eight bytes"))
        };
        counters.push((read(state::ALLOC_COUNT), read(state::LIVE_PEAK)));
    }
    // The same result and the same live size at the end; a plain module never
    // writes either counter, and a measured one counts.
    assert_eq!(results[0], results[1]);
    assert_eq!(counters[0], (0, 0));
    assert!(counters[1].0 > 0 && counters[1].1 >= results[1].1);
}

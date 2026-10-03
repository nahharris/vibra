//! M4 Step 3: deep non-tail recursion in the reference interpreter is bounded
//! only by memory, and exhausting memory is the host event
//! `@runtime.memory-exhausted` (`docs/spec/06-runtime.md`, "Activations and
//! memory" and "Reclamation").

#![allow(clippy::expect_used, clippy::indexing_slicing, missing_docs)]

use vibra_conformance::{
    CaseManifest, CaseObservation, ConformanceProfile, ConformanceRunner, Corpus,
    HandlerError, ProfileDispatcher, ProfileHandler,
};
use vibra_interp::{Interpreter, MemoryBudget, RuntimeError};
use vibra_ir::Value;
use vibra_types::check_source;

/// Checks a source and returns its accepted program.
fn checked(source: &str) -> vibra_types::CheckResult {
    let checked = check_source("activations.vib", source);
    assert!(checked.accepted(), "{:?}\n{source}", checked.diagnostics());
    checked
}

/// `(depth n)` is an operand of a checked addition, so it is never a tail
/// call: `n + 1` activations are live at the base case.
fn depth_source(depth: u64) -> String {
    format!(
        "(defn main () u64 (depth {depth}u64))\n\n\
(defn depth (remaining u64) u64\n  (if (u64.equal remaining 0u64)\n    0u64\n    (match (u64.add-checked (depth (lower remaining)) 1u64)\n      (result.ok total) total\n      (result.err -) 0u64)))\n\n\
(defn lower (count u64) u64\n  (match (u64.sub-checked count 1u64)\n    (result.ok value) value\n    (result.err -) 0u64))\n"
    )
}

/// Non-tail recursion that never reaches a base case.
const FOREVER: &str = "(defn main () u64 (forever 0u64))\n\n(defn forever (count u64) u64\n  (match (u64.add-checked (forever count) 1u64)\n    (result.ok total) total\n    (result.err -) 0u64))\n";

const ADD_ONE: &str = "(defn add-one (count u64) u64\n  (match (u64.add-checked count 1u64)\n    (result.ok value) value\n    (result.err -) 0u64))\n\n(defn lower (count u64) u64\n  (match (u64.sub-checked count 1u64)\n    (result.ok value) value\n    (result.err -) 0u64))\n";

fn run_u64(source: &str) -> vibra_interp::Execution {
    let checked = checked(source);
    Interpreter::run(checked.program().expect("program")).expect("execution")
}

// ---------------------------------------------------------------------------
// Positive: the depth is bounded only by memory.
// ---------------------------------------------------------------------------

#[test]
fn non_tail_recursion_a_hundred_thousand_deep_completes_with_its_result() {
    let execution = run_u64(&depth_source(100_000));
    assert_eq!(execution.value(), Some(&Value::U64(100_000)));
    assert_eq!(
        execution.tail_transfer_count(),
        1,
        "only main's call is a tail call"
    );
    assert!(
        execution.max_activation_depth() > 100_000,
        "every `depth` activation is live at the base case, got {}",
        execution.max_activation_depth()
    );
    assert!(execution.audit_trace().is_empty());
}

#[test]
fn mutual_non_tail_recursion_is_bounded_only_by_memory() {
    let source = format!(
        "(defn main () u64 (even 40000u64))\n\n\
(defn even (n u64) u64\n  (if (u64.equal n 0u64)\n    0u64\n    (add-one (odd (lower n)))))\n\n\
(defn odd (n u64) u64\n  (if (u64.equal n 0u64)\n    0u64\n    (add-one (even (lower n)))))\n\n{ADD_ONE}"
    );
    let execution = run_u64(&source);
    assert_eq!(execution.value(), Some(&Value::U64(40_000)));
    assert!(execution.max_activation_depth() > 40_000);
}

#[test]
fn non_tail_recursion_through_every_callee_kind_completes() {
    let sources = [
        // A function value held in a parameter.
        format!(
            "(defn main () u64 (again 20000u64))\n(defn again (count u64) u64 (grow count again))\n(defn grow (count u64 next (fn (u64) u64)) u64\n  (if (u64.equal count 0u64) 0u64 (add-one (next (lower count)))))\n{ADD_ONE}"
        ),
        // A closure and the function it captures.
        format!(
            "(defn main () u64 (spin 20000u64))\n(defn spin (count u64) u64\n  (if (u64.equal count 0u64) 0u64 (add-one ((make spin) (lower count)))))\n(defn make (back (fn (u64) u64)) (fn (u64) u64) (lambda (count u64) u64 (back count)))\n{ADD_ONE}"
        ),
        // A contract member called through an interface value.
        format!(
            "(defn main () u64 (walker.walk (as walker (counter left: 20000u64))))\n(defint walker (defn walk (value self) u64))\n(deftype counter (record left u64)\n  (impl walker\n    (defn walk (value self) u64\n      (if (u64.equal (value @left) 0u64)\n        0u64\n        (add-one (walker.walk (as walker (counter left: (lower (value @left))))))))))\n{ADD_ONE}"
        ),
    ];
    for source in sources {
        let execution = run_u64(&source);
        assert_eq!(execution.value(), Some(&Value::U64(20_000)), "{source}");
        assert!(execution.max_activation_depth() > 20_000, "{source}");
    }
}

/// A value nested `depth` levels deep, built by non-tail recursion and then
/// discarded: releasing it must not use host stack per level.
fn nested_value_source(depth: u64) -> String {
    format!(
        "(deftype nat (record prev (array nat)))\n\n\
(defn main () u64 (let - (build {depth}u64)) 7u64)\n\n\
(defn build (count u64) nat\n  (if (u64.equal count 0u64)\n    (nat prev: (array.of))\n    (nat prev: (array.of (build (lower count))))))\n\n\
(defn lower (count u64) u64\n  (match (u64.sub-checked count 1u64)\n    (result.ok value) value\n    (result.err -) 0u64))\n"
    )
}

#[test]
fn a_value_nested_five_thousand_and_a_hundred_thousand_deep_is_built_and_dropped() {
    for depth in [5_000, 100_000] {
        let execution = run_u64(&nested_value_source(depth));
        assert_eq!(execution.value(), Some(&Value::U64(7)), "depth {depth}");
    }
}

#[test]
fn a_deeply_nested_entry_result_is_observed_and_released_without_host_stack() {
    // The entry's result is the nested value itself, which is encoded and
    // released after the run on the caller's own stack.
    let source = "(deftype nat (record prev (array nat)))\n\n(defn main () nat (build 20000u64))\n\n(defn build (count u64) nat\n  (if (u64.equal count 0u64)\n    (nat prev: (array.of))\n    (nat prev: (array.of (build (lower count))))))\n\n(defn lower (count u64) u64\n  (match (u64.sub-checked count 1u64)\n    (result.ok value) value\n    (result.err -) 0u64))\n";
    let execution = run_u64(source);
    let observation = execution.canonical_result();
    assert!(
        observation.starts_with("(record type: @nat value: "),
        "{}",
        &observation[..60.min(observation.len())]
    );
    assert_eq!(observation.matches("kind: @record").count(), 20_001);
}

// ---------------------------------------------------------------------------
// Negative: exhaustion is the host event, never a trap or a result.
// ---------------------------------------------------------------------------

#[test]
fn recursion_with_no_base_case_exhausts_a_small_budget() {
    let checked = checked(FOREVER);
    let budget = MemoryBudget::new(4 * 1024 * 1024);
    let error =
        Interpreter::run_with_budget(checked.program().expect("program"), budget)
            .expect_err("no base case must exhaust memory");
    assert_eq!(error, RuntimeError::MemoryExhausted { budget });
    assert!(error.is_host_event());
    assert_eq!(error.program_trap(), None, "a host event is never a trap");
    let diagnostic = error.host_diagnostic().expect("registered host diagnostic");
    assert_eq!(
        diagnostic.code(),
        vibra_diagnostics::DiagnosticCode::RuntimeMemoryExhausted
    );
    assert_eq!(
        diagnostic.primary_span(),
        vibra_diagnostics::ByteSpan::empty_at(0)
    );
    assert_eq!(diagnostic.source_id(), None);
}

#[test]
fn memory_exhaustion_is_reported_by_the_default_budget_too() {
    let checked = checked(FOREVER);
    let error = Interpreter::run(checked.program().expect("program"))
        .expect_err("no base case must exhaust the default budget");
    assert_eq!(
        error,
        RuntimeError::MemoryExhausted {
            budget: MemoryBudget::DEFAULT
        }
    );
}

// ---------------------------------------------------------------------------
// Boundary.
// ---------------------------------------------------------------------------

#[test]
fn the_budget_exactly_at_a_programs_need_completes_and_one_frame_under_does_not() {
    let run_at = |depth: u64, budget: MemoryBudget| {
        let source = depth_source(depth);
        let checked = checked(&source);
        Interpreter::run_with_budget(checked.program().expect("program"), budget)
    };
    // The need is what the first run observed; the per-activation cost is the
    // difference two depths make.
    let large =
        Interpreter::run(checked(&depth_source(3_000)).program().expect("program"))
            .expect("execution");
    let small =
        Interpreter::run(checked(&depth_source(2_000)).program().expect("program"))
            .expect("execution");
    let need = large.peak_memory_bytes();
    let per_activation = (need - small.peak_memory_bytes()) / 1_000;
    assert!(per_activation > 0, "an activation costs memory");

    assert_eq!(
        run_at(3_000, MemoryBudget::new(need))
            .map(|execution| execution.value().cloned()),
        Ok(Some(Value::U64(3_000))),
        "a budget exactly at the need completes"
    );
    for under in [1, per_activation] {
        assert_eq!(
            run_at(3_000, MemoryBudget::new(need - under)).map(|_| ()),
            Err(RuntimeError::MemoryExhausted {
                budget: MemoryBudget::new(need - under)
            }),
            "a budget {under} bytes under the need is exhausted"
        );
    }
}

#[test]
fn a_tail_loop_never_approaches_a_small_budget() {
    // Each iteration reuses the one activation, so a hundred thousand of them
    // fit a budget that a few hundred live activations would exceed.
    let source = "(defn main () u64 (spin 100000u64))\n(defn spin (count u64) u64\n  (if (u64.equal count 0u64) 0u64 (spin (lower count))))\n(defn lower (count u64) u64\n  (match (u64.sub-checked count 1u64)\n    (result.ok value) value\n    (result.err -) 0u64))\n";
    let checked = checked(source);
    let budget = MemoryBudget::new(64 * 1024);
    let execution =
        Interpreter::run_with_budget(checked.program().expect("program"), budget)
            .expect("a tail loop holds one activation");
    assert_eq!(execution.value(), Some(&Value::U64(0)));
    assert!(execution.peak_memory_bytes() < 64 * 1024 / 4);
    assert!(execution.tail_transfer_count() >= 100_000);
}

#[test]
fn a_tail_loop_that_builds_fresh_values_runs_in_bounded_memory() {
    // The earlier arrays are released as the loop goes: only the live value
    // counts against the budget, however many were allocated in total.
    let source = "(defn main () u64 (spin 20000u64 (array.of 1u64 2u64 3u64)))\n(defn spin (count u64 held (array u64)) u64\n  (if (u64.equal count 0u64) (array.length held) (spin (lower count) (array.of count count count))))\n(defn lower (count u64) u64\n  (match (u64.sub-checked count 1u64)\n    (result.ok value) value\n    (result.err -) 0u64))\n";
    let checked = checked(source);
    let execution = Interpreter::run_with_budget(
        checked.program().expect("program"),
        MemoryBudget::new(256 * 1024),
    )
    .expect("live data stays bounded");
    assert_eq!(execution.value(), Some(&Value::U64(3)));
}

#[test]
fn a_growing_value_exhausts_the_budget_through_allocation_not_recursion() {
    // One activation, but the live array doubles every iteration.
    let source = "(defn main () u64 (grow 64u64 (array.of 1u64)))\n(defn grow (count u64 held (array u64)) u64\n  (if (u64.equal count 0u64) (array.length held) (grow (lower count) (array.concat held held))))\n(defn lower (count u64) u64\n  (match (u64.sub-checked count 1u64)\n    (result.ok value) value\n    (result.err -) 0u64))\n";
    let checked = checked(source);
    let budget = MemoryBudget::new(8 * 1024 * 1024);
    let error =
        Interpreter::run_with_budget(checked.program().expect("program"), budget)
            .expect_err("a doubling array exhausts memory");
    assert_eq!(error, RuntimeError::MemoryExhausted { budget });
}

// ---------------------------------------------------------------------------
// Recovery.
// ---------------------------------------------------------------------------

#[test]
fn a_run_after_a_host_event_in_the_same_process_behaves_normally() {
    let exhausting = checked(FOREVER);
    let budget = MemoryBudget::new(2 * 1024 * 1024);
    for _ in 0..2 {
        assert!(
            Interpreter::run_with_budget(
                exhausting.program().expect("program"),
                budget
            )
            .is_err()
        );
        let execution = run_u64(&depth_source(1_000));
        assert_eq!(execution.value(), Some(&Value::U64(1_000)));
        assert!(execution.peak_memory_bytes() < budget.bytes());
    }
}

// ---------------------------------------------------------------------------
// `expect.host_event` in the conformance manifest.
// ---------------------------------------------------------------------------

/// A manifest of `operation` with the inputs that operation takes.
fn manifest(operation: &str, expect: &str) -> String {
    let inputs = match operation {
        "workspace-run" | "workspace-check" | "workspace-test" => {
            "project = \"tree/project.vibon\"\ntree = \"tree\""
        }
        _ => "source = \"input.vib\"",
    };
    let profile = match operation {
        "reader" => "reader-v1",
        "type-check" | "workspace-check" => "static-v1",
        "format" => "tooling-v1",
        _ => "interpreter-v1",
    };
    format!(
        "id = \"V1-RUNTIME-host-event-manifest\"\nrule = \"V1-RUNTIME\"\nprofile = \"{profile}\"\noperation = \"{operation}\"\n\n[inputs]\n{inputs}\n\n[expect]\n{expect}\n"
    )
}

#[test]
fn host_event_is_valid_on_interpret_and_workspace_run() {
    for operation in ["interpret", "workspace-run"] {
        let parsed = CaseManifest::from_str(&manifest(
            operation,
            "accepted = true\nhost_event = \"@runtime.memory-exhausted\"",
        ))
        .expect("host_event is valid here");
        assert_eq!(
            parsed.expectations.host_event.as_deref(),
            Some("@runtime.memory-exhausted")
        );
        assert!(parsed.expectations.interpreter.is_none());
    }
}

#[test]
fn host_event_is_rejected_where_no_host_event_can_end_the_case() {
    for operation in ["reader", "type-check", "workspace-check", "workspace-test"] {
        let error = CaseManifest::from_str(&manifest(
            operation,
            "accepted = true\nhost_event = \"@runtime.memory-exhausted\"",
        ))
        .expect_err("host_event is only for interpret and workspace-run");
        assert!(
            error.to_string().contains("host_event"),
            "{operation}: {error}"
        );
    }
}

#[test]
fn host_event_is_exclusive_with_result_and_trace_snapshots() {
    for snapshots in [
        "interpreter = { result = \"result.vibon\" }",
        "interpreter = { result = \"result.vibon\", audit_trace = \"audit.vibon\" }",
        "interpreter = { audit_trace = \"audit.vibon\" }",
        "wasm = { result = \"result.vibon\", audit_trace = \"audit.vibon\" }",
    ] {
        let error = CaseManifest::from_str(&manifest(
            "interpret",
            &format!(
                "accepted = true\nhost_event = \"@runtime.memory-exhausted\"\n{snapshots}"
            ),
        ))
        .expect_err("a case that ends in a host event records no snapshot");
        assert!(
            error.to_string().contains("host_event"),
            "{snapshots}: {error}"
        );
    }
}

#[test]
fn host_event_names_only_the_registered_event_atom() {
    for event in [
        "@runtime.invalid-checked-program",
        "@runtime.host-stack-exhausted",
        "runtime.memory-exhausted",
        "memory-exhausted",
        "",
    ] {
        let error = CaseManifest::from_str(&manifest(
            "interpret",
            &format!("accepted = true\nhost_event = \"{event}\""),
        ))
        .expect_err("only @runtime.memory-exhausted is a host event");
        assert!(
            error.to_string().contains("host_event"),
            "{event:?}: {error}"
        );
    }
}

#[test]
fn a_case_that_ends_in_a_host_event_is_accepted_by_the_checker() {
    let error = CaseManifest::from_str(&manifest(
        "interpret",
        "accepted = false\nhost_event = \"@runtime.memory-exhausted\"\n\n[[expect.diagnostics]]\ncode = \"@type.numeric-out-of-range\"\nlevel = \"@error\"\nsource = \"input.vib\"\nspan = [16, 21]",
    ))
    .expect_err("a rejected program never runs, so it cannot end in a host event");
    assert!(error.to_string().contains("host_event"), "{error}");
}

struct FixedHandler(CaseObservation);

impl ProfileHandler for FixedHandler {
    fn run(
        &self,
        _case: &vibra_conformance::Case,
    ) -> Result<CaseObservation, HandlerError> {
        Ok(self.0.clone())
    }
}

fn run_fixed_case(
    expect: &str,
    observation: CaseObservation,
) -> vibra_conformance::CaseStatus {
    let root = std::env::temp_dir().join(format!(
        "vibra-host-event-{}-{}",
        std::process::id(),
        expect.len() + usize::from(observation.host_event.is_some())
    ));
    let directory = root.join("V1-RUNTIME-host-event-run");
    std::fs::create_dir_all(&directory).expect("case directory");
    std::fs::write(
        directory.join("case.toml"),
        format!(
            "id = \"V1-RUNTIME-host-event-run\"\nrule = \"V1-RUNTIME\"\nprofile = \"interpreter-v1\"\noperation = \"interpret\"\n\n[inputs]\nsource = \"input.vib\"\n\n[expect]\n{expect}\n"
        ),
    )
    .expect("manifest");
    std::fs::write(directory.join("input.vib"), "(defn main () void (do))\n")
        .expect("input");
    std::fs::write(
        directory.join("result.vibon"),
        "(record type: @void value: void)\n",
    )
    .expect("result");
    let corpus = Corpus::discover(&root).expect("corpus");
    let report = ConformanceRunner::new(
        ProfileDispatcher::new()
            .with_handler(ConformanceProfile::InterpreterV1, FixedHandler(observation)),
    )
    .run(&corpus);
    let _ = std::fs::remove_dir_all(&root);
    report.cases()[0].status.clone()
}

#[test]
fn the_runner_compares_the_host_event_a_case_expects_with_the_one_observed() {
    use vibra_conformance::CaseStatus;
    let event = || CaseObservation {
        accepted: true,
        host_event: Some("@runtime.memory-exhausted".to_owned()),
        ..CaseObservation::default()
    };
    let completed = || CaseObservation {
        accepted: true,
        interpreter: Some(vibra_conformance::ExecutionObservation {
            result: Some("(record type: @void value: void)\n".to_owned()),
            audit_trace: Vec::new(),
        }),
        ..CaseObservation::default()
    };
    let expects_event = "accepted = true\nhost_event = \"@runtime.memory-exhausted\"";
    let expects_result = "accepted = true\ninterpreter = { result = \"result.vibon\" }";

    assert_eq!(run_fixed_case(expects_event, event()), CaseStatus::Passed);
    assert_eq!(
        run_fixed_case(expects_result, completed()),
        CaseStatus::Passed
    );
    // The program completed, but the case expected the host event.
    assert!(matches!(
        run_fixed_case(expects_event, completed()),
        CaseStatus::Failed { reason } if reason.contains("host event")
    ));
    // The program ended in a host event the case did not expect.
    assert!(matches!(
        run_fixed_case(expects_result, event()),
        CaseStatus::Failed { reason } if reason.contains("host event")
    ));
}

//! Focused host and conformance checks for M2 Step 9 tail execution.

#![allow(clippy::expect_used, clippy::indexing_slicing, missing_docs)]

use vibra_conformance::{
    CaseStatus, ConformanceProfile, ConformanceRunner, Corpus, InterpreterV1Handler,
    ProfileDispatcher,
};
use vibra_types::{check_bootstrap_text_import, check_source, load_stdlib};

fn binary_counter_source(bit_count: usize) -> String {
    let names = (0..bit_count)
        .map(|index| format!("b{index}"))
        .collect::<Vec<_>>();
    fn increment_branch(index: usize, names: &[String]) -> String {
        if index == names.len() {
            return "0i32".to_owned();
        }
        let then_branch = increment_branch(index.saturating_add(1), names);
        let arguments = names
            .iter()
            .enumerate()
            .map(|(bit, name)| {
                if bit < index {
                    "false".to_owned()
                } else if bit == index {
                    "true".to_owned()
                } else {
                    name.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        let else_branch = format!("(counter {arguments})");
        format!("(if {} {then_branch} {else_branch})", names[index])
    }
    let initial = std::iter::repeat_n("false", bit_count)
        .collect::<Vec<_>>()
        .join(" ");
    let parameters = names
        .iter()
        .map(|name| format!("{name} bool"))
        .collect::<Vec<_>>()
        .join(" ");
    let body = increment_branch(0, &names);
    format!(
        "(defn answer () i32 (counter {initial}))\n(defn counter ({parameters}) i32 {body})\n"
    )
}

#[test]
fn direct_and_mutual_tail_transfers_reuse_one_activation() {
    let source = r#"
(defn answer () i32 (first true))
(defn first (flag bool) i32
  (if flag (second false) 1i32))
(defn second (flag bool) i32
  (if flag (first false) 2i32))
"#;
    let checked = check_source("tail-mutual.vib", source);
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    let program = checked.program().expect("program");
    let execution = vibra_interp::run(program).expect("execution");
    assert_eq!(execution.value(), Some(&vibra_ir::Value::I32(2)));
    assert_eq!(execution.tail_transfer_count(), 2);
    assert_eq!(execution.max_activation_depth(), 1);
    assert!(execution.audit_trace().is_empty());
}

#[test]
fn mixed_named_and_closure_tail_calls_reuse_the_selected_target() {
    for (condition, expected, transfers, depth) in
        [("true", 1, 1, 1), ("false", 2, 1, 1)]
    {
        let source = format!(
            "\
(defn answer () i32 ((if {condition} leaf (lambda () i32 2i32))))
(defn leaf () i32 1i32)
"
        );
        let checked = check_source("tail-mixed-callable.vib", &source);
        assert!(checked.accepted(), "{:?}", checked.diagnostics());
        let execution =
            vibra_interp::run(checked.program().expect("program")).expect("execution");
        assert_eq!(execution.value(), Some(&vibra_ir::Value::I32(expected)));
        assert_eq!(execution.tail_transfer_count(), transfers);
        assert_eq!(execution.max_activation_depth(), depth);
    }
}

#[test]
fn returned_function_targets_reuse_the_current_activation() {
    let source = r#"
(defn answer () i32 ((make)))
(defn make () (fn () i32) leaf)
(defn leaf () i32 1i32)
"#;
    let checked = check_source("tail-returned-target.vib", source);
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    let execution =
        vibra_interp::run(checked.program().expect("program")).expect("execution");
    assert_eq!(execution.value(), Some(&vibra_ir::Value::I32(1)));
    assert_eq!(execution.tail_transfer_count(), 1);
    assert_eq!(execution.max_activation_depth(), 2);
}

#[test]
fn parameter_targets_reuse_the_current_activation() {
    let source = r#"
(defn answer () i32 (dispatch leaf))
(defn dispatch (f (fn () i32)) i32 (f))
(defn leaf () i32 1i32)
"#;
    let checked = check_source("tail-parameter-target.vib", source);
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    let execution =
        vibra_interp::run(checked.program().expect("program")).expect("execution");
    assert_eq!(execution.value(), Some(&vibra_ir::Value::I32(1)));
    assert_eq!(execution.tail_transfer_count(), 2);
    assert_eq!(execution.max_activation_depth(), 1);
}

#[test]
fn identity_returned_targets_remain_bounded_through_direct_and_local_calls() {
    let direct = r#"
(defn answer () i32 ((identity leaf)))
(defn identity (f (fn () i32)) (fn () i32) f)
(defn leaf () i32 1i32)
"#;
    let checked = check_source("tail-identity-direct.vib", direct);
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    let execution =
        vibra_interp::run(checked.program().expect("program")).expect("execution");
    assert_eq!(execution.value(), Some(&vibra_ir::Value::I32(1)));
    assert_eq!(execution.tail_transfer_count(), 1);

    let local = r#"
(defn answer () i32
  (let selected (identity leaf) (selected)))
(defn identity (f (fn () i32)) (fn () i32) f)
(defn leaf () i32 1i32)
"#;
    let checked = check_source("tail-identity-local.vib", local);
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    let execution =
        vibra_interp::run(checked.program().expect("program")).expect("execution");
    assert_eq!(execution.value(), Some(&vibra_ir::Value::I32(1)));
    assert_eq!(execution.tail_transfer_count(), 1);

    let nested = r#"
(defn answer () i32 ((identity (identity leaf))))
(defn identity (f (fn () i32)) (fn () i32) f)
(defn leaf () i32 1i32)
"#;
    let checked = check_source("tail-identity-nested.vib", nested);
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    let execution =
        vibra_interp::run(checked.program().expect("program")).expect("execution");
    assert_eq!(execution.value(), Some(&vibra_ir::Value::I32(1)));
    assert_eq!(execution.tail_transfer_count(), 1);
}

#[test]
fn mixed_source_and_external_tail_candidates_reuse_only_source_targets() {
    let verification = load_stdlib().expect("bootstrap provenance");
    for (condition, expected, transfers, depth) in
        [("true", 99, 1, 1), ("false", 1, 0, 2)]
    {
        let source = format!(
            "\
(import text @std.text)
(defn answer () u64
  ((if {condition} local-length text.length) \"x\"))
(defn local-length (value str) u64 99u64)
"
        );
        let checked = check_bootstrap_text_import(
            &verification,
            "tail-mixed-external.vib",
            &source,
        );
        assert!(checked.accepted(), "{:?}", checked.diagnostics());
        let execution =
            vibra_interp::run(checked.program().expect("program")).expect("execution");
        assert_eq!(execution.value(), Some(&vibra_ir::Value::U64(expected)));
        assert_eq!(execution.tail_transfer_count(), transfers);
        assert_eq!(execution.max_activation_depth(), depth);
    }
}

#[test]
fn lambda_activations_reuse_for_calls_through_captured_values() {
    let source = r#"
(defn answer () i32
  (let f leaf ((lambda () i32 (f)))))
(defn leaf () i32 1i32)
"#;
    let checked = check_source("tail-lambda-activation.vib", source);
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    let program = checked.program().expect("program");
    // The call of the `lambda` and the call of `f` inside it are both tail
    // calls, so the one activation of `answer` runs all three bodies.
    assert_eq!(program.canonical_vibon().matches("tail: true").count(), 2);
    let execution = vibra_interp::run(program).expect("execution");
    assert_eq!(execution.value(), Some(&vibra_ir::Value::I32(1)));
    assert_eq!(execution.tail_transfer_count(), 2);
    assert_eq!(execution.max_activation_depth(), 1);
}

#[test]
fn returned_external_callables_fall_back_to_ordinary_invocation() {
    let verification = load_stdlib().expect("bootstrap provenance");
    let source = r#"
(import text @std.text)
(defn answer () u64 ((make) "x"))
(defn make () (fn (str) u64) text.length)
"#;
    let checked = check_bootstrap_text_import(
        &verification,
        "tail-returned-external.vib",
        source,
    );
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    let execution =
        vibra_interp::run(checked.program().expect("program")).expect("execution");
    assert_eq!(execution.value(), Some(&vibra_ir::Value::U64(1)));
    assert_eq!(execution.tail_transfer_count(), 0);
}

#[test]
fn unknown_callable_branches_keep_known_recursive_and_external_fallbacks() {
    let verification = load_stdlib().expect("bootstrap provenance");
    for (condition, expected, transfers) in [("true", 99, 1), ("false", 1, 0)] {
        let source = format!(
            "\
(import text @std.text)
(defn answer () u64
  ((if {condition} (make) text.length) \"x\"))
(defn make () (fn (str) u64) local-length)
(defn local-length (value str) u64 99u64)
"
        );
        let checked = check_bootstrap_text_import(
            &verification,
            "tail-unknown-external-branch.vib",
            &source,
        );
        assert!(checked.accepted(), "{:?}", checked.diagnostics());
        let execution =
            vibra_interp::run(checked.program().expect("program")).expect("execution");
        assert_eq!(execution.value(), Some(&vibra_ir::Value::U64(expected)));
        assert_eq!(execution.tail_transfer_count(), transfers);
    }
}

#[test]
fn non_tail_call_keeps_a_live_caller_activation() {
    let source = r#"
(defn answer () i32
  (let value (leaf) value))
(defn leaf () i32 7i32)
"#;
    let checked = check_source("tail-negative.vib", source);
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    let program = checked.program().expect("program");
    assert!(!program.canonical_vibon().contains("tail: true"));
    let execution = vibra_interp::run(program).expect("execution");
    assert_eq!(execution.value(), Some(&vibra_ir::Value::I32(7)));
    assert_eq!(execution.tail_transfer_count(), 0);
    assert_eq!(execution.max_activation_depth(), 2);
}

#[test]
fn source_tail_counter_keeps_bounded_depth_at_two_workload_sizes() {
    let mut depths = Vec::new();
    for bit_count in [15, 17] {
        let source = binary_counter_source(bit_count);
        let checked = check_source("tail-stress.vib", &source);
        assert!(checked.accepted(), "{:?}", checked.diagnostics());
        let program = checked.program().expect("program");
        let execution = vibra_interp::run(program).expect("execution");
        assert_eq!(execution.value(), Some(&vibra_ir::Value::I32(0)));
        assert_eq!(execution.tail_transfer_count(), 1 << bit_count);
        assert_eq!(execution.max_activation_depth(), 1);
        assert!(execution.audit_trace().is_empty());
        depths.push(execution.max_activation_depth());
    }
    assert_eq!(depths, [1, 1]);
}

#[test]
fn real_interpreter_handler_executes_the_tail_stress_workload() {
    let parent =
        std::fs::canonicalize(std::env::temp_dir()).expect("temporary directory");
    let root = parent.join(format!(
        "vibra-conformance-tail-stress-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temporary stress case");
    let case = root.join("V1-RUNTIME-tail-stress");
    std::fs::create_dir_all(&case).expect("stress case directory");
    std::fs::write(case.join("input.vib"), binary_counter_source(17))
        .expect("stress source");
    std::fs::write(
        case.join("result.vibon"),
        "(record type: @i32 value: 0i32)\n",
    )
    .expect("stress result");
    std::fs::write(
        case.join("audit.vibon"),
        "(record format: @audit-trace.v1 events: (array))\n",
    )
    .expect("stress audit");
    std::fs::write(
        case.join("case.toml"),
        "id = \"V1-RUNTIME-tail-stress\"\nrule = \"V1-RUNTIME\"\nprofile = \"interpreter-v1\"\noperation = \"interpret\"\ndescription = \"Tail stress through the real interpreter handler.\"\n\n[inputs]\nsource = \"input.vib\"\n\n[expect]\naccepted = true\ninterpreter = { result = \"result.vibon\", audit_trace = \"audit.vibon\" }\n",
    )
    .expect("stress manifest");

    let corpus = Corpus::discover(&root).expect("temporary stress corpus");
    let dispatcher = ProfileDispatcher::new()
        .with_handler(ConformanceProfile::InterpreterV1, InterpreterV1Handler);
    let report = ConformanceRunner::new(dispatcher)
        .run_case(corpus.cases().first().expect("stress case loaded"));
    assert_eq!(report.status, CaseStatus::Passed);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn captured_callable_values_survive_repeated_tail_transfers() {
    let source = r#"
(defn answer () i32
  (let value 41i32
    (let captured (lambda () i32 value)
      (loop true true captured))))
(defn loop (first bool second bool f (fn () i32)) i32
  (if first
    (loop false true f)
    (if second
      (loop false false f)
      (f))))
"#;
    let checked = check_source("tail-capture-stress.vib", source);
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    let execution =
        vibra_interp::run(checked.program().expect("program")).expect("execution");
    assert_eq!(execution.value(), Some(&vibra_ir::Value::I32(41)));
    // `answer` to `loop`, two self transfers, and `f` itself.
    assert_eq!(execution.tail_transfer_count(), 4);
    assert_eq!(execution.max_activation_depth(), 1);
}

#[test]
fn callable_return_and_unused_initializer_boundaries_remain_valid() {
    let returned = r#"
(defn answer () i32 ((if false a (make))))
(defn a () i32 1i32)
(defn b () i32 2i32)
(defn make () (fn () i32) b)
"#;
    let checked = check_source("tail-return-boundary.vib", returned);
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    let execution =
        vibra_interp::run(checked.program().expect("program")).expect("execution");
    assert_eq!(execution.value(), Some(&vibra_ir::Value::I32(2)));

    let unused = r#"
(defn answer (flag bool) i32
  ((let unused flag leaf)))
(defn leaf () i32 1i32)
"#;
    let checked = check_source("tail-unused-initializer.vib", unused);
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    assert!(
        checked
            .program()
            .expect("program")
            .canonical_vibon()
            .contains("tail: true")
    );
}

/// The source of a corpus case, so a host test and its case cannot drift.
fn corpus_source(case: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/cases")
        .join(case)
        .join("input.vib");
    std::fs::read_to_string(path).expect("corpus case source")
}

/// Runs a corpus case source and checks the result, the transfer count, and
/// that the activation depth stays far below the interpreter's bound.
fn assert_constant_depth(case: &str, expected: u64, transfers: usize, depth: usize) {
    let checked = check_source(format!("{case}.vib"), &corpus_source(case));
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    let execution =
        vibra_interp::run(checked.program().expect("program")).expect("execution");
    assert_eq!(execution.value(), Some(&vibra_ir::Value::U64(expected)));
    assert_eq!(execution.tail_transfer_count(), transfers);
    assert_eq!(execution.max_activation_depth(), depth);
    assert!(depth < vibra_interp::MAX_ACTIVATION_DEPTH / 100);
    assert!(execution.audit_trace().is_empty());
}

#[test]
fn function_value_parameter_tail_calls_reuse_one_activation() {
    // `main` into `apply`; then `apply` into `countdown` for each of the
    // 100001 counts and `countdown` back into `apply` for each of the 100000
    // that is not zero. `lower` is the only nested activation, and its own
    // checked subtraction is one more.
    assert_constant_depth("V1-RUNTIME-tail-function-value-parameter", 11, 200_002, 3);
}

#[test]
fn closure_tail_calls_reuse_one_activation() {
    // `main` into `spin`; `spin` into the closure for each of 100001 counts
    // and the closure back into `spin` for each of 100000. Making the closure
    // is the only nested call besides `lower`.
    assert_constant_depth("V1-RUNTIME-tail-closure-captured-loop", 13, 200_002, 3);
}

#[test]
fn contract_member_tail_calls_reuse_one_activation() {
    // `main` into the first `walk`, then one transfer per remaining count.
    assert_constant_depth(
        "V1-RUNTIME-tail-contract-member-interface-value",
        17,
        100_001,
        3,
    );
}

#[test]
fn unrelated_function_tail_calls_reuse_one_activation() {
    // `main` into `first`, 100000 self transfers, `first` into `second`, 100000
    // more, and `second` into `last`.
    assert_constant_depth("V1-RUNTIME-tail-unrelated-function-loop", 19, 200_003, 3);
}

#[test]
fn tail_calls_between_modules_reuse_one_activation() {
    let project = "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.execute effects: (array))) dependencies: (dict))\n";
    let parent =
        std::fs::canonicalize(std::env::temp_dir()).expect("temporary directory");
    let root = parent.join(format!("vibra-tail-modules-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src/app")).expect("project directory");
    std::fs::write(root.join("project.vibon"), project).expect("project marker");
    std::fs::write(
        root.join("src/app/main.vib"),
        "(import steps @app.steps)\n(defn execute () void (run 100000u64))\n(defn run (count u64) void (steps.step count run))\n",
    )
    .expect("main module");
    std::fs::write(
        root.join("src/app/steps.vib"),
        "(defn step (count u64 next (fn (u64) void)) void visibility: @public\n  (if (u64.equal count 0u64) (do) (next (lower count))))\n(defn lower (count u64) u64\n  (match (u64.sub-checked count 1u64)\n    (result.ok value) value\n    (result.err -) 0u64))\n",
    )
    .expect("steps module");
    let snapshot = vibra_workspace::WorkspaceSnapshot::load(&root).expect("snapshot");
    let target = snapshot
        .project()
        .project()
        .targets()
        .first()
        .expect("binary target");
    let run = vibra_workspace::semantic::run_target(&snapshot, target);
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(
        run.check().status(),
        vibra_workspace::semantic::CheckStatus::Accepted,
        "{:?}",
        run.check().diagnostics()
    );
    let execution = match run.outcome() {
        Some(vibra_workspace::semantic::RunOutcome::Program(execution)) => {
            Some(execution)
        }
        _ => None,
    }
    .expect("the checked program runs");
    // `execute` into `run`; then `run` into `step` and `step` back into `run`
    // across the module boundary.
    assert_eq!(execution.tail_transfer_count(), 200_002);
    assert_eq!(execution.max_activation_depth(), 3);
}

/// A `def` initializer is not an activation's final expression, so a call at
/// its top level is never a tail transfer; only a `lambda` body inside it has
/// a tail position.
#[test]
fn def_initializer_calls_are_never_tail_transfers() {
    let sources = [
        // A direct call to a source function.
        (
            "(def value i32 (leaf))\n(defn leaf () i32 7i32)\n(defn answer () i32 value)\n",
            7,
        ),
        // A call to a closure.
        (
            "(def value i32 ((lambda () i32 8i32)))\n(defn answer () i32 value)\n",
            8,
        ),
        // A contract member called through an interface value.
        (
            "(defint shape (defn name (item self) i32))\n(deftype point (record x i32)\n  (impl shape (defn name (item self) i32 9i32)))\n(def value i32 (shape.name (as shape (point x: 1i32))))\n(defn answer () i32 value)\n",
            9,
        ),
    ];
    for (source, expected) in sources {
        let checked = check_source("tail-def-initializer.vib", source);
        assert!(checked.accepted(), "{:?}", checked.diagnostics());
        let program = checked.program().expect("program");
        assert!(
            !program.canonical_vibon().contains("tail: true"),
            "{source}"
        );
        let execution = vibra_interp::run(program).expect("execution");
        assert_eq!(execution.value(), Some(&vibra_ir::Value::I32(expected)));
        assert_eq!(execution.tail_transfer_count(), 0);
    }
}

#[test]
fn a_lambda_in_a_def_initializer_tail_calls_in_one_activation() {
    let source = "(def drive (fn (u64) u64) (lambda (count u64) u64 (spin count)))\n(defn answer () u64 (drive 100000u64))\n(defn spin (count u64) u64\n  (if (u64.equal count 0u64) 3u64 (spin (lower count))))\n(defn lower (count u64) u64\n  (match (u64.sub-checked count 1u64)\n    (result.ok value) value\n    (result.err -) 0u64))\n";
    let checked = check_source("tail-def-lambda.vib", source);
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    let execution =
        vibra_interp::run(checked.program().expect("program")).expect("execution");
    assert_eq!(execution.value(), Some(&vibra_ir::Value::U64(3)));
    // `answer` into the closure, the closure into `spin`, and 100000 self calls.
    assert_eq!(execution.tail_transfer_count(), 100_002);
    assert_eq!(execution.max_activation_depth(), 3);
}

/// Deep recursion whose every call is an operand keeps its activations live,
/// whatever the callee is, and the interpreter stops it at its bound.
#[test]
fn non_tail_recursion_through_every_callee_kind_still_exhausts_the_host_budget() {
    let lower = "(defn lower (count u64) u64\n  (match (u64.add-checked count 1u64)\n    (result.ok value) value\n    (result.err -) 0u64))\n";
    let sources = [
        // A function value held in a parameter.
        format!(
            "(defn main () u64 (again 0u64))\n(defn again (count u64) u64 (grow count again))\n(defn grow (count u64 next (fn (u64) u64)) u64 (lower (next count)))\n{lower}"
        ),
        // A closure and the function it captures.
        format!(
            "(defn main () u64 (spin 0u64))\n(defn spin (count u64) u64 (lower ((make spin) count)))\n(defn make (back (fn (u64) u64)) (fn (u64) u64) (lambda (count u64) u64 (back count)))\n{lower}"
        ),
        // A contract member called through an interface value.
        format!(
            "(defn main () u64 (walker.walk (as walker (counter left: 0u64))))\n(defint walker (defn walk (value self) u64))\n(deftype counter (record left u64)\n  (impl walker\n    (defn walk (value self) u64 (lower (walker.walk (as walker (counter left: 0u64)))))))\n{lower}"
        ),
    ];
    for source in sources {
        let checked = check_source("deep-non-tail.vib", &source);
        assert!(checked.accepted(), "{:?}", checked.diagnostics());
        assert_eq!(
            vibra_interp::run(checked.program().expect("program")),
            Err(vibra_interp::RuntimeError::HostStackExhausted {
                limit: vibra_interp::MAX_ACTIVATION_DEPTH
            }),
            "{source}"
        );
    }
}

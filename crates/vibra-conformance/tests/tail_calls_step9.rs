//! Focused host and conformance checks for M2 Step 9 tail execution.

#![allow(clippy::expect_used, clippy::indexing_slicing, missing_docs)]

use vibra_conformance::{
    CaseStatus, ConformanceProfile, ConformanceRunner, Corpus, InterpreterV1Handler,
    ProfileDispatcher,
};
use vibra_types::check_source;

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
    assert_eq!(execution.value(), &vibra_ir::Value::I32(2));
    assert_eq!(execution.tail_transfer_count(), 2);
    assert_eq!(execution.max_activation_depth(), 1);
    assert!(execution.audit_trace().is_empty());
}

#[test]
fn mixed_named_and_closure_tail_calls_reuse_only_the_selected_named_target() {
    for (condition, expected, transfers, depth) in
        [("true", 1, 1, 1), ("false", 2, 0, 2)]
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
        assert_eq!(execution.value(), &vibra_ir::Value::I32(expected));
        assert_eq!(execution.tail_transfer_count(), transfers);
        assert_eq!(execution.max_activation_depth(), depth);
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
    assert_eq!(execution.value(), &vibra_ir::Value::I32(7));
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
        assert_eq!(execution.value(), &vibra_ir::Value::I32(0));
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
    assert_eq!(execution.value(), &vibra_ir::Value::I32(41));
    assert_eq!(execution.tail_transfer_count(), 3);
    assert_eq!(execution.max_activation_depth(), 2);
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
    assert_eq!(execution.value(), &vibra_ir::Value::I32(2));

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

//! Focused host and conformance checks for M2 Step 9 tail execution.

#![allow(clippy::expect_used, clippy::indexing_slicing, missing_docs)]

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
fn source_tail_counter_exceeds_one_hundred_thousand_transfers() {
    let source = binary_counter_source(17);
    let checked = check_source("tail-stress.vib", &source);
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    let program = checked.program().expect("program");
    let execution = vibra_interp::run(program).expect("execution");
    assert_eq!(execution.value(), &vibra_ir::Value::I32(0));
    assert_eq!(execution.tail_transfer_count(), 1 << 17);
    assert_eq!(execution.max_activation_depth(), 1);
    assert!(execution.audit_trace().is_empty());
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

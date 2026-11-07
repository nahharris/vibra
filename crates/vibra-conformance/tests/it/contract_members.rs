//! Contract-member forms through the real checker and interpreter
//! (`docs/spec/02-type-system.md`, "Generics" and "Interfaces and methods";
//! `docs/spec/06-runtime.md`, "Generic instantiation" and "Tail calls"; M4
//! Step 2). The generic, labelled, dict-tail, and function-value forms are the
//! corpus cases `V1-RUNTIME-contract-member-*`; what remains here is what a
//! case cannot state: the order of an implementation's own generics, the
//! activation depth of tail calls through each form, and the formatter's rewrite.

use std::fs;
use std::path::Path;

use crate::support::TempDir;
use vibra_diagnostics::DiagnosticCode;
use vibra_interp::Execution;
use vibra_types::check_source;
use vibra_workspace::format_plan::plan_format;

/// One project directory holding one source, removed when it drops.
struct Project(TempDir);

impl Project {
    fn new(source: &str) -> Self {
        let root = TempDir::new("contract-members");
        fs::create_dir_all(root.join("src/hello")).expect("create source root");
        fs::create_dir(root.join("tests")).expect("create test root");
        fs::write(
            root.join("project.vibon"),
            r#"(record format: @project.v1 package: (record name: "hello" version: "0.1.0") targets: (array (record name: @hello kind: @bin root: "src/hello" entry: @hello.main.main effects: (array))) dependencies: (dict))"#,
        )
        .expect("write project");
        fs::write(root.join("src/hello/main.vib"), source).expect("write source");
        Self(root)
    }

    fn format(&self) -> (String, Vec<DiagnosticCode>) {
        let plan = plan_format(&self.0, Path::new("src/hello/main.vib"))
            .expect("a plan from the confined snapshot");
        (
            plan.formatted_text().to_owned(),
            plan.diagnostics()
                .iter()
                .map(vibra_diagnostics::Diagnostic::code)
                .collect(),
        )
    }
}

fn run(source: &str) -> Execution {
    let checked = check_source("members.vib", source);
    assert!(checked.accepted(), "{source}: {:?}", checked.diagnostics());
    vibra_interp::run(checked.program().expect("program")).expect("execution")
}

fn value(source: &str) -> String {
    let result = run(source).canonical_result();
    // The observation is `(record type: T value: V)`; keep V.
    let (_, value) = result
        .trim_end()
        .split_once(" value: ")
        .expect("a canonical result");
    value.strip_suffix(')').expect("closing").to_owned()
}

#[test]
fn a_generic_member_is_selected_through_a_bounded_parameter_and_an_interface_value() {
    // The receiver's type is not known here, so the implementation is
    // selected at run time and the member's `u` comes from the call.
    let source = "\
(defn main () (tuple (tuple str i32) (tuple str str) (array str))
  (tupleof
    (via-bound (alpha 1i32) 7i32)
    (via-value (as labeler (alpha 2i32)))
    (relay (box n: 1i32))))
(defint labeler
  (defn label (value self other u) (tuple str u)
    where: (u any)))
(deftype alpha i32
  (impl labeler
    (defn label (value self other u) (tuple str u)
      where: (u any)
      (tupleof \"A\" other))))
(defn via-bound (value t other u) (tuple str u)
  where: (t labeler u any)
  (labeler.label value other))
(defn via-value (value labeler) (tuple str str)
  (labeler.label value \"s\"))
(defint mapper
  (defn map-to (value self f (fn (self) u)) (array u)
    where: (u any)))
(deftype box (record n i32)
  (impl mapper
    (defn map-to (value self f (fn (self) u)) (array u)
      where: (u any)
      (array.of (f value)))))
(defn relay (x u) (array str)
  where: (u mapper)
  (mapper.map-to x (lambda (y u) str \"relayed\")))";
    assert_eq!(
        value(source),
        "(record kind: @tuple values: (array (record kind: @tuple values: (array \"A\" 7i32)) (record kind: @tuple values: (array \"A\" \"s\")) (record kind: @array values: (array \"relayed\"))))"
    );
}

#[test]
fn many_own_generics_keep_the_contracts_order_whatever_an_implementation_writes() {
    // The implementation lists its generics in another order and names the
    // one its signature never mentions differently; the call still binds each
    // type argument to the parameter it was written for.
    let source = |types: &str| {
        format!(
            "\
(defn main () (tuple (tuple str i32) (tuple u8 str) (tuple str str))
  (tupleof
    (pairing.pair-up types: (str i32) (holder 1i32) \"x\" 2i32)
    (pairing.pair-up (holder 1i32) 3u8 \"z\")
    (pairing.pick types: ({types}) (holder 1i32) \"s\")))
(defint maker
  (defn make () self)
  (defn tag (value self) str))
(deftype red i32
  (impl maker
    (defn make () self (red 1i32))
    (defn tag (value self) str \"red\")))
(deftype blue i32
  (impl maker
    (defn make () self (blue 2i32))
    (defn tag (value self) str \"blue\")))
(defint pairing
  (defn pair-up (value self first u second v) (tuple u v)
    where: (u any v any))
  (defn pick (value self item b) (tuple str b)
    where: (a maker b any)))
(deftype holder i32
  (impl pairing
    (defn pair-up (value self first u second v) (tuple u v)
      where: (v any u any)
      (tupleof first second))
    (defn pick (value self item b) (tuple str b)
      where: (zz maker b any)
      (tupleof (maker.tag (as zz (maker.make))) item))))"
        )
    };
    let tuple = |left: &str, right: &str| {
        format!("(record kind: @tuple values: (array {left} {right}))")
    };
    for (types, tag) in [("red str", "red"), ("blue str", "blue")] {
        assert_eq!(
            value(&source(types)),
            format!(
                "(record kind: @tuple values: (array {} {} {}))",
                tuple("\"x\"", "2i32"),
                tuple("3u8", "\"z\""),
                tuple(&format!("\"{tag}\""), "\"s\"")
            ),
            "types: ({types})"
        );
    }
}

const STEPPER: &str = "\
(defint stepper
  (defn step (value self n u64 tag u) u64
    where: (u any))
  (defn step-labelled (value self n u64) u64
    labelled: (hop u64 1u64))
  (defn step-tail (value self n u64) u64
    variadic: (rest (dict str u64)))
  (defn step-hopped (value self n u64) u64))
(deftype counter (record left u64)
  (impl stepper
    (defn step (value self n u64 tag u) u64
      where: (u any)
      (if (equatable.equal n 0u64)
        17u64
        (stepper.step value (lower n) tag)))
    (defn step-labelled (value self n u64) u64
      labelled: (hop u64 1u64)
      (if (equatable.equal n 0u64)
        18u64
        (stepper.step-labelled value (lower n) hop: hop)))
    (defn step-tail (value self n u64) u64
      variadic: (rest (dict str u64))
      (if (equatable.equal n 0u64)
        19u64
        (stepper.step-tail value (lower n) \"k\" 1u64)))
    (defn step-hopped (value self n u64) u64
      (if (equatable.equal n 0u64)
        20u64
        (hopper value (lower n))))))
(def hopper (fn (counter u64) u64) stepper.step-hopped)
(defn lower (count u64) u64
  (match (u64.sub count 1u64)
    (result.ok value) value
    (result.err -) 0u64))
";

#[test]
fn a_tail_call_through_each_form_reuses_the_activation() {
    // Thousands of calls: the depth assertion below holds only because every
    // call in tail position reuses its activation.
    const CALLS: u64 = 6000;
    for (call, expected) in [
        (
            format!("(stepper.step (counter left: 0u64) {CALLS}u64 \"tag\")"),
            17,
        ),
        (
            format!("(stepper.step-labelled (counter left: 0u64) {CALLS}u64)"),
            18,
        ),
        (
            format!("(stepper.step-tail (counter left: 0u64) {CALLS}u64)"),
            19,
        ),
        // A contract-member value held in a module value, called in tail
        // position by the implementation it selects.
        (
            format!("(stepper.step-hopped (counter left: 0u64) {CALLS}u64)"),
            20,
        ),
    ] {
        // The entry is the first function, so `main` leads.
        let execution = run(&format!("(defn main () u64 {call})\n{STEPPER}"));
        assert_eq!(
            execution.canonical_result().trim_end(),
            format!("(record type: @u64 value: {expected}u64)"),
            "{call}"
        );
        assert!(
            execution.max_activation_depth() < 16,
            "{call}: depth {}",
            execution.max_activation_depth()
        );
        assert!(
            execution.tail_transfer_count() >= usize::try_from(CALLS).expect("count"),
            "{call}: {} transfers",
            execution.tail_transfer_count()
        );
    }
}

#[test]
fn a_contract_call_written_out_of_canonical_order_is_rewritten_idempotently() {
    let source = "\
(defn main () (tuple i32 bool)
  (scaler.scale (gauge base: 1i32) true offset: 7i32 types: (bool) by: 3i32))
(defint scaler
  (defn scale (value self other u) (tuple i32 u)
    where: (u any)
    labelled: (by i32 2i32 offset i32 0i32)))
(deftype gauge (record base i32)
  (impl scaler
    (defn scale (value self other u) (tuple i32 u)
      where: (u any)
      labelled: (by i32 2i32 offset i32 0i32)
      (tupleof by other))))
";
    let value = |source: &str| {
        let checked = check_source("src/hello/main.vib", source);
        assert!(checked.accepted(), "{:?}", checked.diagnostics());
        vibra_interp::run(checked.program().expect("program"))
            .expect("execution")
            .canonical_result()
    };
    let (first, diagnostics) = Project::new(source).format();
    assert_eq!(diagnostics, vec![DiagnosticCode::StyleArgumentOrder]);
    assert!(
        first.contains(
            "(scaler.scale types: (bool) (gauge base: 1i32) true by: 3i32 offset: 7i32)"
        ),
        "{first}"
    );
    // The rewrite keeps the program's result, and a second pass changes
    // nothing and has nothing left to report.
    assert_eq!(value(&first), value(source));
    let (second, diagnostics) = Project::new(&first).format();
    assert_eq!(second, first);
    assert_eq!(diagnostics, Vec::<DiagnosticCode>::new());
}

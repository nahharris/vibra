//! Contract-member forms through the real checker and interpreter
//! (`docs/spec/02-type-system.md`, "Generics" and "Interfaces and methods";
//! `docs/spec/06-runtime.md`, "Generic instantiation" and "Tail calls"; M4
//! Step 2).

#![allow(clippy::expect_used, clippy::indexing_slicing, missing_docs)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use vibra_diagnostics::DiagnosticCode;
use vibra_interp::Execution;
use vibra_types::check_source;
use vibra_workspace::format_plan::plan_format;

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

/// One project directory holding one source, removed when it drops.
struct Project(PathBuf);

impl Project {
    fn new(source: &str) -> Self {
        let nonce = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "vibra-contract-members-step2-{}-{nonce}",
            std::process::id()
        ));
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

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
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

const MAPPER: &str = "\
(defint mapper
  (defn map-to (value self f (fn (self) u)) (array u)
    where: (u any)))
(deftype box (record n i32)
  (impl mapper
    (defn map-to (value self f (fn (self) u)) (array u)
      where: (u any)
      (array.of (f value) (f value)))))
(deftype pair (record left t right t)
  where: (t any)
  (impl mapper
    (defn map-to (value self f (fn (self) u)) (array u)
      where: (u any)
      (array.of (f value)))))
";

#[test]
fn an_abstract_generic_member_runs_at_the_arguments_inference_fixes() {
    let source = format!(
        "{MAPPER}(defn main () (tuple (array str) (array u64))\n  (tupleof\n    (mapper.map-to (box n: 3i32) (lambda (b box) str \"boxed\"))\n    (mapper.map-to (pair left: 1i32 right: 2i32) (lambda (p (pair i32)) u64 5u64))))"
    );
    assert_eq!(
        value(&source),
        "(record kind: @tuple values: (array (record kind: @array values: (array \"boxed\" \"boxed\")) (record kind: @array values: (array 5u64))))"
    );
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

#[test]
fn a_written_types_list_fixes_the_argument_and_agrees_with_inference() {
    let source = format!(
        "{MAPPER}(defn main () (array i32)\n  (mapper.map-to types: (i32) (box n: 4i32) (lambda (b box) i32 9i32)))"
    );
    assert_eq!(
        value(&source),
        "(record kind: @array values: (array 9i32 9i32))"
    );
}

#[test]
fn a_type_argument_only_types_can_fix_reaches_the_implementation() {
    // `u` appears in no signature slot, so the run time can only know it from
    // the call, and the body dispatches on it.
    let source = |argument: &str| {
        format!(
            "\
(defn main () str (probing.probe types: ({argument}) (holder 0i32)))
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
(defint probing
  (defn probe (value self) str
    where: (u maker)))
(deftype holder i32
  (impl probing
    (defn probe (value self) str
      where: (u maker)
      (maker.tag (as u (maker.make))))))"
        )
    };
    assert_eq!(value(&source("red")), "\"red\"");
    assert_eq!(value(&source("blue")), "\"blue\"");
}

#[test]
fn a_generic_interface_takes_its_arguments_then_the_members() {
    let source = "\
(defn main () (tuple (tuple str bool) (tuple i64 bool) crate)
  (tupleof
    (as (tuple str bool) (tagged.tag-with (crate n: 1i32) true))
    (tagged.tag-with types: (i64 bool) (crate n: 1i32) false)
    (builder.build \"seed\")))
(defint tagged
  where: (t any)
  (defn tag-with (value self other u) (tuple t u)
    where: (u any)))
(defint builder
  (defn build (seed u) self
    where: (u any)))
(deftype crate (record n i32)
  (impl (tagged str)
    (defn tag-with (value self other u) (tuple str u)
      where: (u any)
      (tupleof \"s\" other)))
  (impl (tagged i64)
    (defn tag-with (value self other u) (tuple i64 u)
      where: (u any)
      (tupleof 7i64 other)))
  (impl builder
    (defn build (seed u) self
      where: (u any)
      (crate n: 9i32))))";
    assert_eq!(
        value(source),
        "(record kind: @tuple values: (array (record kind: @tuple values: (array \"s\" true)) (record kind: @tuple values: (array 7i64 false)) (record kind: @record type: @crate fields: (record n: 9i32))))"
    );
}

const SCALER: &str = "\
(defint scaler
  (defn scale (value self) (tuple i32 i32)
    labelled: (by i32 2i32 offset i32 0i32)))
(defint collector
  (defn collect (value self) (dict str i32)
    variadic: (entries (dict str i32))))
(deftype gauge (record base i32)
  (impl scaler
    (defn scale (value self) (tuple i32 i32)
      labelled: (by i32 2i32 offset i32 0i32)
      (tupleof by offset)))
  (impl collector
    (defn collect (value self) (dict str i32)
      variadic: (entries (dict str i32))
      entries)))
";

#[test]
fn labelled_operands_bind_by_name_and_omitted_ones_take_the_contract_default() {
    let source = format!(
        "{SCALER}(defn main () (tuple (tuple i32 i32) (tuple i32 i32) (tuple i32 i32) (tuple i32 i32) (tuple i32 i32))\n  (tupleof\n    (scaler.scale (gauge base: 1i32))\n    (scaler.scale (gauge base: 1i32) offset: 7i32 by: 3i32)\n    (scaler.scale (gauge base: 1i32) by: 5i32)\n    (through (gauge base: 1i32))\n    (via-value (as scaler (gauge base: 1i32)))))\n(defn through (value t) (tuple i32 i32)\n  where: (t scaler)\n  (scaler.scale value offset: 4i32))\n(defn via-value (value scaler) (tuple i32 i32)\n  (scaler.scale value by: 8i32))"
    );
    let tuple = |left: i32, right: i32| {
        format!("(record kind: @tuple values: (array {left}i32 {right}i32))")
    };
    assert_eq!(
        value(&source),
        format!(
            "(record kind: @tuple values: (array {} {} {} {} {}))",
            tuple(2, 0),
            tuple(3, 7),
            tuple(5, 0),
            tuple(2, 4),
            tuple(8, 0)
        )
    );
}

#[test]
fn a_dict_tail_collects_pairs_in_key_order_and_may_be_empty() {
    let source = format!(
        "{SCALER}(defn main () (tuple (dict str i32) (dict str i32) (dict str i32))\n  (tupleof\n    (collector.collect (gauge base: 1i32) \"b\" 2i32 \"a\" 1i32)\n    (collector.collect (gauge base: 1i32))\n    (via-value (as collector (gauge base: 1i32)))))\n(defn via-value (value collector) (dict str i32)\n  (collector.collect value \"z\" 26i32))"
    );
    assert_eq!(
        value(&source),
        "(record kind: @tuple values: (array (record kind: @dict entries: (array (tuple \"a\" 1i32) (tuple \"b\" 2i32))) (record kind: @dict entries: (array)) (record kind: @dict entries: (array (tuple \"z\" 26i32)))))"
    );
}

#[test]
fn each_form_is_a_function_value_at_a_written_fn_type() {
    let source = format!(
        "{MAPPER}{SCALER}\
(defn main () (tuple (array str) (tuple i32 i32) (dict str i32))
  (tupleof
    (run-map mapper.map-to (box n: 1i32))
    (run-scale scaler.scale (gauge base: 1i32))
    (run-collect collector.collect (gauge base: 1i32))))
(defn run-map (f (fn (box (fn (box) str)) (array str)) b box) (array str)
  (f b (lambda (x box) str \"v\")))
(defn run-scale (f (fn (gauge) (tuple i32 i32) labelled: (by i32 offset i32)) g gauge) (tuple i32 i32)
  (f g by: 1i32 offset: 2i32))
(defn run-collect (f (fn (gauge) (dict str i32) variadic: (dict str i32)) g gauge) (dict str i32)
  (f g \"k\" 3i32))"
    );
    assert_eq!(
        value(&source),
        "(record kind: @tuple values: (array (record kind: @array values: (array \"v\" \"v\")) (record kind: @tuple values: (array 1i32 2i32)) (record kind: @dict entries: (array (tuple \"k\" 3i32)))))"
    );
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
      (if (u64.equal n 0u64)
        17u64
        (stepper.step value (lower n) tag)))
    (defn step-labelled (value self n u64) u64
      labelled: (hop u64 1u64)
      (if (u64.equal n 0u64)
        18u64
        (stepper.step-labelled value (lower n) hop: hop)))
    (defn step-tail (value self n u64) u64
      variadic: (rest (dict str u64))
      (if (u64.equal n 0u64)
        19u64
        (stepper.step-tail value (lower n) \"k\" 1u64)))
    (defn step-hopped (value self n u64) u64
      (if (u64.equal n 0u64)
        20u64
        (hopper value (lower n))))))
(def hopper (fn (counter u64) u64) stepper.step-hopped)
(defn lower (count u64) u64
  (match (u64.sub-checked count 1u64)
    (result.ok value) value
    (result.err -) 0u64))
";

#[test]
fn a_tail_call_through_each_form_reuses_the_activation() {
    // Far more calls than the interpreter's activation bound; each ends the
    // run only because every call in tail position reuses its activation.
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

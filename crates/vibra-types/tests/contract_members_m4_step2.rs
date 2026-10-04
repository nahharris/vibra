//! Contract members with their own generic parameters, labelled operands, a
//! written `types:` list, and a dict variadic tail, as calls and as function
//! values (`docs/spec/02-type-system.md`, "Generics", "Interfaces and methods",
//! and "Functions as values"; M4 Step 2).

#![allow(clippy::expect_used, clippy::indexing_slicing, missing_docs)]

use vibra_diagnostics::{ByteSpan, DiagnosticCode as C, Level};
use vibra_types::check_source;

/// Interfaces and implementations the cases below call.
const PRELUDE: &str = "\
(defint mapper
  (defn map-to (value self f (fn (self) u)) (array u)
    where: (u any)))
(defint tagged
  where: (t any)
  (defn tag-with (value self other u) (tuple t u)
    where: (u any)))
(defint scaler
  (defn scale (value self) (tuple i32 i32)
    labelled: (by i32 2i32 offset i32 0i32)))
(defint collector
  (defn collect (value self) (dict str i32)
    variadic: (entries (dict str i32))))
(defint maker
  (defn make () self))
(defint probing
  (defn probe (value self) str
    where: (u maker)))
(deftype box (record n i32)
  (impl mapper
    (defn map-to (value self f (fn (self) u)) (array u)
      where: (u any)
      (array.of (f value)))))
(deftype crate (record n i32)
  (impl (tagged str)
    (defn tag-with (value self other u) (tuple str u)
      where: (u any)
      (tupleof \"s\" other)))
  (impl (tagged i64)
    (defn tag-with (value self other u) (tuple i64 u)
      where: (u any)
      (tupleof 7i64 other))))
(deftype gauge (record base i32)
  (impl scaler
    (defn scale (value self) (tuple i32 i32)
      labelled: (by i32 2i32 offset i32 0i32)
      (tupleof by offset)))
  (impl collector
    (defn collect (value self) (dict str i32)
      variadic: (entries (dict str i32))
      entries)))
(deftype holder i32
  (impl probing
    (defn probe (value self) str
      where: (u maker)
      \"held\")))
";

fn report(source: &str) -> Vec<(C, Level)> {
    check_source("members.vib", &format!("{PRELUDE}{source}"))
        .diagnostics()
        .iter()
        .map(|diagnostic| (diagnostic.code(), diagnostic.level()))
        .collect()
}

fn codes(source: &str) -> Vec<C> {
    report(source).into_iter().map(|(code, _)| code).collect()
}

fn ok(source: &str) {
    assert_eq!(codes(source), Vec::<C>::new(), "{source}");
}

#[test]
fn the_three_availability_rejections_are_gone() {
    // M3 reported each of these as `@tool.unavailable`.
    for source in [
        "(defn subject () (array i32) (mapper.map-to (box n: 1i32) (lambda (b box) i32 1i32)))",
        "(defn subject () (tuple i32 i32) (scaler.scale (gauge base: 1i32) by: 3i32))",
        "(defn subject () (dict str i32) (collector.collect (gauge base: 1i32) \"a\" 1i32))",
        "(defn subject () (array i32) (mapper.map-to types: (i32) (box n: 1i32) (lambda (b box) i32 1i32)))",
        "(defn accept (g (fn (box (fn (box) str)) (array str))) i32 0i32)\n(defn subject () i32 (accept mapper.map-to))",
    ] {
        assert!(
            !codes(source).contains(&C::ToolUnavailable),
            "{source}: {:?}",
            codes(source)
        );
    }
}

#[test]
fn an_abstract_member_with_its_own_generic_infers_or_takes_types() {
    ok(
        "(defn subject () (array str) (mapper.map-to (box n: 1i32) (lambda (b box) str \"x\")))",
    );
    ok(
        "(defn subject () (array i32) (mapper.map-to types: (i32) (box n: 1i32) (lambda (b box) i32 1i32)))",
    );
    // The expected type fixes what the operands leave open.
    ok(
        "(defn subject () (array str) (mapper.map-to (box n: 1i32) (lambda (b box) str \"x\")))",
    );
    // One generic that only a written list can fix.
    ok(
        "(defn subject () str (probing.probe types: (maker-red) (holder 0i32)))\n(deftype maker-red i32 (impl maker (defn make () self (maker-red 1i32))))",
    );
    assert_eq!(
        codes("(defn subject () str (probing.probe (holder 0i32)))"),
        vec![C::TypeAmbiguousInference]
    );
}

#[test]
fn types_is_defined_by_the_interface_then_the_member() {
    // `tagged` takes `t`, then the member's `u`.
    ok(
        "(defn subject () (tuple str bool) (tagged.tag-with types: (str bool) (crate n: 1i32) true))",
    );
    ok(
        "(defn subject () (tuple i64 bool) (tagged.tag-with types: (i64 bool) (crate n: 1i32) true))",
    );
    // Without it, both implementations take these operands.
    assert_eq!(
        codes(
            "(defn subject () (tuple str bool) (let r (tagged.tag-with (crate n: 1i32) true)) (as (tuple str bool) r))"
        ),
        vec![C::TypeAmbiguousImplementation]
    );
    // An expected type picks one.
    ok(
        "(defn subject () (tuple str bool) (as (tuple str bool) (tagged.tag-with (crate n: 1i32) true)))",
    );
}

#[test]
fn a_wrong_types_length_or_a_contradiction_is_a_type_argument_mismatch() {
    for source in [
        // Too many, too few, and a member that declares none.
        "(defn subject () (array i32) (mapper.map-to types: (i32 str) (box n: 1i32) (lambda (b box) i32 1i32)))",
        "(defn subject () (tuple str bool) (tagged.tag-with types: (str) (crate n: 1i32) true))",
        "(defn subject () (tuple i32 i32) (scaler.scale types: (i32) (gauge base: 1i32)))",
        // The list contradicts the operand, and the written result type.
        "(defn subject () (tuple str str) (let r (tagged.tag-with types: (str str) (crate n: 1i32) true)) r)",
        "(defn subject () (tuple str bool) (tagged.tag-with types: (str str) (crate n: 1i32) true))",
        // The list names interface arguments the receiver does not implement.
        "(defn subject () (tuple bool bool) (tagged.tag-with types: (bool bool) (crate n: 1i32) true))",
    ] {
        assert_eq!(codes(source), vec![C::TypeTypeArgumentMismatch], "{source}");
    }
}

#[test]
fn a_types_argument_that_misses_the_members_bound_is_unsatisfied() {
    assert_eq!(
        codes("(defn subject () str (probing.probe types: (str) (holder 0i32)))"),
        vec![C::TypeUnsatisfiedBound]
    );
}

#[test]
fn labelled_operands_bind_by_name_in_any_written_order() {
    ok("(defn subject () (tuple i32 i32) (scaler.scale (gauge base: 1i32)))");
    ok("(defn subject () (tuple i32 i32) (scaler.scale (gauge base: 1i32) by: 3i32))");
    ok(
        "(defn subject () (tuple i32 i32) (scaler.scale (gauge base: 1i32) by: 3i32 offset: 7i32))",
    );
    // Written out of declaration order: accepted, with a warning.
    assert_eq!(
        report(
            "(defn subject () (tuple i32 i32) (scaler.scale (gauge base: 1i32) offset: 7i32 by: 3i32))"
        ),
        vec![(C::StyleArgumentOrder, Level::Warning)]
    );
    // A labelled operand may precede the receiver.
    assert_eq!(
        report(
            "(defn subject () (tuple i32 i32) (scaler.scale by: 3i32 (gauge base: 1i32)))"
        ),
        vec![(C::StyleArgumentOrder, Level::Warning)]
    );
}

#[test]
fn an_unknown_duplicate_or_mistyped_label_is_an_argument_mismatch() {
    for source in [
        "(defn subject () (tuple i32 i32) (scaler.scale (gauge base: 1i32) bogus: 1i32))",
        "(defn subject () (tuple i32 i32) (scaler.scale (gauge base: 1i32) by: 1i32 by: 2i32))",
        "(defn subject () (tuple i32 i32) (scaler.scale (gauge base: 1i32) by: \"no\"))",
    ] {
        assert_eq!(codes(source), vec![C::TypeArgumentMismatch], "{source}");
    }
}

#[test]
fn a_dict_tail_takes_pairs_and_may_be_empty() {
    ok("(defn subject () (dict str i32) (collector.collect (gauge base: 1i32)))");
    ok(
        "(defn subject () (dict str i32) (collector.collect (gauge base: 1i32) \"a\" 1i32 \"b\" 2i32))",
    );
    for source in [
        "(defn subject () (dict str i32) (collector.collect (gauge base: 1i32) \"a\"))",
        "(defn subject () (dict str i32) (collector.collect (gauge base: 1i32) \"a\" 1i32 \"b\"))",
        "(defn subject () (dict str i32) (collector.collect (gauge base: 1i32) 1i32 \"a\"))",
    ] {
        assert_eq!(codes(source), vec![C::TypeArgumentMismatch], "{source}");
    }
}

#[test]
fn a_member_is_a_function_value_at_a_written_fn_type() {
    ok("(defn subject () (fn (box (fn (box) str)) (array str)) mapper.map-to)");
    ok(
        "(defn subject () (fn (gauge) (tuple i32 i32) labelled: (by i32 offset i32)) scaler.scale)",
    );
    ok(
        "(defn subject () (fn (gauge) (dict str i32) variadic: (dict str i32)) collector.collect)",
    );
    ok("(defn subject () (fn (crate bool) (tuple str bool)) tagged.tag-with)");
    // The written type fixes the member's own generic, here through an
    // enclosing generic function's parameter.
    ok(
        "(defn subject (seed v) (array v) where: (v any)\n  (apply-map seed mapper.map-to))\n(defn apply-map (seed v g (fn (box (fn (box) v)) (array v))) (array v) where: (v any)\n  (g (box n: 1i32) (lambda (b box) v seed)))",
    );
}

#[test]
fn a_member_value_without_a_written_fn_type_is_ambiguous() {
    assert_eq!(
        codes("(defn subject () i32 (let g mapper.map-to) 0i32)"),
        vec![C::TypeAmbiguousInference]
    );
    assert_eq!(
        codes("(defn subject () i32 (let g scaler.scale) 0i32)"),
        vec![C::TypeAmbiguousInference]
    );
}

#[test]
fn a_member_value_no_implementation_satisfies_is_rejected() {
    // The receiver implements nothing, or the written type is not the member's.
    assert_eq!(
        codes("(defn subject () (fn (str (fn (str) i32)) (array i32)) mapper.map-to)"),
        vec![C::TypeUnsatisfiedBound]
    );
    assert_eq!(
        codes("(defn subject () (fn (box (fn (box) i32)) (array str)) mapper.map-to)"),
        vec![C::TypeMismatch]
    );
    assert_eq!(
        codes(
            "(defn subject () (fn (gauge) (tuple i32 i32) labelled: (by str offset i32)) scaler.scale)"
        ),
        vec![C::TypeMismatch]
    );
    // The written type cannot fix a generic the signature never mentions.
    assert_eq!(
        codes("(defn subject () (fn (holder) str) probing.probe)"),
        vec![C::TypeAmbiguousInference]
    );
}

#[test]
fn a_malformed_contract_call_does_not_stop_the_next_one() {
    assert_eq!(
        codes(
            "(defn bad () (tuple i32 i32) (scaler.scale (gauge base: 1i32) bogus: 1i32))\n\
             (defn good () (array str) (mapper.map-to (box n: 1i32) (lambda (b box) str \"x\")))\n\
             (defn bad-again () (dict str i32) (collector.collect (gauge base: 1i32) \"a\"))\n\
             (defn good-again () (dict str i32) (collector.collect (gauge base: 1i32)))"
        ),
        vec![C::TypeArgumentMismatch, C::TypeArgumentMismatch]
    );
}

#[test]
fn an_interface_and_a_member_that_both_name_a_generic_redeclare_it() {
    let checked = check_source(
        "redeclared.vib",
        "(defint tagged\n  where: (t any)\n  (defn tag-with (value self other t) (tuple t t)\n    where: (t any)))\n(defn ok () i32 1i32)",
    );
    let diagnostics = checked.diagnostics();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code(), C::NameGenericRedeclaration);
    assert_eq!(diagnostics[0].primary_span(), ByteSpan::new(94, 95));
}

#[test]
fn a_receiver_that_spells_a_member_generic_is_not_captured() {
    // The caller's own `u` is the receiver; the member's `u` is its own.
    ok(
        "(defn relay (x u) (array str)\n  where: (u mapper)\n  (mapper.map-to x (lambda (y u) str \"relayed\")))",
    );
}

#[test]
fn the_typed_program_carries_the_members_type_arguments() {
    let checked = check_source(
        "ir.vib",
        &format!(
            "{PRELUDE}(defn answer () (array str) (mapper.map-to types: (str) (box n: 1i32) (lambda (b box) str \"x\")))"
        ),
    );
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    let canonical = checked.program().expect("program").canonical_vibon();
    assert!(
        canonical.contains(
            "contract: @mapper member: @map-to receiver: 0u64 types: (array @str)"
        ),
        "{canonical}"
    );
}

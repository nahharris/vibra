//! Effect declarations, rows, performed rows, and the witness of
//! `@effect.outside-ceiling` (`docs/spec/03-effects.md`; M4 Step 14).

#![allow(clippy::expect_used, clippy::indexing_slicing, missing_docs)]

use vibra_diagnostics::{Diagnostic, DiagnosticCode as C};
use vibra_types::check_source;

fn check(source: &str) -> Vec<Diagnostic> {
    check_source("effects.vib", source).diagnostics().to_vec()
}

fn ok(source: &str) {
    let found = check(source);
    assert!(found.is_empty(), "{source}\n{found:#?}");
}

fn codes(source: &str) -> Vec<C> {
    check(source).iter().map(Diagnostic::code).collect()
}

fn slice(source: &str, span: vibra_diagnostics::ByteSpan) -> &str {
    &source[span.start()..span.end()]
}

/// The witness of one diagnostic: the text of its primary span and the text
/// and note of each related span.
fn witness(source: &str, diagnostic: &Diagnostic) -> (String, Vec<(String, String)>) {
    (
        slice(source, diagnostic.primary_span()).to_owned(),
        diagnostic
            .related()
            .iter()
            .map(|related| {
                (
                    slice(source, related.span).to_owned(),
                    related.message.clone(),
                )
            })
            .collect(),
    )
}

/// The spec's example, in one module: `audit.record` adds `write`, so a
/// caller that names only `audit` is outside its ceiling.
const AUDIT: &str = "(deffect write\n  (defn text (target str event str) void (do)))\n(deffect audit\n  (defn record (target str event str) void\n    effects: (write)\n    (write.text target event)))\n";

#[test]
fn the_effects_chapter_example_reports_the_missing_additive_root_with_its_witness() {
    let source = format!(
        "{AUDIT}(defn handle-request (target str summary str) void\n  effects: (audit)\n  (audit.record target summary))\n"
    );
    let found = check(&source);
    assert_eq!(found.len(), 1, "{found:#?}");
    let diagnostic = &found[0];
    assert_eq!(diagnostic.code(), C::EffectOutsideCeiling);
    let (primary, related) = witness(&source, diagnostic);
    assert_eq!(primary, "(audit.record target summary)");
    assert_eq!(
        related,
        [(
            "(write.text target event)".to_owned(),
            "performs @write via @write.text".to_owned()
        )]
    );
    assert_eq!(diagnostic.notes(), ["performs @write via @audit.record"]);
}

#[test]
fn naming_both_roots_is_within_the_ceiling_and_leaves_no_warning() {
    ok(&format!(
        "{AUDIT}(defn handle-request (target str summary str) void\n  effects: (audit write)\n  (audit.record target summary))\n"
    ));
}

#[test]
fn an_operation_with_an_empty_additive_row_performs_only_its_owner() {
    ok(&format!(
        "{AUDIT}(defn direct (target str) void effects: (write) (write.text target \"x\"))\n"
    ));
    assert_eq!(
        codes(&format!(
            "{AUDIT}(defn direct (target str) void (write.text target \"x\"))\n"
        )),
        vec![C::EffectOutsideCeiling]
    );
}

#[test]
fn an_omitted_ceiling_is_empty_whatever_the_callee_performs() {
    let source = format!("{AUDIT}(defn quiet () void (audit.record \"a\" \"b\"))\n");
    let found = check(&source);
    // `audit` and `write` are both missing: two diagnostics, one for each
    // pair of the function and a missing root, in canonical order of root.
    assert_eq!(found.len(), 2, "{found:#?}");
    assert!(found.iter().all(|d| d.code() == C::EffectOutsideCeiling));
    assert!(
        found[0].message().contains("@audit"),
        "{}",
        found[0].message()
    );
    assert!(
        found[1].message().contains("@write"),
        "{}",
        found[1].message()
    );
}

#[test]
fn an_additive_root_reaches_every_transitive_caller() {
    let source = format!(
        "{AUDIT}(defn middle () void effects: (audit write) (audit.record \"a\" \"b\"))\n(defn outer () void effects: (audit) (middle))\n"
    );
    let found = check(&source);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].code(), C::EffectOutsideCeiling);
    assert!(found[0].message().contains("@write"));
    let (primary, related) = witness(&source, &found[0]);
    assert_eq!(primary, "(middle)");
    assert_eq!(
        related
            .iter()
            .map(|(text, _)| text.as_str())
            .collect::<Vec<_>>(),
        ["(audit.record \"a\" \"b\")", "(write.text target event)"]
    );
    assert_eq!(related[0].1, "performs @write via @audit.record");
    assert_eq!(related[1].1, "performs @write via @write.text");
}

#[test]
fn a_witness_through_three_hops_names_each_call_in_order() {
    let source = format!(
        "{AUDIT}(defn one () void effects: (audit write) (audit.record \"a\" \"b\"))\n(defn two () void effects: (audit write) (one))\n(defn three () void effects: (audit write) (two))\n(defn four () void effects: (audit) (three))\n"
    );
    let found = check(&source);
    assert_eq!(found.len(), 1, "{found:#?}");
    let (primary, related) = witness(&source, &found[0]);
    assert_eq!(primary, "(three)");
    assert_eq!(
        related
            .iter()
            .map(|(text, _)| text.as_str())
            .collect::<Vec<_>>(),
        [
            "(two)",
            "(one)",
            "(audit.record \"a\" \"b\")",
            "(write.text target event)"
        ]
    );
}

#[test]
fn the_first_call_site_in_source_order_is_the_primary_span() {
    let source = format!(
        "{AUDIT}(defn both () void effects: (audit)\n  (audit.record \"first\" \"x\")\n  (audit.record \"second\" \"y\"))\n"
    );
    let found = check(&source);
    assert_eq!(found.len(), 1, "{found:#?}");
    let (primary, _) = witness(&source, &found[0]);
    assert_eq!(primary, "(audit.record \"first\" \"x\")");
}

#[test]
fn the_shorter_of_two_chains_is_the_witness_and_equal_chains_take_the_earlier_call() {
    // `top` reaches `write` through `short` (two hops) and through `long-a`
    // then `long-b` (three hops): the primary span is the first call site in
    // source order, which starts the long chain, and the related spans take
    // the shortest chain from there.
    let source = format!(
        "{AUDIT}(defn deep () void effects: (audit write) (audit.record \"a\" \"b\"))\n(defn long-b () void effects: (audit write) (deep))\n(defn long-a () void effects: (audit write) (long-b))\n(defn short () void effects: (audit write) (deep))\n(defn top () void effects: (audit)\n  (long-a)\n  (short))\n"
    );
    let found = check(&source);
    assert_eq!(found.len(), 1, "{found:#?}");
    let (primary, related) = witness(&source, &found[0]);
    assert_eq!(primary, "(long-a)");
    assert_eq!(
        related
            .iter()
            .map(|(text, _)| text.as_str())
            .collect::<Vec<_>>(),
        [
            "(long-b)",
            "(deep)",
            "(audit.record \"a\" \"b\")",
            "(write.text target event)"
        ]
    );
}

#[test]
fn among_chains_of_one_length_the_earlier_call_site_continues_the_witness() {
    let source = format!(
        "{AUDIT}(defn left () void effects: (audit write) (audit.record \"left\" \"b\"))\n(defn right () void effects: (audit write) (audit.record \"right\" \"b\"))\n(defn middle () void effects: (audit write)\n  (left)\n  (right))\n(defn top () void effects: (audit) (middle))\n"
    );
    let found = check(&source);
    assert_eq!(found.len(), 1, "{found:#?}");
    let (primary, related) = witness(&source, &found[0]);
    assert_eq!(primary, "(middle)");
    assert_eq!(related[0].0, "(left)");
}

#[test]
fn recursion_and_mutual_recursion_close_the_performed_row() {
    ok(&format!(
        "{AUDIT}(defn even (n u32) void effects: (write) (write.text \"e\" \"x\") (odd n))\n(defn odd (n u32) void effects: (write) (even n))\n"
    ));
    // `odd` reaches `write` only through `even`, which `odd` calls.
    assert_eq!(
        codes(&format!(
            "{AUDIT}(defn even (n u32) void effects: (write) (write.text \"e\" \"x\") (odd n))\n(defn odd (n u32) void (even n))\n"
        )),
        vec![C::EffectOutsideCeiling]
    );
    // A self-recursive function with no effect of its own performs none.
    ok("(defn spin (n u32) void (spin n))");
}

#[test]
fn a_callee_that_is_itself_offending_does_not_cascade() {
    let source = format!(
        "{AUDIT}(defn bad () void (write.text \"a\" \"b\"))\n(defn caller () void effects: (write) (bad))\n(defn after () void (do))\n"
    );
    let found = check(&source);
    // Only `bad` is over its ceiling: `caller` sees the performed row of
    // `bad`, which its written ceiling contains.
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(
        slice(&source, found[0].primary_span()),
        "(write.text \"a\" \"b\")"
    );
}

#[test]
fn a_function_value_parameter_contributes_the_row_of_its_type() {
    ok(&format!(
        "{AUDIT}(defn apply (f (fn (str) void effects: (write)) value str) void effects: (write) (f value))\n"
    ));
    let source = format!(
        "{AUDIT}(defn apply (f (fn (str) void effects: (write)) value str) void (f value))\n"
    );
    let found = check(&source);
    assert_eq!(found.len(), 1, "{found:#?}");
    let (primary, related) = witness(&source, &found[0]);
    assert_eq!(primary, "(f value)");
    assert!(related.is_empty());
    assert!(
        found[0].notes()[0].starts_with("performs @write via a function value of type"),
        "{:?}",
        found[0].notes()
    );
}

#[test]
fn creating_a_function_value_performs_nothing() {
    // Naming an operation, and writing a lambda that applies one, are
    // creations; only applying the value performs.
    ok(&format!(
        "{AUDIT}(defn pick () (fn (str str) void effects: (write)) write.text)\n(defn wrap () (fn (str) void effects: (write))\n  (lambda (value str) void effects: (write) (write.text value value)))\n"
    ));
}

#[test]
fn a_lambda_is_checked_against_its_own_row() {
    let source = format!(
        "{AUDIT}(defn make () (fn (str) void)\n  (lambda (value str) void (write.text value value)))\n"
    );
    let found = check(&source);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].code(), C::EffectOutsideCeiling);
    assert_eq!(
        slice(&source, found[0].primary_span()),
        "(write.text value value)"
    );
}

#[test]
fn a_function_value_of_one_row_is_not_another_row_in_either_direction() {
    let pure = format!(
        "{AUDIT}(defn pure-one (value str) void (do))\n(defn effectful-one (value str) void effects: (write) (write.text value value))\n"
    );
    assert_eq!(
        codes(&format!(
            "{pure}(defn take (f (fn (str) void effects: (write))) void (do))\n(defn give () void (take pure-one))\n"
        )),
        vec![C::TypeArgumentMismatch]
    );
    assert_eq!(
        codes(&format!(
            "{pure}(defn take (f (fn (str) void)) void (do))\n(defn give () void (take effectful-one))\n"
        )),
        vec![C::TypeArgumentMismatch]
    );
}

#[test]
fn a_let_value_a_let_else_value_and_fallback_and_a_return_operand_join_the_row() {
    let program = |element: &str, ceiling: &str| {
        format!(
            "{AUDIT}(defn see () (option i32) effects: (write) (write.text \"a\" \"b\") (option.some 1i32))
(defn probe (o (option i32) c bool) i32 {ceiling} {element})
"
        )
    };
    for element in [
        "(let seen (see)) 0i32",
        "(let-else (option.some v) (see) (return 0i32)) v",
        "(let-else (option.some v) o (do (see) (return 0i32))) v",
        "(let - (if c (return (do (see) 1i32)) 2i32)) 0i32",
    ] {
        assert_eq!(
            codes(&program(element, "")),
            vec![C::EffectOutsideCeiling],
            "{element}"
        );
        ok(&program(element, "effects: (write)"));
    }
}

#[test]
fn a_written_root_that_is_never_performed_is_a_warning_at_its_entry() {
    let source = format!("{AUDIT}(defn idle () void effects: (write audit) (do))\n");
    let found = check(&source);
    assert_eq!(found.len(), 2, "{found:#?}");
    assert!(found.iter().all(|d| d.code() == C::ContractUnusedEffect));
    let texts = found
        .iter()
        .map(|d| slice(&source, d.primary_span()))
        .collect::<Vec<_>>();
    assert!(
        texts.contains(&"write") && texts.contains(&"audit"),
        "{texts:?}"
    );
}

#[test]
fn an_unused_additive_root_of_an_operation_is_a_warning_and_still_reaches_callers() {
    let source = "(deffect write (defn text (target str) void (do)))\n(deffect lazy\n  (defn nothing () void effects: (write) (do)))\n(defn caller () void (lazy.nothing))\n";
    let found = check(source);
    let found_codes = found.iter().map(Diagnostic::code).collect::<Vec<_>>();
    assert!(found_codes.contains(&C::ContractUnusedEffect), "{found:#?}");
    // The caller performs `lazy` and `write` whether or not the body does.
    assert_eq!(
        found_codes
            .iter()
            .filter(|code| **code == C::EffectOutsideCeiling)
            .count(),
        2,
        "{found:#?}"
    );
}

#[test]
fn an_atom_in_a_row_and_an_unknown_symbol_are_invalid_references() {
    assert_eq!(
        codes(
            "(deffect write (defn text (target str) void (do)))\n(defn f () void effects: (@write) (do))"
        ),
        vec![C::EffectInvalidReference]
    );
    let source = "(defn f () void effects: (nowhere) (do))";
    let found = check(source);
    assert_eq!(
        found.iter().map(Diagnostic::code).collect::<Vec<_>>(),
        vec![C::EffectInvalidReference]
    );
    assert_eq!(slice(source, found[0].primary_span()), "nowhere");
    // A symbol that names a function, a value, or a type is not a root.
    assert_eq!(
        codes("(defn helper () void (do))\n(defn f () void effects: (helper) (do))"),
        vec![C::EffectInvalidReference]
    );
}

#[test]
fn a_function_type_row_is_resolved_at_the_entry() {
    let source = "(defn accept (callback (fn (i32) i32 effects: (nowhere))) i32 0i32)";
    let found = check(source);
    assert_eq!(
        found.iter().map(Diagnostic::code).collect::<Vec<_>>(),
        vec![C::EffectInvalidReference]
    );
    assert_eq!(slice(source, found[0].primary_span()), "nowhere");
}

#[test]
fn effects_on_a_deffect_is_an_invalid_attribute_at_that_attribute() {
    let source = "(deffect write effects: (write) (defn text (target str) void (do)))";
    let found = check(source);
    assert_eq!(
        found.iter().map(Diagnostic::code).collect::<Vec<_>>(),
        vec![C::SyntaxInvalidAttribute]
    );
    assert_eq!(slice(source, found[0].primary_span()), "effects: (write)");
}

#[test]
fn a_duplicate_entry_names_one_root() {
    // The specification calls a row duplicate-free and gives a repeated
    // entry no diagnostic code; the row is a set, so it keeps one root.
    ok(&format!(
        "{AUDIT}(defn twice () void effects: (write write) (write.text \"a\" \"b\"))\n"
    ));
}

#[test]
fn textually_equal_roots_declared_twice_are_a_redeclaration() {
    assert_eq!(
        codes(
            "(deffect write (defn a () void (do)))\n(deffect write (defn b () void (do)))"
        ),
        vec![C::NameRedeclaration]
    );
    assert_eq!(
        codes("(deffect write (defn a () void (do)))\n(defn write () void (do))"),
        vec![C::NameRedeclaration]
    );
}

#[test]
fn operations_of_one_root_share_one_flat_namespace() {
    assert_eq!(
        codes("(deffect write (defn a () void (do)) (defn a () void (do)))"),
        vec![C::NameMemberCollision]
    );
}

#[test]
fn a_deep_call_chain_is_searched_without_host_recursion() {
    // 3000 functions each calling the next; the last performs `write`. The
    // outermost, with an empty ceiling, is the only offender.
    let depth = 3000;
    let mut source = String::from(
        "(deffect write (defn text (target str) void (do)))\n(defn hop0 () void effects: (write) (write.text \"x\"))\n",
    );
    for index in 1..depth {
        source.push_str(&format!(
            "(defn hop{index} () void effects: (write) (hop{}))\n",
            index - 1
        ));
    }
    source.push_str(&format!("(defn top () void (hop{}))\n", depth - 1));
    let found = check(&source);
    assert_eq!(found.len(), 1, "{}", found.len());
    assert_eq!(
        found[0].related().len(),
        usize::try_from(depth).expect("depth")
    );
}

#[test]
fn an_implementation_within_its_contract_ceiling_is_accepted() {
    ok("(deffect write (defn text (target str) void (do)))
(defint logger
  (defn log (value self) void effects: (write)))
(deftype console (record id i32)
  (impl logger
    (defn log (value self) void effects: (write) (write.text \"c\"))))
(defn use-it (target logger) void effects: (write) (logger.log target))
");
}

#[test]
fn an_implementation_over_its_contract_ceiling_is_outside_it() {
    let source = "(deffect write (defn text (target str) void (do)))
(deffect extra (defn more () void (do)))
(defint logger
  (defn log (value self) void effects: (write)))
(deftype console (record id i32)
  (impl logger
    (defn log (value self) void effects: (write) (write.text \"c\") (extra.more))))
";
    let found = check(source);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].code(), C::EffectOutsideCeiling);
    assert_eq!(slice(source, found[0].primary_span()), "(extra.more)");
}

#[test]
fn a_contract_member_call_contributes_the_ceiling_whatever_the_implementation_performs()
{
    // The implementation is pure, yet the call through the contract performs
    // the ceiling, so the caller must write it.
    let source = "(deffect write (defn text (target str) void (do)))
(defint logger
  (defn log (value self) void effects: (write)))
(deftype console (record id i32)
  (impl logger
    (defn log (value self) void effects: (write) (do))))
(defn use-it (target logger) void (logger.log target))
";
    let found = check(source);
    let found_codes = found.iter().map(Diagnostic::code).collect::<Vec<_>>();
    assert!(found_codes.contains(&C::EffectOutsideCeiling), "{found:#?}");
    let offender = found
        .iter()
        .find(|d| d.code() == C::EffectOutsideCeiling)
        .expect("diagnostic");
    assert_eq!(
        slice(source, offender.primary_span()),
        "(logger.log target)"
    );
    assert!(
        offender.notes()[0].contains("contract member"),
        "{:?}",
        offender.notes()
    );
}

#[test]
fn a_caller_of_a_function_that_did_not_check_is_not_told_its_root_is_unused() {
    // `broken` has a type error, so its performed row is unknown: `caller`
    // may be performing `write` through it, and no warning is sound.
    let source = "(deffect write (defn text (target str) void (do)))
(defn broken () void (write.text 1i32))
(defn caller () void effects: (write) (broken))
";
    let found = codes(source);
    assert!(!found.contains(&C::ContractUnusedEffect), "{found:?}");
    assert!(!found.contains(&C::EffectOutsideCeiling), "{found:?}");
}

#[test]
fn a_callback_parameter_accepts_only_its_exact_row() {
    let declarations = "(deffect write (defn text (target str) void (do)))
(defn call-it (f (fn (str) str) value str) str (f value))
(defn wrote (value str) str effects: (write) (write.text value) value)
";
    assert_eq!(
        codes(&format!(
            "{declarations}(defn use-it () str effects: (write) (call-it wrote \"x\"))\n"
        )),
        vec![C::TypeArgumentMismatch]
    );
}

#[test]
fn a_generic_callback_unifies_only_at_its_exact_row() {
    let declarations = "(deffect write (defn text (target str) void (do)))
(defn call-it (f (fn (t) t) value t) t where: (t any) (f value))
(defn wrote (value str) str effects: (write) (write.text value) value)
";
    assert_eq!(
        codes(&format!(
            "{declarations}(defn use-it () str effects: (write) (call-it wrote \"x\"))\n"
        )),
        vec![C::TypeArgumentMismatch]
    );
}

#[test]
fn an_implementation_that_writes_another_row_is_not_the_contract_signature() {
    let source = "(deffect write (defn text (target str) void (do)))
(defint logger (defn log (value self) void effects: (write)))
(deftype console (record id i32)
  (impl logger
    (defn log (value self) void (do))))
";
    assert_eq!(codes(source), vec![C::TypeMismatch]);
}

#[test]
fn a_lambda_row_that_is_never_performed_is_a_warning_at_its_entry() {
    // The lambda wraps a function of the smaller row and so declares the row
    // it must offer; the unused root is then the warning.
    let source = "(deffect write (defn text (target str) void (do)))
(defn make () (fn (str) void effects: (write))
  (lambda (value str) void effects: (write) (do)))
";
    let found = check(source);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].code(), C::ContractUnusedEffect);
    assert_eq!(slice(source, found[0].primary_span()), "write");
    assert!(found[0].primary_span().start() > source.find("lambda").expect("lambda"));
}

#[test]
fn a_module_value_initializer_has_the_empty_ceiling() {
    // A `def` takes no `effects:` attribute, so applying an operation in its
    // initializer is outside the default-empty ceiling, as in any body.
    let source =
        "(deffect marker (defn mark () i32 1i32))\n(def value i32 (marker.mark))\n";
    let found = check(source);
    assert_eq!(
        found.iter().map(Diagnostic::code).collect::<Vec<_>>(),
        vec![C::EffectOutsideCeiling],
        "{found:#?}"
    );
    assert_eq!(slice(source, found[0].primary_span()), "(marker.mark)");
    // A lambda in the initializer performs nothing until it is applied.
    ok(
        "(deffect marker (defn mark () i32 1i32))\n(def maker (fn () i32 effects: (marker)) (lambda () i32 effects: (marker) (marker.mark)))\n",
    );
}

#[test]
fn an_effect_root_is_not_a_value_and_not_a_type() {
    let found =
        codes("(deffect marker (defn mark () i32 1i32))\n(defn f () void (marker))");
    assert!(!found.is_empty());
    assert!(
        found.contains(&C::NameWrongEntityKind)
            || found.contains(&C::NameUnknownSymbol),
        "{found:?}"
    );
    assert_eq!(
        codes(
            "(deffect marker (defn mark () i32 1i32))\n(defn f (value marker) void (do))"
        ),
        vec![C::NameWrongEntityKind]
    );
}

#[test]
fn a_generic_function_may_write_a_row_and_accept_a_callback_of_it() {
    ok("(deffect marker (defn mark () i32 1i32))
(defn apply (f (fn (t) t effects: (marker)) value t) t
  where: (t any)
  effects: (marker)
  (f value))
(defn use-it () i32
  effects: (marker)
  (apply (lambda (x i32) i32 effects: (marker) (marker.mark)) 3i32))
");
}

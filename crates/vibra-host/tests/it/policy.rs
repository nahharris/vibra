//! `vibra_host::policy` and `vibra_host::path`: the `@scopePolicy.v1`
//! document, its normal form, its hash, its limits, and its evaluation, and
//! the name profile (`docs/spec/03-effects.md`, "Scope policies").
//!
//! Every expected canonical text and hash below is written from the chapter.
//! `F(prefix depth actions names)` in a comment is a `@fileRule` rooted at
//! `@notes`.

use std::fmt::Write as _;

use vibra_host::path::RelativeNames;
use vibra_host::policy::{
    Action, DirAction, DirDepth, DirectoryRule, Expr, FileAction, FileDepth, FileRule,
    MAX_DEPTH, MAX_SIZE, NameTest, Policy, PolicyError, Request, TooComplex, count,
    decode, hash, normalize, permits,
};
use vibra_ir::names::NameError;

use crate::support::{ANY_NAME, on_small_stack, suffix};

/// The document around `expression`.
fn document(expression: &str) -> String {
    format!("{{format: @scopePolicy.v1 family: @filesystem expression: {expression}}}")
}

/// Decodes the document around `expression`.
fn policy(expression: &str) -> Policy {
    decode(document(expression).as_bytes()).expect("the policy decodes")
}

fn decode_error(text: &str) -> PolicyError {
    decode(text.as_bytes()).expect_err("the document is rejected")
}

/// `F(prefix depth actions names)`, as wire text.
fn file(prefix: &str, depth: &str, actions: &str, names: &str) -> String {
    format!(
        "{{kind: @fileRule root: @notes prefix: [{prefix}] depth: {depth} actions: \
         [{actions}] names: {names}}}"
    )
}

/// A `@directoryRule` rooted at `@notes`, as wire text.
fn dir(prefix: &str, depth: &str, actions: &str) -> String {
    format!(
        "{{kind: @directoryRule root: @notes prefix: [{prefix}] depth: {depth} \
         actions: [{actions}]}}"
    )
}

fn any_of(items: &[&str]) -> String {
    format!("{{kind: @anyOf items: [{}]}}", items.join(" "))
}

fn all_of(items: &[&str]) -> String {
    format!("{{kind: @allOf items: [{}]}}", items.join(" "))
}

fn exact(names: &[&str]) -> String {
    let names = names
        .iter()
        .map(|name| format!("\"{name}\""))
        .collect::<Vec<_>>();
    format!("{{kind: @exact names: [{}]}}", names.join(" "))
}

fn owned(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

fn allows(policy: &Policy, names: &[&str], action: Action) -> bool {
    let names = owned(names);
    permits(
        policy.expression(),
        &Request {
            names: &names,
            action,
        },
    )
}

const READ: Action = Action::File(FileAction::Read);
const REPLACE: Action = Action::File(FileAction::Replace);
const LIST: Action = Action::Directory(DirAction::List);

/// A raw directory rule rooted at `@notes`, selecting `prefix` itself.
fn raw_dir(prefix: &str) -> Expr {
    Expr::DirectoryRule(DirectoryRule {
        root: "@notes".to_owned(),
        prefix: RelativeNames::new([prefix]).expect("a valid name"),
        depth: DirDepth::Itself,
        actions: vec![DirAction::List],
    })
}

fn dir_text(prefix: &str) -> String {
    dir(&format!("\"{prefix}\""), "@self", "@list")
}

// ---------------------------------------------------------------------------
// The wire form.

#[test]
fn decode_a_file_rule() {
    let rule = file("", "@tree", "@read", ANY_NAME);
    let decoded = policy(&rule);
    assert_eq!(
        decoded.canonical_bytes(),
        "{format: @scopePolicy.v1 family: @filesystem expression: {kind: @fileRule root: \
         @notes prefix: [] depth: @tree actions: [@read] names: {kind: @anyName}}}"
    );
    let Expr::FileRule(rule) = decoded.expression() else {
        panic!("a file rule");
    };
    assert_eq!(rule.root, "@notes");
    assert!(rule.prefix.is_empty());
    assert_eq!(rule.depth, FileDepth::Tree);
    assert_eq!(rule.actions, vec![FileAction::Read]);
    assert_eq!(rule.names, NameTest::AnyName);
}

#[test]
fn an_unknown_field_is_an_error() {
    let rule = "{kind: @fileRule root: @notes prefix: [] depth: @tree actions: [@read] \
                names: {kind: @anyName} extra: 1u8}";
    assert!(matches!(
        decode_error(&document(rule)),
        PolicyError::Malformed { .. }
    ));
    let at_the_top = format!(
        "{{format: @scopePolicy.v1 family: @filesystem expression: {} extra: 1u8}}",
        file("", "@tree", "@read", ANY_NAME)
    );
    assert!(matches!(
        decode_error(&at_the_top),
        PolicyError::Malformed { .. }
    ));
}

#[test]
fn an_unknown_kind_is_an_error() {
    assert!(matches!(
        decode_error(&document("{kind: @glob}")),
        PolicyError::Malformed { .. }
    ));
    let names = file("", "@tree", "@read", "{kind: @glob}");
    assert!(matches!(
        decode_error(&document(&names)),
        PolicyError::Malformed { .. }
    ));
}

#[test]
fn an_unknown_version_is_an_error() {
    let text = format!(
        "{{format: @scopePolicy.v2 family: @filesystem expression: {}}}",
        file("", "@tree", "@read", ANY_NAME)
    );
    assert!(matches!(
        decode_error(&text),
        PolicyError::UnsupportedVersion { .. }
    ));
}

#[test]
fn a_missing_field_is_an_error() {
    let rule =
        "{kind: @fileRule root: @notes prefix: [] depth: @tree actions: [@read]}";
    let PolicyError::Malformed { reason, .. } = decode_error(&document(rule)) else {
        panic!("malformed");
    };
    assert!(reason.contains("`names`"), "{reason}");
}

#[test]
fn an_empty_suffix_is_an_error() {
    let rule = file("", "@tree", "@read", &suffix(""));
    let PolicyError::Malformed { reason, .. } = decode_error(&document(&rule)) else {
        panic!("malformed");
    };
    assert!(reason.contains("empty suffix"), "{reason}");
}

#[test]
fn the_family_http_is_reserved() {
    let text = format!(
        "{{format: @scopePolicy.v1 family: @http expression: {}}}",
        file("", "@tree", "@read", ANY_NAME)
    );
    assert!(matches!(
        decode_error(&text),
        PolicyError::ReservedFamily { .. }
    ));
}

#[test]
fn a_prefix_name_outside_the_profile_is_an_error() {
    let rule = file("\"..\"", "@tree", "@read", ANY_NAME);
    assert!(matches!(
        decode_error(&document(&rule)),
        PolicyError::Malformed { .. }
    ));
}

// ---------------------------------------------------------------------------
// The normal form and its hash.

#[test]
fn normalize_flattens_nested_wrappers_of_one_kind() {
    let (a, b, c) = (raw_dir("a"), raw_dir("b"), raw_dir("c"));
    // allOf[allOf[A B] C], written with each list out of order.
    let raw = Expr::AllOf(vec![c.clone(), Expr::AllOf(vec![b.clone(), a.clone()])]);
    let expected = all_of(&[&dir_text("a"), &dir_text("b"), &dir_text("c")]);
    assert_eq!(
        normalize(&raw)
            .expect("within the limits")
            .canonical_bytes(),
        expected
    );
    let written = all_of(&[&all_of(&[&dir_text("a"), &dir_text("b")]), &dir_text("c")]);
    assert_eq!(
        policy(&written).canonical_bytes(),
        document(&expected),
        "decoding normalizes"
    );
    // Wrappers of distinct kinds are not flattened.
    let mixed = Expr::AllOf(vec![c, Expr::AnyOf(vec![a, b])]);
    assert_eq!(
        normalize(&mixed)
            .expect("within the limits")
            .canonical_bytes(),
        all_of(&[&any_of(&[&dir_text("a"), &dir_text("b")]), &dir_text("c")])
    );
}

#[test]
fn normalize_removes_duplicates_and_single_item_wrappers() {
    let raw = Expr::AnyOf(vec![raw_dir("a"), raw_dir("a")]);
    assert_eq!(
        normalize(&raw)
            .expect("within the limits")
            .canonical_bytes(),
        dir_text("a")
    );
    let wrapped = all_of(&[&any_of(&[&dir_text("a")])]);
    assert_eq!(policy(&wrapped).canonical_bytes(), document(&dir_text("a")));
}

#[test]
fn normalize_sorts_sets() {
    let rule = file("", "@tree", "@stat @read @stat", &exact(&["b", "a", "b"]));
    assert_eq!(
        policy(&rule).canonical_bytes(),
        document(&file("", "@tree", "@read @stat", &exact(&["a", "b"])))
    );
}

#[test]
fn normalize_keeps_the_order_of_a_prefix() {
    let rule = file("\"b\" \"a\"", "@tree", "@read", ANY_NAME);
    assert_eq!(policy(&rule).canonical_bytes(), document(&rule));
}

#[test]
fn normalize_name_tests_like_expressions() {
    let names = any_of(&[&any_of(&[&suffix(".txt"), &suffix(".md")]), &suffix(".md")]);
    let rule = file("", "@tree", "@read", &names);
    assert_eq!(
        policy(&rule).canonical_bytes(),
        document(&file(
            "",
            "@tree",
            "@read",
            &any_of(&[&suffix(".md"), &suffix(".txt")])
        ))
    );
}

#[test]
fn equal_normal_forms_have_one_hash() {
    let (a, b) = (dir_text("a"), dir_text("b"));
    let first = policy(&any_of(&[&a, &b]));
    let second = policy(&any_of(&[&b, &a]));
    assert_eq!(hash(&first), hash(&second));
}

#[test]
fn equal_meanings_may_hash_differently() {
    let (a, b) = (dir_text("a"), dir_text("b"));
    let plain = policy(&a);
    let wrapped = policy(&all_of(&[&a, &any_of(&[&a, &b])]));
    assert_ne!(hash(&plain), hash(&wrapped));
}

#[test]
fn the_hash_is_sha256_over_the_canonical_bytes() {
    // Computed outside the crate: the SHA-256 of
    // `{format: @scopePolicy.v1 family: @filesystem expression: {kind: @fileRule
    // root: @notes prefix: [] depth: @tree actions: [@read] names: {kind:
    // @suffix value: ".txt"}}}`, on one line.
    let notes = policy(&file("", "@tree", "@read", &suffix(".txt")));
    assert_eq!(
        hash(&notes),
        "sha256:7c5a3b882cc9519c712c1bc358509980e5278c00c8e146869f454cbaf61d2b2f"
    );
}

// ---------------------------------------------------------------------------
// The meaning.

#[test]
fn none_permits_nothing() {
    let none = policy("{kind: @none}");
    assert!(!allows(&none, &["a.txt"], READ));
    assert!(!allows(&none, &[], LIST));
    assert!(!allows(&none, &["d"], Action::Directory(DirAction::Stat)));
}

#[test]
fn an_empty_all_of_permits() {
    assert!(allows(&policy(&all_of(&[])), &["a.txt"], READ));
}

#[test]
fn an_empty_any_of_permits_nothing() {
    assert!(!allows(&policy(&any_of(&[])), &["a.txt"], READ));
}

#[test]
fn a_file_rule_children_is_one_further_name() {
    let children = policy(&file("", "@children", "@read", ANY_NAME));
    assert!(allows(&children, &["a.txt"], READ));
    assert!(!allows(&children, &["d", "a.txt"], READ));
}

#[test]
fn a_file_rule_tree_is_any_positive_number_of_names() {
    let tree = policy(&file("", "@tree", "@read", ANY_NAME));
    assert!(allows(&tree, &["d", "e", "a.txt"], READ));
    let below = policy(&file("\"d\"", "@tree", "@read", ANY_NAME));
    assert!(!allows(&below, &["d"], READ), "the prefix itself");
    assert!(allows(&below, &["d", "a.txt"], READ));
}

#[test]
fn a_prefix_is_compared_by_names() {
    let reports = policy(&file("\"reports\"", "@tree", "@read", ANY_NAME));
    assert!(!allows(&reports, &["reportsOld", "a.txt"], READ));
    assert!(allows(&reports, &["reports", "a.txt"], READ));
}

#[test]
fn a_file_rule_permits_only_its_actions() {
    let read = policy(&file("", "@tree", "@read", ANY_NAME));
    assert!(!allows(&read, &["a.txt"], REPLACE));
}

/// `ATK-policy-rule-fields-crossed`: reading under `a` and replacing under
/// `b` does not permit replacing under `a`.
#[test]
fn atk_policy_rule_fields_crossed() {
    let crossed = policy(&any_of(&[
        &file("\"a\"", "@tree", "@read", ANY_NAME),
        &file("\"b\"", "@tree", "@replace", ANY_NAME),
    ]));
    assert!(!allows(&crossed, &["a", "x"], REPLACE));
    assert!(allows(&crossed, &["a", "x"], READ));
    assert!(allows(&crossed, &["b", "x"], REPLACE));
}

#[test]
fn a_suffix_accepts_exact_scalars_at_the_end() {
    let text = policy(&file("", "@tree", "@read", &suffix(".txt")));
    assert!(allows(&text, &["notes.txt"], READ));
    assert!(allows(&text, &[".txt"], READ));
    assert!(!allows(&text, &["notes.TXT"], READ));
    assert!(!allows(&text, &["notes.txt.exe"], READ));
}

#[test]
fn exact_names() {
    let exact = policy(&file("", "@tree", "@read", &exact(&["a", "b"])));
    assert!(allows(&exact, &["a"], READ));
    assert!(!allows(&exact, &["c"], READ));
}

#[test]
fn name_tests_combine_with_all_of_and_any_of() {
    let both = policy(&file(
        "",
        "@tree",
        "@read",
        &all_of(&[&suffix(".txt"), &exact(&["a.txt"])]),
    ));
    assert!(allows(&both, &["a.txt"], READ));
    assert!(!allows(&both, &["b.txt"], READ));
    let either = policy(&file(
        "",
        "@tree",
        "@read",
        &any_of(&[&suffix(".md"), &exact(&["a.txt"])]),
    ));
    assert!(allows(&either, &["a.txt"], READ));
    assert!(allows(&either, &["c.md"], READ));
    assert!(!allows(&either, &["b.txt"], READ));
}

#[test]
fn a_file_rule_grants_no_listing() {
    let read = policy(&file("", "@tree", "@read", ANY_NAME));
    assert!(!allows(&read, &[], LIST));
}

#[test]
fn a_directory_rule_self_is_the_prefix() {
    let own = policy(&dir("\"d\"", "@self", "@list"));
    assert!(allows(&own, &["d"], LIST));
    assert!(!allows(&own, &["d", "e"], LIST));
}

#[test]
fn a_directory_rule_children_is_one_name_further() {
    let children = policy(&dir("\"d\"", "@children", "@list"));
    assert!(allows(&children, &["d", "e"], LIST));
    assert!(!allows(&children, &["d"], LIST));
}

#[test]
fn a_directory_rule_tree_is_the_prefix_and_below() {
    let tree = policy(&dir("\"d\"", "@tree", "@list"));
    assert!(allows(&tree, &["d"], LIST));
    assert!(allows(&tree, &["d", "e", "f"], LIST));
    assert!(!allows(&tree, &["x"], LIST));
}

#[test]
fn stat_covers_the_kind_of_its_rule() {
    let stat = policy(&file("", "@tree", "@stat", ANY_NAME));
    assert!(!allows(&stat, &["d"], Action::Directory(DirAction::Stat)));
    assert!(allows(&stat, &["d"], Action::File(FileAction::Stat)));
    let directories = policy(&dir("", "@tree", "@stat"));
    assert!(!allows(
        &directories,
        &["a.txt"],
        Action::File(FileAction::Stat)
    ));
}

// ---------------------------------------------------------------------------
// The limits.

/// A file rule whose `@exact` holds `names` distinct names: two nodes and one
/// action, so `names + 3` nodes and entries.
fn exact_rule(names: usize) -> String {
    let mut list = String::new();
    for index in 0..names {
        let _ = write!(list, " \"n{index}\"");
    }
    format!(
        "{{kind: @fileRule root: @notes prefix: [] depth: @tree actions: [@read] names: \
         {{kind: @exact names: [{}]}}}}",
        list.trim_start()
    )
}

#[test]
fn a_policy_of_4096_nodes_and_entries_is_accepted() {
    let accepted = policy(&exact_rule(4_093));
    assert_eq!(accepted.size().total, MAX_SIZE);
    assert_eq!(accepted.size().depth, 2);
}

#[test]
fn a_policy_of_4097_nodes_and_entries_is_too_complex() {
    let text = document(&exact_rule(4_094));
    assert_eq!(decode_error(&text), PolicyError::TooComplex);
}

/// `levels` wrappers that alternate `anyOf` and `allOf`, each over a distinct
/// directory rule and the next wrapper, around one directory rule: a depth of
/// `levels + 1` that normalization keeps.
fn alternating(levels: usize) -> String {
    let mut text = String::new();
    for level in 0..levels {
        let kind = if level % 2 == 0 { "@anyOf" } else { "@allOf" };
        let _ = write!(
            text,
            "{{kind: {kind} items: [{} ",
            dir_text(&format!("l{level}"))
        );
    }
    text.push_str(&dir_text("x"));
    for _ in 0..levels {
        text.push_str("]}");
    }
    text
}

#[test]
fn a_policy_of_depth_64_is_accepted_and_65_is_too_complex() {
    let deepest = policy(&alternating(MAX_DEPTH - 1));
    assert_eq!(deepest.size().depth, MAX_DEPTH);
    assert_eq!(
        decode_error(&document(&alternating(MAX_DEPTH))),
        PolicyError::TooComplex
    );
}

#[test]
fn each_narrowing_of_the_ancestry_adds_one_level() {
    let deepest = policy(&alternating(MAX_DEPTH - 1));
    assert!(Policy::new(deepest.expression(), 0).is_ok());
    assert_eq!(Policy::new(deepest.expression(), 1), Err(TooComplex));
    let shallow = policy(&dir_text("a"));
    assert!(Policy::new(shallow.expression(), MAX_DEPTH - 1).is_ok());
    assert_eq!(
        Policy::new(shallow.expression(), MAX_DEPTH),
        Err(TooComplex)
    );
}

#[test]
fn the_count_is_taken_after_normalization() {
    // 5,000 copies of one rule, each a node and two entries, under one
    // wrapper: 15,001 as written, and one rule once duplicates are removed.
    let raw = Expr::AnyOf((0..5_000).map(|_| raw_dir("a")).collect());
    assert_eq!(count(&raw).total, 15_001);
    let normal = Policy::new(&raw, 0).expect("within the limits");
    assert_eq!(normal.size().total, 3);
    assert_eq!(normal.canonical_bytes(), document(&dir_text("a")));
}

/// `levels` nested `anyOf`, each over a distinct directory rule and the next
/// wrapper, around `none`.
fn nested_any_of(levels: usize) -> String {
    let mut text = String::new();
    for level in 0..levels {
        let _ = write!(
            text,
            "{{kind: @anyOf items: [{} ",
            dir_text(&format!("n{level}"))
        );
    }
    text.push_str("{kind: @none}");
    for _ in 0..levels {
        text.push_str("]}");
    }
    document(&text)
}

#[test]
fn a_deep_input_is_too_complex_without_overflow() {
    let text = nested_any_of(100_000);
    let result = on_small_stack(move || decode(text.as_bytes()));
    assert_eq!(result, Err(PolicyError::TooComplex));
}

#[test]
fn a_deep_input_of_single_item_wrappers_normalizes_to_its_item() {
    // Counted after normalization: the 100,000 wrappers are removed.
    let levels = 100_000;
    let mut text = String::new();
    for _ in 0..levels {
        text.push_str("{kind: @anyOf items: [");
    }
    text.push_str(&dir_text("a"));
    for _ in 0..levels {
        text.push_str("]}");
    }
    let text = document(&text);
    let result =
        on_small_stack(move || decode(text.as_bytes()).map(|p| p.canonical_bytes()));
    assert_eq!(result, Ok(document(&dir_text("a"))));
}

#[test]
fn a_deep_expression_is_walked_without_recursion() {
    let outcome = on_small_stack(|| {
        let mut deep = Expr::None;
        for level in 0..100_000 {
            let item = raw_dir(&format!("n{level}"));
            deep = if level % 2 == 0 {
                Expr::AnyOf(vec![item, deep])
            } else {
                Expr::AllOf(vec![item, deep])
            };
        }
        let copy = deep.clone();
        let equal = copy == deep;
        let size = count(&deep);
        let names = owned(&["n0"]);
        let permitted = permits(
            &deep,
            &Request {
                names: &names,
                action: LIST,
            },
        );
        let normalized = normalize(&deep);
        let rules = deep.rules().count();
        (equal, size.depth, permitted, normalized.is_err(), rules)
    });
    // The outermost wrapper is an `allOf` whose own rule is `n99999`, so the
    // request for `n0` is denied.
    assert_eq!(outcome, (true, 100_001, false, true, 100_000));
}

// ---------------------------------------------------------------------------
// The name profile.

#[test]
fn a_relative_path_checks_every_name() {
    assert!(RelativeNames::new(Vec::<String>::new()).is_ok());
    let invalid = RelativeNames::new(["a", "..", "b"]).expect_err("`..` is no name");
    assert_eq!(invalid.index, 1);
    assert_eq!(invalid.error, NameError::DotName);
    let reports = RelativeNames::new(["reports"]).expect("a valid name");
    assert!(reports.is_prefix_of(&owned(&["reports", "a.txt"])));
    assert!(!reports.is_prefix_of(&owned(&["reportsOld"])));
}

#[test]
fn a_file_rule_names_its_fields_together() {
    // A raw rule built in code is normalized like a decoded one.
    let rule = Expr::FileRule(FileRule {
        root: "@notes".to_owned(),
        prefix: RelativeNames::empty(),
        depth: FileDepth::Children,
        actions: vec![FileAction::Stat, FileAction::Read],
        names: NameTest::AnyName,
    });
    assert_eq!(
        normalize(&rule)
            .expect("within the limits")
            .canonical_bytes(),
        file("", "@children", "@read @stat", ANY_NAME)
    );
}

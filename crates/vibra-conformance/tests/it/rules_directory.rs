//! The rule index is `rules.tsv` and every `rules/*.tsv` of a conformance
//! root, and a rule identity is a row of exactly one file.

use std::path::PathBuf;

use crate::support::{reader_case, rules, scratch_directory, write};

use vibra_conformance::{Corpus, RuleIndex, RuleStatus, RulesError};

/// A conformance root holding `files`, each a path below the root and its
/// text.
fn conformance_root(files: &[(&str, String)]) -> PathBuf {
    let root = scratch_directory("rules-directory");
    for (path, text) in files {
        write(&root, path, text);
    }
    root
}

/// The file, line, and message of a format error of a rules file.
fn invalid(error: RulesError) -> (String, usize, String) {
    let text = error.to_string();
    match error {
        RulesError::Invalid {
            file,
            line,
            message,
        } => (file, line, message),
        RulesError::Io { .. } => panic!("a format error, not {text}"),
    }
}

#[test]
fn rules_two_files() {
    let root = conformance_root(&[
        ("rules.tsv", rules(&[("V2-EFFECT-a", "active")])),
        ("rules/m2-01.tsv", rules(&[("V2-GRANT-b", "retired")])),
    ]);
    let index = RuleIndex::from_root(&root).expect("the index loads");
    assert_eq!(index.status("V2-EFFECT-a"), Some(RuleStatus::Active));
    assert_eq!(index.status("V2-GRANT-b"), Some(RuleStatus::Retired));
    assert_eq!(index.len(), 2);
}

#[test]
fn rules_two_files_a_case_may_address_a_rule_of_the_directory() {
    let id = "V2-SRC-READER-b";
    let root = conformance_root(&[
        ("rules.tsv", rules(&[("V2-EFFECT-a", "active")])),
        ("rules/m2-01.tsv", rules(&[(id, "active")])),
        (
            &format!("cases/SRC-READER/{id}/case.toml"),
            reader_case(id, id),
        ),
        (
            &format!("cases/SRC-READER/{id}/input.vibon"),
            "{}".to_owned(),
        ),
    ]);
    let corpus = Corpus::discover(root.join("cases")).expect("the corpus loads");
    assert_eq!(corpus.len(), 1);
}

#[test]
fn rules_no_directory() {
    let root = conformance_root(&[(
        "rules.tsv",
        rules(&[("V2-EFFECT-a", "active"), ("V2-GRANT-b", "retired")]),
    )]);
    let index = RuleIndex::from_root(&root).expect("the index loads");
    assert_eq!(
        index,
        RuleIndex::parse(&rules(&[
            ("V2-EFFECT-a", "active"),
            ("V2-GRANT-b", "retired"),
        ]))
        .expect("the text decodes")
    );

    let id = "V2-SRC-READER-x";
    let root = conformance_root(&[
        ("rules.tsv", rules(&[(id, "active")])),
        (
            &format!("cases/SRC-READER/{id}/case.toml"),
            reader_case(id, id),
        ),
        (
            &format!("cases/SRC-READER/{id}/input.vibon"),
            "{}".to_owned(),
        ),
    ]);
    let corpus = Corpus::discover(root.join("cases")).expect("the corpus loads");
    assert_eq!(corpus.len(), 1);
}

#[test]
fn rules_empty_file() {
    let root = conformance_root(&[
        ("rules.tsv", rules(&[("V2-EFFECT-a", "active")])),
        ("rules/m2-01.tsv", rules(&[])),
    ]);
    let index = RuleIndex::from_root(&root).expect("the index loads");
    assert_eq!(index.len(), 1);
    assert_eq!(index.status("V2-EFFECT-a"), Some(RuleStatus::Active));
}

#[test]
fn rules_duplicate_across_files() {
    let root = conformance_root(&[
        ("rules.tsv", rules(&[("V2-EFFECT-a", "active")])),
        (
            "rules/m2-01.tsv",
            rules(&[("V2-GRANT-b", "active"), ("V2-EFFECT-a", "active")]),
        ),
    ]);
    let (file, line, message) =
        invalid(RuleIndex::from_root(&root).expect_err("the duplicate is an error"));
    assert_eq!((file.as_str(), line), ("rules/m2-01.tsv", 3));
    assert_eq!(
        message,
        "rule `V2-EFFECT-a` appears twice: its first row is rules.tsv line 2"
    );

    // The same error as a duplicate inside one file.
    let (file, line, message) = invalid(
        RuleIndex::parse(&rules(&[
            ("V2-EFFECT-a", "active"),
            ("V2-EFFECT-a", "retired"),
        ]))
        .expect_err("the duplicate is an error"),
    );
    assert_eq!((file.as_str(), line), ("rules.tsv", 3));
    assert_eq!(
        message,
        "rule `V2-EFFECT-a` appears twice: its first row is rules.tsv line 2"
    );
}

#[test]
fn rules_duplicate_across_files_names_the_later_file_in_byte_order() {
    let root = conformance_root(&[
        ("rules.tsv", rules(&[])),
        ("rules/m2-10.tsv", rules(&[("V2-EFFECT-a", "active")])),
        ("rules/m2-02.tsv", rules(&[("V2-EFFECT-a", "active")])),
    ]);
    let (file, line, message) =
        invalid(RuleIndex::from_root(&root).expect_err("the duplicate is an error"));
    assert_eq!((file.as_str(), line), ("rules/m2-10.tsv", 2));
    assert!(message.contains("rules/m2-02.tsv line 2"), "{message}");
}

#[test]
fn rules_bad_header_in_a_directory_file() {
    let root = conformance_root(&[
        ("rules.tsv", rules(&[("V2-EFFECT-a", "active")])),
        ("rules/x.tsv", "id\tstatus\n".to_owned()),
    ]);
    let error = RuleIndex::from_root(&root).expect_err("the header is an error");
    let text = error.to_string();
    let (file, line, message) = invalid(error);
    assert_eq!((file.as_str(), line), ("rules/x.tsv", 1));
    assert!(message.contains("the header must be"), "{message}");
    assert!(text.starts_with("rules/x.tsv line 1: "), "{text}");
}

#[test]
fn rules_not_a_tsv() {
    let root = conformance_root(&[
        ("rules.tsv", rules(&[("V2-EFFECT-a", "active")])),
        ("rules/README.md", "not a rules file\n".to_owned()),
        ("rules/m2-01.tsv.bak", "id\tstatus\n".to_owned()),
    ]);
    let index = RuleIndex::from_root(&root).expect("the other entries are ignored");
    assert_eq!(index.len(), 1);
}

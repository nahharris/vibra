//! The `@tree.v2` observation, against the two examples of the diagnostics
//! chapter ("Persistent observations").

use vibra_syntax::{parse_data, parse_source, tree_document};

/// The chapter's tree of the valid document `[1u8]`.
const LIST_EXAMPLE: &str = r#"{
  format: @tree.v2
  root: {
    kind: @data
    children: [
      {
        kind: @list
        children: [
          {kind: @punctuation text: "["}
          {kind: @integer text: "1u8"}
          {kind: @punctuation text: "]"}
        ]
      }
    ]
  }
}"#;

/// The chapter's tree of the malformed document `[1u8}`.
const RECOVERED_EXAMPLE: &str = r#"{
  format: @tree.v2
  root: {
    kind: @data
    children: [
      {
        kind: @list
        children: [
          {kind: @punctuation text: "["}
          {kind: @integer text: "1u8"}
          {kind: @error children: [{kind: @punctuation text: "}"}]}
          {kind: @missing text: ""}
        ]
      }
    ]
  }
}"#;

/// Whether the tree of `input` equals `expected` as a decoded VIBON value.
fn tree_matches(input: &str, expected: &str) -> bool {
    let document = parse_data(None, input.as_bytes());
    let tree = tree_document(&document).expect("a tree");
    let actual = parse_data(None, tree.as_bytes());
    assert!(
        actual.diagnostics().is_empty(),
        "{tree}: {:?}",
        actual.diagnostics()
    );
    let expected = parse_data(None, expected.as_bytes());
    assert!(
        expected.diagnostics().is_empty(),
        "{:?}",
        expected.diagnostics()
    );
    actual
        .value()
        .expect("a value")
        .decoded_eq(expected.value().expect("a value"))
}

#[test]
fn the_tree_of_a_valid_list_is_the_chapters_first_example() {
    assert!(tree_matches("[1u8]", LIST_EXAMPLE));
}

#[test]
fn the_tree_of_a_recovered_list_is_the_chapters_second_example() {
    assert!(tree_matches("[1u8}", RECOVERED_EXAMPLE));
    assert!(!tree_matches("[1u8]", RECOVERED_EXAMPLE));
}

#[test]
fn trivia_and_fields_are_in_the_tree() {
    let document = parse_data(None, b"{a: 1u8 // one\n}");
    let tree = tree_document(&document).unwrap();
    assert_eq!(
        tree,
        "{format: @tree.v2 root: {kind: @data children: [{kind: @record children: [\
         {kind: @punctuation text: \"{\"} {kind: @field children: [\
         {kind: @identifier text: \"a\"} {kind: @punctuation text: \":\"} \
         {kind: @whitespace text: \" \"} {kind: @integer text: \"1u8\"}]} \
         {kind: @whitespace text: \" \"} {kind: @comment text: \"// one\"} \
         {kind: @whitespace text: \"\\n\"} {kind: @punctuation text: \"}\"}]}]}}"
    );
}

#[test]
fn a_source_tree_has_the_source_root_and_no_generic_group_node() {
    let document = parse_source(None, b"f(x)");
    let tree = tree_document(&document).unwrap();
    assert!(tree.starts_with("{format: @tree.v2 root: {kind: @source children: ["));
    assert!(!tree.contains("@paren"));
    let decoded = parse_data(None, tree.as_bytes());
    assert!(decoded.diagnostics().is_empty(), "{tree}");
}

#[test]
fn a_document_that_is_not_utf8_has_no_tree() {
    assert!(tree_document(&parse_data(None, &[0x7B, 0xFF, 0x7D])).is_none());
}

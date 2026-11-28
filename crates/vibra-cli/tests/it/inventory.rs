//! The inventory is the one table: one row per line of the command surface,
//! read by the parser, `help`, and the unavailable outcome.

use std::path::Path;

use serde_json::Value;
use vibra_cli::args::parse;
use vibra_cli::inventory::{LINES, OPERATION_PROFILES, capabilities, profile_of};

/// The specification chapter the command surface is written in.
const TOOLING: &str = include_str!("../../../../docs/spec/05-tooling.md");

/// The command lines of the specification's "Command surface" block, after
/// `vibra [GLOBAL...] `.
fn specified_lines() -> Vec<String> {
    let section = TOOLING
        .split("## Command surface")
        .nth(1)
        .expect("the chapter has a command surface");
    let block = section
        .split("```text")
        .nth(1)
        .and_then(|rest| rest.split("```").next())
        .expect("the command surface has a code block");
    block
        .lines()
        .filter_map(|line| line.strip_prefix("vibra [GLOBAL...] "))
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_inventory_has_one_row_per_line_of_the_command_surface() {
    let specified = specified_lines();
    assert_eq!(specified.len(), 18);
    let rows = LINES
        .iter()
        .map(|line| line.synopsis.to_owned())
        .collect::<Vec<_>>();
    assert_eq!(rows, specified);
}

#[test]
fn every_parsed_command_has_an_inventory_row() {
    for (position, line) in LINES.iter().enumerate() {
        let mut arguments = line
            .words
            .iter()
            .map(|word| (*word).into())
            .collect::<Vec<_>>();
        for index in 0..line.required {
            arguments.push(match (index, line.first_operand.first()) {
                (0, Some(kind)) => (*kind).into(),
                _ => "x".into(),
            });
        }
        let invocation = parse(&arguments, Path::new(".")).expect("a well-formed line");
        assert!(
            std::ptr::eq(invocation.line, &LINES[position]),
            "{:?} selects its own row",
            line.words
        );
    }
    for spelling in ["--help", "-h"] {
        let invocation = parse(&[spelling.into()], Path::new(".")).expect("help");
        assert_eq!(invocation.line.name(), "help");
    }
}

#[test]
fn the_capabilities_are_the_rows() {
    let capabilities = capabilities();
    assert_eq!(capabilities.len(), LINES.len());
    for (capability, line) in capabilities.iter().zip(&LINES) {
        assert_eq!(capability.command, line.name());
        assert_eq!(capability.supported, line.supported);
    }
}

/// The profile table of the diagnostics chapter: each row's profile and the
/// case operations it lists as its own.
fn specified_profiles() -> Vec<(String, String)> {
    let chapter =
        include_str!("../../../../docs/spec/07-diagnostics-and-conformance.md");
    let section = chapter
        .split("## Conformance profiles")
        .nth(1)
        .expect("the chapter has the profile table");
    let mut rows = Vec::new();
    for line in section.lines().skip_while(|line| !line.starts_with("| `")) {
        let Some(cells) = line.strip_prefix('|') else {
            break;
        };
        let cells = cells.split('|').map(str::trim).collect::<Vec<_>>();
        let profile = cells[0].trim_matches('`');
        for operation in cells[2].split(", ") {
            if let Some(operation) = operation
                .strip_prefix('`')
                .and_then(|rest| rest.strip_suffix('`'))
            {
                rows.push((operation.to_owned(), profile.to_owned()));
            }
        }
    }
    rows
}

#[test]
fn the_operation_table_is_the_profile_table_of_the_chapter_and_help() {
    let mut expected = specified_profiles();
    assert_eq!(expected.len(), 10, "{expected:?}");
    expected.push(("help".to_owned(), "toolingV2".to_owned()));
    let mut table = OPERATION_PROFILES
        .iter()
        .map(|(operation, profile)| ((*operation).to_owned(), (*profile).to_owned()))
        .collect::<Vec<_>>();
    expected.sort();
    table.sort();
    assert_eq!(table, expected);
}

#[test]
fn inventory_derived_from_the_profiles() {
    let capabilities = capabilities();
    for (capability, line) in capabilities.iter().zip(&LINES) {
        let derived = line
            .operations
            .iter()
            .map(|operation| profile_of(operation).expect("a known operation"))
            .collect::<Vec<_>>();
        assert_eq!(capability.profiles, derived, "{}", capability.command);
    }
}

/// No row of the inventory names a profile by hand: the `Line` record has no
/// field for one, and the source of the inventory spells a profile only in
/// the operation table.
#[test]
fn no_command_names_a_profile_by_hand() {
    let source = include_str!("../../src/inventory.rs");
    let table = source
        .split("pub static OPERATION_PROFILES")
        .nth(1)
        .and_then(|rest| rest.split("];").next())
        .expect("the operation table");
    let outside = source.replacen(table, "", 1);
    for profile in [
        "readerV2",
        "staticV2",
        "interpreterV2",
        "toolingV2",
        "nativeV1",
        "fullV2",
    ] {
        assert!(
            !outside.contains(&format!("\"{profile}\"")),
            "`{profile}` is named outside the operation table"
        );
    }
}

#[test]
fn the_envelope_schema_names_exactly_the_commands_of_the_inventory() {
    let schema: Value =
        serde_json::from_str(vibra_schema::COMMAND_RESULT_SCHEMA).expect("valid JSON");
    let mut named = schema["properties"]["command"]["enum"]
        .as_array()
        .expect("the command enum")
        .iter()
        .map(|name| name.as_str().expect("a string").to_owned())
        .collect::<Vec<_>>();
    assert_eq!(named.first().map(String::as_str), Some(""));
    named.remove(0);
    let mut inventory = LINES.iter().map(|line| line.name()).collect::<Vec<_>>();
    named.sort();
    inventory.sort();
    assert_eq!(named, inventory);
}

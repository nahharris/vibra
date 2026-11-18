//! The inventory is the one table: one row per line of the command surface,
//! read by the parser, `help`, and the unavailable outcome.

use std::path::Path;

use serde_json::Value;
use vibra_cli::args::parse;
use vibra_cli::inventory::{LINES, capabilities};

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
fn the_capabilities_are_the_rows_and_only_help_is_supported() {
    let capabilities = capabilities();
    assert_eq!(capabilities.len(), LINES.len());
    for (capability, line) in capabilities.iter().zip(&LINES) {
        assert_eq!(capability.command, line.name());
        assert_eq!(capability.supported, line.name() == "help");
        assert_eq!(capability.supported, line.supported);
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

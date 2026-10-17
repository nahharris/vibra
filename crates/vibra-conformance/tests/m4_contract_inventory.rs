//! Keeps the M4 surface inventory exhaustive and owned as the AST evolves.

#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::path::{Path, PathBuf};

const INVENTORY: &str = "docs/roadmap/milestone-4/supported-surface.md";

const ENUMS: [&str; 9] = [
    "ExpressionKind",
    "PatternKind",
    "VariadicBinding",
    "Declaration",
    "TypeMember",
    "TypeExpr",
    "VariadicType",
    "DeftypeBody",
    "Attribute",
];

/// The inherited rows of the README's M3 deferral inventory.
const INHERITED: [&str; 9] = ["I1", "I2", "I3", "I4", "I5", "I6", "I7", "I8", "I9"];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate lives two levels below the workspace root")
        .to_path_buf()
}

fn enum_variants(source: &str, enum_name: &str) -> Vec<String> {
    let marker = format!("pub enum {enum_name} ");
    let start = source
        .find(&marker)
        .unwrap_or_else(|| panic!("missing {marker}"));
    let body = source[start..]
        .split_once('{')
        .map(|(_, rest)| rest)
        .expect("enum body");
    let mut variants = Vec::new();
    for line in body.lines() {
        if line.starts_with('}') {
            break;
        }
        let trimmed = line.trim_start();
        if !trimmed
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_uppercase())
        {
            continue;
        }
        let end = trimmed.find(['(', '{', ',', ' ']).unwrap_or(trimmed.len());
        let variant = &trimmed[..end];
        if variant
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            variants.push(variant.to_owned());
        }
    }
    variants
}

fn inventory_text() -> String {
    std::fs::read_to_string(workspace_root().join(INVENTORY))
        .expect("read the M4 inventory")
}

/// The inventory's variant rows as `(variant, disposition, owner)`.
fn variant_rows() -> Vec<(String, String, String)> {
    inventory_text()
        .lines()
        .filter_map(|line| {
            let cells: Vec<&str> = line.split('|').map(str::trim).collect();
            // A variant row splits into "", variant, disposition, owner, notes, "".
            if cells.len() != 6 || !cells[1].contains("::") {
                return None;
            }
            Some((
                cells[1].trim_matches('`').to_owned(),
                cells[2].to_owned(),
                cells[3].to_owned(),
            ))
        })
        .collect()
}

/// The inherited rows as `(id, owner)`.
fn inherited_rows() -> Vec<(String, String)> {
    inventory_text()
        .lines()
        .filter_map(|line| {
            let cells: Vec<&str> = line.split('|').map(str::trim).collect();
            // An inherited row splits into "", id, row, owner, "".
            if cells.len() != 5 {
                return None;
            }
            let id = cells[1];
            let numbered = id.strip_prefix('I').is_some_and(|rest| {
                !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())
            });
            numbered.then(|| (id.to_owned(), cells[3].to_owned()))
        })
        .collect()
}

/// The step number of an owner spelled `Step 5a`, with a lettered split step
/// belonging to its numbered step, or the first step of a longer owner such as
/// `Step 2; Wasm in Step 9`.
fn owning_step(owner: &str) -> Option<u8> {
    let first = owner.split(';').next()?.trim();
    first
        .strip_prefix("Step ")?
        .trim_end_matches(|c: char| c.is_ascii_lowercase())
        .parse::<u8>()
        .ok()
}

#[test]
fn every_ast_variant_has_exactly_one_m4_row() {
    let ast = std::fs::read_to_string(
        workspace_root().join("crates/vibra-syntax/src/ast.rs"),
    )
    .expect("read the AST");
    let rows = variant_rows();
    let mut expected = 0;
    for enum_name in ENUMS {
        let variants = enum_variants(&ast, enum_name);
        assert!(!variants.is_empty(), "{enum_name} has no variants");
        for variant in variants {
            expected += 1;
            let key = format!("{enum_name}::{variant}");
            let count = rows.iter().filter(|row| row.0 == key).count();
            assert_eq!(count, 1, "{key} needs exactly one M4 inventory row");
        }
    }
    assert_eq!(
        rows.len(),
        expected,
        "the inventory lists a variant the AST lacks"
    );
}

#[test]
fn every_m4_row_names_an_owning_step_inside_its_stage() {
    for (variant, disposition, owner) in variant_rows() {
        match disposition.as_str() {
            "Lowered" => {
                let step = owning_step(&owner)
                    .unwrap_or_else(|| panic!("{variant} has no owning step"));
                assert!(
                    (5..=11).contains(&step),
                    "{variant} is lowered outside Steps 5-11"
                );
            }
            "Stage 4B" => {
                let step = owning_step(&owner)
                    .unwrap_or_else(|| panic!("{variant} has no owning step"));
                assert!(
                    (14..=16).contains(&step),
                    "{variant} is owned outside its Stage 4B steps"
                );
            }
            "Static" => {
                assert_eq!(owner, "—", "{variant} is checked only and has no owner");
            }
            other => panic!("{variant} has unknown disposition {other}"),
        }
    }
}

#[test]
fn every_inherited_row_has_one_owning_step() {
    let rows = inherited_rows();
    for id in INHERITED {
        let owners: Vec<&(String, String)> =
            rows.iter().filter(|row| row.0 == id).collect();
        assert_eq!(owners.len(), 1, "inherited row {id} needs exactly one row");
        let step = owning_step(&owners[0].1)
            .unwrap_or_else(|| panic!("inherited row {id} has no owning step"));
        assert!(
            (2..=22).contains(&step),
            "inherited row {id} is owned outside Steps 2-22"
        );
    }
    assert_eq!(
        rows.len(),
        INHERITED.len(),
        "the inventory lists an inherited row the README does not"
    );
}

//! Keeps the M3 surface inventory exhaustive and owned as the AST evolves.

#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::path::{Path, PathBuf};

const INVENTORY: &str = "docs/roadmap/milestone-3/supported-surface.md";

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

/// The inventory's variant rows as `(variant, disposition, owner)`.
fn inventory_rows() -> Vec<(String, String, String)> {
    let text = std::fs::read_to_string(workspace_root().join(INVENTORY))
        .expect("read the M3 inventory");
    text.lines()
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

#[test]
fn every_ast_variant_has_exactly_one_m3_row() {
    let ast = std::fs::read_to_string(
        workspace_root().join("crates/vibra-syntax/src/ast.rs"),
    )
    .expect("read the AST");
    let rows = inventory_rows();
    let mut expected = 0;
    for enum_name in ENUMS {
        let variants = enum_variants(&ast, enum_name);
        assert!(!variants.is_empty(), "{enum_name} has no variants");
        for variant in variants {
            expected += 1;
            let key = format!("{enum_name}::{variant}");
            let count = rows.iter().filter(|row| row.0 == key).count();
            assert_eq!(count, 1, "{key} needs exactly one M3 inventory row");
        }
    }
    assert_eq!(
        rows.len(),
        expected,
        "the inventory lists a variant the AST lacks"
    );
}

#[test]
fn every_m3_row_names_an_owning_step() {
    for (variant, disposition, owner) in inventory_rows() {
        match disposition.as_str() {
            "Stage 3A" | "Stage 3B" => {
                // A lettered step such as `4b` belongs to its numbered step.
                let step = owner
                    .strip_prefix("Step ")
                    .map(|number| {
                        number.trim_end_matches(|c: char| c.is_ascii_lowercase())
                    })
                    .and_then(|number| number.parse::<u8>().ok())
                    .unwrap_or_else(|| panic!("{variant} has no owning step"));
                let range = if disposition == "Stage 3A" {
                    2..=8
                } else {
                    11..=15
                };
                assert!(
                    range.contains(&step),
                    "{variant} is owned outside its stage"
                );
            }
            "M2" | "M4" => assert_eq!(owner, "—", "{variant} is not M3 work"),
            other => panic!("{variant} has unknown disposition {other}"),
        }
    }
}

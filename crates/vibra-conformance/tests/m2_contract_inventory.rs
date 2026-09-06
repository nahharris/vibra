//! Keeps the M2 supported/deferred inventory exhaustive as the M1 AST evolves.

#![allow(clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate lives two levels below the workspace root")
        .to_path_buf()
}

fn enum_variants(source: &str, enum_name: &str) -> Vec<String> {
    let marker = format!("pub enum {enum_name}");
    let start = source
        .find(&marker)
        .unwrap_or_else(|| panic!("missing {marker}"));
    let body = &source[start..];
    let body = body
        .split_once('{')
        .map(|(_, rest)| rest)
        .expect("enum body");
    let mut variants = Vec::new();
    for line in body.lines() {
        if line.starts_with('}') {
            break;
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with("///") || trimmed.is_empty() {
            continue;
        }
        let Some(first) = trimmed.chars().next() else {
            continue;
        };
        if !first.is_ascii_uppercase() {
            continue;
        }
        let end = trimmed.find(['(', '{', ',', ' ']).unwrap_or(trimmed.len());
        let variant = &trimmed[..end];
        if variant
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            variants.push(variant.to_owned());
        }
    }
    variants
}

#[test]
fn every_m1_ast_variant_has_an_m2_disposition() {
    let root = workspace_root();
    let ast = std::fs::read_to_string(root.join("crates/vibra-syntax/src/ast.rs"))
        .expect("read the M1 AST");
    let inventory = std::fs::read_to_string(
        root.join("docs/roadmap/milestone-2/supported-surface.md"),
    )
    .expect("read the M2 inventory");
    for enum_name in [
        "ExpressionKind",
        "PatternKind",
        "VariadicBinding",
        "Declaration",
        "TypeMember",
        "TypeExpr",
        "VariadicType",
        "DeftypeBody",
        "Attribute",
    ] {
        let variants = enum_variants(&ast, enum_name);
        assert!(!variants.is_empty(), "{enum_name} has no variants");
        for variant in variants {
            let token = format!("`{enum_name}::{variant}`");
            assert!(
                inventory.contains(&token),
                "missing M2 disposition for {token}"
            );
        }
    }
}

#[test]
fn deferred_forms_have_one_explicit_availability_code() {
    let inventory = std::fs::read_to_string(
        workspace_root().join("docs/roadmap/milestone-2/supported-surface.md"),
    )
    .expect("read the M2 inventory");
    let deferred = inventory
        .lines()
        .filter(|line| line.contains("| deferred |"))
        .count();
    assert!(deferred >= 10, "inventory lost deferred M3/M4 coverage");
    assert!(inventory.contains("@tool.unavailable"));
}

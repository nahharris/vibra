//! `migration.md` names every public variant of `Type`, `Expr`, `Value`,
//! `CallTarget`, and `IrError` of `vibra-ir-v1` exactly once.

use std::path::Path;

const MIGRATION: &str = include_str!("../../migration.md");

/// The public variants of `vibra-ir-v1`, by enum.
const V1: &[(&str, &[&str])] = &[
    (
        "Type",
        &[
            "Bool",
            "Void",
            "Never",
            "Char",
            "Str",
            "Bytes",
            "Atom",
            "AtomSingleton",
            "I8",
            "I16",
            "I32",
            "I64",
            "U8",
            "U16",
            "U32",
            "U64",
            "F32",
            "F64",
            "Function",
            "Declared",
            "Record",
            "Enum",
            "Param",
            "Applied",
            "Tuple",
            "Array",
            "Dict",
            "Union",
            "Interface",
            "Any",
        ],
    ),
    (
        "Expr",
        &[
            "Literal",
            "External",
            "Host",
            "Default",
            "Sequence",
            "Variable",
            "Global",
            "Function",
            "Closure",
            "Captured",
            "Let",
            "Match",
            "Widen",
            "Try",
            "Return",
            "If",
            "Call",
            "Record",
            "Variant",
            "Wrap",
            "Project",
            "Tuple",
            "TupleProject",
            "Array",
            "Dict",
            "Lookup",
        ],
    ),
    (
        "Value",
        &[
            "Bool", "Void", "Char", "Str", "Bytes", "Atom", "I8", "I16", "I32", "I64",
            "U8", "U16", "U32", "U64", "F32", "F64",
        ],
    ),
    ("CallTarget", &["Direct", "Indirect", "Contract"]),
    (
        "IrError",
        &[
            "NoFunctions",
            "InvalidEntry",
            "DuplicateFunction",
            "ResultTypeMismatch",
            "InvalidSlotCount",
            "InvalidExpression",
            "RecursiveCall",
            "GlobalInitializerCycle",
        ],
    ),
];

/// The first column of every table row: `` `Type::Bool` ``.
fn migrated() -> Vec<&'static str> {
    MIGRATION
        .lines()
        .filter_map(|line| line.strip_prefix("| `"))
        .filter_map(|rest| rest.split_once('`').map(|(name, _)| name))
        .collect()
}

#[test]
fn every_v1_variant_appears_exactly_once() {
    let migrated = migrated();
    let mut expected = Vec::new();
    for (enumeration, variants) in V1 {
        for variant in *variants {
            let name = format!("{enumeration}::{variant}");
            let count = migrated.iter().filter(|row| **row == name).count();
            assert_eq!(count, 1, "`{name}` appears {count} times");
            expected.push(name);
        }
    }
    assert_eq!(migrated.len(), expected.len(), "no row names anything else");
}

#[test]
fn every_successor_is_a_variant_or_removed() {
    for line in MIGRATION.lines().filter(|line| line.starts_with("| `")) {
        let successor = line.split('|').nth(2).unwrap().trim();
        assert!(
            successor == "removed"
                || (successor.starts_with('`')
                    && successor.ends_with('`')
                    && successor.contains("::")),
            "{line}"
        );
    }
}

/// The variant names of `pub enum <name>` in a v1 source: the lines at one
/// level of indentation that start with an upper-case identifier.
fn variants_in(source: &str, enumeration: &str) -> Vec<String> {
    let header = format!("pub enum {enumeration} {{");
    let Some(start) = source.find(&header) else {
        panic!("no `{header}`");
    };
    source[start + header.len()..]
        .lines()
        .take_while(|line| *line != "}")
        .filter_map(|line| line.strip_prefix("    "))
        .filter(|line| {
            line.starts_with(|character: char| character.is_ascii_uppercase())
        })
        .map(|line| {
            line.chars()
                .take_while(char::is_ascii_alphanumeric)
                .collect::<String>()
        })
        .collect()
}

/// The list above is the v1 source's while the v1 crate is in the tree.
#[test]
fn the_v1_list_matches_the_v1_source_when_present() {
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../vibra-ir-v1/src/lib.rs");
    let Ok(source) = std::fs::read_to_string(source) else {
        return;
    };
    let source = source.replace("\r\n", "\n");
    for (enumeration, variants) in V1 {
        assert_eq!(
            variants_in(&source, enumeration),
            variants
                .iter()
                .map(|variant| (*variant).to_owned())
                .collect::<Vec<_>>(),
            "{enumeration}"
        );
    }
}

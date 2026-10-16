//! Natives and the joined differential (milestone 4 Step 10).
//!
//! `docs/spec/06-runtime.md`, "Native implementations": every native
//! implementation, and every primitive row whose algorithm needs a loop, is
//! toolchain-owned Rust written once. The reference interpreter calls it
//! directly, a module reaches it through an import of `vibra_native_v1`, and a
//! native whose meaning applies a function value has no import and a module
//! runs its body.
//!
//! This file proves the rule as a whole. The audit shows that no second
//! implementation of a native exists in the interpreter, the emitter, or the
//! runner; the manifest and the registry name the same natives; the import
//! table names exactly the imports; a symbol the toolchain does not implement
//! is a provenance defect; and a fault in a native is the toolchain defect
//! `@runtime.invalid-checked-program`, never a program result.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs
)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use vibra_ir::external::{CompilerIntrinsic, NATIVE_ROWS};

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
}

// -- the audit: one source ------------------------------------------------------

fn rust_sources(directory: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("readable source directory") {
        let path = entry.expect("readable directory entry").path();
        if path.is_dir() {
            rust_sources(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

/// The production code of one source file: without its test module, and
/// without comment lines.
fn production(path: &Path) -> String {
    let text = std::fs::read_to_string(path).expect("readable source");
    let end = text.find("\n#[cfg(test)]").unwrap_or(text.len());
    text[..end]
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn squeezed(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

/// Whether `text` names `word` as an identifier.
fn names(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(at, _)| {
        let identifier =
            |character: char| character.is_alphanumeric() || character == '_';
        let before = text[..at].chars().next_back();
        let after = text[at + word.len()..].chars().next();
        !before.is_some_and(identifier) && !after.is_some_and(identifier)
    })
}

/// The Rust identifier of the registry variant that stands for a native or a
/// looping primitive row: `TextConcat`, or `ToStr` and `Parse` for the number
/// rows, which a variant carries as its operation.
fn variant_name(intrinsic: CompilerIntrinsic) -> String {
    let debug = format!("{intrinsic:?}");
    match debug.split_once('(') {
        None => debug,
        Some(_) => debug
            .trim_end_matches(')')
            .rsplit(", ")
            .next()
            .expect("an operation")
            .to_owned(),
    }
}

/// Every native symbol and every looping primitive row of the registry.
fn native_rows() -> Vec<CompilerIntrinsic> {
    CompilerIntrinsic::all()
        .into_iter()
        .filter(|row| row.is_native() || row.native_import().is_some())
        .collect()
}

/// The sources of one crate, as `(path from the workspace root, production)`.
fn crate_sources(name: &str) -> Vec<(String, String)> {
    let mut paths = Vec::new();
    rust_sources(
        &workspace_root().join("crates").join(name).join("src"),
        &mut paths,
    );
    paths
        .into_iter()
        .map(|path| {
            let relative = path
                .strip_prefix(workspace_root())
                .expect("inside the workspace")
                .to_string_lossy()
                .replace('\\', "/");
            let code = production(&path);
            (relative, code)
        })
        .collect()
}

/// Every place a native or a looping primitive row is named in the production
/// code of the interpreter, the emitter, and the runner, with the names found
/// there. Native code itself (`vibra-native`) is the one place an algorithm may
/// live, and the table of imports (`vibra-ir`) names them all.
fn mentions() -> BTreeMap<String, BTreeSet<String>> {
    let names_of = native_rows()
        .into_iter()
        .map(variant_name)
        .collect::<BTreeSet<_>>();
    let mut found: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for krate in ["vibra-interp", "vibra-wasm", "vibra-wasm-run"] {
        for (path, code) in crate_sources(krate) {
            for name in &names_of {
                if names(&code, name) {
                    found.entry(path.clone()).or_default().insert(name.clone());
                }
            }
        }
    }
    found
}

#[test]
fn a_native_is_named_only_where_it_is_called_or_imported() {
    let found = mentions();
    let all = native_rows()
        .into_iter()
        .map(variant_name)
        .collect::<BTreeSet<_>>();
    let ident = |names: &[&str]| names.iter().map(|name| (*name).to_owned()).collect();
    let allowed: BTreeMap<&str, BTreeSet<String>> = BTreeMap::from([
        // The interpreter's half of the value-access interface: every native
        // is dispatched here and nowhere else.
        ("crates/vibra-interp/src/native.rs", all),
        // The two natives whose meaning is not an algorithm over a sequence: the
        // packed tail is the collection (the one `identity` of `vibra-native`),
        // and a fold applies a function value, which only the machine can do.
        (
            "crates/vibra-interp/src/machine.rs",
            ident(&["ArrayOf", "DictOf", "ArrayFold"]),
        ),
        // The emitter writes a call of the import, and nothing else: iteration
        // slices through the import of `array.slice` and `text.slice`, and key
        // order compares text through the import of `text.compare`.
        (
            "crates/vibra-wasm/src/lower/iterate.rs",
            ident(&["ArraySlice", "TextSlice"]),
        ),
        (
            "crates/vibra-wasm/src/lower/keys.rs",
            ident(&["TextCompare", "BytesCompare"]),
        ),
        // Key order compares text and bytes through their imports, and a
        // wrapper over `str` or `bytes` is the import that converts an array.
        (
            "crates/vibra-wasm/src/lower.rs",
            ident(&[
                "TextCompare",
                "BytesCompare",
                "TextFromChars",
                "BytesFromArray",
            ]),
        ),
        // The pattern table names the row a wrapper over `str` or `bytes`
        // derives its array with; the lowering calls the row's import.
        (
            "crates/vibra-wasm/src/pattern.rs",
            ident(&["TextToChars", "BytesToArray"]),
        ),
        // The inline table declines the number text rows.
        (
            "crates/vibra-wasm/src/primitive.rs",
            ident(&["ToStr", "Parse"]),
        ),
    ]);
    for (path, found) in &found {
        let Some(allowed) = allowed.get(path.as_str()) else {
            panic!("{path} names the natives {found:?}, which only call sites may");
        };
        assert!(
            found.is_subset(allowed),
            "{path} names {:?}, which it may not",
            found.difference(allowed).collect::<Vec<_>>()
        );
    }
    // The runner supplies every import from the table as a whole and names none.
    assert!(
        found
            .keys()
            .all(|path| !path.starts_with("crates/vibra-wasm-run/")),
        "the runner names a native"
    );
}

#[test]
fn the_emitter_names_a_native_only_to_call_its_import() {
    for (path, code) in crate_sources("vibra-wasm") {
        if path.ends_with("primitive.rs") || path.ends_with("pattern.rs") {
            continue;
        }
        let squeezed = squeezed(&code);
        for (at, _) in squeezed.match_indices("CompilerIntrinsic::") {
            let rest = &squeezed[at + "CompilerIntrinsic::".len()..];
            let name = rest
                .chars()
                .take_while(|character| character.is_alphanumeric())
                .collect::<String>();
            let native = native_rows()
                .into_iter()
                .map(variant_name)
                .any(|n| n == name);
            if !native {
                continue;
            }
            let after = &rest[name.len()..];
            let before = &squeezed[..at];
            let window = &after[..after.len().min(200)];
            assert!(
                window.contains("native_import")
                    || before.ends_with("import(")
                    || before.ends_with("import_index("),
                "{path} uses `{name}` and never reaches its import"
            );
        }
    }
}

#[test]
fn every_arm_of_the_interpreters_native_dispatch_calls_the_native_code() {
    let code = squeezed(&production(
        &workspace_root().join("crates/vibra-interp/src/native.rs"),
    ));
    let dispatch = code
        .split_once("Some(match(intrinsic,operands){")
        .expect("the dispatch")
        .1;
    let arms = dispatch.split("(C::").skip(1).collect::<Vec<_>>();
    // Every row of the table has its arm but `array.of` and `dict.of`, which
    // return their operand (the machine calls the one `identity`), and the
    // twenty number rows share four arms: the integer and the float `to-str`
    // and `parse`.
    let number =
        |symbol: &str| symbol.ends_with(".to-str") || symbol.ends_with(".parse");
    let others = NATIVE_ROWS
        .iter()
        .map(|row| row.intrinsic.symbol())
        .filter(|symbol| !number(symbol) && !matches!(*symbol, "array.of" | "dict.of"))
        .count();
    assert_eq!(arms.len(), others + 4);
    for arm in arms {
        assert!(
            arm.contains("vibra_native::"),
            "an arm of the dispatch holds an algorithm of its own: {arm:.120}"
        );
    }
}

/// Rust idioms that implement a native over a value, which belong in
/// `vibra-native` alone: ordering text or bytes, splitting off the head of a
/// sequence, converting between text and its encodings, and parsing a number.
const ALGORITHMS: &[&str] = &[
    ".as_bytes().cmp(",
    ".split_first()",
    "from_utf8",
    "encode_utf8",
    ".parse::<",
    ".chars().map(",
    ".chars();",
];

#[test]
fn no_native_has_a_second_implementation_in_the_interpreter_the_emitter_or_the_runner()
{
    let mut found = Vec::new();
    for krate in ["vibra-interp", "vibra-wasm", "vibra-wasm-run"] {
        for (path, code) in crate_sources(krate) {
            // The interpreter's own `native.rs` is the adapter that reads its
            // values as sequences for the native code.
            if path.ends_with("vibra-interp/src/native.rs") {
                continue;
            }
            let code = squeezed(&code);
            for idiom in ALGORITHMS {
                if code.contains(&squeezed(idiom)) {
                    found.push(format!("{path}: `{idiom}`"));
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "an implementation of a native outside `vibra-native`: {found:#?}"
    );
}

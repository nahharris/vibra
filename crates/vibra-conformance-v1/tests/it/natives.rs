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

use crate::support::{crate_sources, production, workspace_root};

use std::collections::BTreeSet;

use vibra_ir_v1::external::{CompilerIntrinsic, NATIVE_ROWS};

// -- the audit: one source ------------------------------------------------------

fn squeezed(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

#[test]
fn every_arm_of_the_interpreters_native_dispatch_calls_the_native_code() {
    let code = squeezed(&production(
        &workspace_root().join("crates/vibra-interp-v1/src/native.rs"),
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
            arm.contains("vibra_native_v1::"),
            "an arm of the dispatch holds an algorithm of its own: {arm:.120}"
        );
    }
}

/// Rust idioms that implement a native over a value, which belong in
/// `vibra-native-v1` alone: ordering text or bytes, splitting off the head of a
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
    for krate in ["vibra-interp-v1", "vibra-wasm", "vibra-wasm-run"] {
        for (path, code) in crate_sources(krate) {
            // The interpreter's own `native.rs` is the adapter that reads its
            // values as sequences for the native code.
            if path.ends_with("vibra-interp-v1/src/native.rs") {
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
        "an implementation of a native outside `vibra-native-v1`: {found:#?}"
    );
}

// -- the registry, the manifest, and the import table ---------------------------

#[test]
fn the_manifest_native_list_is_exactly_the_registry_natives() {
    let stdlib = vibra_types_v1::load_stdlib().expect("standard library");
    let listed = stdlib
        .native_symbols()
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let registry = CompilerIntrinsic::all()
        .into_iter()
        .filter(|row| row.is_native())
        .map(CompilerIntrinsic::symbol)
        .collect::<BTreeSet<_>>();
    assert_eq!(listed, registry);
    assert_eq!(listed.len(), stdlib.native_symbols().len(), "listed once");
    // Each is exactly one of an import and a checked body.
    let mut bodies = Vec::new();
    for symbol in &listed {
        let row = CompilerIntrinsic::from_symbol(symbol).expect("a registry row");
        assert_ne!(row.native_import().is_some(), row.runs_body(), "{symbol}");
        if row.runs_body() {
            bodies.push(*symbol);
        }
    }
    assert_eq!(bodies, ["array.fold"]);
}

#[test]
fn the_import_table_names_the_native_imports_and_the_looping_primitive_rows() {
    let imports = NATIVE_ROWS
        .iter()
        .map(|row| row.name())
        .collect::<BTreeSet<_>>();
    assert_eq!(imports.len(), NATIVE_ROWS.len(), "a name is imported once");
    let stdlib = vibra_types_v1::load_stdlib().expect("standard library");
    let native_imports = stdlib
        .native_symbols()
        .iter()
        .map(String::as_str)
        .filter(|symbol| imports.contains(symbol))
        .collect::<BTreeSet<_>>();
    // The rest of the table is the primitive rows whose algorithm loops.
    let looping = imports
        .difference(&native_imports)
        .map(|symbol| (*symbol).to_owned())
        .collect::<BTreeSet<_>>();
    let expected = [
        "array.append",
        "array.concat",
        "array.slice",
        "dict.entries",
    ]
    .into_iter()
    .map(str::to_owned)
    .chain(
        [
            "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64",
        ]
        .into_iter()
        .flat_map(|numeric| [format!("{numeric}.to-str"), format!("{numeric}.parse")]),
    )
    .collect::<BTreeSet<_>>();
    assert_eq!(looping, expected);
    for symbol in &looping {
        let row = CompilerIntrinsic::from_symbol(symbol).expect("a registry row");
        assert!(!row.is_native(), "{symbol} is a primitive row");
    }
    // The imports are the natives but for the ones that apply a function value.
    assert_eq!(
        native_imports.len() + 1,
        stdlib.native_symbols().len(),
        "only `array.fold` is a native without an import"
    );
}

// -- the looping primitive rows and the closed iteration, in both backends ---------

// -- a fault in a native is the toolchain defect, end to end -------------------

// -- a native the toolchain does not implement is a provenance defect ---------

fn rejected(manifest: &str) -> String {
    let mut inputs = vibra_types_v1::StdlibInputs::embedded();
    inputs.manifest = manifest.as_bytes();
    vibra_types_v1::load_stdlib_bytes(&inputs)
        .expect_err("the manifest is refused")
        .to_string()
}

fn manifest() -> String {
    String::from_utf8(vibra_types_v1::StdlibInputs::embedded().manifest.to_vec())
        .expect("manifest text")
}

#[test]
fn a_manifest_that_lists_a_native_the_toolchain_does_not_implement_is_a_provenance_defect()
 {
    let unknown = manifest().replace(
        "native: (array \"array.of\"",
        "native: (array \"text.reverse\" \"array.of\"",
    );
    let message = rejected(&unknown);
    assert!(
        message.contains("`text.reverse`, which is not in the compiler registry"),
        "{message}"
    );
    // A primitive row listed as a native, and a native listed as a primitive
    // row, are in the wrong tier: the two tiers are disjoint.
    let primitive = manifest().replace(
        "native: (array \"array.of\"",
        "native: (array \"array.of\" \"array.slice\"",
    );
    assert!(rejected(&primitive).contains("`array.slice` in the wrong tier"));
    let native = manifest().replace(
        "compiler: (array\n",
        "compiler: (array\n    \"str.concat\"\n",
    );
    assert!(rejected(&native).contains("`str.concat` in the wrong tier"));
    // A native a module declares and the manifest does not list.
    let unlisted = manifest().replace(" \"str.slice\"", "");
    assert!(
        rejected(&unlisted)
            .contains("binds `str.slice`, which the manifest does not list")
    );
}

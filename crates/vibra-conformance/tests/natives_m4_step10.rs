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

// -- the registry, the manifest, and the import table ---------------------------

#[test]
fn the_manifest_native_list_is_exactly_the_registry_natives() {
    let stdlib = vibra_types::load_stdlib().expect("standard library");
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
    let stdlib = vibra_types::load_stdlib().expect("standard library");
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

/// What one program answers in the interpreter and in the module, and the
/// imports the module declares.
struct Ran {
    interpreter: String,
    wasm: String,
    imports: Vec<(String, String)>,
}

fn imports_of(bytes: &[u8]) -> Vec<(String, String)> {
    let mut found = Vec::new();
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        if let wasmparser::Payload::ImportSection(section) = payload.expect("a module")
        {
            for import in section.into_imports() {
                let import = import.expect("an import");
                found.push((import.module.to_owned(), import.name.to_owned()));
            }
        }
    }
    found
}

fn run_both(source: &str) -> Ran {
    let checked = vibra_types::check_source("input.vib", source);
    let program = checked
        .program()
        .unwrap_or_else(|| panic!("{source}\n{:?}", checked.diagnostics()));
    let interpreter = vibra_interp::run(program)
        .expect("execution")
        .canonical_result();
    let module = vibra_wasm::emit(program)
        .unwrap_or_else(|error| panic!("{source}\nnot lowered: {error}"));
    let ty = program.entry().signature().result();
    let runner =
        vibra_wasm_run::Runner::new(vibra_wasm_run::MemoryLimit::new(64 * 1024 * 1024))
            .expect("the engine configures");
    let wasm = match runner
        .run_observed(module.bytes(), &ty, program.types())
        .expect("the module runs")
    {
        vibra_wasm_run::Observed::Completed { value, .. } => {
            value.canonical_observation(&ty)
        }
        vibra_wasm_run::Observed::Stopped(outcome) => {
            panic!("{source}\nthe module stopped: {outcome:?}")
        }
    };
    Ran {
        interpreter,
        wasm,
        imports: imports_of(module.bytes()),
    }
}

/// Samples of the rows with no Vibra body, which the shared vectors do not cover
/// because their operands are arrays: the written type of the result, the call,
/// and the value the specification gives it, written out as a literal.
fn loop_samples() -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
    vec![
        (
            "array.append",
            "(array i32)",
            "(array.append (array.of 1i32 2i32) 3i32)",
            "(array.of 1i32 2i32 3i32)",
        ),
        (
            "array.append",
            "(array i32)",
            "(array.append (as (array i32) (array.of)) 7i32)",
            "(array.of 7i32)",
        ),
        (
            "array.append",
            "(array str)",
            "(array.append (array.of \"a\" \"\") \"b\")",
            "(array.of \"a\" \"\" \"b\")",
        ),
        (
            "array.append",
            "(array (array u8))",
            "(array.append (array.of (array.of 1u8)) (array.of))",
            "(array.of (array.of 1u8) (as (array u8) (array.of)))",
        ),
        (
            "array.concat",
            "(array i32)",
            "(array.concat (as (array i32) (array.of)) (as (array i32) (array.of)))",
            "(as (array i32) (array.of))",
        ),
        (
            "array.concat",
            "(array i32)",
            "(array.concat (array.of 1i32) (as (array i32) (array.of)))",
            "(array.of 1i32)",
        ),
        (
            "array.concat",
            "(array i32)",
            "(array.concat (as (array i32) (array.of)) (array.of 1i32 2i32))",
            "(array.of 1i32 2i32)",
        ),
        (
            "array.concat",
            "(array str)",
            "(array.concat (array.of \"a\") (array.of \"b\" \"a\"))",
            "(array.of \"a\" \"b\" \"a\")",
        ),
        (
            "array.slice",
            "(option (array i32))",
            "(array.slice (array.of 1i32 2i32 3i32) 1u64 3u64)",
            "(option.some (array.of 2i32 3i32))",
        ),
        (
            "array.slice",
            "(option (array i32))",
            "(array.slice (array.of 1i32 2i32 3i32) 3u64 3u64)",
            "(option.some (as (array i32) (array.of)))",
        ),
        (
            "array.slice",
            "(option (array i32))",
            "(array.slice (array.of 1i32 2i32 3i32) 2u64 1u64)",
            "(as (option (array i32)) (option.none))",
        ),
        (
            "array.slice",
            "(option (array i32))",
            "(array.slice (array.of 1i32 2i32 3i32) 0u64 4u64)",
            "(as (option (array i32)) (option.none))",
        ),
        (
            "array.slice",
            "(option (array i32))",
            "(array.slice (as (array i32) (array.of)) 0u64 0u64)",
            "(option.some (as (array i32) (array.of)))",
        ),
        (
            "array.slice",
            "(option (array str))",
            "(array.slice (array.of \"a\" \"b\") 1u64 2u64)",
            "(option.some (array.of \"b\"))",
        ),
        // The slice of a dict's entries is how the module leaves the remainder
        // of `iter.next` over a dict.
        (
            "array.slice",
            "(option (array (tuple str i32)))",
            "(array.slice (dict.entries (dict.of \"b\" 2i32 \"a\" 1i32 \"c\" 3i32)) 1u64 3u64)",
            "(option.some (array.of (tupleof \"b\" 2i32) (tupleof \"c\" 3i32)))",
        ),
        (
            "dict.entries",
            "(array (tuple str i32))",
            "(dict.entries (dict.of \"b\" 2i32 \"a\" 1i32 \"b\" 3i32))",
            "(array.of (tupleof \"a\" 1i32) (tupleof \"b\" 3i32))",
        ),
        (
            "dict.entries",
            "(array (tuple str i32))",
            "(dict.entries (as (dict str i32) (dict.of)))",
            "(as (array (tuple str i32)) (array.of))",
        ),
        // Atoms are ordered by their spelling through `text.compare`: a prefix
        // before its extension, and `-` (U+002D) before a letter.
        (
            "dict.entries",
            "(array (tuple atom i32))",
            "(dict.entries (dict.of @b 1i32 @ab 2i32 @a 3i32 @a-b 4i32))",
            "(array.of (tupleof @a 3i32) (tupleof @a-b 4i32) (tupleof @ab 2i32) (tupleof @b 1i32))",
        ),
        (
            "dict.entries",
            "(array (tuple bytes u8))",
            "(dict.entries (dict.of (bytes (array.of 2u8)) 1u8 (bytes (array.of 1u8 9u8)) 2u8 (bytes (array.of)) 3u8))",
            "(array.of (tupleof (bytes (array.of)) 3u8) (tupleof (bytes (array.of 1u8 9u8)) 2u8) (tupleof (bytes (array.of 2u8)) 1u8))",
        ),
        // `iter.next`: the first item and the iterator that remains.
        (
            "array.slice",
            "(option (tuple i32 (array i32)))",
            "(iter.next (array.of 1i32 2i32 3i32))",
            "(option.some (tupleof 1i32 (array.of 2i32 3i32)))",
        ),
        (
            "array.slice",
            "(option (tuple i32 (array i32)))",
            "(iter.next (array.of 1i32))",
            "(option.some (tupleof 1i32 (as (array i32) (array.of))))",
        ),
        (
            "array.slice",
            "(option (tuple i32 (array i32)))",
            "(iter.next (as (array i32) (array.of)))",
            "(as (option (tuple i32 (array i32))) (option.none))",
        ),
        (
            "array.slice",
            "(option (tuple (tuple str i32) (dict str i32)))",
            "(iter.next (dict.of \"b\" 2i32 \"a\" 1i32 \"c\" 3i32))",
            "(option.some (tupleof (tupleof \"a\" 1i32) (dict.of \"b\" 2i32 \"c\" 3i32)))",
        ),
        (
            "array.slice",
            "(option (tuple (tuple str i32) (dict str i32)))",
            "(iter.next (dict.of \"a\" 1i32))",
            "(option.some (tupleof (tupleof \"a\" 1i32) (as (dict str i32) (dict.of))))",
        ),
        (
            "array.slice",
            "(option (tuple (tuple str i32) (dict str i32)))",
            "(iter.next (as (dict str i32) (dict.of)))",
            "(as (option (tuple (tuple str i32) (dict str i32))) (option.none))",
        ),
        (
            "array.slice",
            "(option (tuple (tuple atom i32) (dict atom i32)))",
            "(iter.next (dict.of (as atom @b) 1i32 (as atom @a) 2i32))",
            "(option.some (tupleof (tupleof (as atom @a) 2i32) (dict.of (as atom @b) 1i32)))",
        ),
        (
            "text.slice",
            "(option (tuple char str))",
            "(iter.next \"h\u{e9}\u{1d11e}\")",
            "(option.some (tupleof \\h \"\u{e9}\u{1d11e}\"))",
        ),
        (
            "text.slice",
            "(option (tuple char str))",
            "(iter.next \"\")",
            "(as (option (tuple char str)) (option.none))",
        ),
    ]
}

#[test]
fn the_looping_rows_and_iteration_agree_across_both_backends_and_the_literal() {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut declared: BTreeSet<(String, String)> = BTreeSet::new();
    for (row, ty, call, expected) in loop_samples() {
        let source =
            format!("(defn main () {ty} {call})\n(import iter @std.iter.iter)\n");
        let wanted = format!("(defn main () {ty} {expected})\n");
        let ran = run_both(&source);
        let literal = run_both(&wanted);
        assert_eq!(
            ran.interpreter, literal.interpreter,
            "the interpreter's `{row}` differs from the specified value of `{call}`"
        );
        assert_eq!(
            ran.wasm, ran.interpreter,
            "the module's `{row}` differs from the interpreter on `{call}`"
        );
        assert_eq!(literal.wasm, literal.interpreter, "{expected}");
        *counts.entry(row).or_default() += 1;
        declared.extend(ran.imports);
    }
    eprintln!("looping rows and iteration, both backends: {counts:?}");
    for row in [
        "array.append",
        "array.concat",
        "array.slice",
        "dict.entries",
    ] {
        assert!(counts.get(row).is_some_and(|count| *count >= 3), "{row}");
    }
    // The module imports the rows it uses, by their registry names, from the
    // native import module alone.
    let table = NATIVE_ROWS
        .iter()
        .map(|row| row.name())
        .collect::<BTreeSet<_>>();
    for (module, name) in &declared {
        assert_eq!(module, "vibra_native_v1", "{name}");
        assert!(table.contains(name.as_str()), "`{name}` is not on the list");
    }
    for name in [
        "array.append",
        "array.concat",
        "array.slice",
        "dict.entries",
        "text.slice",
        "text.compare",
    ] {
        assert!(
            declared.iter().any(|(_, declared)| declared == name),
            "no program imported `{name}`"
        );
    }
}

// -- a native the toolchain does not implement is a provenance defect ---------

fn rejected(manifest: &str) -> String {
    let mut inputs = vibra_types::StdlibInputs::embedded();
    inputs.manifest = manifest.as_bytes();
    vibra_types::load_stdlib_bytes(&inputs)
        .expect_err("the manifest is refused")
        .to_string()
}

fn manifest() -> String {
    String::from_utf8(vibra_types::StdlibInputs::embedded().manifest.to_vec())
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
        "compiler: (array\n    \"text.concat\"\n",
    );
    assert!(rejected(&native).contains("`text.concat` in the wrong tier"));
    // A native a module declares and the manifest does not list.
    let unlisted = manifest().replace(" \"text.slice\"", "");
    assert!(
        rejected(&unlisted)
            .contains("binds `text.slice`, which the manifest does not list")
    );
}

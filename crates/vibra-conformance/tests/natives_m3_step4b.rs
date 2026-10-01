//! The body/native differential harness (`docs/spec/06-runtime.md`, "Native
//! implementations"): every native implementation the standard-library
//! manifest lists runs against its Vibra body over the same inputs, and both
//! must produce the same canonical result.

#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::path::Path;

use vibra_syntax::{Attribute, Declaration, Literal, TypeMember};

/// Sample calls for each native symbol: the written result type and operand
/// lists. A native without samples fails the harness, so a new native must
/// bring its own.
fn samples(symbol: &str) -> &'static [(&'static str, &'static str)] {
    match symbol {
        "array.of" => &[("(array i32)", ""), ("(array i32)", "1i32 2i32 3i32")],
        // Subtraction does not commute, so the samples fix the fold order.
        "array.fold" => &[
            (
                "i32",
                "(array.of 1i32 2i32 3i32) 10i32 (lambda (total i32 item i32) i32 (match (i32.sub-checked total item) (result.ok difference) difference - 0i32))",
            ),
            (
                "i32",
                "(as (array i32) (array.of)) 7i32 (lambda (total i32 item i32) i32 item)",
            ),
        ],
        "map.of" => &[
            ("(map str i32)", ""),
            ("(map str i32)", "\"b\" 2i32 \"a\" 1i32 \"b\" 3i32"),
        ],
        "text.concat" => &[("str", "\"hé\" \"llo\""), ("str", "\"\" \"\"")],
        "text.length" => &[("u64", "\"\""), ("u64", "\"h𝄞é\"")],
        "text.equal" => &[("bool", "\"é\" \"é\""), ("bool", "\"a\" \"ab\"")],
        "text.compare" => &[
            ("core.ordering", "\"ab\" \"b\""),
            ("core.ordering", "\"ab\" \"a\""),
            ("core.ordering", "\"\u{FFFF}\" \"𝄞\""),
            ("core.ordering", "\"é\" \"é\""),
        ],
        "text.slice" => &[
            ("(option str)", "\"h𝄞llo\" 1u64 3u64"),
            ("(option str)", "\"abc\" 2u64 1u64"),
            ("(option str)", "\"abc\" 0u64 4u64"),
        ],
        "text.to-chars" => &[("(array char)", "\"hé\""), ("(array char)", "\"\"")],
        "text.from-chars" => &[("str", r"(array.of \h \é)"), ("str", "(array.of)")],
        "text.to-utf8" => &[("bytes", "\"aé€𝄞\""), ("bytes", "\"\"")],
        "text.from-utf8" => &[
            (
                "(result str core.conversion-error)",
                "(bytes (array.of 97u8 195u8 169u8 240u8 157u8 132u8 158u8))",
            ),
            (
                "(result str core.conversion-error)",
                "(bytes (array.of 255u8))",
            ),
            (
                "(result str core.conversion-error)",
                "(bytes (array.of 192u8 128u8))",
            ),
            (
                "(result str core.conversion-error)",
                "(bytes (array.of 237u8 160u8 128u8))",
            ),
            (
                "(result str core.conversion-error)",
                "(bytes (array.of 244u8 144u8 128u8 128u8))",
            ),
            (
                "(result str core.conversion-error)",
                "(bytes (array.of 226u8 130u8))",
            ),
        ],
        "bytes.length" => &[("u64", "(bytes (array.of 1u8 2u8))")],
        "bytes.concat" => {
            &[("bytes", "(bytes (array.of 1u8)) (bytes (array.of 2u8 3u8))")]
        }
        "bytes.equal" => &[
            ("bool", "(bytes (array.of 1u8)) (bytes (array.of 1u8))"),
            ("bool", "(bytes (array.of 1u8)) (bytes (array.of))"),
        ],
        "bytes.compare" => &[
            (
                "core.ordering",
                "(bytes (array.of 1u8 9u8)) (bytes (array.of 2u8))",
            ),
            ("core.ordering", "(bytes (array.of 1u8)) (bytes (array.of))"),
        ],
        "bytes.slice" => &[
            ("(option bytes)", "(bytes (array.of 1u8 2u8 3u8)) 1u64 3u64"),
            ("(option bytes)", "(bytes (array.of 1u8)) 1u64 2u64"),
        ],
        "bytes.to-array" => &[("(array u8)", "(bytes (array.of 7u8 8u8))")],
        "bytes.from-array" => &[("bytes", "(array.of 7u8 8u8)")],
        _ => &[],
    }
}

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
}

/// Each native member of `@std.builtin` as `(symbol, type path, reference
/// source)`, where the reference is the member rewritten as an ordinary
/// function over its own Vibra body.
fn native_members() -> Vec<(String, String, String)> {
    let path = workspace_root().join("stdlib/src/std/builtin.vib");
    let text = std::fs::read_to_string(&path).expect("builtin module");
    let document = vibra_syntax::parse_source(&path, &text).expect("parse");
    let ast = document.ast().expect("builtin ast");
    let mut members = Vec::new();
    for declaration in ast.declarations() {
        let Declaration::Deftype(owner) = declaration else {
            continue;
        };
        let owner_generics = owner
            .attributes()
            .items()
            .iter()
            .find_map(|attribute| match attribute {
                Attribute::Where(bindings) => Some(
                    bindings
                        .iter()
                        .map(|binding| format!("{} any", binding.name().value()))
                        .collect::<Vec<_>>()
                        .join(" "),
                ),
                _ => None,
            })
            .unwrap_or_default();
        for member in owner.members() {
            let TypeMember::Method(method) = member else {
                continue;
            };
            let Some(symbol) = method.attributes().items().iter().find_map(
                |attribute| match attribute {
                    Attribute::Native(Literal::String(symbol)) => {
                        Some(symbol.value().to_owned())
                    }
                    _ => None,
                },
            ) else {
                continue;
            };
            let span = method.span();
            let source = &text[span.start()..span.end()];
            // Drop the member-only attributes and name the owner's generics
            // together with the member's own.
            let member_generics = method
                .attributes()
                .items()
                .iter()
                .find_map(|attribute| match attribute {
                    Attribute::Where(bindings) => Some(
                        bindings
                            .iter()
                            .map(|binding| format!(" {} any", binding.name().value()))
                            .collect::<String>(),
                    ),
                    _ => None,
                })
                .unwrap_or_default();
            let mut lines = source
                .lines()
                .filter(|line| {
                    let line = line.trim_start();
                    !line.starts_with("native:")
                        && !line.starts_with("visibility:")
                        && !line.starts_with("where:")
                })
                .map(str::to_owned)
                .collect::<Vec<_>>();
            let header = lines[0].replacen(
                &format!("(defn {} ", method.name().value()),
                "(defn reference ",
                1,
            );
            lines[0] =
                format!("{header}\n  where: ({owner_generics}{member_generics})");
            // A recursive body calls the member itself, which is the reference.
            let path = format!("{}.{}", owner.name().value(), method.name().value());
            let reference = lines.join("\n").replace(&path, "reference");
            members.push((symbol, path, reference));
        }
    }
    members
}

fn run(source_id: &str, source: &str) -> String {
    let checked = vibra_types::check_source(source_id, source);
    finish(source, &checked)
}

/// Runs a module source as trusted standard-library code.
fn run_trusted(source_id: &str, source: &str) -> String {
    let stdlib = vibra_types::load_stdlib().expect("standard library");
    let checked =
        vibra_types::check_standard_library_source(&stdlib, source_id, source);
    finish(source, &checked)
}

fn finish(source: &str, checked: &vibra_types::CheckResult) -> String {
    let program = checked
        .program()
        .unwrap_or_else(|| panic!("{source}\n{:?}", checked.diagnostics()));
    vibra_interp::run(program)
        .expect("execution")
        .canonical_result()
}

/// Each native function of a standard-library module as `(symbol, module
/// path, function name)`.
fn module_natives() -> Vec<(String, &'static str, String)> {
    let mut natives = Vec::new();
    for module in ["std/text.vib", "std/bytes.vib"] {
        let path = workspace_root().join("stdlib/src").join(module);
        let text = std::fs::read_to_string(&path).expect("module");
        let document = vibra_syntax::parse_source(&path, &text).expect("parse");
        for declaration in document.ast().expect("module ast").declarations() {
            let Declaration::Defn(function) = declaration else {
                continue;
            };
            if let Some(symbol) =
                function.attributes().items().iter().find_map(|attribute| {
                    match attribute {
                        Attribute::Native(Literal::String(symbol)) => {
                            Some(symbol.value().to_owned())
                        }
                        _ => None,
                    }
                })
            {
                natives.push((symbol, module, function.name().value().to_owned()));
            }
        }
    }
    natives
}

/// The module rewritten as one checkable source after `main`: without the
/// imports the single-module checker cannot follow and every declaration that
/// depends on them, and without `native:` when `bodies` is set, so each
/// function runs its Vibra body.
fn module_source(module: &str, bodies: bool) -> String {
    let path = workspace_root().join("stdlib/src").join(module);
    let text = std::fs::read_to_string(&path).expect("module");
    let document = vibra_syntax::parse_source(&path, &text).expect("parse");
    let declarations = document.ast().expect("module ast").declarations();
    let words = |source: &str| -> Vec<String> {
        source
            .split(|character: char| {
                !(character.is_alphanumeric() || matches!(character, '-' | '.'))
            })
            .map(str::to_owned)
            .collect()
    };
    // Aliases of other modules, then every function reaching one, to a
    // fixed point.
    let mut dropped: Vec<String> = declarations
        .iter()
        .filter_map(|declaration| match declaration {
            // `@std.core` and its declarations are visible to every run.
            Declaration::Import(import)
                if import.target().value() != "std.core"
                    && !import.target().value().starts_with("std.core.") =>
            {
                Some(import.alias().value().to_owned())
            }
            _ => None,
        })
        .collect();
    loop {
        let before = dropped.len();
        for declaration in declarations {
            let Declaration::Defn(function) = declaration else {
                continue;
            };
            let span = declaration.span();
            let reaches = words(&text[span.start()..span.end()]).iter().any(|word| {
                dropped
                    .iter()
                    .any(|name| word == name || word.starts_with(&format!("{name}.")))
            });
            let name = function.name().value().to_owned();
            if reaches && !dropped.contains(&name) {
                dropped.push(name);
            }
        }
        if dropped.len() == before {
            break;
        }
    }
    let mut kept = Vec::new();
    for declaration in declarations {
        let span = declaration.span();
        let source = &text[span.start()..span.end()];
        let removed = match declaration {
            Declaration::Import(import) => {
                dropped.contains(&import.alias().value().to_owned())
            }
            Declaration::Defn(function) => {
                dropped.contains(&function.name().value().to_owned())
            }
            _ => false,
        };
        if removed {
            continue;
        }
        kept.push(
            source
                .lines()
                .filter(|line| !(bodies && line.trim_start().starts_with("native:")))
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
    kept.join("\n\n")
}

#[test]
fn every_listed_native_matches_its_body() {
    let stdlib = vibra_types::load_stdlib().expect("standard library");
    let members = native_members();
    let modules = module_natives();
    let listed = stdlib.native_symbols();
    assert!(!listed.is_empty());
    for symbol in listed {
        let samples = samples(symbol);
        assert!(!samples.is_empty(), "`{symbol}` has no harness samples");
        for (result, operands) in samples {
            let (native, body) = if let Some((_, path, reference)) =
                members.iter().find(|(member, _, _)| member == symbol)
            {
                (
                    run(
                        "native.vib",
                        &format!("(defn main () {result} ({path} {operands}))"),
                    ),
                    // The body is standard-library code, checked where it lives.
                    run(
                        "stdlib/src/std/builtin.vib",
                        &format!(
                            "(defn main () {result} (reference {operands}))\n{reference}"
                        ),
                    ),
                )
            } else {
                let (_, module, name) = modules
                    .iter()
                    .find(|(native, _, _)| native == symbol)
                    .unwrap_or_else(|| panic!("`{symbol}` has no native declaration"));
                let source_id = format!("stdlib/src/{module}");
                let main = format!("(defn main () {result} ({name} {operands}))");
                (
                    run_trusted(
                        &source_id,
                        &format!("{main}\n\n{}", module_source(module, false)),
                    ),
                    run_trusted(
                        &source_id,
                        &format!("{main}\n\n{}", module_source(module, true)),
                    ),
                )
            };
            assert_eq!(
                native, body,
                "`{symbol}` disagrees with its body on `{operands}`"
            );
        }
    }
}

#[test]
fn the_harness_reports_a_body_that_disagrees() {
    // A reference that drops its tail must not match `array.of`.
    let native = run(
        "native.vib",
        "(defn main () (array i32) (array.of 1i32 2i32))",
    );
    let wrong = run(
        "stdlib/src/std/builtin.vib",
        "(defn main () (array i32) (reference 1i32 2i32))\n\
         (defn reference () (array t)\n  where: (t any)\n  variadic: (items (array t))\n  (array.of))",
    );
    assert_ne!(native, wrong);
}

/// Operand pairs for each type whose key contracts the standard library
/// implements: the module that writes the implementation, then the pairs.
fn key_samples() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        (
            "std/bool.vib",
            vec!["false true", "true false", "true true", "false false"],
        ),
        (
            "std/text.vib",
            vec!["\"ab\" \"b\"", "\"b\" \"ab\"", "\"é\" \"é\"", "\"\" \"a\""],
        ),
        (
            "std/bytes.vib",
            vec![
                "(bytes (array.of 1u8 2u8)) (bytes (array.of 1u8 3u8))",
                "(bytes (array.of 2u8)) (bytes (array.of 1u8 9u8))",
                "(bytes (array.of)) (bytes (array.of))",
            ],
        ),
        (
            "std/core.vib",
            vec![
                "-1i8 1i8",
                "300i16 -300i16",
                "7i32 7i32",
                "-9i64 -8i64",
                "200u8 100u8",
                "1u16 2u16",
                "5u32 5u32",
                "18446744073709551615u64 0u64",
                r"\a \b",
                r"\é \e",
                r"\z \z",
            ],
        ),
    ]
}

/// The closed registry is the native of the library's key conformances
/// (`docs/spec/02-type-system.md`, "Nominal declarations"): each written
/// implementation must answer as canonical key order does.
#[test]
fn every_library_key_conformance_matches_the_closed_registry() {
    let stdlib = vibra_types::load_stdlib().expect("standard library");
    let imports = "(import core @std.core)\n(import equatable @std.core.equatable)\n(import ordered @std.core.ordered)";
    for (module, pairs) in key_samples() {
        let source_id = format!("stdlib/src/{module}");
        let library = module_source(module, true);
        // `@std.core` names its own ordering without an alias.
        let ordering = if module == "std/core.vib" {
            "ordering"
        } else {
            "core.ordering"
        };
        for operands in pairs {
            for (closed_result, result, member) in [
                ("core.ordering", ordering, "ordered.compare"),
                ("bool", "bool", "equatable.equal"),
            ] {
                let closed = run(
                    "closed.vib",
                    &format!(
                        "(defn main () {closed_result} ({member} {operands}))\n\n{imports}"
                    ),
                );
                let source = format!(
                    "(defn main () {result} ({member} {operands}))\n\n{library}"
                );
                let checked = vibra_types::check_standard_library_source(
                    &stdlib, &source_id, &source,
                );
                let program = checked
                    .program()
                    .unwrap_or_else(|| panic!("{module}\n{:?}", checked.diagnostics()));
                // The call must reach a written implementation, not the registry.
                assert!(
                    !program.canonical_vibon().contains("closed: @key."),
                    "`{member} {operands}` in {module} fell back to the closed registry"
                );
                let written = vibra_interp::run(program)
                    .expect("execution")
                    .canonical_result();
                assert_eq!(
                    closed, written,
                    "`{member}` of {module} disagrees with canonical key order on `{operands}`"
                );
            }
        }
    }
}

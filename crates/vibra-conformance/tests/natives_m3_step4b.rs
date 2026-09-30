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
        "map.of" => &[
            ("(map str i32)", ""),
            ("(map str i32)", "\"b\" 2i32 \"a\" 1i32 \"b\" 3i32"),
        ],
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
            // Drop the member-only attributes and name the owner's generics.
            let mut lines = source
                .lines()
                .filter(|line| {
                    let line = line.trim_start();
                    !line.starts_with("native:") && !line.starts_with("visibility:")
                })
                .map(str::to_owned)
                .collect::<Vec<_>>();
            let header = lines[0].replacen(
                &format!("(defn {} ", method.name().value()),
                "(defn reference ",
                1,
            );
            lines[0] = format!("{header}\n  where: ({owner_generics})");
            members.push((
                symbol,
                format!("{}.{}", owner.name().value(), method.name().value()),
                lines.join("\n"),
            ));
        }
    }
    members
}

fn run(source_id: &str, source: &str) -> String {
    let checked = vibra_types::check_source(source_id, source);
    let program = checked
        .program()
        .unwrap_or_else(|| panic!("{source}\n{:?}", checked.diagnostics()));
    vibra_interp::run(program)
        .expect("execution")
        .canonical_result()
}

#[test]
fn every_listed_native_matches_its_body() {
    let stdlib = vibra_types::load_stdlib().expect("standard library");
    let members = native_members();
    let listed = stdlib.native_symbols();
    assert!(!listed.is_empty());
    for symbol in listed {
        let (_, path, reference) = members
            .iter()
            .find(|(member, _, _)| member == symbol)
            .unwrap_or_else(|| {
                panic!("`{symbol}` has no native member in @std.builtin")
            });
        let samples = samples(symbol);
        assert!(!samples.is_empty(), "`{symbol}` has no harness samples");
        for (result, operands) in samples {
            let native = run(
                "native.vib",
                &format!("(defn main () {result} ({path} {operands}))"),
            );
            // The body is standard-library code, checked where it lives.
            let body = run(
                "stdlib/src/std/builtin.vib",
                &format!("(defn main () {result} (reference {operands}))\n{reference}"),
            );
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

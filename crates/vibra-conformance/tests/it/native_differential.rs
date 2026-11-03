//! The body/native differential harness (`docs/spec/06-runtime.md`, "Native
//! implementations"): every native implementation the standard-library
//! manifest lists runs against its Vibra body over the same inputs, and both
//! must produce the same canonical result.

use crate::support::{runner, runner_with, workspace_root};

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
        "dict.of" => &[
            ("(dict str i32)", ""),
            ("(dict str i32)", "\"b\" 2i32 \"a\" 1i32 \"b\" 3i32"),
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
                        .map(|binding| {
                            format!(
                                "{} {}",
                                binding.name().value(),
                                binding.bound().value()
                            )
                        })
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
                            .map(|binding| {
                                format!(
                                    " {} {}",
                                    binding.name().value(),
                                    binding.bound().value()
                                )
                            })
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

/// What one program answers in the two backends.
struct Ran {
    /// The reference interpreter's canonical result.
    interpreter: String,
    /// The module's, or `None` when the emitter does not lower the program.
    wasm: Option<String>,
}

fn run(source_id: &str, source: &str) -> Ran {
    let checked = vibra_types::check_source(source_id, source);
    finish(source, &checked)
}

fn finish(source: &str, checked: &vibra_types::CheckResult) -> Ran {
    let program = checked
        .program()
        .unwrap_or_else(|| panic!("{source}\n{:?}", checked.diagnostics()));
    let interpreter = vibra_interp::run(program)
        .expect("execution")
        .canonical_result();
    let wasm = vibra_wasm::emit(program).ok().map(|module| {
        let ty = program.entry().signature().result();
        let runner = runner();
        match runner
            .run_observed(module.bytes(), &ty, program.types())
            .expect("the module runs")
        {
            vibra_wasm_run::Observed::Completed { value, .. } => {
                value.canonical_observation(&ty)
            }
            vibra_wasm_run::Observed::Stopped(outcome) => {
                panic!("{source}\nthe module stopped: {outcome:?}")
            }
        }
    });
    Ran { interpreter, wasm }
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

/// The alias a source gives the standard-library module at `module`, such as
/// `text` for `std/text.vib`.
fn alias_of(module: &str) -> &str {
    module
        .strip_prefix("std/")
        .and_then(|name| name.strip_suffix(".vib"))
        .unwrap_or(module)
}

/// The module rewritten as ordinary source after `main`, so that each function
/// runs its Vibra body: without its `deftype`s, which the role types `str` and
/// `bytes` already are in every source, and without `native:`.
fn body_source(module: &str) -> String {
    let path = workspace_root().join("stdlib/src").join(module);
    let text = std::fs::read_to_string(&path).expect("module");
    let document = vibra_syntax::parse_source(&path, &text).expect("parse");
    document
        .ast()
        .expect("module ast")
        .declarations()
        .iter()
        .filter(|declaration| !matches!(declaration, Declaration::Deftype(_)))
        .map(|declaration| {
            let span = declaration.span();
            text[span.start()..span.end()]
                .lines()
                .filter(|line| !line.trim_start().starts_with("native:"))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[test]
fn every_listed_native_matches_its_body() {
    let stdlib = vibra_types::load_stdlib().expect("standard library");
    let members = native_members();
    let modules = module_natives();
    let listed = stdlib.native_symbols();
    assert!(!listed.is_empty());
    let mut counts = std::collections::BTreeMap::new();
    for symbol in listed {
        let mut samples = samples(symbol)
            .iter()
            .map(|(result, operands)| ((*result).to_owned(), (*operands).to_owned()))
            .collect::<Vec<_>>();
        assert!(!samples.is_empty(), "`{symbol}` has no harness samples");
        samples.extend(extra_samples(symbol));
        counts.insert(symbol.clone(), samples.len());
        for (result, operands) in &samples {
            let (native, body) = if let Some((_, path, reference)) =
                members.iter().find(|(member, _, _)| member == symbol)
            {
                (
                    run(
                        "native.vib",
                        &format!("(defn main () {result} ({path} {operands}))"),
                    ),
                    // The body is the member rewritten as an ordinary function.
                    run(
                        "reference.vib",
                        &format!(
                            "(import option @std.option)\n(import ordered @std.core.ordered)\n(defn main () {result} (reference {operands}))\n{reference}"
                        ),
                    ),
                )
            } else {
                let (_, module, name) = modules
                    .iter()
                    .find(|(native, _, _)| native == symbol)
                    .unwrap_or_else(|| panic!("`{symbol}` has no native declaration"));
                let alias = alias_of(module);
                (
                    // The native: the module as the standard library embeds it.
                    run(
                        "native.vib",
                        &format!(
                            "(import core @std.core)\n(import library @std.{alias})\n(defn main () {result} (library.{name} {operands}))"
                        ),
                    ),
                    // The body: the module's function, written in an ordinary source.
                    run(
                        "body.vib",
                        &format!(
                            "(defn main () {result} ({name} {operands}))\n\n{}",
                            body_source(module)
                        ),
                    ),
                )
            };
            assert_eq!(
                native.interpreter, body.interpreter,
                "`{symbol}` disagrees with its body on `{operands}`"
            );
            // The program that names the native lowers: a module reaches a
            // native with an import through it, and runs the checked body of
            // one with none (a native that applies a function value). Either
            // must give the interpreter's answer.
            let row = intrinsic_of(symbol);
            assert_ne!(
                row.native_import().is_some(),
                row.runs_body(),
                "`{symbol}` has an import or a checked body, never both"
            );
            let wasm = native
                .wasm
                .as_ref()
                .unwrap_or_else(|| panic!("the program of `{symbol}` is not lowered"));
            assert_eq!(
                wasm, &native.interpreter,
                "the module's `{symbol}` disagrees on `{operands}`"
            );
            let body_wasm = body
                .wasm
                .unwrap_or_else(|| panic!("the body of `{symbol}` is not lowered"));
            assert_eq!(
                body_wasm, body.interpreter,
                "the lowered body of `{symbol}` disagrees on `{operands}`"
            );
        }
    }
    // Each sample ran on four paths: the interpreter calling the native code,
    // the interpreter running the body, the module calling the import (or
    // running the body of a native with none), and the module running the
    // lowered body.
    eprintln!(
        "{} natives, {} samples on four paths: {counts:?}",
        counts.len(),
        counts.values().sum::<usize>()
    );
    assert_eq!(counts.len(), listed.len());
    for (symbol, count) in &counts {
        assert!(*count >= 3, "`{symbol}` has only {count} samples");
    }
}

/// Samples at the edges of the domain, which a literal table would be long to
/// write: a large input, the widest scalars, a sequence of every width, and the
/// ill-formed UTF-8 the decoder must refuse.
fn extra_samples(symbol: &str) -> Vec<(String, String)> {
    let sample = |result: &str, operands: String| (result.to_owned(), operands);
    // Scalars of one, two, three, and four UTF-8 bytes, repeated.
    let text = |times: usize| "a\u{e9}\u{20ac}\u{1d11e}".repeat(times);
    let array = |ty: &str, items: String| (format!("(array {ty})"), items);
    let numbers = |count: usize, suffix: &str| {
        (0..count)
            .map(|index| format!("{index}{suffix}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let byte_array = |bytes: &[u8]| {
        format!(
            "(bytes (array.of {}))",
            bytes
                .iter()
                .map(|byte| format!("{byte}u8"))
                .collect::<Vec<_>>()
                .join(" ")
        )
    };
    let long = (0..=255_u8).cycle().take(1000).collect::<Vec<_>>();
    match symbol {
        "array.of" => vec![
            array("i32", "0i32".to_owned()),
            array("i32", numbers(300, "i32")),
            array("str", "\"\" \"a\" \"\u{1d11e}\"".to_owned()),
            array("(array u8)", "(array.of) (array.of 1u8)".to_owned()),
        ],
        "dict.of" => vec![
            sample("(dict str i32)", "\"a\" 1i32".to_owned()),
            // Keys of atoms, which are ordered by their spelling, a prefix
            // before its extension, and a key written twice, whose last
            // entry wins.
            sample(
                "(dict atom i32)",
                "@b 1i32 @a 2i32 @ab 3i32 @a-b 4i32 @b 5i32".to_owned(),
            ),
            sample("(dict char i32)", r"\b 1i32 \a 2i32 \é 3i32".to_owned()),
            sample(
                "(dict bytes i32)",
                format!(
                    "{} 1i32 {} 2i32 {} 3i32",
                    byte_array(&[2]),
                    byte_array(&[1, 9]),
                    byte_array(&[])
                ),
            ),
        ],
        "array.fold" => vec![sample(
            "i32",
            format!(
                "(array.of {}) 0i32 (lambda (total i32 item i32) i32 item)",
                numbers(150, "i32")
            ),
        )],
        "text.concat" => vec![
            sample("str", format!("{} {}", quoted(&text(1)), quoted(""))),
            sample("str", format!("{} {}", quoted(""), quoted(&text(1)))),
            sample(
                "str",
                format!("{} {}", quoted(&text(200)), quoted(&text(200))),
            ),
        ],
        "text.length" => vec![sample("u64", quoted(&text(250)))],
        "text.equal" => vec![
            sample(
                "bool",
                format!("{} {}", quoted(&text(40)), quoted(&text(40))),
            ),
            sample(
                "bool",
                format!("{} {}", quoted(&text(40)), quoted(&text(41))),
            ),
            sample("bool", format!("{} {}", quoted(""), quoted(""))),
        ],
        "text.compare" => vec![
            sample("core.ordering", format!("{} {}", quoted(""), quoted("a"))),
            sample("core.ordering", format!("{} {}", quoted("a"), quoted(""))),
            sample(
                "core.ordering",
                format!("{} {}", quoted("\u{10ffff}"), quoted("\u{e000}")),
            ),
            sample(
                "core.ordering",
                format!("{} {}", quoted(&text(30)), quoted(&text(30))),
            ),
            sample(
                "core.ordering",
                format!("{} {}", quoted("ab"), quoted("a-b")),
            ),
        ],
        "text.slice" => vec![
            sample("(option str)", format!("{} 0u64 0u64", quoted(&text(3)))),
            sample("(option str)", format!("{} 0u64 12u64", quoted(&text(3)))),
            sample("(option str)", format!("{} 12u64 12u64", quoted(&text(3)))),
            sample("(option str)", format!("{} 13u64 13u64", quoted(&text(3)))),
            sample("(option str)", format!("{} 1u64 1u64", quoted(""))),
            sample("(option str)", format!("{} 1u64 2u64", quoted(&text(3)))),
            sample(
                "(option str)",
                format!("{} 0u64 18446744073709551615u64", quoted("abc")),
            ),
        ],
        "text.to-chars" => vec![
            array("char", quoted(&text(1))),
            array("char", quoted(&text(120))),
        ],
        "text.from-chars" => vec![
            sample("str", r"(array.of \a \é \€ \𝄞)".to_owned()),
            sample("str", r"(array.of \a)".to_owned()),
        ],
        "text.to-utf8" => vec![
            sample(
                "bytes",
                quoted("\u{7f}\u{80}\u{7ff}\u{800}\u{ffff}\u{10000}\u{10ffff}"),
            ),
            sample("bytes", quoted(&text(150))),
        ],
        "text.from-utf8" => {
            let result = "(result str core.conversion-error)";
            let mut samples = [
                &[][..],
                &[0x7f][..],
                &[0xc2, 0x80][..],
                &[0xe0, 0xa0, 0x80][..],
                &[0xf0, 0x90, 0x80, 0x80][..],
                &[0xf4, 0x8f, 0xbf, 0xbf][..],
                // Ill-formed: a stray continuation, an overlong form of every
                // length, a surrogate, a value past U+10FFFF, a truncated
                // sequence at the end, and a lead byte that is no lead.
                &[0x80][..],
                &[0xc1, 0xbf][..],
                &[0xe0, 0x9f, 0xbf][..],
                &[0xf0, 0x8f, 0xbf, 0xbf][..],
                &[0xed, 0xa0, 0x80][..],
                &[0xf4, 0x90, 0x80, 0x80][..],
                &[0x61, 0xf0, 0x9d, 0x84][..],
                &[0xf8, 0x88, 0x80, 0x80, 0x80][..],
                &[0xe2, 0x82, 0x41][..],
            ]
            .iter()
            .map(|bytes| sample(result, byte_array(bytes)))
            .collect::<Vec<_>>();
            samples.push(sample(result, byte_array(&text(100).into_bytes())));
            samples.push(sample(result, byte_array(&long)));
            samples
        }
        "bytes.length" => vec![
            sample("u64", byte_array(&[])),
            sample("u64", byte_array(&long)),
        ],
        "bytes.concat" => vec![
            sample("bytes", format!("{} {}", byte_array(&[]), byte_array(&[]))),
            sample("bytes", format!("{} {}", byte_array(&[]), byte_array(&[7]))),
            sample(
                "bytes",
                format!("{} {}", byte_array(&long), byte_array(&long)),
            ),
        ],
        "bytes.equal" => vec![
            sample("bool", format!("{} {}", byte_array(&[]), byte_array(&[]))),
            sample(
                "bool",
                format!("{} {}", byte_array(&long), byte_array(&long)),
            ),
            sample(
                "bool",
                format!("{} {}", byte_array(&[1, 2]), byte_array(&[1, 3])),
            ),
        ],
        "bytes.compare" => vec![
            sample(
                "core.ordering",
                format!("{} {}", byte_array(&[]), byte_array(&[])),
            ),
            sample(
                "core.ordering",
                format!("{} {}", byte_array(&[255]), byte_array(&[0, 0])),
            ),
            sample(
                "core.ordering",
                format!("{} {}", byte_array(&long), byte_array(&long)),
            ),
        ],
        "bytes.slice" => vec![
            sample("(option bytes)", format!("{} 0u64 0u64", byte_array(&[]))),
            sample(
                "(option bytes)",
                format!("{} 0u64 1000u64", byte_array(&long)),
            ),
            sample(
                "(option bytes)",
                format!("{} 1000u64 1000u64", byte_array(&long)),
            ),
            sample(
                "(option bytes)",
                format!("{} 1u64 1001u64", byte_array(&long)),
            ),
            sample("(option bytes)", format!("{} 3u64 2u64", byte_array(&long))),
        ],
        "bytes.to-array" => {
            vec![array("u8", byte_array(&[])), array("u8", byte_array(&long))]
        }
        "bytes.from-array" => vec![
            sample("bytes", "(as (array u8) (array.of))".to_owned()),
            sample("bytes", format!("(array.of {})", numbers(255, "u8"))),
        ],
        _ => Vec::new(),
    }
}

/// A string literal of source for `text`, every character but an ASCII letter or
/// digit spelled as an escape.
fn quoted(text: &str) -> String {
    let mut out = String::from("\"");
    for character in text.chars() {
        if character.is_ascii_alphanumeric() {
            out.push(character);
        } else {
            out.push_str(&format!("\\u{{{:X}}}", u32::from(character)));
        }
    }
    out.push('"');
    out
}

/// A source literal of a vector operand of the text and bytes rows.
fn operand(value: &vibra_ir::Value) -> String {
    use vibra_ir::Value;
    match value {
        Value::Str(text) => quoted(text),
        Value::Bytes(bytes) => format!(
            "(bytes (array.of {}))",
            bytes
                .iter()
                .map(|byte| format!("{byte}u8"))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        Value::U64(value) => format!("{value}u64"),
        other => panic!("not a vector operand: {other:?}"),
    }
}

/// The written type of the result of a text or bytes row.
fn written_result(intrinsic: vibra_ir::external::CompilerIntrinsic) -> &'static str {
    use vibra_ir::external::CompilerIntrinsic as C;
    match intrinsic {
        C::TextConcat => "str",
        C::TextLength | C::BytesLength => "u64",
        C::TextEqual | C::BytesEqual => "bool",
        C::TextCompare | C::BytesCompare => "core.ordering",
        C::TextSlice => "(option str)",
        C::TextToUtf8 | C::BytesConcat => "bytes",
        C::TextFromUtf8 => "(result str core.conversion-error)",
        C::BytesSlice => "(option bytes)",
        other => panic!("no vectors for {}", other.symbol()),
    }
}

/// The shared sample vectors of the text and bytes rows hold both backends to
/// the same outcomes, which the standard library's own `str` and slice
/// operations computed, so the interpreter calling the native code and the
/// module calling the import cannot drift apart on a boundary: a scalar of each
/// width, a prefix, an empty range, a range past the end, and ill-formed UTF-8.
#[test]
fn the_text_and_bytes_vectors_hold_the_interpreter_and_the_module_to_one_answer() {
    use vibra_ir::{ObservedValue, Type};
    let modules = module_natives();
    let mut total = 0;
    for intrinsic in vibra_ir::external::CompilerIntrinsic::all() {
        let vectors = intrinsic.vectors();
        let symbol = intrinsic.symbol();
        if vectors.is_empty()
            || !(symbol.starts_with("text.") || symbol.starts_with("bytes."))
        {
            continue;
        }
        let (_, module, name) = modules
            .iter()
            .find(|(native, _, _)| native == symbol)
            .unwrap_or_else(|| panic!("`{symbol}` has no native declaration"));
        let result = written_result(intrinsic);
        let alias = alias_of(module);
        let calls = vectors
            .iter()
            .map(|vector| {
                let operands = vector
                    .operands
                    .iter()
                    .map(operand)
                    .collect::<Vec<_>>()
                    .join(" ");
                format!("(library.{name} {operands})")
            })
            .collect::<Vec<_>>()
            .join("\n    ");
        let types = vec![result; vectors.len()].join(" ");
        let checked = vibra_types::check_source(
            "vectors.vib",
            &format!(
                "(import core @std.core)\n(import library @std.{alias})\n(defn main () (tuple {types})\n  (tupleof\n    {calls}))"
            ),
        );
        let program = checked
            .program()
            .unwrap_or_else(|| panic!("{symbol}\n{:?}", checked.diagnostics()));
        let Type::Tuple(components) = program.entry().signature().result() else {
            panic!("the entry is a tuple");
        };
        let expected = ObservedValue::Tuple {
            type_id: None,
            values: vectors
                .iter()
                .zip(&components)
                .map(|(vector, ty)| vector.outcome.observed(ty))
                .collect(),
        }
        .canonical_vibon();
        let entry = program.entry().signature().result();
        let interpreted = vibra_interp::run(program)
            .expect("execution")
            .canonical_result();
        assert!(
            interpreted.contains(&expected),
            "the interpreter disagrees with the vectors of `{symbol}`"
        );
        let module_bytes = vibra_wasm::emit(program)
            .unwrap_or_else(|error| panic!("`{symbol}` lowers: {error}"));
        let runner = runner_with(256 * 1024 * 1024);
        let observed = match runner
            .run_observed(module_bytes.bytes(), &entry, program.types())
            .expect("the module runs")
        {
            vibra_wasm_run::Observed::Completed { value, .. } => {
                value.canonical_observation(&entry)
            }
            vibra_wasm_run::Observed::Stopped(outcome) => {
                panic!("`{symbol}` stopped: {outcome:?}")
            }
        };
        assert_eq!(
            observed, interpreted,
            "the module disagrees with the interpreter on the vectors of `{symbol}`"
        );
        total += vectors.len();
    }
    assert!(total > 1000, "only {total} vectors ran");
}

fn intrinsic_of(symbol: &str) -> vibra_ir::external::CompilerIntrinsic {
    vibra_ir::external::CompilerIntrinsic::from_symbol(symbol)
        .unwrap_or_else(|| panic!("`{symbol}` is not a registry symbol"))
}

#[test]
fn the_harness_reports_a_body_that_disagrees() {
    // A reference that drops its tail must not match `array.of`.
    let native = run(
        "native.vib",
        "(defn main () (array i32) (array.of 1i32 2i32))",
    );
    let wrong = run(
        "reference.vib",
        "(defn main () (array i32) (reference 1i32 2i32))\n\
         (defn reference () (array t)\n  where: (t any)\n  variadic: (items (array t))\n  (array.of))",
    );
    assert_ne!(native.interpreter, wrong.interpreter);
}

/// Operand pairs for each type whose key contracts the standard library
/// implements, each with the canonical key order of the pair: the module that
/// writes the implementation, then `(operands, order)`.
fn key_samples() -> Vec<(&'static str, Vec<(&'static str, &'static str)>)> {
    vec![
        (
            "std/bool.vib",
            vec![
                ("false true", "less"),
                ("true false", "greater"),
                ("true true", "equal"),
                ("false false", "equal"),
            ],
        ),
        (
            "std/text.vib",
            vec![
                ("\"ab\" \"b\"", "less"),
                ("\"b\" \"ab\"", "greater"),
                ("\"é\" \"é\"", "equal"),
                ("\"\" \"a\"", "less"),
            ],
        ),
        (
            "std/bytes.vib",
            vec![
                (
                    "(bytes (array.of 1u8 2u8)) (bytes (array.of 1u8 3u8))",
                    "less",
                ),
                (
                    "(bytes (array.of 2u8)) (bytes (array.of 1u8 9u8))",
                    "greater",
                ),
                ("(bytes (array.of)) (bytes (array.of))", "equal"),
            ],
        ),
        (
            "std/core.vib",
            vec![
                ("-1i8 1i8", "less"),
                ("300i16 -300i16", "greater"),
                ("7i32 7i32", "equal"),
                ("-9i64 -8i64", "less"),
                ("200u8 100u8", "greater"),
                ("1u16 2u16", "less"),
                ("5u32 5u32", "equal"),
                ("18446744073709551615u64 0u64", "greater"),
                (r"\a \b", "less"),
                (r"\é \e", "greater"),
                (r"\z \z", "equal"),
            ],
        ),
    ]
}

/// Each written key conformance of the standard library answers as canonical
/// key order does (`docs/spec/02-type-system.md`, "Nominal declarations"). A
/// module whose implementation is not in scope reaches the closed registry
/// instead, and that answer must be the same one.
#[test]
fn every_library_key_conformance_answers_canonical_key_order() {
    let imports = "(import core @std.core)\n(import equatable @std.core.equatable)\n(import ordered @std.core.ordered)";
    let mut closed_comparisons = 0;
    for (module, pairs) in key_samples() {
        // The module imported whole puts its written implementations in scope.
        let library = format!("(import library @std.{})", alias_of(module));
        for (operands, order) in pairs {
            for (result, member, expected) in [
                (
                    "core.ordering",
                    "ordered.compare",
                    format!("variant: @{order}"),
                ),
                (
                    "bool",
                    "equatable.equal",
                    format!("value: {}", order == "equal"),
                ),
            ] {
                let call = format!("(defn main () {result} ({member} {operands}))");
                let written_source = format!("{library}\n{call}\n\n{imports}");
                let checked = vibra_types::check_source("written.vib", &written_source);
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
                assert!(
                    written.contains(&expected),
                    "`{member}` of {module} does not answer `{expected}` on `{operands}`: {written}"
                );
                let closed_source = format!("{call}\n\n{imports}");
                let closed_checked =
                    vibra_types::check_source("closed.vib", &closed_source);
                if closed_checked.program().is_some_and(|program| {
                    program.canonical_vibon().contains("closed: @key.")
                }) {
                    closed_comparisons += 1;
                    assert_eq!(
                        finish(&closed_source, &closed_checked).interpreter,
                        written,
                        "`{member}` of {module} disagrees with the closed registry on `{operands}`"
                    );
                }
            }
        }
    }
    // Text and bytes are not in scope without their import: their closed
    // registry is the reference for those pairs.
    assert!(closed_comparisons >= 14, "{closed_comparisons}");
}

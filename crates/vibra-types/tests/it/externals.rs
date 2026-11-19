//! External declarations against the `@compiler` registry: every bound
//! symbol is a row with the declaration's owner and signature, and each of
//! the three faults has its code and its span.

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::external::CompilerIntrinsic;
use vibra_syntax::parse_source_with;
use vibra_syntax::source::{SourceOptions, module};
use vibra_types::{check_externals, load_stdlib};

use crate::support::{embedded_source, load_with_module, notes, replace_once};

/// The faults `check_externals` gives for `source` read as the module
/// `path` of the standard library.
fn check(path: &str, source: &str) -> Vec<Diagnostic> {
    let stdlib = load_stdlib().expect("the embedded standard library loads");
    let document = parse_source_with(
        Some("fixture.vib"),
        source.as_bytes(),
        SourceOptions {
            standard_library: true,
        },
    );
    assert!(
        document.diagnostics().is_empty(),
        "{:?}",
        document.diagnostics()
    );
    let view = module(&document).expect("the fixture has a source tree");
    check_externals(&view, path, stdlib.names())
}

/// Fails the test: `found` is not exactly one diagnostic.
fn assert_one(found: &[Diagnostic]) {
    assert_eq!(found.len(), 1, "expected one diagnostic, found {found:?}");
}

fn text(source: &str, span: ByteSpan) -> &str {
    source.get(span.start()..span.end()).unwrap_or("")
}

fn i32_with(member: &str) -> String {
    format!("type I32 of Intrinsic(@i32)\n  visibility @public\n{member}endtype\n")
}

#[test]
fn every_embedded_external_is_a_registry_row_of_its_owner() {
    let stdlib = load_stdlib().expect("the embedded standard library loads");
    for module in stdlib.modules() {
        let source = String::from_utf8_lossy(module.bytes()).into_owned();
        assert_eq!(
            check(module.path(), &source),
            Vec::new(),
            "`{}`",
            module.path()
        );
    }
    for symbol in stdlib.compiler_symbols() {
        assert!(
            CompilerIntrinsic::from_symbol(symbol).is_some(),
            "`{symbol}` is not a registry row"
        );
    }
    assert!(stdlib.host_symbols().is_empty());
    assert!(stdlib.native_symbols().is_empty());
}

#[test]
fn the_bound_symbols_are_the_manifest_s_compiler_list() {
    let stdlib = load_stdlib().expect("the embedded standard library loads");
    let mut bound = Vec::new();
    for module in stdlib.modules() {
        let source = String::from_utf8_lossy(module.bytes()).into_owned();
        for line in source.lines() {
            if let Some(symbol) = line.trim().strip_prefix("symbol \"") {
                bound.push(symbol.trim_end_matches('"').to_owned());
            }
        }
    }
    bound.sort();
    assert_eq!(bound, stdlib.compiler_symbols());
}

#[test]
fn an_unknown_symbol_is_reported_at_the_symbol() {
    let source = i32_with(
        "  fn addRounded\n    takes {left: Self right: Self}\n    returns sum: Self\n    \
         visibility @public\n    external @compiler\n    symbol \"i32.addRounded\"\n  endfn\n",
    );
    let found = check("@std.builtin", &source);
    let [diagnostic] = found.as_slice() else {
        assert_one(&found);
        return;
    };
    assert_eq!(diagnostic.code(), DiagnosticCode::ExternalUnknownSymbol);
    assert_eq!(
        text(&source, diagnostic.primary_span()),
        "\"i32.addRounded\""
    );
}

#[test]
fn a_signature_mismatch_is_reported_at_the_declaration_with_the_expected_signature() {
    let source = i32_with(
        "  fn addChecked\n    takes {left: Self right: Self}\n    returns sum: I32\n    \
         visibility @public\n    external @compiler\n    symbol \"i32.addChecked\"\n  endfn\n",
    );
    let found = check("@std.builtin", &source);
    let [diagnostic] = found.as_slice() else {
        assert_one(&found);
        return;
    };
    assert_eq!(diagnostic.code(), DiagnosticCode::ExternalSignatureMismatch);
    assert!(text(&source, diagnostic.primary_span()).starts_with("fn addChecked"));
    assert!(text(&source, diagnostic.primary_span()).ends_with("endfn"));
    let notes = diagnostic.notes().join("\n");
    assert!(notes.starts_with("expected "), "{notes}");
    assert!(notes.contains("Result(I32 ArithmeticError)"), "{notes}");
}

#[test]
fn an_owner_mismatch_is_reported_at_the_symbol_related_to_the_owner() {
    let source = "type I64 of Intrinsic(@i64)\n  visibility @public\n  fn addChecked\n    \
                  takes {left: Self right: Self}\n    \
                  returns sum: Result(Self ArithmeticError)\n    visibility @public\n    \
                  external @compiler\n    symbol \"i32.addChecked\"\n  endfn\nendtype\n";
    let source = format!("use @std.core.ArithmeticError enduse\n\n{source}");
    let found = check("@std.builtin", &source);
    let [diagnostic] = found.as_slice() else {
        assert_one(&found);
        return;
    };
    assert_eq!(diagnostic.code(), DiagnosticCode::ExternalOwnerMismatch);
    assert_eq!(
        text(&source, diagnostic.primary_span()),
        "\"i32.addChecked\""
    );
    let related = diagnostic
        .related()
        .iter()
        .map(|related| text(&source, related.span))
        .collect::<Vec<_>>();
    assert_eq!(related, vec!["I64"]);
}

#[test]
fn compiler_on_a_proc_is_a_signature_mismatch() {
    let source = i32_with(
        "  proc add\n    takes {left: Self right: Self}\n    returns sum: Self\n    \
         visibility @public\n    external @compiler\n    symbol \"i32.add\"\n  endproc\n",
    );
    let found = check("@std.builtin", &source);
    let codes = found.iter().map(Diagnostic::code).collect::<Vec<_>>();
    assert_eq!(codes, vec![DiagnosticCode::ExternalSignatureMismatch]);
}

#[test]
fn a_labelled_parameter_is_a_signature_mismatch() {
    let source = i32_with(
        "  fn add\n    takes {left: Self right: Self}\n    labelled {wrap: Bool be true}\n    \
         returns sum: Self\n    \
         visibility @public\n    external @compiler\n    symbol \"i32.add\"\n  endfn\n",
    );
    let found = check("@std.builtin", &source);
    let codes = found.iter().map(Diagnostic::code).collect::<Vec<_>>();
    assert_eq!(codes, vec![DiagnosticCode::ExternalSignatureMismatch]);
}

#[test]
fn an_unknown_host_symbol_is_reported_at_the_symbol() {
    let source = "proc launch\n  takes {}\n  returns done: Void\n  visibility @public\n  \
                  external @host\n  symbol \"rocket.launch\"\nendproc\n";
    let found = check("@std.rocket", source);
    let [diagnostic] = found.as_slice() else {
        assert_one(&found);
        return;
    };
    assert_eq!(diagnostic.code(), DiagnosticCode::ExternalUnknownSymbol);
    assert_eq!(text(source, diagnostic.primary_span()), "\"rocket.launch\"");
}

#[test]
fn the_loader_reports_a_signature_mismatch_as_a_defect() {
    let builtin = embedded_source("@std.builtin");
    let changed = replace_once(
        &builtin,
        "returns sum: Result(Self ArithmeticError)\n    visibility @public\n    \
         external @compiler\n    symbol \"i32.addChecked\"\n",
        "returns sum: I32\n    visibility @public\n    \
         external @compiler\n    symbol \"i32.addChecked\"\n",
    );
    let error = load_with_module("@std.builtin", &changed);
    assert_eq!(
        error.as_ref().map(|error| error.diagnostic().code()),
        Some(DiagnosticCode::RuntimeInvalidCheckedProgram)
    );
    let notes = notes(error.as_ref());
    assert!(notes.contains("@external.signatureMismatch"), "{notes}");
}

#[test]
fn the_loader_reports_an_owner_mismatch_as_a_defect() {
    let builtin = embedded_source("@std.builtin");
    let changed = replace_once(
        &builtin,
        "symbol \"i64.addChecked\"\n",
        "symbol \"i32.addChecked\"\n",
    );
    let error = load_with_module("@std.builtin", &changed);
    assert_eq!(
        error.as_ref().map(|error| error.diagnostic().code()),
        Some(DiagnosticCode::RuntimeInvalidCheckedProgram)
    );
    let notes = notes(error.as_ref());
    assert!(notes.contains("@external.ownerMismatch"), "{notes}");
}

#[test]
fn the_loader_reports_an_external_fault_as_a_defect() {
    let builtin = embedded_source("@std.builtin");
    let changed = replace_once(
        &builtin,
        "symbol \"i32.add\"\n",
        "symbol \"i32.addRounded\"\n",
    );
    let error = load_with_module("@std.builtin", &changed);
    assert_eq!(
        error.as_ref().map(|error| error.diagnostic().code()),
        Some(DiagnosticCode::RuntimeInvalidCheckedProgram)
    );
    let notes = notes(error.as_ref());
    assert!(notes.contains("@external.unknownSymbol"), "{notes}");
    assert!(notes.contains("stdlib/src/std/builtin.vib"), "{notes}");
}

#[test]
fn a_name_that_is_not_the_symbol_s_member_is_a_signature_mismatch() {
    let source = i32_with(
        "  fn plus\n    takes {left: Self right: Self}\n    returns sum: Self\n    \
         visibility @public\n    external @compiler\n    symbol \"i32.add\"\n  endfn\n",
    );
    let found = check("@std.builtin", &source);
    let [diagnostic] = found.as_slice() else {
        assert_one(&found);
        return;
    };
    assert_eq!(diagnostic.code(), DiagnosticCode::ExternalSignatureMismatch);
    assert!(text(&source, diagnostic.primary_span()).starts_with("fn plus"));
    assert!(text(&source, diagnostic.primary_span()).ends_with("endfn"));
    assert_eq!(diagnostic.notes(), ["expected the name `add`"]);
}

/// The assertions are module-level functions; the owner of a module-level
/// function is the last part of its module path, so the rows of `assert`
/// fit `@std.assert` and no other module.
#[test]
fn the_assertion_rows_are_owned_by_the_module_assert() {
    let source = embedded_source("@std.assert");
    assert_eq!(check("@std.assert", &source), Vec::new());
    let found = check("@std.check", &source);
    let codes = found.iter().map(Diagnostic::code).collect::<Vec<_>>();
    assert_eq!(codes, vec![DiagnosticCode::ExternalOwnerMismatch; 3]);
}

#[test]
fn the_manifest_lists_the_assertion_module_and_its_three_symbols() {
    let stdlib = load_stdlib().expect("the embedded standard library loads");
    let assert = stdlib
        .modules()
        .iter()
        .find(|module| module.path() == "@std.assert")
        .expect("`@std.assert` is embedded");
    assert!(assert.roles().is_empty());
    for symbol in ["assert.equal", "assert.false", "assert.true"] {
        assert!(
            stdlib
                .compiler_symbols()
                .iter()
                .any(|listed| listed == symbol),
            "`{symbol}` is not listed"
        );
        let row = CompilerIntrinsic::from_symbol(symbol).expect("a registry row");
        assert_eq!(row.owner(), "assert");
    }
}

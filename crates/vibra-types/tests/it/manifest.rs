//! The manifest: its schema, its canonical layout, its order, and its
//! digests.

use vibra_syntax::{decode, parse_data};
use vibra_types::{StdlibInputs, load_stdlib, manifest_schema};

use crate::support::{
    digest, embedded_manifest, load_with_manifest, load_with_module, notes,
    replace_once,
};

#[test]
fn the_manifest_decodes_through_its_schema() {
    let manifest = StdlibInputs::embedded().manifest;
    let decoded = decode(&manifest_schema(), Some("stdlib/manifest.vibon"), manifest);
    assert!(
        decoded.diagnostics().is_empty(),
        "{:?}",
        decoded.diagnostics()
    );
    assert!(decoded.document().is_some());
}

#[test]
fn the_manifest_round_trips_through_the_formatter() {
    let manifest = StdlibInputs::embedded().manifest;
    let decoded = decode(&manifest_schema(), Some("stdlib/manifest.vibon"), manifest);
    let document = decoded.document().expect("the manifest decodes");
    let canonical = document.canonical_bytes();
    let formatted = vibra_fmt::format_document(&parse_data(None, canonical.as_bytes()));
    assert_eq!(
        formatted.as_deref(),
        Some(String::from_utf8_lossy(manifest).as_ref()),
        "the manifest is not the formatter's layout of its canonical text"
    );
}

#[test]
fn every_digest_is_the_digest_of_its_module() {
    let stdlib = load_stdlib().expect("the embedded standard library loads");
    let manifest = embedded_manifest();
    for module in stdlib.modules() {
        let expected = digest(module.bytes());
        assert_eq!(module.digest(), expected, "`{}`", module.path());
        assert!(
            manifest.contains(&expected),
            "`{}` digest is not recorded",
            module.path()
        );
    }
}

#[test]
fn a_changed_module_byte_is_a_digest_fault() {
    let source = crate::support::embedded_source("@std.option");
    let changed = format!("{source}\n");
    let inputs =
        StdlibInputs::embedded().with_module("@std.option", changed.as_bytes());
    let error = vibra_types::load_stdlib_from(&inputs).err();
    let notes = notes(error.as_ref());
    assert!(
        notes.contains("the module `@std.option` has the digest"),
        "{notes}"
    );
    assert_eq!(
        error.map(|error| error.diagnostic().code().as_atom()),
        Some("@runtime.invalidCheckedProgram")
    );
}

#[test]
fn modules_out_of_order_are_a_fault() {
    let manifest = embedded_manifest();
    let moved = replace_once(&manifest, "path: @std.bool\n", "path: @std.zzz\n");
    let notes = notes(load_with_manifest(&moved).as_ref());
    assert!(
        notes.contains(
            "the modules' paths are not sorted by their UTF-8 bytes with each once: \
             `@std.builtin` follows `@std.zzz`"
        ),
        "{notes}"
    );
}

#[test]
fn compiler_symbols_out_of_order_are_a_fault() {
    let manifest = embedded_manifest();
    let swapped = replace_once(&manifest, "\"char.fromU32\"", "\"zzz.fromU32\"");
    let notes = notes(load_with_manifest(&swapped).as_ref());
    assert!(
        notes.contains("the compiler symbols are not sorted"),
        "{notes}"
    );
}

#[test]
fn an_unlisted_compiler_symbol_is_a_fault() {
    let manifest = embedded_manifest();
    let without = replace_once(&manifest, "\"list.next\"", "");
    let notes = notes(load_with_manifest(&without).as_ref());
    assert!(
        notes.contains(
            "the manifest does not list `list.next` among its compiler symbols"
        ),
        "{notes}"
    );
}

#[test]
fn a_native_symbol_is_a_fault() {
    let manifest = embedded_manifest();
    let with_native = replace_once(
        &manifest,
        "nativeSymbol: []",
        "nativeSymbol: [\"std.sort\"]",
    );
    let notes = notes(load_with_manifest(&with_native).as_ref());
    assert!(
        notes.contains("`std.sort` names no native implementation"),
        "{notes}"
    );
}

#[test]
fn a_module_without_an_entry_is_a_fault() {
    let inputs = StdlibInputs::embedded().with_module("@std.extra", b"");
    let notes = notes(vibra_types::load_stdlib_from(&inputs).err().as_ref());
    assert!(
        notes.contains("the embedded module `@std.extra` has no manifest entry"),
        "{notes}"
    );
}

#[test]
fn an_entry_without_a_module_is_a_fault() {
    let inputs = StdlibInputs::embedded().without_module("@std.result");
    let notes = notes(vibra_types::load_stdlib_from(&inputs).err().as_ref());
    assert!(
        notes.contains("the module `@std.result` is listed but not embedded"),
        "{notes}"
    );
}

#[test]
fn a_manifest_field_outside_the_schema_is_a_fault() {
    let manifest = embedded_manifest();
    let extra =
        replace_once(&manifest, "nativeSymbol: []", "nativeSymbol: [] native: []");
    let error = load_with_manifest(&extra);
    assert!(error.is_some(), "an extra field must not load");
}

#[test]
fn language_roles_that_differ_from_the_claims_are_a_fault() {
    let manifest = embedded_manifest();
    let changed =
        replace_once(&manifest, "languageRoles: [@option]", "languageRoles: []");
    let notes = notes(load_with_manifest(&changed).as_ref());
    assert!(
        notes
            .contains("@option: the manifest's language roles of `@std.option` differ"),
        "{notes}"
    );
}

#[test]
fn a_module_the_reader_rejects_is_a_fault() {
    let notes = notes(load_with_module("@std.option", "type Option(T)\n").as_ref());
    assert!(notes.contains("@std.option"), "{notes}");
}

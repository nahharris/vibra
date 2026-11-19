//! Each language role is claimed exactly once, on a declaration of the
//! shape the role requires.

use vibra_diagnostics::DiagnosticCode;
use vibra_types::{Role, load_stdlib};

use crate::support::{embedded_source, load_with_module, notes, replace_once};

#[test]
fn every_role_is_claimed_exactly_once() {
    let stdlib = load_stdlib().expect("the embedded standard library loads");
    let claimed = stdlib.roles().keys().copied().collect::<Vec<_>>();
    assert_eq!(claimed, Role::ALL);
    let per_module = stdlib
        .modules()
        .iter()
        .map(|module| (module.path(), module.roles().to_vec()))
        .collect::<Vec<_>>();
    assert_eq!(
        per_module,
        vec![
            ("@std.bool", vec![Role::Bool]),
            ("@std.builtin", vec![Role::Str, Role::Bytes, Role::Dict]),
            ("@std.core", vec![Role::Equatable, Role::Ordered]),
            ("@std.iter", vec![Role::Iter]),
            ("@std.option", vec![Role::Option]),
            ("@std.result", vec![Role::Result]),
        ]
    );
}

#[test]
fn a_missing_role_is_a_fault() {
    let option = embedded_source("@std.option");
    let changed = replace_once(&option, "  role @option\n", "");
    let error = load_with_module("@std.option", &changed);
    assert_eq!(
        error.as_ref().map(|error| error.diagnostic().code()),
        Some(DiagnosticCode::RuntimeInvalidCheckedProgram)
    );
    let notes = notes(error.as_ref());
    assert!(
        notes.contains("@option: no declaration claims the role"),
        "{notes}"
    );
}

#[test]
fn a_repeated_role_is_a_fault() {
    let result = embedded_source("@std.result");
    let changed = format!(
        "{result}\ntype Maybe(T)\n  case none\n  case some {{value: T}}\n  role @option\n  \
         visibility @public\nendtype\n"
    );
    let notes = notes(load_with_module("@std.result", &changed).as_ref());
    assert!(
        notes.contains("@option: 2 declarations claim the role; `Option` of `@std.option` repeats it")
            || notes.contains(
                "@option: 2 declarations claim the role; `Maybe` of `@std.result` repeats it"
            ),
        "{notes}"
    );
}

#[test]
fn a_misshapen_role_is_a_fault() {
    let bool_module = embedded_source("@std.bool");
    let changed = bool_module
        .replace("case false\n  case true\n", "case no\n  case yes\n")
        .replace("Bool.true()", "Bool.yes()")
        .replace("Bool.false()", "Bool.no()");
    let notes = notes(load_with_module("@std.bool", &changed).as_ref());
    assert!(
        notes.contains(
            "@bool: the declaration `Bool` does not have the shape the role requires: \
             its cases are not false, true"
        ),
        "{notes}"
    );
}

#[test]
fn a_role_on_the_wrong_kind_of_declaration_is_a_fault() {
    let option = embedded_source("@std.option");
    let changed = format!(
        "{option}\ninterface Walk\n  role @iter\n  visibility @public\n  type Item endtype\n  \
         fn next\n    takes {{value: Self}}\n    returns step: Option({{Item Self}})\n  endfn\n\
         endinterface\n"
    );
    let notes = notes(load_with_module("@std.option", &changed).as_ref());
    assert!(
        notes.contains("@iter: 2 declarations claim the role"),
        "{notes}"
    );
}

#[test]
fn an_option_with_its_cases_reordered_is_misshapen() {
    let option = embedded_source("@std.option");
    let changed = replace_once(
        &option,
        "  case none\n  case some {value: T}\n",
        "  case some {value: T}\n  case none\n",
    );
    let notes = notes(load_with_module("@std.option", &changed).as_ref());
    assert!(
        notes.contains("its cases are not none, some {value}"),
        "{notes}"
    );
}

#[test]
fn an_iter_without_next_is_misshapen() {
    let iter = embedded_source("@std.iter");
    let changed = iter.replacen("fn next\n", "fn advance\n", 1);
    let notes = notes(load_with_module("@std.iter", &changed).as_ref());
    assert!(
        notes.contains("@iter: the declaration `Iter` does not have the shape"),
        "{notes}"
    );
}

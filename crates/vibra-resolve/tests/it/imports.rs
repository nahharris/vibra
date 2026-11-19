//! Imports: locators, units, derived names, access, and cycles.

use vibra_diagnostics::DiagnosticCode as C;

use vibra_resolve::{Anchor, DeclarationId, DeclarationKind};

use crate::support::{
    STD_PACKAGE, assert_clean, codes, flat, resolve_modules, span_of, std_identity,
};

const MODEL: &str = "type User of {name: Str}\n  visibility @public\n  fn nameLength\n    takes {user: Self}\n    returns length: U64\n    visibility @public\n    do\n      return 0u64\n  endfn\nendtype\n\ntype Secret of U8 endtype\n";

#[test]
fn module_declaration_and_alias_imports_resolve() {
    for main in [
        "use @app.model enduse\n\ntype Account of model.User endtype\n",
        "use @app.model.User enduse\n\ntype Account of User endtype\n",
        "use @app.model as m enduse\n\ntype Account of m.User endtype\n",
    ] {
        let resolution = resolve_modules(&[("@app.main", main), ("@app.model", MODEL)]);
        assert_clean(&resolution);
        assert_eq!(resolution.imports().len(), 1);
        assert!(resolution.imports()[0].to().is_some());
        let in_main = resolution
            .references()
            .iter()
            .filter(|reference| reference.source_id() == "src/app/main.vib")
            .count();
        assert_eq!(in_main, 1, "{main}");
    }
}

#[test]
fn the_local_name_is_the_last_part_or_the_alias() {
    let resolution = resolve_modules(&[
        (
            "@app.main",
            "use @app.model enduse\n\nuse @app.model.User as Person enduse\n",
        ),
        ("@app.model", MODEL),
    ]);
    assert_clean(&resolution);
    let names = resolution
        .imports()
        .iter()
        .map(|import| import.name().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(names, ["model", "Person"]);
}

#[test]
fn a_walk_descends_directories_and_stops_at_the_first_module() {
    let resolution = resolve_modules(&[
        ("@app.main", "use @app.billing.cents.Cents enduse\n"),
        (
            "@app.billing.cents",
            "type Cents of U64\n  visibility @public\nendtype\n",
        ),
    ]);
    assert_clean(&resolution);
    let directory = resolve_modules(&[
        ("@app.main", "use @app.billing enduse\n"),
        ("@app.billing.cents", "type Cents of U64 endtype\n"),
    ]);
    assert_eq!(codes(&directory), [C::ModuleUnknownPath]);
}

#[test]
fn an_import_of_a_member_is_the_wrong_kind() {
    let main = "use @app.model.User.nameLength enduse\n";
    let resolution = resolve_modules(&[("@app.main", main), ("@app.model", MODEL)]);
    assert_eq!(
        flat(&resolution),
        [(
            C::NameWrongEntityKind,
            "src/app/main.vib".to_owned(),
            4,
            30,
            Vec::new()
        )]
    );
    assert_eq!(resolution.imports()[0].to(), None);
}

#[test]
fn an_unknown_declaration_or_member_is_an_unknown_symbol_at_the_part() {
    let main = "use @app.model.Nobody enduse\n";
    let resolution = resolve_modules(&[("@app.main", main), ("@app.model", MODEL)]);
    let (start, end) = span_of(main, "Nobody", 0);
    assert_eq!(
        flat(&resolution),
        [(
            C::NameUnknownSymbol,
            "src/app/main.vib".to_owned(),
            start,
            end,
            Vec::new()
        )]
    );
    let member = "use @app.model.User.age enduse\n";
    let resolution = resolve_modules(&[("@app.main", member), ("@app.model", MODEL)]);
    let (start, end) = span_of(member, "age", 0);
    assert_eq!(
        flat(&resolution),
        [(
            C::NameUnknownSymbol,
            "src/app/main.vib".to_owned(),
            start,
            end,
            Vec::new()
        )]
    );
}

#[test]
fn an_import_of_a_private_declaration_resolves_and_is_reported() {
    let main = "use @app.model.Secret enduse\n";
    let resolution = resolve_modules(&[("@app.main", main), ("@app.model", MODEL)]);
    let secret = span_of(MODEL, "Secret", 0);
    assert_eq!(
        flat(&resolution),
        [(
            C::NamePrivateAccess,
            "src/app/main.vib".to_owned(),
            4,
            21,
            vec![("src/app/model.vib".to_owned(), secret.0, secret.1)]
        )]
    );
    assert!(
        resolution.imports()[0].to().is_some(),
        "resolution is total"
    );
}

#[test]
fn a_private_declaration_through_a_module_import_is_reported_at_the_path() {
    let main = "use @app.model enduse\n\ntype Account of model.Secret endtype\n";
    let resolution = resolve_modules(&[("@app.main", main), ("@app.model", MODEL)]);
    let (start, end) = span_of(main, "model.Secret", 0);
    let secret = span_of(MODEL, "Secret", 0);
    assert_eq!(
        flat(&resolution),
        [(
            C::NamePrivateAccess,
            "src/app/main.vib".to_owned(),
            start,
            end,
            vec![("src/app/model.vib".to_owned(), secret.0, secret.1)]
        )]
    );
}

#[test]
fn unknown_paths() {
    for (main, extra) in [
        ("use @app.nothere enduse\n", None),
        (
            "use @app.Model enduse\n",
            Some(("@app.model", "type User of U8 endtype\n")),
        ),
        ("use @tests.t enduse\n", Some(("@tests.t", ""))),
        ("use @money.cents enduse\n", None),
        ("use @std enduse\n", None),
        ("use @std.nothere enduse\n", None),
    ] {
        let mut modules = vec![("@app.main", main)];
        modules.extend(extra);
        let resolution = resolve_modules(&modules);
        let locator = main.trim_start_matches("use ").split(' ').next().unwrap();
        let (start, end) = span_of(main, locator, 0);
        assert_eq!(
            flat(&resolution),
            [(
                C::ModuleUnknownPath,
                "src/app/main.vib".to_owned(),
                start,
                end,
                Vec::new()
            )],
            "{main}"
        );
    }
}

#[test]
fn a_test_module_names_every_local_target() {
    let resolution = resolve_modules(&[
        ("@tests.t", "use @app.model.User enduse\n"),
        ("@app.model", MODEL),
    ]);
    assert_clean(&resolution);
}

#[test]
fn the_standard_library_unit_resolves_by_its_modules() {
    let resolution = resolve_modules(&[("@app.main", "use @std.core enduse\n")]);
    assert_clean(&resolution);
    let core = Anchor::new(
        STD_PACKAGE.0,
        STD_PACKAGE.1,
        "@std.core",
        "",
        DeclarationKind::Module,
        "core",
    )
    .identity();
    assert_eq!(resolution.imports()[0].to(), Some(&core));
    let absent = resolve_modules(&[("@app.main", "use @std.io enduse\n")]);
    assert_eq!(codes(&absent), [C::ModuleUnknownPath], "no `@std.io` in M1");
}

#[test]
fn a_declaration_of_the_standard_library_is_imported_with_its_identity() {
    let text =
        "use @std.core.ArithmeticError enduse\n\ntype A of ArithmeticError endtype\n";
    let resolution = resolve_modules(&[("@app.main", text)]);
    assert_clean(&resolution);
    let error = std_identity("@std.core", DeclarationKind::Type, "ArithmeticError");
    assert_eq!(
        resolution.imports()[0].to().map(DeclarationId::as_str),
        Some(error.as_str())
    );
    let (start, end) = span_of(text, "ArithmeticError", 1);
    let references = resolution
        .references()
        .iter()
        .map(|reference| {
            (
                reference.span().start(),
                reference.span().end(),
                reference.to().as_str().to_owned(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(references, [(start, end, error)]);
}

#[test]
fn an_import_inside_the_standard_library_is_no_re_export() {
    let text = "use @std.builtin.ArithmeticError enduse\n";
    let resolution = resolve_modules(&[("@app.main", text)]);
    assert_eq!(
        codes(&resolution),
        [C::NameUnknownSymbol],
        "an import is no re-export"
    );
}

#[test]
fn a_two_module_cycle_is_reported_at_the_later_edge() {
    let resolution = resolve_modules(&[
        ("@app.a", "use @app.b enduse\n"),
        ("@app.b", "use @app.a enduse\n"),
    ]);
    assert_eq!(
        flat(&resolution),
        [(
            C::ModuleImportCycle,
            "src/app/b.vib".to_owned(),
            4,
            10,
            vec![("src/app/a.vib".to_owned(), 4, 10)]
        )]
    );
}

#[test]
fn a_longer_cycle_relates_every_earlier_edge_in_order() {
    let resolution = resolve_modules(&[
        ("@app.a", "use @app.b enduse\n"),
        ("@app.b", "use @app.c.C enduse\n"),
        (
            "@app.c",
            "use @app.a enduse\n\ntype C of U8\n  visibility @public\nendtype\n",
        ),
    ]);
    assert_eq!(
        flat(&resolution),
        [(
            C::ModuleImportCycle,
            "src/app/c.vib".to_owned(),
            4,
            10,
            vec![
                ("src/app/a.vib".to_owned(), 4, 10),
                ("src/app/b.vib".to_owned(), 4, 12),
            ]
        )]
    );
}

#[test]
fn a_module_importing_itself_is_a_cycle() {
    let resolution = resolve_modules(&[("@app.a", "use @app.a enduse\n")]);
    assert_eq!(codes(&resolution), [C::ModuleImportCycle]);
}

#[test]
fn an_acyclic_diamond_is_accepted() {
    let resolution = resolve_modules(&[
        ("@app.a", "use @app.b enduse\n\nuse @app.c enduse\n"),
        ("@app.b", "use @app.d enduse\n"),
        ("@app.c", "use @app.d enduse\n"),
        ("@app.d", ""),
    ]);
    assert_clean(&resolution);
}

#![allow(clippy::expect_used, clippy::indexing_slicing, missing_docs)]

use vibra_diagnostics::{DiagnosticCode, Level};
use vibra_resolve::{DeclarationId, EntityKind, ResolveInput, Resolver};
use vibra_syntax::parse_data;

#[test]
fn same_module_forward_reference_resolves_to_a_provenanced_id() {
    let input = ResolveInput::single_module(
        "demo",
        "1.0.0",
        "app",
        "main",
        br#"(defn caller () void (callee))
(defn callee () void (do))"#,
    );
    let snapshot = Resolver::resolve(input);

    assert!(snapshot.accepted(), "{:?}", snapshot.diagnostics());
    assert_eq!(snapshot.references().len(), 1);
    let target = snapshot.references()[0]
        .target()
        .expect("forward call target");
    assert_eq!(target.kind(), EntityKind::Function);
    assert_eq!(target.unit(), "app");
    assert_eq!(target.module(), ["main"]);
    assert_eq!(target.package().name(), "demo");
    assert_eq!(target.name(), "callee");
}

#[test]
fn imported_private_declaration_reports_access_with_stable_span() {
    let input = ResolveInput::new(
        "demo",
        "1.0.0",
        vec![vibra_resolve::SourceUnit::bin(
            "app",
            Some("app.main.run"),
            vec![
                vibra_resolve::SourceModule::new(
                    "app",
                    ["main"],
                    "src/main.vib",
                    br#"(import lib @app.lib)
(defn run () void (lib.secret))"#,
                ),
                vibra_resolve::SourceModule::new(
                    "app",
                    ["lib"],
                    "src/lib.vib",
                    br#"(defn secret () void (do))"#,
                ),
            ],
        )],
    );
    let snapshot = Resolver::resolve(input);

    assert!(!snapshot.accepted());
    assert!(snapshot.diagnostics().iter().any(|diagnostic| {
        diagnostic.code() == DiagnosticCode::NamePrivateAccess
            && diagnostic.level() == Level::Error
            && diagnostic.source_id() == Some("src/main.vib")
    }));
}

#[test]
fn declaration_identity_includes_owner_and_package_provenance() {
    let id = DeclarationId::new(
        "demo",
        "1.0.0",
        "app",
        ["main"],
        ["Thing", "method"],
        EntityKind::Function,
    );
    assert_eq!(id.canonical(), "@demo@1.0.0/app.main.Thing.method");
    assert_eq!(id.package().version(), "1.0.0");
}

#[test]
fn import_cycles_are_reported_after_module_headers_are_collected() {
    let input = ResolveInput::new(
        "demo",
        "1.0.0",
        vec![vibra_resolve::SourceUnit::bin(
            "app",
            Some("app.a.run"),
            vec![
                vibra_resolve::SourceModule::new(
                    "app",
                    ["a"],
                    "src/a.vib",
                    b"(import b @app.b)\n(defn run () void visibility: @public (b.call))",
                ),
                vibra_resolve::SourceModule::new(
                    "app",
                    ["b"],
                    "src/b.vib",
                    b"(import a @app.a)\n(defn call () void visibility: @public (a.run))",
                ),
            ],
        )],
    );
    let snapshot = Resolver::resolve(input);

    assert!(!snapshot.accepted());
    assert_eq!(
        snapshot
            .diagnostics()
            .iter()
            .filter(|diagnostic| diagnostic.code() == DiagnosticCode::ModuleImportCycle)
            .count(),
        1
    );
}

#[test]
fn entry_uses_the_own_target_rule_and_classifies_the_found_entity() {
    let outside = ResolveInput::new(
        "demo",
        "1.0.0",
        vec![vibra_resolve::SourceUnit::bin(
            "app",
            Some("other.main.run"),
            vec![vibra_resolve::SourceModule::new(
                "app",
                ["main"],
                "src/main.vib",
                b"(defn run () void (do))",
            )],
        )],
    );
    let snapshot = Resolver::resolve(outside);
    assert!(snapshot.diagnostics().iter().any(
        |diagnostic| diagnostic.code() == DiagnosticCode::ProjectEntryOutsideTarget
    ));

    let wrong_kind = ResolveInput::new(
        "demo",
        "1.0.0",
        vec![vibra_resolve::SourceUnit::bin(
            "app",
            Some("app.main.value"),
            vec![vibra_resolve::SourceModule::new(
                "app",
                ["main"],
                "src/main.vib",
                b"(def value i32 0i32)",
            )],
        )],
    );
    let snapshot = Resolver::resolve(wrong_kind);
    assert!(
        snapshot
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code() == DiagnosticCode::NameWrongEntityKind)
    );
}

#[test]
fn duplicate_top_level_names_and_lexical_bindings_have_distinct_contracts() {
    let input = ResolveInput::single_module(
        "demo",
        "1.0.0",
        "app",
        "main",
        b"(def value i32 0i32)\n(defn value () void (do))\n(defn f (x i32) void (let x 0i32 (do)))",
    );
    let snapshot = Resolver::resolve(input);

    assert!(
        snapshot
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code() == DiagnosticCode::NameRedeclaration)
    );
}

#[test]
fn atoms_and_literals_remain_values_while_unknown_symbol_paths_are_errors() {
    let input = ResolveInput::single_module(
        "demo",
        "1.0.0",
        "app",
        "main",
        b"(defn f () void @app.missing)\n(defn g () void missing)",
    );
    let snapshot = Resolver::resolve(input);

    assert_eq!(
        snapshot
            .diagnostics()
            .iter()
            .filter(|diagnostic| diagnostic.code() == DiagnosticCode::NameUnknownSymbol)
            .count(),
        1
    );
}

#[test]
fn resolved_artifact_is_structured_vibon_with_stable_text() {
    let input = ResolveInput::single_module(
        "demo",
        "1.0.0",
        "app",
        "main",
        b"(defn run () void (do))",
    );
    let snapshot = Resolver::resolve(input);
    let artifact = snapshot.canonical_vibon();
    let document = parse_data(std::path::Path::new("resolved.vibon"), &artifact)
        .expect("resolved artifact loader");

    assert!(document.accepted(), "{:?}", document.diagnostics());
    assert!(artifact.ends_with("\n"));
}

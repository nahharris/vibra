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
fn bare_imported_module_alias_reports_wrong_entity_kind_in_a_body() {
    let input = ResolveInput::new(
        "demo",
        "1.0.0",
        vec![vibra_resolve::SourceUnit::bin(
            "app",
            None,
            vec![
                vibra_resolve::SourceModule::new(
                    "app",
                    ["main"],
                    "src/main.vib",
                    b"(import lib @app.lib)\n(defn run () void lib)",
                ),
                vibra_resolve::SourceModule::new(
                    "app",
                    ["lib"],
                    "src/lib.vib",
                    b"(defn present () void visibility: @public (do))",
                ),
            ],
        )],
    );
    let snapshot = Resolver::resolve(input);

    let diagnostic = snapshot
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.code() == DiagnosticCode::NameWrongEntityKind)
        .expect("bare imported module alias diagnostic");
    assert_eq!(diagnostic.source_id(), Some("src/main.vib"));
    assert_eq!(
        diagnostic.primary_span(),
        vibra_diagnostics::ByteSpan::new(40, 43)
    );
    assert!(diagnostic.related().iter().any(|related| {
        related.source_id.as_deref() == Some("src/lib.vib")
            && related.span == vibra_diagnostics::ByteSpan::empty_at(0)
    }));
    let target = snapshot.references()[0]
        .target()
        .expect("resolved imported module target");
    assert_eq!(target.kind(), EntityKind::Module);
    assert_eq!(target.module(), ["lib"]);
    assert!(target.path().is_empty());
    assert!(
        !snapshot
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code() == DiagnosticCode::NameUnknownSymbol)
    );
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
fn entry_naming_a_module_reports_wrong_entity_kind_with_module_provenance() {
    let input = ResolveInput::new(
        "demo",
        "1.0.0",
        vec![vibra_resolve::SourceUnit::bin(
            "app",
            Some("app.main"),
            vec![vibra_resolve::SourceModule::new(
                "app",
                ["main"],
                "src/main.vib",
                b"(defn run () void (do))",
            )],
        )],
    );
    let snapshot = Resolver::resolve(input);

    assert!(snapshot.diagnostics().iter().any(|diagnostic| {
        diagnostic.code() == DiagnosticCode::NameWrongEntityKind
            && diagnostic
                .related()
                .iter()
                .any(|related| related.source_id.as_deref() == Some("src/main.vib"))
    }));
    assert!(
        !snapshot
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code() == DiagnosticCode::NameUnknownSymbol)
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

#[test]
fn importing_a_declaration_reports_wrong_entity_kind_before_unknown_path() {
    let input = ResolveInput::new(
        "demo",
        "1.0.0",
        vec![vibra_resolve::SourceUnit::bin(
            "app",
            None,
            vec![
                vibra_resolve::SourceModule::new(
                    "app",
                    ["main"],
                    "src/main.vib",
                    b"(import greet @app.lib.greet)\n(defn run () void (do))",
                ),
                vibra_resolve::SourceModule::new(
                    "app",
                    ["lib"],
                    "src/lib.vib",
                    b"(defn greet () void visibility: @public (do))",
                ),
            ],
        )],
    );
    let snapshot = Resolver::resolve(input);

    assert!(snapshot.diagnostics().iter().any(|diagnostic| {
        diagnostic.code() == DiagnosticCode::NameWrongEntityKind
            && diagnostic
                .related()
                .iter()
                .any(|related| related.source_id.as_deref() == Some("src/lib.vib"))
    }));
    assert!(
        !snapshot
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code() == DiagnosticCode::ModuleUnknownPath)
    );
    assert_eq!(snapshot.imports().len(), 1);
    assert!(snapshot.imports()[0].module().is_none());
}

#[test]
fn importing_a_missing_member_reports_unknown_symbol_at_the_referring_import() {
    let input = ResolveInput::new(
        "demo",
        "1.0.0",
        vec![vibra_resolve::SourceUnit::bin(
            "app",
            None,
            vec![
                vibra_resolve::SourceModule::new(
                    "app",
                    ["main"],
                    "src/main.vib",
                    b"(import lib @app.lib.missing)\n(defn run () void (do))",
                ),
                vibra_resolve::SourceModule::new(
                    "app",
                    ["lib"],
                    "src/lib.vib",
                    b"(defn present () void visibility: @public (do))",
                ),
            ],
        )],
    );
    let snapshot = Resolver::resolve(input);

    let diagnostic = snapshot
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.code() == DiagnosticCode::NameUnknownSymbol)
        .expect("missing imported member diagnostic");
    assert_eq!(diagnostic.source_id(), Some("src/main.vib"));
    assert_eq!(
        diagnostic.primary_span(),
        vibra_diagnostics::ByteSpan::new(0, 29)
    );
    assert!(
        !snapshot
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code() == DiagnosticCode::ModuleUnknownPath)
    );
}

#[test]
fn entry_missing_member_reports_unknown_symbol_at_the_project_reference() {
    let input = ResolveInput::new(
        "demo",
        "1.0.0",
        vec![vibra_resolve::SourceUnit::new(
            "app",
            vibra_resolve::TargetKind::Bin,
            Some(vibra_resolve::ReferencePath::new(
                ["app", "main", "missing"],
                "project.vibon",
                vibra_diagnostics::ByteSpan::new(141, 158),
            )),
            vec![vibra_resolve::SourceModule::new(
                "app",
                ["main"],
                "src/main.vib",
                b"(defn run () void (do))",
            )],
        )],
    );
    let snapshot = Resolver::resolve(input);

    let diagnostic = snapshot
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.code() == DiagnosticCode::NameUnknownSymbol)
        .expect("missing entry member diagnostic");
    assert_eq!(diagnostic.source_id(), Some("project.vibon"));
    assert_eq!(
        diagnostic.primary_span(),
        vibra_diagnostics::ByteSpan::new(141, 158)
    );
    assert!(
        !snapshot
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code() == DiagnosticCode::ModuleUnknownPath)
    );
}

#[test]
fn entries_accept_only_module_level_functions() {
    let input = ResolveInput::new(
        "demo",
        "1.0.0",
        vec![vibra_resolve::SourceUnit::bin(
            "app",
            Some("app.main.user.run"),
            vec![vibra_resolve::SourceModule::new(
                "app",
                ["main"],
                "src/main.vib",
                b"(deftype user (record field str) (defn run () void (do)))",
            )],
        )],
    );
    let snapshot = Resolver::resolve(input);

    assert!(
        snapshot
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code() == DiagnosticCode::NameWrongEntityKind)
    );
}

#[test]
fn lexical_and_lambda_label_bindings_cannot_shadow_visible_names() {
    let input = ResolveInput::new(
        "demo",
        "1.0.0",
        vec![vibra_resolve::SourceUnit::bin(
            "app",
            None,
            vec![vibra_resolve::SourceModule::new(
                "app",
                ["main"],
                "src/main.vib",
                br#"(import lib @app.lib)
(def x i32 0i32)
(defn f (lib i32) void
  (lambda () void labelled: (lib i32 0i32) (do lib)))"#,
            )],
        )],
    );
    let snapshot = Resolver::resolve(input);

    assert!(
        snapshot
            .diagnostics()
            .iter()
            .filter(|diagnostic| diagnostic.code() == DiagnosticCode::NameRedeclaration)
            .count()
            >= 2
    );
    assert!(
        !snapshot
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code() == DiagnosticCode::NameUnknownSymbol)
    );
}

#[test]
fn deferred_declaration_forms_and_members_are_explicitly_unavailable() {
    let input = ResolveInput::single_module(
        "demo",
        "1.0.0",
        "app",
        "main",
        br#"(deftype user (record field str) (defn method () void (do)))
(defint iface (defn call () void))
(deffect io (defn op () void))"#,
    );
    let snapshot = Resolver::resolve(input);

    assert!(
        snapshot
            .diagnostics()
            .iter()
            .filter(|diagnostic| diagnostic.code() == DiagnosticCode::ToolUnavailable)
            .count()
            >= 6
    );
}

#[test]
fn import_cycle_related_span_comes_from_the_traversed_cycle_edge() {
    let input = ResolveInput::new(
        "demo",
        "1.0.0",
        vec![vibra_resolve::SourceUnit::lib(
            "app",
            vec![
                vibra_resolve::SourceModule::new(
                    "app",
                    ["a"],
                    "src/a.vib",
                    b"(import b @app.b)\n(defn a () void (do))",
                ),
                vibra_resolve::SourceModule::new(
                    "app",
                    ["b"],
                    "src/b.vib",
                    b"(import c @app.c)\n(defn b () void (do))",
                ),
                vibra_resolve::SourceModule::new(
                    "app",
                    ["c"],
                    "src/c.vib",
                    b"(import a @app.a)\n(defn c () void (do))",
                ),
            ],
        )],
    );
    let snapshot = Resolver::resolve(input);

    assert!(snapshot.diagnostics().iter().any(|diagnostic| {
        diagnostic.code() == DiagnosticCode::ModuleImportCycle
            && diagnostic
                .related()
                .iter()
                .any(|related| related.source_id.as_deref() == Some("src/a.vib"))
    }));
}

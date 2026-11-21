//! The `@resolved.v2` observation.

use vibra_resolve::{Anchor, DeclarationKind};

use crate::support::resolve_modules;

fn id(module: &str, owner: &str, kind: DeclarationKind, name: &str) -> String {
    Anchor::new("hello", "0.1.0", module, owner, kind, name)
        .identity()
        .as_str()
        .to_owned()
}

#[test]
fn the_snapshot_is_the_closed_record_in_its_orders() {
    let main = "use @app.model enduse\n\ntype Account of model.User endtype\n";
    let model = "type User of @user\n  visibility @public\n  fn make returns u: Self do return User(@user) endfn\nendtype\n";
    let resolution = resolve_modules(&[("@app.model", model), ("@app.main", main)]);
    assert!(resolution.diagnostics().is_empty());
    let mut declarations = [
        (
            id("@app.main", "", DeclarationKind::Type, "Account"),
            "{package: \"hello\" version: \"0.1.0\" module: @app.main kind: @type name: \"Account\"} access: @private span: {sourceId: \"src/app/main.vib\" start: 23u64 end: 57u64}",
        ),
        (
            id("@app.model", "", DeclarationKind::Type, "User"),
            "{package: \"hello\" version: \"0.1.0\" module: @app.model kind: @type name: \"User\"} access: @public span: {sourceId: \"src/app/model.vib\" start: 0u64 end: 101u64}",
        ),
        (
            id("@app.model", "User", DeclarationKind::Fn, "make"),
            "{package: \"hello\" version: \"0.1.0\" module: @app.model owner: \"User\" kind: @fn name: \"make\"} access: @private span: {sourceId: \"src/app/model.vib\" start: 42u64 end: 93u64}",
        ),
    ];
    declarations.sort();
    let declarations = declarations
        .iter()
        .map(|(identity, rest)| format!("{{identity: \"{identity}\" anchor: {rest}}}"))
        .collect::<Vec<_>>()
        .join(" ");
    let expected = format!(
        "{{format: @resolved.v2 package: {{name: \"hello\" version: \"0.1.0\"}} modules: [{{path: @app.main unit: @app sourceId: \"src/app/main.vib\"}} {{path: @app.model unit: @app sourceId: \"src/app/model.vib\"}}] declarations: [{declarations}] imports: [{{span: {{sourceId: \"src/app/main.vib\" start: 0u64 end: 21u64}} path: @app.model name: \"model\" to: \"{}\"}}] references: [{{span: {{sourceId: \"src/app/main.vib\" start: 39u64 end: 49u64}} to: \"{user}\"}} {{span: {{sourceId: \"src/app/model.vib\" start: 76u64 end: 80u64}} to: \"{user}\"}}]}}",
        id("@app.model", "", DeclarationKind::Module, "model"),
        user = id("@app.model", "", DeclarationKind::Type, "User"),
    );
    assert_eq!(resolution.canonical_text(), expected);
    let parsed = vibra_syntax::parse_data(None, resolution.canonical_text().as_bytes());
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
}

#[test]
fn an_unresolved_import_has_no_target() {
    let resolution = resolve_modules(&[("@app.main", "use @app.nothere enduse\n")]);
    assert!(
        resolution
            .canonical_text()
            .contains("imports: [{span: {sourceId: \"src/app/main.vib\" start: 0u64 end: 23u64} path: @app.nothere name: \"nothere\"}]")
    );
}

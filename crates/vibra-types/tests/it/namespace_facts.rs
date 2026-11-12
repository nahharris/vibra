//! Immutable checked name resolutions for graph projections.

use vibra_resolve::{Resolver, SourceModule, SourceUnit};
use vibra_types::{
    CheckedNamespaceRole, check_resolved, load_stdlib, standard_resolve_input,
};

#[test]
fn successful_type_interface_and_effect_names_are_exported_with_targets() {
    let source = concat!(
        "(deftype item i32)\n",
        "(defint marker\n",
        "  (defn mark (value self) str))\n",
        "(deffect write (defn text (target str) void (do)))\n",
        "(defn generic (input t) str where: (t marker) \"x\")\n",
        "(defn typed (input item) item input)\n",
        "(defn hop () void effects: (write) (write.text \"x\"))\n",
    );
    let stdlib = load_stdlib().expect("embedded standard library");
    let resolved = Resolver::resolve(standard_resolve_input(
        "demo",
        "0.1.0",
        vec![SourceUnit::lib(
            "app",
            vec![SourceModule::new("app", ["main"], "src/main.vib", source)],
        )],
        Some(&stdlib),
    ));
    assert!(resolved.accepted(), "{:?}", resolved.diagnostics());

    let checked =
        check_resolved(&resolved, &["src/main.vib".to_owned()], Some(&stdlib));
    assert!(checked.accepted(), "{:?}", checked.diagnostics());

    let facts = checked.namespace_facts();
    assert!(facts.iter().any(|fact| {
        fact.source_id() == "src/main.vib"
            && fact.written() == "item"
            && fact.role() == CheckedNamespaceRole::Type
            && fact.identity().ends_with(".item")
    }));
    assert!(facts.iter().any(|fact| {
        fact.source_id() == "src/main.vib"
            && fact.written() == "marker"
            && fact.role() == CheckedNamespaceRole::Interface
            && fact.identity().ends_with(".marker")
    }));
    assert!(facts.iter().any(|fact| {
        fact.source_id() == "src/main.vib"
            && fact.written() == "write"
            && fact.role() == CheckedNamespaceRole::Effect
            && fact.identity().ends_with(".write")
    }));
}

#[test]
fn native_source_bodies_remain_available_beside_accelerated_wrappers() {
    let stdlib = load_stdlib().expect("verified standard library");
    let resolved = Resolver::resolve(standard_resolve_input(
        "demo",
        "0.1.0",
        vec![SourceUnit::lib(
            "app",
            vec![SourceModule::new(
                "app",
                ["main"],
                "src/main.vib",
                "(defn measure (text str) u64 (str.length text))",
            )],
        )],
        Some(&stdlib),
    ));
    assert!(resolved.accepted(), "{:?}", resolved.diagnostics());
    let ordinary =
        check_resolved(&resolved, &["src/main.vib".to_owned()], Some(&stdlib));
    let ordinary_native = ordinary
        .modules()
        .expect("ordinary checked modules")
        .functions()
        .iter()
        .find(|function| function.name().ends_with("/std.str.str.length"))
        .expect("ordinary native");
    assert_eq!(ordinary_native.semantic_body().0, ordinary_native.body());
    let checked = vibra_types::check_resolved_complete(
        &resolved,
        &["src/main.vib".to_owned()],
        Some(&stdlib),
    );
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    let native = checked
        .modules()
        .expect("checked modules")
        .functions()
        .iter()
        .find(|function| function.name().ends_with("/std.str.str.length"))
        .expect("admitted trusted native");
    assert_ne!(native.semantic_body().0, native.body());
    assert_eq!(native.module_body().0, native.body());
    assert!(native.semantic_body().1 >= native.signature().fixed_parameter_count());
}

#[test]
fn complete_projection_materializes_unreferenced_builtin_members_explicitly() {
    let stdlib = load_stdlib().expect("verified standard library");
    let resolved = Resolver::resolve(standard_resolve_input(
        "demo",
        "0.1.0",
        vec![SourceUnit::lib(
            "app",
            vec![SourceModule::new(
                "app",
                ["main"],
                "src/main.vib",
                "(defn main () void void)",
            )],
        )],
        Some(&stdlib),
    ));
    let sources = ["src/main.vib".to_owned()];
    let ordinary = check_resolved(&resolved, &sources, Some(&stdlib));
    let complete =
        vibra_types::check_resolved_complete(&resolved, &sources, Some(&stdlib));
    assert!(complete.accepted(), "{:?}", complete.diagnostics());
    let has_dict_of = |checked: &vibra_types::ResolvedCheckResult| {
        checked
            .modules()
            .expect("checked modules")
            .functions()
            .iter()
            .any(|function| function.name().ends_with("/std.builtin.dict.of"))
    };
    assert!(!has_dict_of(&ordinary));
    assert!(has_dict_of(&complete));
}

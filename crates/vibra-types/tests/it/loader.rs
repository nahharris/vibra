//! The embedded library loads, and what it holds.

use vibra_ir::Library;
use vibra_types::{PRELUDE, Role, declaration_identity, load_stdlib};

#[test]
fn the_embedded_library_loads() {
    let loaded = load_stdlib();
    if let Err(error) = &loaded {
        println!("{error}");
    }
    let stdlib = loaded.expect("the embedded standard library loads");
    println!(
        "loaded {} modules and {} roles",
        stdlib.modules().len(),
        stdlib.roles().len()
    );
    assert_eq!(stdlib.modules().len(), 11);
    assert_eq!(stdlib.roles().len(), 9);
    assert_eq!(stdlib.package_name(), "std");
    assert_eq!(stdlib.package_version(), "0.0.2");
}

#[test]
fn every_role_has_its_declaration() {
    let stdlib = load_stdlib().expect("the embedded standard library loads");
    let expected = [
        (Role::Bool, "@std.bool", "Bool", "@type"),
        (Role::Str, "@std.str", "Str", "@type"),
        (Role::Bytes, "@std.bytes", "Bytes", "@type"),
        (Role::Option, "@std.option", "Option", "@type"),
        (Role::Result, "@std.result", "Result", "@type"),
        (Role::Dict, "@std.builtin", "Dict", "@type"),
        (Role::Iter, "@std.iter", "Iter", "@interface"),
        (Role::Equatable, "@std.core", "Equatable", "@interface"),
        (Role::Ordered, "@std.core", "Ordered", "@interface"),
    ];
    for (role, module, name, kind) in expected {
        let declaration = stdlib.role(role).expect("every role has a declaration");
        assert_eq!(
            (declaration.module(), declaration.name(), declaration.kind()),
            (module, name, kind),
            "{}",
            role.atom()
        );
        assert_eq!(
            declaration.identity(),
            declaration_identity("std", "0.0.2", module, "", kind, name)
        );
    }
}

#[test]
fn the_ir_library_types_are_bound() {
    let stdlib = load_stdlib().expect("the embedded standard library loads");
    for library in Library::ALL {
        let declaration = stdlib
            .library()
            .get(&library)
            .expect("every library declaration is bound");
        let id = declaration
            .type_id()
            .expect("every declaration has an IR identity");
        assert_eq!(stdlib.library_types().get(library), Some(&id));
    }
    let ordering = stdlib
        .library()
        .get(&Library::Ordering)
        .expect("Ordering is bound");
    assert_eq!(ordering.path(), "std.core.Ordering");
}

#[test]
fn the_prelude_table_binds_every_declared_name() {
    let stdlib = load_stdlib().expect("the embedded standard library loads");
    let builtin = [
        "Void", "Never", "Atom", "Char", "I8", "I16", "I32", "I64", "U8", "U16", "U32",
        "U64", "F32", "F64", "List", "Dict",
    ];
    let expected = |name: &str| match name {
        "Bool" | "true" | "false" => Some("@std.bool"),
        "Option" => Some("@std.option"),
        "Result" => Some("@std.result"),
        "Iter" => Some("@std.iter"),
        "Str" => Some("@std.str"),
        "Bytes" => Some("@std.bytes"),
        "Equatable" | "Ordered" | "From" | "TryFrom" | "Ordering" => Some("@std.core"),
        "Any" | "Self" => None,
        name if builtin.contains(&name) => Some("@std.builtin"),
        _ => Some("unexpected"),
    };
    for name in PRELUDE {
        let found = stdlib
            .prelude()
            .get(name)
            .map(|declaration| declaration.module());
        assert_eq!(found, expected(name), "the prelude name `{name}`");
    }
    assert_eq!(stdlib.prelude().len(), PRELUDE.len() - 2);
    let kinds = |name: &str| {
        stdlib
            .prelude()
            .get(name)
            .map(|declaration| declaration.kind())
    };
    assert_eq!(kinds("true"), Some("@value"));
    assert_eq!(kinds("Iter"), Some("@interface"));
    assert_eq!(kinds("I32"), Some("@type"));
}

#[test]
fn the_overlay_is_the_unit_std() {
    let stdlib = load_stdlib().expect("the embedded standard library loads");
    let overlay = stdlib.overlay();
    assert_eq!(overlay.unit, "@std");
    let modules = overlay
        .modules
        .iter()
        .map(|module| {
            (
                module.path.as_str(),
                module.segments.join("."),
                module.source_id.as_str(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        modules,
        vec![
            (
                "@std.assert",
                "assert".to_owned(),
                "stdlib/src/std/assert.vib"
            ),
            ("@std.bool", "bool".to_owned(), "stdlib/src/std/bool.vib"),
            (
                "@std.builtin",
                "builtin".to_owned(),
                "stdlib/src/std/builtin.vib"
            ),
            ("@std.bytes", "bytes".to_owned(), "stdlib/src/std/bytes.vib"),
            ("@std.core", "core".to_owned(), "stdlib/src/std/core.vib"),
            ("@std.iter", "iter".to_owned(), "stdlib/src/std/iter.vib"),
            (
                "@std.option",
                "option".to_owned(),
                "stdlib/src/std/option.vib"
            ),
            (
                "@std.policy",
                "policy".to_owned(),
                "stdlib/src/std/policy.vib"
            ),
            (
                "@std.process",
                "process".to_owned(),
                "stdlib/src/std/process.vib"
            ),
            (
                "@std.result",
                "result".to_owned(),
                "stdlib/src/std/result.vib"
            ),
            ("@std.str", "str".to_owned(), "stdlib/src/std/str.vib"),
        ]
    );
    assert_eq!(&overlay.prelude, stdlib.prelude());
}

#[test]
fn loading_twice_gives_the_same_library() {
    assert_eq!(load_stdlib(), load_stdlib());
}

#[test]
fn the_overlay_is_resolution_s_embedded_unit() {
    let stdlib = load_stdlib().expect("the embedded standard library loads");
    let unit = stdlib.overlay().embedded_unit();
    assert_eq!(unit.name(), "@std");
    assert_eq!(unit.package().name(), "std");
    assert_eq!(unit.package().version(), "0.0.2");
    let modules = unit
        .modules()
        .iter()
        .map(|module| (module.path(), module.source_id()))
        .collect::<Vec<_>>();
    assert_eq!(
        modules,
        [
            ("@std.assert", "stdlib/src/std/assert.vib"),
            ("@std.bool", "stdlib/src/std/bool.vib"),
            ("@std.builtin", "stdlib/src/std/builtin.vib"),
            ("@std.bytes", "stdlib/src/std/bytes.vib"),
            ("@std.core", "stdlib/src/std/core.vib"),
            ("@std.iter", "stdlib/src/std/iter.vib"),
            ("@std.option", "stdlib/src/std/option.vib"),
            ("@std.policy", "stdlib/src/std/policy.vib"),
            ("@std.process", "stdlib/src/std/process.vib"),
            ("@std.result", "stdlib/src/std/result.vib"),
            ("@std.str", "stdlib/src/std/str.vib"),
        ]
    );
    for module in unit.modules() {
        let loaded = stdlib.module(module.path()).expect("a loaded module");
        assert_eq!(module.bytes(), loaded.bytes());
    }
    assert_eq!(
        unit.prelude().len(),
        PRELUDE.len() - 2,
        "all but `Any` and `Self`"
    );
    for binding in unit.prelude() {
        let declaration = stdlib
            .prelude()
            .get(binding.name())
            .expect("a prelude name");
        assert_eq!(binding.identity(), declaration.identity());
    }
}

#[test]
fn resolution_binds_a_prelude_name_through_the_embedded_unit() {
    let stdlib = load_stdlib().expect("the embedded standard library loads");
    let input = vibra_resolve::ResolveInput::new(
        vibra_resolve::PackageId::new("hello", "0.1.0"),
        vec![vibra_resolve::SourceUnit::new(
            "@app",
            vibra_resolve::UnitKind::Lib,
            vec![vibra_resolve::SourceModule::new(
                "@app.main",
                "src/app/main.vib",
                b"type A of Str endtype\n".to_vec(),
            )],
        )],
        stdlib.overlay().embedded_unit(),
    );
    let resolution = vibra_resolve::resolve(&input);
    assert!(
        resolution.diagnostics().is_empty(),
        "{:?}",
        resolution.diagnostics()
    );
    let str_identity = stdlib.prelude().get("Str").expect("`Str`").identity();
    let targets = resolution
        .references()
        .iter()
        .map(|reference| reference.to().as_str())
        .collect::<Vec<_>>();
    assert_eq!(targets, [str_identity]);
}

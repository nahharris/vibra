//! Declaration identities: the type chapter's test vector, and the anchor
//! that holds no body.

use vibra_resolve::{Anchor, DeclarationKind};

use crate::support::resolve_modules;

#[test]
fn the_type_chapters_test_vector() {
    let anchor = Anchor::new(
        "hello",
        "0.1.0",
        "@hello.main",
        "",
        DeclarationKind::Proc,
        "main",
    );
    assert_eq!(
        anchor.identity().as_str(),
        "sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad"
    );
}

/// The identity of the one declaration named `name` in `text`, `@app.model`.
fn identity_of(text: &str, name: &str) -> String {
    let resolution = resolve_modules(&[("@app.model", text)]);
    let matching = resolution
        .declarations()
        .iter()
        .filter(|declaration| declaration.anchor().name() == name)
        .collect::<Vec<_>>();
    assert_eq!(matching.len(), 1, "{:?}", resolution.declarations());
    matching[0].identity().as_str().to_owned()
}

#[test]
fn a_types_identity_does_not_change_with_its_fields_and_changes_with_its_name() {
    let before = identity_of(
        "type User of {name: Str}\n  visibility @public\nendtype\n",
        "User",
    );
    let fields = identity_of(
        "type User of {name: Str age: U8}\n  visibility @public\nendtype\n",
        "User",
    );
    let body = identity_of("type User of U64 endtype\n", "User");
    let renamed = identity_of(
        "type Person of {name: Str}\n  visibility @public\nendtype\n",
        "Person",
    );
    assert_eq!(before, fields);
    assert_eq!(before, body);
    assert_ne!(before, renamed);
    assert_eq!(
        before,
        Anchor::new(
            "hello",
            "0.1.0",
            "@app.model",
            "",
            DeclarationKind::Type,
            "User"
        )
        .identity()
        .as_str()
    );
}

#[test]
fn members_are_anchored_under_their_owner() {
    let text = "type Shape\n  case circle {radius: F64}\n  case empty\n  fn area\n    takes {shape: Self}\n    returns area: F64\n    do\n      return 0.0f64\n  endfn\nendtype\n\ninterface Source\n  type Item endtype\n  proc read\n    takes {source: Self}\n    returns item: Item\n  endproc\nendinterface\n\nlet limit: U8 be 1u8 endlet\n";
    let resolution = resolve_modules(&[("@app.model", text)]);
    let mut anchors = resolution
        .declarations()
        .iter()
        .map(|declaration| {
            let anchor = declaration.anchor();
            (
                anchor.owner().to_owned(),
                anchor.kind().as_atom(),
                anchor.name().to_owned(),
            )
        })
        .collect::<Vec<_>>();
    anchors.sort();
    assert_eq!(
        anchors,
        vec![
            (String::new(), "@interface", "Source".to_owned()),
            (String::new(), "@type", "Shape".to_owned()),
            (String::new(), "@value", "limit".to_owned()),
            ("Shape".to_owned(), "@case", "circle".to_owned()),
            ("Shape".to_owned(), "@case", "empty".to_owned()),
            ("Shape".to_owned(), "@fn", "area".to_owned()),
            ("Source".to_owned(), "@associatedType", "Item".to_owned()),
            ("Source".to_owned(), "@proc", "read".to_owned()),
        ]
    );
    let area = resolution
        .declarations()
        .iter()
        .find(|declaration| declaration.anchor().name() == "area")
        .unwrap();
    assert_eq!(
        area.identity(),
        &Anchor::new(
            "hello",
            "0.1.0",
            "@app.model",
            "Shape",
            DeclarationKind::Fn,
            "area"
        )
        .identity()
    );
}

#[test]
fn a_test_is_anchored_by_its_name_value() {
    let resolution = resolve_modules(&[(
        "@tests.math",
        "use @std.assert enduse\n\ntest \"adds \\\"two\\\"\" do\n  void\nendtest\n",
    )]);
    let test = resolution
        .declarations()
        .iter()
        .find(|declaration| declaration.anchor().kind() == DeclarationKind::Test)
        .expect("one test");
    assert_eq!(test.anchor().name(), "adds \"two\"");
    assert_eq!(test.anchor().module(), "@tests.math");
}

use std::path::Path;

#[test]
fn legacy_yaml_editor_stack_is_not_shipped() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    let library = std::fs::read_to_string(root.join("src/lib.rs")).unwrap();

    assert!(!manifest.contains("yaml-edit"));
    assert!(!library.contains("pub mod code"));
    assert!(!library.contains("mod yaml_subset"));
    assert!(!root.join("src/code").exists());
    assert!(!root.join("src/yaml_subset.rs").exists());
}

#[test]
fn legacy_yaml_tooling_helpers_are_not_shipped() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let library = std::fs::read_to_string(root.join("src/lib.rs")).unwrap();
    let tooling = std::fs::read_to_string(root.join("src/tooling.rs")).unwrap();

    assert!(!library.contains("pub mod annotations"));
    assert!(!library.contains("pub mod macro_expand"));
    assert!(!root.join("src/annotations.rs").exists());
    assert!(!root.join("src/macro_expand.rs").exists());
    assert!(!tooling.contains("serde_yaml"));
    assert!(!tooling.contains("load_legacy_yaml_program"));
}

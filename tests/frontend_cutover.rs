use std::path::Path;

#[test]
fn legacy_yaml_editor_stack_is_not_shipped() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    let library = std::fs::read_to_string(root.join("src/lib.rs")).unwrap();

    assert!(!manifest.contains("yaml-edit"));
    assert!(!manifest.contains("serde_yaml"));
    assert!(!library.contains("pub mod code"));
    assert!(!library.contains("mod yaml_subset"));
    assert!(!root.join("src/code").exists());
    assert!(!root.join("src/yaml_subset.rs").exists());
}

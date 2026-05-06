use std::path::Path;

#[test]
fn import_cycle_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.vibra");
    let b = dir.path().join("b.vibra");
    std::fs::write(
        &a,
        "io:\n  $import: ./b.vibra\n",
    )
    .unwrap();
    std::fs::write(
        &b,
        "io:\n  $import: ./a.vibra\n",
    )
    .unwrap();
    let err = vibra::load::load_program(&a).unwrap_err();
    let s = err.to_string();
    assert!(
        s.contains("cycle") || s.contains("E-MOD-003"),
        "unexpected error: {s}"
    );
}

#[test]
fn hello_example_compiles_and_runs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let p = root.join("examples/hello.vibra");
    let prog = vibra::load::load_program(&p).unwrap();
    let msg = vibra::lower::extract_print_message(&prog).unwrap();
    assert_eq!(msg, "Hello, World!");
    let wasm = vibra::emit::emit_println_wasm(msg.as_bytes());
    vibra::run_wasmer::run_wasm(&wasm).unwrap();
}

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
    let lowered = vibra::lower::lower_program(&prog).unwrap();
    vibra::execute::run_lowered(&lowered, &vibra::runtime::RunConfig::default()).unwrap();
}

#[test]
fn lower_forwards_import_args_from_stdlib_contract() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let p = root.join("examples/hello.vibra");
    let prog = vibra::load::load_program(&p).unwrap();
    let lowered = vibra::lower::lower_program(&prog).unwrap();
    let first = lowered.statements.first().expect("has first statement");
    match first {
        vibra::lower::Statement::Call(call) => {
            assert_eq!(call.function.import.module, "wasi_snapshot_preview1");
            assert_eq!(call.function.import.name, "fd_write");
            assert_eq!(call.function.arg_names, vec!["msg".to_string()]);
            assert_eq!(call.function.wasm_args.len(), 2);
        }
        _ => panic!("expected first statement call"),
    }
}

#[test]
fn fs_roundtrip_works_with_preopen() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("roundtrip.vibra");
    let io_import = root.join("stdlib/io.vibra").display().to_string().replace('\\', "/");
    let fs_import = root.join("stdlib/fs.vibra").display().to_string().replace('\\', "/");
    let tmp_dir = dir.path().join("tmp").display().to_string().replace('\\', "/");
    let note_path = dir
        .path()
        .join("tmp")
        .join("note.txt")
        .display()
        .to_string()
        .replace('\\', "/");
    std::fs::write(
        &entry,
        format!(
            r#"io:
  $import: "{io_import}"
fs:
  $import: "{fs_import}"
main:
  $function:
    args: $void
    return: $void
    do:
      - $fs.create-dir-all: "{tmp_dir}"
      - $fs.write-file:
          path: "{note_path}"
          contents: "abc"
      - $let:
          data:
            $fs.read-file: "{note_path}"
      - $io.println: $data
"#
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).unwrap();
    vibra::execute::run_lowered(
        &lowered,
        &vibra::runtime::RunConfig {
            preopen_host_dirs: vec![root.to_path_buf(), dir.path().to_path_buf()],
            ..vibra::runtime::RunConfig::default()
        },
    )
    .unwrap();
}

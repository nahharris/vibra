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
    let types_import = root
        .join("stdlib/types.vibra")
        .display()
        .to_string()
        .replace('\\', "/");
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
types:
  $import: "{types_import}"
main:
  $function:
    args: $void
    return: $void
    do:
      - $fs.create-dir-all:
          $types.Path.FromStr: "{tmp_dir}"
      - $fs.write-file:
          path:
            $types.Path.FromStr: "{note_path}"
          contents: "abc"
      - $let:
          data:
            $fs.read-file:
              $types.Path.FromStr: "{note_path}"
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

#[test]
fn primitive_type_annotations_are_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let std = dir.path().join("typed.vibra");
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &std,
        r#"takes-int8:
  $function:
    args:
      value: $int8
    return: $void
    do:
      - $wasm:
          import:
            module: wasi_snapshot_preview1
            name: fd_write
          args:
            - $args.value
takes-int16:
  $function:
    args:
      value: $int16
    return: $void
    do:
      - $wasm:
          import:
            module: wasi_snapshot_preview1
            name: fd_write
          args:
            - $args.value
takes-int32:
  $function:
    args:
      value: $int32
    return: $void
    do:
      - $wasm:
          import:
            module: wasi_snapshot_preview1
            name: fd_write
          args:
            - $args.value
takes-int64:
  $function:
    args:
      value: $int64
    return: $void
    do:
      - $wasm:
          import:
            module: wasi_snapshot_preview1
            name: fd_write
          args:
            - $args.value
takes-uint8:
  $function:
    args:
      value: $uint8
    return: $void
    do:
      - $wasm:
          import:
            module: wasi_snapshot_preview1
            name: fd_write
          args:
            - $args.value
takes-uint16:
  $function:
    args:
      value: $uint16
    return: $void
    do:
      - $wasm:
          import:
            module: wasi_snapshot_preview1
            name: fd_write
          args:
            - $args.value
takes-uint32:
  $function:
    args:
      value: $uint32
    return: $void
    do:
      - $wasm:
          import:
            module: wasi_snapshot_preview1
            name: fd_write
          args:
            - $args.value
takes-uint64:
  $function:
    args:
      value: $uint64
    return: $void
    do:
      - $wasm:
          import:
            module: wasi_snapshot_preview1
            name: fd_write
          args:
            - $args.value
takes-float32:
  $function:
    args:
      value: $float32
    return: $void
    do:
      - $wasm:
          import:
            module: wasi_snapshot_preview1
            name: fd_write
          args:
            - $args.value
takes-float64:
  $function:
    args:
      value: $float64
    return: $void
    do:
      - $wasm:
          import:
            module: wasi_snapshot_preview1
            name: fd_write
          args:
            - $args.value
identity:
  $function:
    args:
      value: $MyType
    return: $Result
    do:
      - $wasm:
          import:
            module: wasi_snapshot_preview1
            name: fd_write
          args:
            - $args.value
"#,
    )
    .unwrap();

    std::fs::write(
        &entry,
        r#"typed:
  $import: ./typed.vibra
main:
  $function:
    args: $void
    return: $void
    do:
      - $typed.takes-int8: 1
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog);
    assert!(lowered.is_ok(), "expected primitive annotations to lower");
}

#[test]
fn rejects_empty_args_mapping_for_zero_arg_functions() {
    let dir = tempfile::tempdir().unwrap();
    let std = dir.path().join("bad-args.vibra");
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &std,
        r#"no-args:
  $function:
    args: {}
    return: $void
    do:
      - $wasm:
          import:
            module: wasi_snapshot_preview1
            name: fd_write
          args: []
"#,
    )
    .unwrap();

    std::fs::write(
        &entry,
        r#"typed:
  $import: ./bad-args.vibra
main:
  $function:
    args: $void
    return: $void
    do:
      - $typed.no-args: $void
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = vibra::lower::lower_program(&prog).unwrap_err().to_string();
    assert!(
        err.contains("invalid function args") || err.contains("args: $void"),
        "unexpected error for empty args mapping: {err}"
    );
}

#[test]
fn rejects_missing_return_annotation() {
    let dir = tempfile::tempdir().unwrap();
    let std = dir.path().join("bad-return.vibra");
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &std,
        r#"missing-return:
  $function:
    args: $void
    do:
      - $wasm:
          import:
            module: wasi_snapshot_preview1
            name: fd_write
          args: []
"#,
    )
    .unwrap();

    std::fs::write(
        &entry,
        r#"typed:
  $import: ./bad-return.vibra
main:
  $function:
    args: $void
    return: $void
    do:
      - $typed.missing-return: $void
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = vibra::lower::lower_program(&prog).unwrap_err().to_string();
    assert!(
        err.contains("function missing return"),
        "unexpected error for missing return: {err}"
    );
}

#[test]
fn parses_union_declarations_from_imports() {
    let dir = tempfile::tempdir().unwrap();
    let unions = dir.path().join("unions.vibra");
    let std = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &unions,
        r#"MaybeText:
  $union:
    variants:
      Some: $str
      None: $void
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"u:
  $import: "{u}"
io:
  $import: "{io}"
main:
  $function:
    args: $void
    return: $void
    do:
      - $let:
          msg:
            $u.MaybeText.Some: "hello"
      - $match:
          target: $msg
          arms:
            Some:
              bind: text
              do:
                - $io.println: $text
            None:
              do:
                - $io.println: "none"
"#,
            u = unions.display().to_string().replace('\\', "/"),
            io = std.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog);
    assert!(lowered.is_ok(), "expected union + match program to lower");
}

#[test]
fn constructor_payload_type_is_validated() {
    let dir = tempfile::tempdir().unwrap();
    let unions = dir.path().join("unions.vibra");
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &unions,
        r#"MaybeNumber:
  $union:
    variants:
      Some: $int
      None: $void
expects-maybe:
  $function:
    args:
      value: $MaybeNumber
    return: $void
    do:
      - $wasm:
          import:
            module: wasi_snapshot_preview1
            name: fd_write
          args:
            - $const.1
            - "ok"
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"u:
  $import: "{u}"
io:
  $import: "{io}"
main:
  $function:
    args: $void
    return: $void
    do:
      - $u.expects-maybe:
          $u.MaybeNumber.Some: "oops"
"#,
            u = unions.display().to_string().replace('\\', "/"),
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = vibra::lower::lower_program(&prog).unwrap_err().to_string();
    assert!(
        err.contains("payload type mismatch") || err.contains("type mismatch"),
        "unexpected error: {err}"
    );
}

#[test]
fn match_reports_unknown_variant() {
    let dir = tempfile::tempdir().unwrap();
    let unions = dir.path().join("unions.vibra");
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &unions,
        r#"MaybeText:
  $union:
    variants:
      Some: $str
      None: $void
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"u:
  $import: "{u}"
io:
  $import: "{io}"
main:
  $function:
    args: $void
    return: $void
    do:
      - $let:
          value:
            $u.MaybeText.Some: "hello"
      - $match:
          target: $value
          arms:
            Some:
              bind: text
              do:
                - $io.println: $text
            Missing:
              do:
                - $io.println: "bad"
"#,
            u = unions.display().to_string().replace('\\', "/"),
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = vibra::lower::lower_program(&prog).unwrap_err().to_string();
    assert!(
        err.contains("unknown variant `Missing`"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_raw_path_when_domain_path_required() {
    let dir = tempfile::tempdir().unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let io = root.join("stdlib/io.vibra").display().to_string().replace('\\', "/");
    let fs = root.join("stdlib/fs.vibra").display().to_string().replace('\\', "/");
    let types = root
        .join("stdlib/types.vibra")
        .display()
        .to_string()
        .replace('\\', "/");
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &entry,
        format!(
            r#"io:
  $import: "{io}"
fs:
  $import: "{fs}"
types:
  $import: "{types}"
main:
  $function:
    args: $void
    return: $void
    do:
      - $fs.read-file: "tmp/file.txt"
"#
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = vibra::lower::lower_program(&prog).unwrap_err().to_string();
    assert!(
        err.contains("expected Named(\"types.Path\")")
            || err.contains("expected Named(\"Path\")")
            || err.contains("expected"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_raw_fd_when_domain_fd_required() {
    let dir = tempfile::tempdir().unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let io = root.join("stdlib/io.vibra").display().to_string().replace('\\', "/");
    let types = root
        .join("stdlib/types.vibra")
        .display()
        .to_string()
        .replace('\\', "/");
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &entry,
        format!(
            r#"io:
  $import: "{io}"
types:
  $import: "{types}"
main:
  $function:
    args: $void
    return: $void
    do:
      - $io.read-line: 0
"#
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = vibra::lower::lower_program(&prog).unwrap_err().to_string();
    assert!(
        err.contains("expected Named(\"types.Fd\")")
            || err.contains("expected Named(\"Fd\")")
            || err.contains("expected"),
        "unexpected error: {err}"
    );
}

#[test]
fn accepts_domain_wrappers_for_io_fs_calls() {
    let dir = tempfile::tempdir().unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let io = root.join("stdlib/io.vibra").display().to_string().replace('\\', "/");
    let fs = root.join("stdlib/fs.vibra").display().to_string().replace('\\', "/");
    let types = root
        .join("stdlib/types.vibra")
        .display()
        .to_string()
        .replace('\\', "/");
    let note_path = dir
        .path()
        .join("tmp")
        .join("wrapped.txt")
        .display()
        .to_string()
        .replace('\\', "/");
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &entry,
        format!(
            r#"io:
  $import: "{io}"
fs:
  $import: "{fs}"
types:
  $import: "{types}"
main:
  $function:
    args: $void
    return: $void
    do:
      - $let:
          stdin-fd:
            $types.Fd.FromInt: 0
      - $let:
          p:
            $types.Path.FromStr: "{note_path}"
      - $fs.create-dir-all:
          $types.Path.FromStr: "{tmp_dir}"
      - $fs.write-file:
          path: $p
          contents: "abc"
      - $let:
          text:
            $fs.read-file: $p
      - $io.write-all:
          fd: $stdin-fd
          bytes: $text
"#,
            tmp_dir = dir.path().join("tmp").display().to_string().replace('\\', "/")
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog);
    assert!(lowered.is_ok(), "expected wrapped domain values to type-check");
}

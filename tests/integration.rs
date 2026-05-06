use std::path::Path;

#[test]
fn import_cycle_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.vibra");
    let b = dir.path().join("b.vibra");
    std::fs::write(&a, "io:\n  $import: ./b.vibra\n").unwrap();
    std::fs::write(&b, "io:\n  $import: ./a.vibra\n").unwrap();
    let err = vibra::load::load_program(&a).unwrap_err();
    let s = err.to_string();
    assert!(s.contains("cycle") || s.contains("E-MOD-003"), "unexpected error: {s}");
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
fn enum_match_lowers_with_new_syntax() {
    let dir = tempfile::tempdir().unwrap();
    let model = dir.path().join("model.vibra");
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &model,
        r#"integer:
  $union: [$int64, $int32, $int16, $int8]
number:
  $enum:
    int: $integer
    none: $void
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"m:
  $import: "{m}"
io:
  $import: "{io}"
main:
  $function:
    args: $void
    return: $void
    do:
      - $let:
          value:
            $m.number.int: 7
      - $match:
          target: $value
          arms:
            int:
              bind: x
              do:
                - $io.println: "int"
            none:
              do:
                - $io.println: "none"
"#,
            m = model.display().to_string().replace('\\', "/"),
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog);
    assert!(lowered.is_ok(), "expected enum + match program to lower");
}

#[test]
fn rejects_legacy_variants_union_syntax() {
    let dir = tempfile::tempdir().unwrap();
    let bad = dir.path().join("bad.vibra");
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &bad,
        r#"maybe-text:
  $union:
    variants:
      some: $str
      none: $void
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"u:
  $import: "{u}"
main:
  $function:
    args: $void
    return: $void
    do: []
"#,
            u = bad.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = vibra::lower::lower_program(&prog).unwrap_err().to_string();
    assert!(
        err.contains("legacy `variants` union syntax was removed")
            || err.contains("invalid union declaration"),
        "unexpected error: {err}"
    );
}

#[test]
fn warns_for_non_kebab_case_symbols() {
    let dir = tempfile::tempdir().unwrap();
    let mod_file = dir.path().join("symbols.vibra");
    let entry = dir.path().join("entry.vibra");
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();

    std::fs::write(
        &mod_file,
        r#"BadType:
  $enum:
    NotTag: $str
doThing:
  $function:
    args:
      BadArg: $str
    return: $void
    do:
      - $wasm:
          import:
            module: wasi_snapshot_preview1
            name: fd_write
          args:
            - $args.BadArg
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"BadImport:
  $import: "{m}"
io:
  $import: "{io}"
main:
  $function:
    args: $void
    return: $void
    do:
      - $io.println: "ok"
"#,
            m = mod_file.display().to_string().replace('\\', "/"),
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).unwrap();
    assert!(
        lowered.warnings.iter().any(|w| w.contains("non-kebab-case")),
        "expected at least one kebab-case warning, got {:?}",
        lowered.warnings
    );
}

#[test]
fn supports_void_enum_constructor_without_payload() {
    let dir = tempfile::tempdir().unwrap();
    let model = dir.path().join("model.vibra");
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &model,
        r#"option:
  $enum:
    none: $void
    some: $str
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"m:
  $import: "{m}"
io:
  $import: "{io}"
main:
  $function:
    args: $void
    return: $void
    do:
      - $let:
          value-none: $m.option.none
      - $match:
          target: $value-none
          arms:
            none:
              do:
                - $io.println: "none"
            some:
              bind: text
              do:
                - $io.println: $text
"#,
            m = model.display().to_string().replace('\\', "/"),
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog);
    assert!(
        lowered.is_ok(),
        "expected void enum constructor without payload to lower"
    );
}

#[test]
fn rejects_removed_int_float_aliases() {
    let dir = tempfile::tempdir().unwrap();
    let bad = dir.path().join("bad.vibra");
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &bad,
        r#"takes-old-int:
  $function:
    args:
      x: $int
    return: $void
    do:
      - $wasm:
          import:
            module: wasi_snapshot_preview1
            name: fd_sync
          args:
            - $const.1
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"u:
  $import: "{u}"
main:
  $function:
    args: $void
    return: $void
    do: []
"#,
            u = bad.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = vibra::lower::lower_program(&prog).unwrap_err();
    let err_msg = format!("{err:?}");
    assert!(
        err_msg.contains("type alias `$int` was removed"),
        "unexpected error: {err_msg}"
    );
}

#[test]
fn numeric_literals_are_compatible_with_explicit_numeric_types() {
    let dir = tempfile::tempdir().unwrap();
    let mod_file = dir.path().join("numeric.vibra");
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &mod_file,
        r#"accepts-int32:
  $function:
    args:
      x: $int32
    return: $void
    do:
      - $wasm:
          import:
            module: wasi_snapshot_preview1
            name: fd_sync
          args:
            - $const.1
accepts-float32:
  $function:
    args:
      x: $float32
    return: $void
    do:
      - $wasm:
          import:
            module: wasi_snapshot_preview1
            name: fd_sync
          args:
            - $const.1
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"n:
  $import: "{n}"
main:
  $function:
    args: $void
    return: $void
    do:
      - $n.accepts-int32: 7
      - $n.accepts-float32: 3.14
"#,
            n = mod_file.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog);
    assert!(
        lowered.is_ok(),
        "expected numeric literals to be compatible with explicit numeric primitive types"
    );
}

#[test]
fn option_forall_union_allows_t_or_void_and_disallows_reverse_coercion() {
    let dir = tempfile::tempdir().unwrap();
    let model = dir.path().join("model.vibra");
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &model,
        r#"option:
  $forall:
    types: [t]
    in:
      $union: [$void, $t]
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"m:
  $import: "{m}"
io:
  $import: "{io}"
use-option:
  $function:
    args:
      value: $m.option
    return: $void
    do:
      - $io.println: "using option"
expect-int:
  $function:
    args:
      value: $int64
    return: $void
    do:
      - $io.write-raw:
          fd: $args.value
          bytes: "x"
main:
  $function:
    args: $void
    return: $void
    do:
      - $use-option: 7
      - $use-option: null
      - $expect-int: null
"#,
            m = model.display().to_string().replace('\\', "/"),
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog);
    assert!(
        lowered.is_err(),
        "expected reverse coercion from option to int to fail and require a match/narrowing"
    );
}

#[test]
fn result_forall_ok_and_err_type_params() {
    let dir = tempfile::tempdir().unwrap();
    let model = dir.path().join("model.vibra");
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &model,
        r#"result:
  $forall:
    types: [t, e]
    in:
      $enum:
        ok: $t
        err: $e
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"m:
  $import: "{m}"
io:
  $import: "{io}"
main:
  $function:
    args: $void
    return: $void
    do:
      - $let:
          r-ok:
            $m.result.ok: 99
      - $match:
          target: $r-ok
          arms:
            ok:
              bind: x
              do:
                - $io.write-raw:
                    fd: $x
                    bytes: "ok"
            err:
              bind: y
              do:
                - $io.println: $y
      - $let:
          r-err:
            $m.result.err: "fail"
      - $match:
          target: $r-err
          arms:
            ok:
              bind: x2
              do:
                - $io.write-raw:
                    fd: $x2
                    bytes: "no"
            err:
              bind: y2
              do:
                - $io.println: $y2
"#,
            m = model.display().to_string().replace('\\', "/"),
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog);
    assert!(
        lowered.is_ok(),
        "expected result ok/err arms to get correct payload types, got {:?}",
        lowered.as_ref().err()
    );
}

#[test]
fn forall_only_generic_names_no_unscoped_uppercase_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let bad = dir.path().join("bad.vibra");
    let good = dir.path().join("good.vibra");
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let entry_bad = dir.path().join("entry_bad.vibra");
    let entry_good = dir.path().join("entry_good.vibra");

    std::fs::write(
        &bad,
        r#"opt:
  $enum:
    some: $T
    none: $void
"#,
    )
    .unwrap();
    std::fs::write(
        &good,
        r#"opt:
  $forall:
    types: [t]
    in:
      $enum:
        some: $t
        none: $void
"#,
    )
    .unwrap();

    std::fs::write(
        &entry_bad,
        format!(
            r#"m:
  $import: "{m}"
io:
  $import: "{io}"
main:
  $function:
    args: $void
    return: $void
    do:
      - $let:
          v:
            $m.opt.some: 7
      - $io.println: "bad"
"#,
            m = bad.display().to_string().replace('\\', "/"),
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    std::fs::write(
        &entry_good,
        format!(
            r#"m:
  $import: "{m}"
io:
  $import: "{io}"
main:
  $function:
    args: $void
    return: $void
    do:
      - $let:
          v:
            $m.opt.some: 7
      - $io.println: "good"
"#,
            m = good.display().to_string().replace('\\', "/"),
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog_bad = vibra::load::load_program(&entry_bad).unwrap();
    assert!(
        vibra::lower::lower_program(&prog_bad).is_err(),
        "unscoped $T should be a named type, not a generic; int payload must not unify"
    );

    let prog_good = vibra::load::load_program(&entry_good).unwrap();
    assert!(
        vibra::lower::lower_program(&prog_good).is_ok(),
        "scoped $forall type param should allow int payload on some"
    );
}

#[test]
fn zero_arg_call_accepts_null_payload() {
    let dir = tempfile::tempdir().unwrap();
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &entry,
        format!(
            r#"io:
  $import: "{io}"
main:
  $function:
    args: $void
    return: $void
    do:
      - $io.flush-stdout: null
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog);
    assert!(
        lowered.is_ok(),
        "expected zero-arg function call with null payload to lower"
    );
}

#[test]
fn zero_arg_call_rejects_void_payload_literal() {
    let dir = tempfile::tempdir().unwrap();
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &entry,
        format!(
            r#"io:
  $import: "{io}"
main:
  $function:
    args: $void
    return: $void
    do:
      - $io.flush-stdout: $void
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = vibra::lower::lower_program(&prog).unwrap_err().to_string();
    assert!(
        err.contains("zero-arg call payload must be `null`"),
        "unexpected error: {err}"
    );
}

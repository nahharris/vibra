use std::path::Path;

#[test]
fn match_arms_use_case_key() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"main:
  $function: $void
  return: $void
  do:
    - $match: 0
      when:
        - case: 0
          do: []
        - case: {$wildcard: null}
          do: []
"#,
    )
    .unwrap();

    let loaded = vibra::load::load_program(&entry).unwrap();
    vibra::lower::lower_program(&loaded).expect("`case` should be the canonical match arm key");
}

#[test]
fn legacy_pattern_match_arm_key_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"main:
  $function: $void
  return: $void
  do:
    - $match: 0
      when:
        - pattern: 0
          do: []
"#,
    )
    .unwrap();

    let loaded = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&loaded).unwrap_err());
    assert!(err.contains("E-ONE-008"), "unexpected error: {err}");
    assert!(err.contains("case"), "expected migration hint: {err}");
}

#[test]
fn generic_alias_instantiation_prefers_current_module_scope() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let io = std::fs::canonicalize(root.join("stdlib/io.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"io:
  $import: "{}"
main:
  $function: $void
  return: $void
  do: []
"#,
            io.display().to_string().replace('\\', "/")
        ),
    )
    .unwrap();

    let loaded = vibra::load::load_program(&entry).unwrap();
    vibra::lower::lower_program(&loaded)
        .expect("nested `$result.result` must resolve in the current module scope");
}

#[test]
fn mut_ref_and_set_forms_lower() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"main:
  $function: $void
  return: $void
  do:
    - $let:
        count: {$mut: 0}
    - $let:
        reader: {$ref: $count}
    - $let:
        writer: {$ref: {$mut: $count}}
    - $set:
        count: 1
    - $set:
        writer: 2
"#,
    )
    .unwrap();

    let loaded = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&loaded).expect("mut/ref program should lower");
    let dump = format!("{lowered:?}");
    assert!(
        dump.contains("Set"),
        "expected set statements in IR: {dump}"
    );
    assert!(
        dump.contains("Mutable"),
        "expected mutable type metadata: {dump}"
    );
    assert!(
        dump.contains("Reference"),
        "expected reference expressions: {dump}"
    );
}

#[test]
fn set_rejects_immutable_binding() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"main:
  $function: $void
  return: $void
  do:
    - $let:
        count: 0
    - $set:
        count: 1
"#,
    )
    .unwrap();

    let loaded = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&loaded).unwrap_err());
    assert!(err.contains("E-SET-002"), "unexpected error: {err}");
}

#[test]
fn set_rejects_read_only_reference() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"main:
  $function: $void
  return: $void
  do:
    - $let:
        count: {$mut: 0}
    - $let:
        reader: {$ref: $count}
    - $set:
        reader: 1
"#,
    )
    .unwrap();

    let loaded = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&loaded).unwrap_err());
    assert!(err.contains("E-SET-002"), "unexpected error: {err}");
}

#[test]
fn mutable_and_reference_type_wrappers_parse() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"take-mut:
  $function:
    value: {$mut: $int64}
  return: $int64
  do:
    - $return: $args.value
take-ref:
  $function:
    value: {$ref: {$mut: $int64}}
  return: $int64
  do:
    - $return: $args.value
main:
  $function: $void
  return: $void
  do: []
"#,
    )
    .unwrap();

    let loaded = vibra::load::load_program(&entry).unwrap();
    vibra::lower::lower_program(&loaded).expect("mut/ref type wrappers should parse");
}

#[test]
fn wasm_abi_uses_i32_addresses_for_mutable_values_and_refs() {
    use vibra::wasm_abi::{layout_of, AbiType, StorageClass};

    let scalar = layout_of(&AbiType::I64);
    assert_eq!(scalar.size, 8);
    assert_eq!(scalar.align, 8);
    assert_eq!(scalar.storage, StorageClass::Direct);

    let mutable = layout_of(&AbiType::Mutable(Box::new(AbiType::I64)));
    assert_eq!(mutable.size, 4);
    assert_eq!(mutable.align, 4);
    assert_eq!(mutable.storage, StorageClass::ArenaAddress);

    let reference = layout_of(&AbiType::Reference(Box::new(AbiType::I64)));
    assert_eq!(reference.size, 4);
    assert_eq!(reference.storage, StorageClass::ArenaAddress);
}

#[test]
fn wasm_abi_aggregate_layout_is_aligned() {
    use vibra::wasm_abi::{layout_of, AbiType, StorageClass};

    let record = layout_of(&AbiType::Record(vec![AbiType::I32, AbiType::I64]));
    assert_eq!(record.size, 16);
    assert_eq!(record.align, 8);
    assert_eq!(record.field_offsets, vec![0, 8]);
    assert_eq!(record.storage, StorageClass::CopiedPointer);
}

#[test]
#[ignore = "removed by policy value model"]
fn function_grants_side_channel_allows_primary_args_and_grant_forwarding() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fs = std::fs::canonicalize(root.join("stdlib/fs.vibra")).unwrap();
    let security = std::fs::canonicalize(root.join("stdlib/security.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data.txt");
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"fs:
  $import: "{fs}"
security:
  $import: "{security}"
main:
  $function: $void
  grants:
    fs-write: $security.grant.mandatory
    fs-read: $security.grant.optional
  return: $void
  do:
    - $let:
        path:
          $fs.path.new: "{path}"
    - $fs.write-string-all: $path
      s: "hello"
      =grants:
        - $grants.fs-write
    - $if:
        $security.granted: $grants.fs-read
      then:
        - $let:
            text:
              $fs.read-to-string: $path
              =grants:
                - $grants.fs-read
      else: []
"#,
            fs = fs.display().to_string().replace('\\', "/"),
            security = security.display().to_string().replace('\\', "/"),
            path = data.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).expect("new grant side channel should lower");
    vibra::execute::run_lowered(
        &lowered,
        &vibra::runtime::RunConfig {
            program_name: "vibra-test".to_string(),
            argv: Vec::new(),
            allow_write: vec![dir.path().to_path_buf()],
            allow_read: vec![dir.path().to_path_buf()],
            ..vibra::runtime::RunConfig::default()
        },
    )
    .expect("grant side-channel fs program should run");
    assert_eq!(std::fs::read_to_string(data).unwrap(), "hello");
}

#[test]
#[ignore = "removed by policy value model"]
fn missing_mandatory_grant_forwarding_is_rejected() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fs = std::fs::canonicalize(root.join("stdlib/fs.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"fs:
  $import: "{fs}"
main:
  $function: $void
  return: $void
  do:
    - $let:
        path:
          $fs.path.new: "x"
    - $fs.read-to-string: $path
"#,
            fs = fs.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("missing mandatory grant `fs-read`"),
        "expected missing mandatory grant diagnostic, got: {err}"
    );
}

#[test]
#[ignore = "removed by policy value model"]
fn mandatory_grant_forwarded_but_denied_fails_before_callee_runs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fs = std::fs::canonicalize(root.join("stdlib/fs.vibra")).unwrap();
    let security = std::fs::canonicalize(root.join("stdlib/security.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data.txt");
    std::fs::write(&data, "secret").unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"fs:
  $import: "{fs}"
security:
  $import: "{security}"
main:
  $function: $void
  grants:
    fs-read: $security.grant.mandatory
  return: $void
  do:
    - $let:
        path:
          $fs.path.new: "{path}"
    - $let:
        text:
          $fs.read-to-string: $path
          =grants:
            - $grants.fs-read
"#,
            fs = fs.display().to_string().replace('\\', "/"),
            security = security.display().to_string().replace('\\', "/"),
            path = data.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).unwrap();
    let err = format!(
        "{:#}",
        vibra::execute::run_lowered(
            &lowered,
            &vibra::runtime::RunConfig {
                program_name: "vibra-test".to_string(),
                argv: Vec::new(),
                ..vibra::runtime::RunConfig::default()
            },
        )
        .unwrap_err()
    );
    assert!(
        err.contains("mandatory grant `fs-read` was not granted"),
        "expected denied mandatory grant preflight failure, got: {err}"
    );
}

#[test]
#[ignore = "removed by policy value model"]
fn grant_forwarding_requires_token_in_scope() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fs = std::fs::canonicalize(root.join("stdlib/fs.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data.txt");
    std::fs::write(&data, "secret").unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"fs:
  $import: "{fs}"
main:
  $function: $void
  return: $void
  do:
    - $let:
        path:
          $fs.path.new: "{path}"
    - $let:
        text:
          $fs.read-to-string: $path
          =grants:
            - $grants.fs-read
"#,
            fs = fs.display().to_string().replace('\\', "/"),
            path = data.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).unwrap();
    let err = format!(
        "{:#}",
        vibra::execute::run_lowered(
            &lowered,
            &vibra::runtime::RunConfig {
                program_name: "vibra-test".to_string(),
                argv: Vec::new(),
                allow_read: vec![dir.path().to_path_buf()],
                ..vibra::runtime::RunConfig::default()
            },
        )
        .unwrap_err()
    );
    assert!(
        err.contains("grant `fs-read` is not available in this scope"),
        "expected unavailable grant token rejection, got: {err}"
    );
}

#[test]
fn nested_function_grants_are_rejected() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let security = std::fs::canonicalize(root.join("stdlib/security.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"security:
  $import: "{security}"
main:
  $function:
    args: $void
    grants:
      fs-read: $security.grant.mandatory
  return: $void
  do: []
"#,
            security = security.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("E-ONE-001"),
        "expected non-canonical nested function rejection, got: {err}"
    );
}

#[test]
fn implicit_subject_function_is_rejected_with_e_one_001() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        "identity:\n  $function: $str\n  return: $str\n  do:\n    - $return: $args.subject\nmain:\n  $function: $void\n  return: $void\n  do: []\n",
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(err.contains("E-ONE-001"), "unexpected error: {err}");
}

#[test]
fn void_function_with_sibling_args_is_rejected_with_e_one_001() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        "identity:\n  $function: $void\n  args:\n    value: $str\n  return: $str\n  do:\n    - $return: $args.value\nmain:\n  $function: $void\n  return: $void\n  do: []\n",
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(err.contains("E-ONE-001"), "unexpected error: {err}");
}

#[test]
fn labeled_primary_is_only_available_through_args_namespace() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        "identity:\n  $function:\n    value: $str\n  return: $str\n  do:\n    - $return: $value\nmain:\n  $function: $void\n  return: $void\n  do: []\n",
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("could not infer type for `$return` expression"),
        "unexpected error: {err}"
    );
}

#[test]
fn grant_names_must_be_kebab_case() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let security = std::fs::canonicalize(root.join("stdlib/security.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"security:
  $import: "{security}"
main:
  $function: $void
  grants:
    fs_read: $security.grant.optional
  return: $void
  do: []
"#,
            security = security.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("grant names must be kebab-case"),
        "expected grant declaration kebab-case rejection, got: {err}"
    );
}

#[test]
#[ignore = "removed by policy value model"]
fn grant_forwarding_refs_must_be_kebab_case() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fs = std::fs::canonicalize(root.join("stdlib/fs.vibra")).unwrap();
    let security = std::fs::canonicalize(root.join("stdlib/security.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"fs:
  $import: "{fs}"
security:
  $import: "{security}"
main:
  $function: $void
  grants:
    fs-read: $security.grant.optional
  return: $void
  do:
    - $let:
        path:
          $fs.path.new: "x"
    - $fs.read-to-string: $path
      =grants:
        - $grants.fs_read
"#,
            fs = fs.display().to_string().replace('\\', "/"),
            security = security.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("grant references must use `$grants.<kebab-name>`"),
        "expected grant forwarding kebab-case rejection, got: {err}"
    );
}

#[test]
#[ignore = "removed by policy value model"]
fn dotted_grant_reference_is_rejected() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let security = std::fs::canonicalize(root.join("stdlib/security.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"security:
  $import: "{security}"
main:
  $function: $void
  grants:
    fs-read: $security.grant.optional
  return: $void
  do:
    - $let:
        ok:
          $security.granted: $grants.fs.read
"#,
            security = security.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("grant references must use `$grants.<kebab-name>`"),
        "expected dotted grant reference rejection, got: {err}"
    );
}

#[test]
fn import_cycle_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.vibra");
    let b = dir.path().join("b.vibra");
    std::fs::write(&a, "io:\n  $import: ./b.vibra\n").unwrap();
    std::fs::write(&b, "io:\n  $import: ./a.vibra\n").unwrap();
    let err = vibra::load::load_program(&a).unwrap_err();
    let s = err.to_string();
    assert!(
        s.contains("cycle") || s.contains("E-MOD-003"),
        "unexpected error: {s}"
    );
}

#[test]
fn private_module_symbol_is_reachable_locally() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"-main-helper:
  $function: $void
  return: $void
  do:
    - $return: null
main:
  $function: $void
  return: $void
  do:
    - $-main-helper: null
"#,
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog);
    assert!(
        lowered.is_ok(),
        "expected private helper to lower: {:?}",
        lowered.err()
    );
}

#[test]
fn private_import_alias_is_usable_locally() {
    let dir = tempfile::tempdir().unwrap();
    let helper = dir.path().join("helper.vibra");
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &helper,
        r#"noop:
  $function: $void
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
            r#"-h:
  $import: "{h}"
main:
  $function: $void
  return: $void
  do:
    - $-h.noop: null
"#,
            h = helper.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    vibra::lower::lower_program(&prog).unwrap();
}

#[test]
fn imported_module_private_helper_works_internally() {
    let dir = tempfile::tempdir().unwrap();
    let lib = dir.path().join("lib.vibra");
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &lib,
        r#"-priv:
  $function: $void
  return: $void
  do:
    - $return: null
pub-entry:
  $function: $void
  return: $void
  do:
    - $-priv: null
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"m:
  $import: "{m}"
main:
  $function: $void
  return: $void
  do:
    - $m.pub-entry: null
"#,
            m = lib.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    vibra::lower::lower_program(&prog).unwrap();
}

#[test]
fn importer_cannot_reference_private_symbol_on_imported_module() {
    let dir = tempfile::tempdir().unwrap();
    let lib = dir.path().join("lib.vibra");
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &lib,
        r#"-priv:
  $function: $void
  return: $void
  do:
    - $return: null
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"m:
  $import: "{m}"
main:
  $function: $void
  return: $void
  do:
    - $m.-priv: null
"#,
            m = lib.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("unknown function") && err.contains("$m.-priv"),
        "unexpected error: {err}"
    );
}

#[test]
fn importer_cannot_reference_private_type_on_imported_module() {
    let dir = tempfile::tempdir().unwrap();
    let lib = dir.path().join("lib.vibra");
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &lib,
        r#"-priv-t:
  $record:
    x: $int32
pub-nop:
  $function: $void
  return: $void
  do:
    - $return: null
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"m:
  $import: "{m}"
use-ty:
  $function:
    subject: $m.-priv-t
  return: $void
  do:
    - $return: null
main:
  $function: $void
  return: $void
  do:
    - $m.pub-nop: null
"#,
            m = lib.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("unknown type") && err.contains("m.-priv-t"),
        "unexpected error: {err}"
    );
}

#[test]
fn importer_cannot_use_private_enum_constructor_on_imported_module() {
    let dir = tempfile::tempdir().unwrap();
    let lib = dir.path().join("lib.vibra");
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &lib,
        r#"-priv-e:
  $enum:
    a: $void
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"m:
  $import: "{m}"
main:
  $function: $void
  return: $void
  do:
    - $let:
        value:
          $m.-priv-e.a: null
"#,
            m = lib.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("unknown enum reference") && err.contains("m.-priv-e"),
        "unexpected error: {err}"
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
  $function: $void
  return: $void
  do:
      - $let:
          value:
            $m.number.int: 7
      - $match: $value
        when:
            - case:
                $m.number.int:
                  $bind: x
              do:
                - $io.println: "int"
            - case:
                $m.number.none: null
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
fn legacy_mapping_match_arms_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &entry,
        r#"maybe:
  $enum:
    some: $str
    none: $void
main:
  $function: $void
  return: $void
  do:
      - $let:
          value:
            $maybe.some: "x"
      - $match: $value
        when:
            some:
              bind: x
              do: []
            none:
              do: []
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("$match `when` must be a sequence"),
        "expected legacy mapping `when` to be rejected, got: {err}"
    );
}

#[test]
fn structured_match_form_is_rejected_with_e_one_007() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &entry,
        r#"maybe:
  $enum:
    some: $str
    none: $void
main:
  $function: $void
  return: $void
  do:
      - $let:
          value:
            $maybe.some: "x"
      - $match:
          target: $value
          arms:
            - case:
                $maybe.some:
                  $bind: x
              do: []
            - case:
                $maybe.none: null
              do: []
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("E-ONE-007"),
        "expected structured `$match` to be rejected with E-ONE-007, got: {err}"
    );
}

#[test]
fn match_arm_rebinding_does_not_leak_to_parent_runtime_scope() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();

    std::fs::write(
        &entry,
        format!(
            r#"io:
  $import: "{io}"
maybe:
  $enum:
    some: $str
    none: $void
main:
  $function: $void
  return: $void
  do:
      - $let:
          x: "outer"
      - $let:
          value:
            $maybe.some: "payload"
      - $match: $value
        when:
            - case:
                $maybe.some:
                  $bind: payload
              do:
                - $let:
                    x: 42
            - case:
                $maybe.none: null
              do: []
      - $io.println: $x
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).unwrap();
    vibra::execute::run_lowered(&lowered, &vibra::runtime::RunConfig::default())
        .expect("outer x should remain a string after the match arm");
}

#[test]
fn if_branch_let_does_not_leak_into_other_branch_or_after() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();

    std::fs::write(
        &entry,
        format!(
            r#"io:
  $import: "{io}"
main:
  $function: $void
  return: $void
  do:
      - $if: true
        then:
          - $let:
              x: 42
        else:
          - $io.println: $x
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("could not infer type"),
        "expected lowering to reject `$x` in else when only bound in then, got: {err}"
    );
}

#[test]
fn if_merges_locals_when_both_branches_bind_same_name_with_same_type() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();

    std::fs::write(
        &entry,
        format!(
            r#"io:
  $import: "{io}"
main:
  $function: $void
  return: $void
  do:
      - $if: true
        then:
          - $let:
              x: "then"
        else:
          - $let:
              x: "else"
      - $io.println: $x
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).expect("both branches bind x: int");
    vibra::execute::run_lowered(&lowered, &vibra::runtime::RunConfig::default()).unwrap();
}

#[test]
fn while_body_let_does_not_leak_after_loop() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();

    std::fs::write(
        &entry,
        format!(
            r#"io:
  $import: "{io}"
main:
  $function: $void
  return: $void
  do:
      - $while: false
        do:
          - $let:
              x: 42
      - $io.println: $x
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("could not infer type"),
        "expected lowering to reject `$x` after `$while` when only bound in body, got: {err}"
    );
}

#[test]
fn record_tuple_array_and_map_patterns_bind_values() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();

    std::fs::write(
        &entry,
        format!(
            r#"io:
  $import: "{io}"
main:
  $function: $void
  return: $void
  do:
      - $let:
          value:
            $record:
              pair:
                $tuple: [7, "seven"]
              tags:
                $array: ["a", "b"]
              table:
                $map:
                  - key: "lang"
                    value: "vibra"
      - $match: $value
        when:
            - case:
                $record:
                  pair:
                    $tuple:
                      - {{ $bind: n }}
                      - {{ $bind: word }}
                  tags:
                    $array:
                      - "a"
                      - {{ $wildcard: null }}
                  table:
                    $map:
                      - key: "lang"
                        value: {{ $bind: language }}
              do:
                - $io.println: $word
                - $io.println: $language
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).unwrap();
    vibra::execute::run_lowered(&lowered, &vibra::runtime::RunConfig::default())
        .expect("composite pattern should bind nested values");
}

#[test]
fn newtype_and_nominal_interface_patterns_match_runtime_type_tags() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &entry,
        r#"display:
  $interface:
    fmt:
      $fn-type:
        args:
          $record:
            x: $self
        return: $str
meter:
  $newtype: $int64
  =impl:
    $display:
      fmt:
        $function: $self
        return: $str
        do:
            - $return: "meter"
main:
  $function: $void
  return: $void
  do:
      - $let:
          distance:
            $cast: 7
            into: $meter
      - $match: $distance
        when:
            - case:
                $interface: $display
              do:
                - $let:
                    matched: "display"
            - case:
                $wildcard: null
              do:
                - $let:
                    matched: "other"
      - $match: $distance
        when:
            - case:
                $newtype:
                  type: $meter
                  inner:
                    $bind: raw
              do:
                - $let:
                    seen: $raw
            - case:
                $wildcard: null
              do: []
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).unwrap();
    vibra::execute::run_lowered(&lowered, &vibra::runtime::RunConfig::default())
        .expect("newtype/interface patterns should use runtime type tags");
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
  $function: $void
  return: $void
  do:
      - $wasm:
          import:
            module: wasi_snapshot_preview1
            name: fd_sync
          args:
            - $const.1
"#,
            u = bad.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("legacy `variants` union syntax was removed"),
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
  $function: $void
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
        lowered
            .warnings
            .iter()
            .any(|w| w.contains("non-kebab-case")),
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
  $function: $void
  return: $void
  do:
      - $let:
          value-none: $m.option.none
      - $match: $value-none
        when:
            - case:
                $m.option.none: null
              do:
                - $io.println: "none"
            - case:
                $m.option.some:
                  $bind: text
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
        "expected void enum constructor without payload to lower: {:?}",
        lowered.err()
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
    input: $int
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
  $function: $void
  return: $void
  do:
      - $wasm:
          import:
            module: wasi_snapshot_preview1
            name: fd_sync
          args:
            - $const.1
"#,
            u = bad.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = vibra::lower::lower_program(&prog).unwrap_err();
    let err_msg = format!("{err:#}");
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
    input: $int32
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
    input: $float32
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
  $function: $void
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
fn newtype_decl_lowers_and_requires_explicit_cast() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"meter:
  $newtype: $int64
take-meter:
  $function:
    input: $meter
  return: $void
  do:
      - $wasm:
          import:
            module: wasi_snapshot_preview1
            name: fd_sync
          args:
            - $const.1
main:
  $function: $void
  return: $void
  do:
      - $let:
          v:
            $cast: 7
            into: $meter
      - $take-meter: $v
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered =
        vibra::lower::lower_program(&prog).expect("$newtype plus explicit $cast should lower");
    let sig = lowered
        .functions
        .get("take-meter")
        .expect("take-meter registered");
    assert_eq!(
        sig.arg_types[0],
        vibra::lower::TypeRef::Named("meter".to_string())
    );
}

#[test]
fn cast_rejects_legacy_nested_payload_shape() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"meter:
  $newtype: $int64
main:
  $function: $void
  return: $void
  do:
      - $let:
          v:
            $cast:
              from: 7
              to: $meter
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("E-CAST-002"),
        "expected nested `$cast` payload to be rejected, got: {err}"
    );
}

#[test]
fn cast_rejects_identity_conversion() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"main:
  $function: $void
  return: $void
  do:
      - $let:
          v:
            $cast: 7
            into: $int64
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("E-CAST-001"),
        "expected identity `$cast` to be rejected, got: {err}"
    );
}

#[test]
fn newtype_does_not_accept_inner_type_implicitly() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"meter:
  $newtype: $int64
take-meter:
  $function:
    input: $meter
  return: $void
  do:
      - $wasm:
          import:
            module: wasi_snapshot_preview1
            name: fd_sync
          args:
            - $const.1
main:
  $function: $void
  return: $void
  do:
      - $take-meter: 7
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("E-NEWTYPE-001"),
        "expected implicit inner -> newtype coercion to be rejected, got: {err}"
    );
}

#[test]
fn cast_rejects_cross_newtype_conversion() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"meter:
  $newtype: $int64
second:
  $newtype: $int64
take-second:
  $function:
    input: $second
  return: $void
  do:
      - $wasm:
          import:
            module: wasi_snapshot_preview1
            name: fd_sync
          args:
            - $const.1
main:
  $function: $void
  return: $void
  do:
      - $let:
          m:
            $cast: 7
            into: $meter
      - $let:
          s:
            $cast: $m
            into: $second
      - $take-second: $s
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("E-CAST-001"),
        "expected cross-newtype cast rejection, got: {err}"
    );
}

#[test]
fn fs_writable_interface_rejects_read_file() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fs = std::fs::canonicalize(root.join("stdlib/fs.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"fs:
  $import: "{fs}"
main:
  $function: $void
  return: $void
  do:
      - $let:
          f:
            $cast: 0
            into: $fs.read-file
      - $fs.writable.write-string: $f
        s: "nope"
"#,
            fs = fs.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("E-BOUND-001"),
        "expected writable dispatch on read-file to be rejected, got: {err}"
    );
}

#[test]
#[ignore = "old grant-status API removed by grant side-channel model"]
fn mode_safe_fs_roundtrip_runs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fs = std::fs::canonicalize(root.join("stdlib/fs.vibra")).unwrap();
    let security = std::fs::canonicalize(root.join("stdlib/security.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    let data = dir.path().join("hello.txt");
    std::fs::write(
        &entry,
        format!(
            r#"fs:
  $import: "{fs}"
security:
  $import: "{security}"
main:
  $function:
    input: $security.grants
  return: $void
  do:
      - $let:
          p:
            $fs.path.new: "{path}"
      - $match: $args.input.fs-write
        when:
            - case:
                $security.grant-status.granted:
                  $bind: write-grant
              do:
                - $let:
                    opened-write:
                      $fs.open-write: $p
                      grant: $write-grant
                - $match: $opened-write
                  when:
                      - case:
                          $result.result.ok:
                            $bind: out
                        do:
                          - $fs.writable.write-string: $out
                            s: "from vibra fs"
                          - $fs.closeable.close: $out
                      - case:
                          $result.result.err:
                            $bind: err
                        do: []
            - case:
                $security.grant-status.denied:
                  $bind: write-denied
              do: []
      - $match: $args.input.fs-read
        when:
            - case:
                $security.grant-status.granted:
                  $bind: read-grant
              do:
                - $let:
                    opened-read:
                      $fs.open-read: $p
                      grant: $read-grant
                - $match: $opened-read
                  when:
                      - case:
                          $result.result.ok:
                            $bind: input
                        do:
                          - $let:
                              text:
                                $fs.readable.read-string: $input
                          - $fs.closeable.close: $input
                      - case:
                          $result.result.err:
                            $bind: err2
                        do: []
            - case:
                $security.grant-status.denied:
                  $bind: read-denied
              do: []
"#,
            fs = fs.display().to_string().replace('\\', "/"),
            security = security.display().to_string().replace('\\', "/"),
            path = data.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).expect("mode-safe fs program should lower");
    vibra::execute::run_lowered(
        &lowered,
        &vibra::runtime::RunConfig {
            program_name: "vibra-test".to_string(),
            argv: Vec::new(),
            preopen_host_dirs: vec![dir.path().to_path_buf()],
            ..vibra::runtime::RunConfig::default()
        },
    )
    .expect("mode-safe fs roundtrip should run");
    assert_eq!(std::fs::read_to_string(data).unwrap(), "from vibra fs");
}

#[test]
#[ignore = "removed by policy value model"]
fn capability_values_cannot_be_created_with_cast() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"secret:
  $capability: fs-read
main:
  $function: $void
  return: $void
  do:
      - $let:
          forged:
            $cast: "not a grant"
            into: $secret
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("E-CAP-001"),
        "expected capability cast rejection, got: {err}"
    );
}

#[test]
fn capability_type_constructor_is_removed() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"secret:
  $capability: fs-read
main:
  $function: $void
  return: $void
  do: []
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("unknown form `$capability`"),
        "expected removed capability diagnostic, got: {err}"
    );
}

#[test]
fn policy_type_alias_lowers_and_can_be_used_in_signature() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"read-policy:
  $policy:
    fs-read:
      - requirement: mandatory
        scopes:
          - dir: .
uses-policy:
  $function:
    policy: $read-policy
  return: $void
  do:
    - $let:
        ok: true
main:
  $function:
    policy: $read-policy
  return: $void
  do:
    - $uses-policy: $args.policy
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    vibra::lower::lower_program(&prog).expect("policy type alias should lower");
}

#[test]
fn policy_narrow_rejects_widening() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"narrow-policy:
  $policy:
    fs-read:
      - requirement: mandatory
        scopes:
          - file: ./config/app.yaml
wide-policy:
  $policy:
    fs-read:
      - requirement: mandatory
        scopes:
          - dir: .
main:
  $function:
    policy: $narrow-policy
  return: $void
  do:
    - $let:
        widened:
          $policy.narrow: $args.policy
          into: $wide-policy
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("policy narrowing cannot widen authority"),
        "expected widening rejection, got: {err}"
    );
}

#[test]
fn policy_narrow_rejects_sibling_directory_prefix_escape() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    let sibling = dir.path().join("root2");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&sibling).unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"root-policy:
  $policy:
    fs-read:
      - requirement: mandatory
        scopes:
          - dir: "{root}"
sibling-policy:
  $policy:
    fs-read:
      - requirement: mandatory
        scopes:
          - file: "{sibling}/file.txt"
main:
  $function:
    policy: $root-policy
  return: $void
  do:
    - $let:
        widened:
          $policy.narrow: $args.policy
          into: $sibling-policy
"#,
            root = root.display().to_string().replace('\\', "/"),
            sibling = sibling.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("policy narrowing cannot widen authority"),
        "expected sibling escape rejection, got: {err}"
    );
}

#[test]
fn policy_narrow_named_alias_executes_with_concrete_policy_value() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"root-policy:
  $policy:
    fs-read:
      - requirement: mandatory
        scopes:
          - dir: .
read-policy:
  $policy:
    fs-read:
      - requirement: mandatory
        scopes:
          - file: ./config.yaml
main:
  $function:
    policy: $root-policy
  return: $void
  do:
    - $let:
        narrowed:
          $policy.narrow: $args.policy
          into: $read-policy
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).unwrap();
    vibra::execute::run_lowered(
        &lowered,
        &vibra::runtime::RunConfig {
            approved_policy: Some(vibra::lower::PolicyType {
                domains: std::collections::BTreeMap::from([(
                    "fs-read".to_string(),
                    vec![vibra::lower::PolicyGroup {
                        requirement: vibra::lower::GrantRequirement::Mandatory,
                        scopes: vec![vibra::lower::PolicyScope::Dir(".".to_string())],
                    }],
                )]),
            }),
            ..vibra::runtime::RunConfig::default()
        },
    )
    .expect("named policy aliases should narrow at runtime");
}

#[test]
fn main_injection_uses_declared_policy_not_broader_approval() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fs = std::fs::canonicalize(root.join("stdlib/fs.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let allowed = dir.path().join("allowed");
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(&allowed).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    let secret = outside.join("secret.txt");
    std::fs::write(&secret, "secret").unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"fs:
  $import: "{fs}"
main:
  $function:
    policy:
      $policy:
        fs-read:
          - requirement: mandatory
            scopes:
              - dir: "{allowed}"
  return: $void
  do:
    - $let:
        path:
          $fs.path.new: "{secret}"
    - $let:
        text:
          $fs.read-to-string: $path
          policy: $args.policy
"#,
            fs = fs.display().to_string().replace('\\', "/"),
            allowed = allowed.display().to_string().replace('\\', "/"),
            secret = secret.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).unwrap();
    let err = format!(
        "{:#}",
        vibra::execute::run_lowered(
            &lowered,
            &vibra::runtime::RunConfig {
                approved_policy: Some(vibra::lower::PolicyType {
                    domains: std::collections::BTreeMap::from([(
                        "fs-read".to_string(),
                        vec![vibra::lower::PolicyGroup {
                            requirement: vibra::lower::GrantRequirement::Mandatory,
                            scopes: vec![vibra::lower::PolicyScope::Dir(
                                dir.path().display().to_string().replace('\\', "/"),
                            )],
                        }],
                    )]),
                }),
                ..vibra::runtime::RunConfig::default()
            },
        )
        .unwrap_err()
    );
    assert!(
        err.contains("outside approved policy"),
        "expected injected policy to be narrowed to main declaration, got: {err}"
    );
}

#[test]
fn legacy_function_grants_are_rejected_after_policy_redesign() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let security = std::fs::canonicalize(root.join("stdlib/security.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"security:
  $import: "{security}"
main:
  $function: $void
  grants:
    fs-read: $security.grant.optional
  return: $void
  do: []
"#,
            security = security.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("function `grants` were removed; use `$policy` arguments"),
        "expected migration diagnostic, got: {err}"
    );
}

#[test]
fn main_policy_argument_is_injected_and_authorizes_fs_read() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fs = std::fs::canonicalize(root.join("stdlib/fs.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data.txt");
    std::fs::write(&data, "secret").unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"fs:
  $import: "{fs}"
main:
  $function:
    policy:
      $policy:
        fs-read:
          - requirement: mandatory
            scopes:
              - dir: "{dir}"
  return: $void
  do:
    - $let:
        path:
          $fs.path.new: "{path}"
    - $let:
        text:
          $fs.read-to-string: $path
          policy: $args.policy
"#,
            fs = fs.display().to_string().replace('\\', "/"),
            dir = dir.path().display().to_string().replace('\\', "/"),
            path = data.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).unwrap();
    vibra::execute::run_lowered(
        &lowered,
        &vibra::runtime::RunConfig {
            approved_policy: Some(vibra::lower::PolicyType {
                domains: std::collections::BTreeMap::from([(
                    "fs-read".to_string(),
                    vec![vibra::lower::PolicyGroup {
                        requirement: vibra::lower::GrantRequirement::Mandatory,
                        scopes: vec![vibra::lower::PolicyScope::Dir(
                            dir.path().display().to_string().replace('\\', "/"),
                        )],
                    }],
                )]),
            }),
            ..vibra::runtime::RunConfig::default()
        },
    )
    .expect("approved policy should authorize fs read");
}

#[test]
fn main_mandatory_policy_requires_full_coverage_before_body_runs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fs = std::fs::canonicalize(root.join("stdlib/fs.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"fs:
  $import: "{fs}"
main:
  $function:
    policy:
      $policy:
        fs-read:
          - requirement: mandatory
            scopes:
              - dir: "{dir}"
  return: $void
  do:
    - $let:
        path:
          $fs.path.new: "{path}"
"#,
            fs = fs.display().to_string().replace('\\', "/"),
            dir = dir.path().display().to_string().replace('\\', "/"),
            path = dir
                .path()
                .join("data.txt")
                .display()
                .to_string()
                .replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).unwrap();
    let err = format!(
        "{:#}",
        vibra::execute::run_lowered(&lowered, &vibra::runtime::RunConfig::default()).unwrap_err()
    );
    assert!(
        err.contains("mandatory policy coverage is missing"),
        "expected mandatory policy preflight failure, got: {err}"
    );
}

#[test]
fn fs_open_read_requires_policy_argument() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fs = std::fs::canonicalize(root.join("stdlib/fs.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    let data = dir.path().join("data.txt");
    std::fs::write(&data, "hello").unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"fs:
  $import: "{fs}"
main:
  $function: $void
  return: $void
  do:
      - $let:
          p:
            $fs.path.new: "{path}"
      - $let:
          opened:
            $fs.open-read: $p
"#,
            fs = fs.display().to_string().replace('\\', "/"),
            path = data.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("missing value argument `policy`"),
        "expected missing policy argument rejection, got: {err}"
    );
}

#[test]
#[ignore = "old grant-status API removed by grant side-channel model"]
fn fs_access_is_denied_without_any_runtime_grant() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fs = std::fs::canonicalize(root.join("stdlib/fs.vibra")).unwrap();
    let security = std::fs::canonicalize(root.join("stdlib/security.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    let data = dir.path().join("data.txt");
    std::fs::write(&data, "hello").unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"fs:
  $import: "{fs}"
security:
  $import: "{security}"
main:
  $function:
    input: $security.grants
  return: $void
  do:
      - $let:
          p:
            $fs.path.new: "{path}"
      - $match: $args.input.fs-read
        when:
            - case:
                $security.grant-status.granted:
                  $bind: grant
              do:
                - $let:
                    opened:
                      $fs.open-read: $p
                      grant: $grant
            - case:
                $security.grant-status.denied:
                  $bind: reason
              do: []
"#,
            fs = fs.display().to_string().replace('\\', "/"),
            security = security.display().to_string().replace('\\', "/"),
            path = data.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).unwrap();
    vibra::execute::run_lowered(&lowered, &vibra::runtime::RunConfig::default())
        .expect("denied grant path should be matchable and skip privileged fs access");
}

#[test]
#[ignore = "old grant-status API removed by grant side-channel model"]
fn fs_grant_rejects_sibling_prefix_escape() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fs = std::fs::canonicalize(root.join("stdlib/fs.vibra")).unwrap();
    let security = std::fs::canonicalize(root.join("stdlib/security.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let allowed = dir.path().join("root");
    let sibling = dir.path().join("root2");
    std::fs::create_dir_all(&allowed).unwrap();
    std::fs::create_dir_all(&sibling).unwrap();
    let target = sibling.join("escape.txt");
    std::fs::write(&target, "secret").unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"fs:
  $import: "{fs}"
security:
  $import: "{security}"
main:
  $function:
    input: $security.grants
  return: $void
  do:
      - $let:
          p:
            $fs.path.new: "{path}"
      - $match: $args.input.fs-read
        when:
            - case:
                $security.grant-status.granted:
                  $bind: grant
              do:
                - $let:
                    opened:
                      $fs.open-read: $p
                      grant: $grant
            - case:
                $security.grant-status.denied:
                  $bind: reason
              do: []
"#,
            fs = fs.display().to_string().replace('\\', "/"),
            security = security.display().to_string().replace('\\', "/"),
            path = target.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).unwrap();
    let err = vibra::execute::run_lowered(
        &lowered,
        &vibra::runtime::RunConfig {
            program_name: "vibra-test".to_string(),
            argv: Vec::new(),
            preopen_host_dirs: vec![allowed],
            ..vibra::runtime::RunConfig::default()
        },
    )
    .unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("outside configured grants"),
        "expected sibling escape denial, got: {msg}"
    );
}

#[test]
#[ignore = "grant attenuation was removed by grant side-channel model"]
fn fs_narrow_read_grant_limits_delegated_scope() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fs = std::fs::canonicalize(root.join("stdlib/fs.vibra")).unwrap();
    let security = std::fs::canonicalize(root.join("stdlib/security.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let allowed = dir.path().join("allowed");
    let denied = dir.path().join("denied");
    std::fs::create_dir_all(&allowed).unwrap();
    std::fs::create_dir_all(&denied).unwrap();
    std::fs::write(allowed.join("ok.txt"), "ok").unwrap();
    std::fs::write(denied.join("no.txt"), "no").unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"fs:
  $import: "{fs}"
security:
  $import: "{security}"
main:
  $function:
    input: $security.grants
  return: $void
  do:
      - $let:
          allow-root:
            $fs.path.new: "{allowed}"
      - $let:
          denied-file:
            $fs.path.new: "{denied_file}"
      - $match: $args.input.fs-read
        when:
            - case:
                $security.grant-status.granted:
                  $bind: read-grant
              do:
                - $let:
                    narrowed:
                      $fs.narrow-read: $read-grant
                      p: $allow-root
                - $match: $narrowed
                  when:
                      - case:
                          $result.result.ok:
                            $bind: narrow-grant
                        do:
                          - $let:
                              opened:
                                $fs.open-read: $denied-file
                                grant: $narrow-grant
                      - case:
                          $result.result.err:
                            $bind: narrow-err
                        do: []
            - case:
                $security.grant-status.denied:
                  $bind: read-denied
              do: []
"#,
            fs = fs.display().to_string().replace('\\', "/"),
            security = security.display().to_string().replace('\\', "/"),
            allowed = allowed.display().to_string().replace('\\', "/"),
            denied_file = denied
                .join("no.txt")
                .display()
                .to_string()
                .replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).unwrap();
    let err = vibra::execute::run_lowered(
        &lowered,
        &vibra::runtime::RunConfig {
            program_name: "vibra-test".to_string(),
            argv: Vec::new(),
            preopen_host_dirs: vec![dir.path().to_path_buf()],
            ..vibra::runtime::RunConfig::default()
        },
    )
    .unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("outside configured grants"),
        "expected narrowed grant to reject delegated escape, got: {msg}"
    );
}

#[test]
#[ignore = "old grant-status API removed by grant side-channel model"]
fn denied_grant_reason_uses_import_alias() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let security = std::fs::canonicalize(root.join("stdlib/security.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &entry,
        format!(
            r#"sec:
  $import: "{security}"
main:
  $function:
    input: $sec.grants
  return: $void
  do:
      - $match: $args.input.stdin-read
        when:
            - case:
                $sec.grant-status.denied:
                  $sec.denial-reason.not-granted: null
              do: []
            - case:
                $sec.grant-status.granted:
                  $bind: stdin-grant
              do: []
"#,
            security = security.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).unwrap();
    vibra::execute::run_lowered(&lowered, &vibra::runtime::RunConfig::default())
        .expect("denial reason enum key should follow the security import alias");
}

#[test]
#[ignore = "old grant-status API removed by grant side-channel model"]
fn fs_write_grant_allows_nonexistent_configured_scope() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fs = std::fs::canonicalize(root.join("stdlib/fs.vibra")).unwrap();
    let security = std::fs::canonicalize(root.join("stdlib/security.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    let allowed = dir.path().join("created-later");
    let file = allowed.join("data.txt");

    std::fs::write(
        &entry,
        format!(
            r#"fs:
  $import: "{fs}"
security:
  $import: "{security}"
main:
  $function:
    input: $security.grants
  return: $void
  do:
      - $let:
          dir-path:
            $fs.path.new: "{allowed}"
      - $let:
          file-path:
            $fs.path.new: "{file}"
      - $match: $args.input.fs-write
        when:
            - case:
                $security.grant-status.granted:
                  $bind: write-grant
              do:
                - $let:
                    made:
                      $fs.create-dir-all: $dir-path
                      grant: $write-grant
                - $match: $made
                  when:
                      - case:
                          $result.result.ok: null
                        do: []
                      - case:
                          $result.result.err:
                            $bind: make-err
                        do: []
                - $let:
                    written:
                      $fs.write-string-all: $file-path
                      s: "hello"
                      grant: $write-grant
                - $match: $written
                  when:
                      - case:
                          $result.result.ok: null
                        do: []
                      - case:
                          $result.result.err:
                            $bind: write-err
                        do: []
            - case:
                $security.grant-status.denied:
                  $bind: write-denied
              do: []
"#,
            fs = fs.display().to_string().replace('\\', "/"),
            security = security.display().to_string().replace('\\', "/"),
            allowed = allowed.display().to_string().replace('\\', "/"),
            file = file.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).unwrap();
    vibra::execute::run_lowered(
        &lowered,
        &vibra::runtime::RunConfig {
            program_name: "vibra-test".to_string(),
            argv: Vec::new(),
            allow_write: vec![allowed],
            ..vibra::runtime::RunConfig::default()
        },
    )
    .expect("nonexistent configured write scope should authorize created descendants");
    assert_eq!(std::fs::read_to_string(file).unwrap(), "hello");
}

#[test]
#[ignore = "grant attenuation was removed by grant side-channel model"]
fn fs_narrow_write_grant_allows_nonexistent_descendant_scope() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fs = std::fs::canonicalize(root.join("stdlib/fs.vibra")).unwrap();
    let security = std::fs::canonicalize(root.join("stdlib/security.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let allowed = dir.path().join("allowed");
    std::fs::create_dir_all(&allowed).unwrap();
    let narrowed_dir = allowed.join("created-later");
    let file = narrowed_dir.join("data.txt");
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &entry,
        format!(
            r#"fs:
  $import: "{fs}"
security:
  $import: "{security}"
main:
  $function:
    input: $security.grants
  return: $void
  do:
      - $let:
          narrow-root:
            $fs.path.new: "{narrowed_dir}"
      - $let:
          file-path:
            $fs.path.new: "{file}"
      - $match: $args.input.fs-write
        when:
            - case:
                $security.grant-status.granted:
                  $bind: write-grant
              do:
                - $let:
                    narrowed:
                      $fs.narrow-write: $write-grant
                      p: $narrow-root
                - $match: $narrowed
                  when:
                      - case:
                          $result.result.ok:
                            $bind: narrow-grant
                        do:
                          - $let:
                              made:
                                $fs.create-dir-all: $narrow-root
                                grant: $narrow-grant
                          - $match: $made
                            when:
                                - case:
                                    $result.result.ok: null
                                  do: []
                                - case:
                                    $result.result.err:
                                      $bind: make-err
                                  do: []
                          - $let:
                              written:
                                $fs.write-string-all: $file-path
                                s: "hello"
                                grant: $narrow-grant
                          - $match: $written
                            when:
                                - case:
                                    $result.result.ok: null
                                  do: []
                                - case:
                                    $result.result.err:
                                      $bind: write-err
                                  do: []
                      - case:
                          $result.result.err:
                            $bind: narrow-err
                        do: []
            - case:
                $security.grant-status.denied:
                  $bind: write-denied
              do: []
"#,
            fs = fs.display().to_string().replace('\\', "/"),
            security = security.display().to_string().replace('\\', "/"),
            narrowed_dir = narrowed_dir.display().to_string().replace('\\', "/"),
            file = file.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).unwrap();
    vibra::execute::run_lowered(
        &lowered,
        &vibra::runtime::RunConfig {
            program_name: "vibra-test".to_string(),
            argv: Vec::new(),
            allow_write: vec![allowed],
            ..vibra::runtime::RunConfig::default()
        },
    )
    .expect("narrowed grant to nonexistent descendant should authorize that descendant");
    assert_eq!(std::fs::read_to_string(file).unwrap(), "hello");
}

#[test]
#[ignore = "old grant-status API removed by grant side-channel model"]
fn env_set_invalid_name_returns_err_result() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let env_mod = std::fs::canonicalize(root.join("stdlib/env.vibra")).unwrap();
    let security = std::fs::canonicalize(root.join("stdlib/security.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &entry,
        format!(
            r#"env:
  $import: "{env_mod}"
security:
  $import: "{security}"
main:
  $function:
    input: $security.grants
  return: $void
  do:
      - $match: $args.input.env-write
        when:
            - case:
                $security.grant-status.granted:
                  $bind: env-grant
              do:
                - $let:
                    set-result:
                      $env.set: "BAD=NAME"
                      value: "value"
                      grant: $env-grant
                - $match: $set-result
                  when:
                      - case:
                          $result.result.ok: null
                        do: []
                      - case:
                          $result.result.err:
                            $env.env-error.invalid-name: null
                        do: []
            - case:
                $security.grant-status.denied:
                  $bind: env-denied
              do: []
"#,
            env_mod = env_mod.display().to_string().replace('\\', "/"),
            security = security.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).unwrap();
    vibra::execute::run_lowered(
        &lowered,
        &vibra::runtime::RunConfig {
            program_name: "vibra-test".to_string(),
            argv: Vec::new(),
            allow_env_write: vec!["*".to_string()],
            ..vibra::runtime::RunConfig::default()
        },
    )
    .expect("invalid env var names should be structured env-error results");
}

#[test]
fn duplicate_nested_imports_are_idempotent() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let io = std::fs::canonicalize(root.join("stdlib/io.vibra")).unwrap();
    let fs = std::fs::canonicalize(root.join("stdlib/fs.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &entry,
        format!(
            r#"io:
  $import: "{io}"
fs:
  $import: "{fs}"
main:
  $function: $void
  return: $void
  do:
      - $io.println: "ok"
"#,
            io = io.display().to_string().replace('\\', "/"),
            fs = fs.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog);
    assert!(
        lowered.is_ok(),
        "duplicate nested imports should not collide: {:?}",
        lowered.err()
    );
}

/// Issue #27: two different parent modules may each import a child under the same
/// local key (`util`). Nested defs must not share one global `util.*` namespace.
#[test]
fn nested_import_same_alias_is_scoped_to_parent() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let io = std::fs::canonicalize(root.join("stdlib/io.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let leaf_a = dir.path().join("leaf-a.vibra");
    let leaf_b = dir.path().join("leaf-b.vibra");
    let mod_a = dir.path().join("a.vibra");
    let mod_b = dir.path().join("b.vibra");
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &leaf_a,
        r#"id:
  $function: $void
  return: $str
  do:
    - $return: "A"
"#,
    )
    .unwrap();
    std::fs::write(
        &leaf_b,
        r#"id:
  $function: $void
  return: $str
  do:
    - $return: "B"
"#,
    )
    .unwrap();
    std::fs::write(
        &mod_a,
        format!(
            r#"util:
  $import: "{leaf}"
io:
  $import: "{io}"
call:
  $function: $void
  return: $void
  do:
    - $let:
        x:
          $util.id: null
    - $io.println: $x
"#,
            leaf = leaf_a.display().to_string().replace('\\', "/"),
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    std::fs::write(
        &mod_b,
        format!(
            r#"util:
  $import: "{leaf}"
io:
  $import: "{io}"
call:
  $function: $void
  return: $void
  do:
    - $let:
        x:
          $util.id: null
    - $io.println: $x
"#,
            leaf = leaf_b.display().to_string().replace('\\', "/"),
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"a:
  $import: "{a}"
b:
  $import: "{b}"
io:
  $import: "{io}"
main:
  $function: $void
  return: $void
  do:
    - $a.call: null
    - $b.call: null
"#,
            a = mod_a.display().to_string().replace('\\', "/"),
            b = mod_b.display().to_string().replace('\\', "/"),
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered =
        vibra::lower::lower_program(&prog).expect("nested same-alias imports should lower");
    assert!(
        lowered.functions.contains_key("a.util.id"),
        "expected nested fn under a.util.* (issue #27); util-related keys: {:?}",
        lowered
            .functions
            .keys()
            .filter(|k| k.contains("util"))
            .collect::<Vec<_>>()
    );
    assert!(
        lowered.functions.contains_key("b.util.id"),
        "expected nested fn under b.util.* (issue #27); util-related keys: {:?}",
        lowered
            .functions
            .keys()
            .filter(|k| k.contains("util"))
            .collect::<Vec<_>>()
    );
    vibra::execute::run_lowered(&lowered, &vibra::runtime::RunConfig::default()).unwrap();
}

#[test]
fn imported_module_cannot_use_entry_import_alias_transitively() {
    let dir = tempfile::tempdir().unwrap();
    let leaf = dir.path().join("leaf.vibra");
    let helper = dir.path().join("helper.vibra");
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &leaf,
        r#"value:
  $function: $void
  return: $str
  do:
    - $return: "hidden"
"#,
    )
    .unwrap();
    std::fs::write(
        &helper,
        r#"call:
  $function: $void
  return: $str
  do:
    - $return:
        $leaf.value: null
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"leaf:
  $import: "{leaf}"
helper:
  $import: "{helper}"
main:
  $function: $void
  return: $void
  do:
    - $helper.call: null
"#,
            leaf = leaf.display().to_string().replace('\\', "/"),
            helper = helper.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let err = format!("{:#}", vibra::load::load_program(&entry).unwrap_err());
    assert!(
        err.contains("E-MOD-004") && err.contains("leaf"),
        "expected direct-import diagnostic, got: {err}"
    );
}

#[test]
fn imported_module_direct_import_alias_is_usable() {
    let dir = tempfile::tempdir().unwrap();
    let leaf = dir.path().join("leaf.vibra");
    let helper = dir.path().join("helper.vibra");
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &leaf,
        r#"value:
  $function: $void
  return: $str
  do:
    - $return: "visible"
"#,
    )
    .unwrap();
    std::fs::write(
        &helper,
        format!(
            r#"leaf:
  $import: "{leaf}"
call:
  $function: $void
  return: $str
  do:
    - $return:
        $leaf.value: null
"#,
            leaf = leaf.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"helper:
  $import: "{helper}"
main:
  $function: $void
  return: $void
  do:
    - $helper.call: null
"#,
            helper = helper.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    vibra::lower::lower_program(&prog).expect("direct import alias should lower");
}

#[test]
fn imported_module_cannot_use_transitive_type_or_enum_alias() {
    let dir = tempfile::tempdir().unwrap();
    let types = dir.path().join("types.vibra");
    let helper = dir.path().join("helper.vibra");
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &types,
        r#"outcome:
  $enum:
    ok: $str
    err: $str
"#,
    )
    .unwrap();
    std::fs::write(
        &helper,
        r#"make:
  $function: $void
  return: $types.outcome
  do:
    - $return:
        $types.outcome.ok: "hidden"
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"types:
  $import: "{types}"
helper:
  $import: "{helper}"
main:
  $function: $void
  return: $void
  do:
    - $helper.make: null
"#,
            types = types.display().to_string().replace('\\', "/"),
            helper = helper.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let err = format!("{:#}", vibra::load::load_program(&entry).unwrap_err());
    assert!(
        err.contains("E-MOD-004") && err.contains("types"),
        "expected direct-import type diagnostic, got: {err}"
    );
}

#[test]
fn doc_annotation_mentioning_import_alias_does_not_require_direct_import() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &entry,
        r#"helper:
  $function: $void
  return: $void
  =doc: "See $result.result for the canonical shape."
  do:
    - $return: null
main:
  $function: $void
  return: $void
  do:
    - $helper: null
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    vibra::lower::lower_program(&prog).expect("=doc text should not require imports");
}

#[test]
fn same_module_qualified_local_symbol_is_allowed() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &entry,
        r#"outcome:
  $enum:
    ok: $str
    err: $str
make:
  $function: $void
  return: $outcome
  do:
    - $return:
        $outcome.ok: "local"
main:
  $function: $void
  return: $void
  do:
    - $make: null
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    vibra::lower::lower_program(&prog).expect("same-module qualified refs should lower");
}

#[test]
#[ignore = "old grant-status API removed by grant side-channel model"]
fn path_level_fs_apis_return_matchable_results() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fs = std::fs::canonicalize(root.join("stdlib/fs.vibra")).unwrap();
    let security = std::fs::canonicalize(root.join("stdlib/security.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    let work_dir = dir.path().join("work");
    let data = work_dir.join("data.txt");

    std::fs::write(
        &entry,
        format!(
            r#"fs:
  $import: "{fs}"
security:
  $import: "{security}"
main:
  $function:
    input: $security.grants
  return: $void
  do:
      - $let:
          dir-path:
            $fs.path.new: "{work_dir}"
      - $let:
          file-path:
            $fs.path.new: "{data}"
      - $match: $args.input.fs-write
        when:
            - case:
                $security.grant-status.granted:
                  $bind: write-grant
              do:
                - $let:
                    made:
                      $fs.create-dir-all: $dir-path
                      grant: $write-grant
                - $match: $made
                  when:
                      - case:
                          $result.result.ok: null
                        do: []
                      - case:
                          $result.result.err:
                            $bind: made-err
                        do: []
                - $let:
                    written:
                      $fs.write-string-all: $file-path
                      s: "hello"
                      grant: $write-grant
                - $match: $written
                  when:
                      - case:
                          $result.result.ok: null
                        do: []
                      - case:
                          $result.result.err:
                            $bind: written-err
                        do: []
                - $let:
                    appended:
                      $fs.append-string: $file-path
                      s: " world"
                      grant: $write-grant
                - $match: $appended
                  when:
                      - case:
                          $result.result.ok: null
                        do: []
                      - case:
                          $result.result.err:
                            $bind: appended-err
                        do: []
                - $match: $args.input.fs-read
                  when:
                      - case:
                          $security.grant-status.granted:
                            $bind: read-grant
                        do:
                          - $let:
                              read:
                                $fs.read-to-string: $file-path
                                grant: $read-grant
                          - $match: $read
                            when:
                                - case:
                                    $result.result.ok:
                                      $bind: read-ok
                                  do: []
                                - case:
                                    $result.result.err:
                                      $bind: read-err
                                  do: []
                          - $let:
                              stat:
                                $fs.metadata: $file-path
                                grant: $read-grant
                          - $match: $stat
                            when:
                                - case:
                                    $result.result.ok:
                                      $bind: stat-ok
                                  do: []
                                - case:
                                    $result.result.err:
                                      $bind: stat-err
                                  do: []
                          - $let:
                              canon:
                                $fs.canonicalize: $file-path
                                grant: $read-grant
                          - $match: $canon
                            when:
                                - case:
                                    $result.result.ok:
                                      $bind: canon-ok
                                  do: []
                                - case:
                                    $result.result.err:
                                      $bind: canon-err
                                  do: []
                          - $let:
                              entries:
                                $fs.read-dir: $dir-path
                                grant: $read-grant
                          - $match: $entries
                            when:
                                - case:
                                    $result.result.ok:
                                      $bind: entries-ok
                                  do: []
                                - case:
                                    $result.result.err:
                                      $bind: entries-err
                                  do: []
                      - case:
                          $security.grant-status.denied:
                            $bind: read-denied
                        do: []
                - $let:
                    removed-file:
                      $fs.remove-file: $file-path
                      grant: $write-grant
                - $match: $removed-file
                  when:
                      - case:
                          $result.result.ok: null
                        do: []
                      - case:
                          $result.result.err:
                            $bind: removed-file-err
                        do: []
                - $let:
                    removed-dir:
                      $fs.remove-dir: $dir-path
                      grant: $write-grant
                - $match: $removed-dir
                  when:
                      - case:
                          $result.result.ok: null
                        do: []
                      - case:
                          $result.result.err:
                            $bind: removed-dir-err
                        do: []
            - case:
                $security.grant-status.denied:
                  $bind: write-denied
              do: []
"#,
            fs = fs.display().to_string().replace('\\', "/"),
            security = security.display().to_string().replace('\\', "/"),
            work_dir = work_dir.display().to_string().replace('\\', "/"),
            data = data.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).unwrap();
    vibra::execute::run_lowered(
        &lowered,
        &vibra::runtime::RunConfig {
            program_name: "vibra-test".to_string(),
            argv: Vec::new(),
            preopen_host_dirs: vec![dir.path().to_path_buf()],
            ..vibra::runtime::RunConfig::default()
        },
    )
    .expect("path-level fs APIs should return matchable result values");
}

#[test]
#[ignore = "old grant-status API removed by grant side-channel model"]
fn path_level_fs_errors_return_err_results() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fs = std::fs::canonicalize(root.join("stdlib/fs.vibra")).unwrap();
    let security = std::fs::canonicalize(root.join("stdlib/security.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    let missing = dir.path().join("missing.txt");

    std::fs::write(
        &entry,
        format!(
            r#"fs:
  $import: "{fs}"
security:
  $import: "{security}"
main:
  $function:
    input: $security.grants
  return: $void
  do:
      - $let:
          missing-path:
            $fs.path.new: "{missing}"
      - $match: $args.input.fs-read
        when:
            - case:
                $security.grant-status.granted:
                  $bind: read-grant
              do:
                - $let:
                    read:
                      $fs.read-to-string: $missing-path
                      grant: $read-grant
                - $match: $read
                  when:
                      - case:
                          $result.result.ok:
                            $bind: read-ok
                        do: []
                      - case:
                          $result.result.err:
                            $bind: read-err
                        do: []
            - case:
                $security.grant-status.denied:
                  $bind: read-denied
              do: []
"#,
            fs = fs.display().to_string().replace('\\', "/"),
            security = security.display().to_string().replace('\\', "/"),
            missing = missing.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).unwrap();
    vibra::execute::run_lowered(
        &lowered,
        &vibra::runtime::RunConfig {
            program_name: "vibra-test".to_string(),
            argv: Vec::new(),
            preopen_host_dirs: vec![dir.path().to_path_buf()],
            ..vibra::runtime::RunConfig::default()
        },
    )
    .expect("path-level fs errors should be returned as result.err values");
}

#[test]
fn tagged_option_rejects_raw_payload_and_null_coercions() {
    let dir = tempfile::tempdir().unwrap();
    let model = dir.path().join("model.vibra");
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &model,
        r#"option:
  $enum:
    some: $t
    none: $void
  =where: {t: []}
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
    input:
      $m.option:
        t: $int64
  return: $void
  do:
      - $io.println: "using option"
expect-int:
  $function:
    input: $int64
  return: $void
  do:
      - $io.println: "x"
main:
  $function: $void
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
        "expected raw values and null to require explicit option constructors"
    );
}

#[test]
fn legacy_option_sugar_is_rejected_with_stable_code() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"legacy:
  $option: $str
main:
  $function: $void
  return: $void
  do: []
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = vibra::lower::lower_program(&prog).unwrap_err();
    assert!(
        format!("{err:#}").contains("E-OPTION-001"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn legacy_option_sugar_with_mapped_inner_type_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"holder:
  $record:
    value:
      $option:
        $array: $str
main:
  $function: $void
  return: $void
  do: []
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = vibra::lower::lower_program(&prog).unwrap_err();
    assert!(
        format!("{err:#}").contains("E-OPTION-001"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn direct_void_union_is_rejected_with_stable_code() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"legacy:
  $union: [$void, $str]
main:
  $function: $void
  return: $void
  do: []
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = vibra::lower::lower_program(&prog).unwrap_err();
    assert!(
        format!("{err:#}").contains("E-OPTION-001"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn generic_alias_named_option_remains_valid() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"option:
  $enum:
    some: $t
    none: $void
  =where: {t: []}
holder:
  $record:
    value:
      $option:
        t: $str
main:
  $function: $void
  return: $void
  do: []
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    vibra::lower::lower_program(&prog).expect("generic alias named option should remain valid");
}

#[test]
fn result_where_ok_and_err_type_params() {
    let dir = tempfile::tempdir().unwrap();
    let model = dir.path().join("model.vibra");
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &model,
        r#"result:
  $enum:
    ok: $t
    err: $e
  =where: {t: [], e: []}
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
  $function: $void
  return: $void
  do:
      - $let:
          r-ok:
            $m.result.ok: 99
      - $match: $r-ok
        when:
            - case:
                $m.result.ok:
                  $bind: x
              do:
                - $io.println: "ok"
            - case:
                $m.result.err:
                  $bind: y
              do:
                - $io.println: $y
      - $let:
          r-err:
            $m.result.err: "fail"
      - $match: $r-err
        when:
            - case:
                $m.result.ok:
                  $bind: x2
              do:
                - $io.println: "no"
            - case:
                $m.result.err:
                  $bind: y2
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
fn where_only_generic_names_no_unscoped_uppercase_fallback() {
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
  $enum:
    some: $t
    none: $void
  =where: {t: []}
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
  $function: $void
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
  $function: $void
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
        "scoped `=where` type param should allow int payload on some"
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
  $function: $void
  return: $void
  do:
      - $io.flush: null
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
  $function: $void
  return: $void
  do:
      - $io.flush: $void
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("zero-arg call payload must be `null`"),
        "unexpected error: {err}"
    );
}

#[test]
fn generic_user_fn_identity_returns_value() {
    let dir = tempfile::tempdir().unwrap();
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"io:
  $import: "{io}"
identity:
  $function:
    input: $t
  return: $t
  do:
      - $return: $args.input
  =where: {{t: []}}
main:
  $function: $void
  return: $void
  do:
      - $let:
          n:
            $identity: 7
            t: $int64
      - $io.println: "ok"
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).unwrap();
    vibra::execute::run_lowered(&lowered, &vibra::runtime::RunConfig::default()).unwrap();
}

#[test]
fn generic_call_requires_explicit_type_args() {
    let dir = tempfile::tempdir().unwrap();
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"io:
  $import: "{io}"
identity:
  $function:
    input: $t
  return: $t
  do:
      - $return: $args.input
  =where: {{t: []}}
main:
  $function: $void
  return: $void
  do:
      - $identity: 7
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let err = vibra::lower::lower_program(&prog).unwrap_err().to_string();
    assert!(
        err.contains("missing type argument `t`"),
        "unexpected error: {err}"
    );
}

#[test]
fn generic_call_rejects_unknown_keys() {
    let dir = tempfile::tempdir().unwrap();
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"io:
  $import: "{io}"
identity:
  $function:
    input: $t
  return: $t
  do:
      - $return: $args.input
  =where: {{t: []}}
main:
  $function: $void
  return: $void
  do:
      - $identity: 7
        t: $int64
        q: 1
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let err = vibra::lower::lower_program(&prog).unwrap_err().to_string();
    assert!(
        err.contains("unexpected key `q`")
            || err.contains("unexpected argument or type parameter `q`"),
        "unexpected error: {err}"
    );
}

#[test]
fn bool_literals_are_compatible_with_bool_args() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"accepts-bool:
  $function:
    x: $bool
  return: $void
  do:
    - $wasm:
        import:
          module: wasi_snapshot_preview1
          name: fd_sync
        args:
          - $const.1
main:
  $function: $void
  return: $void
  do:
      - $accepts-bool:
          x: true
      - $accepts-bool:
          x: false
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog);
    assert!(
        lowered.is_ok(),
        "expected true/false literals to lower as $bool"
    );
}

#[test]
fn bool_literal_is_rejected_for_non_bool_arg() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"accepts-int:
  $function:
    x: $int64
  return: $void
  do:
    - $wasm:
        import:
          module: wasi_snapshot_preview1
          name: fd_sync
        args:
          - $const.1
main:
  $function: $void
  return: $void
  do:
      - $accepts-int:
          x: true
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("type mismatch in call `$accepts-int` arg `x`"),
        "expected bool -> int mismatch, got: {err}"
    );
}

#[test]
#[ignore = "old grant-status API removed by grant side-channel model"]
fn fs_exists_returns_boolean_runtime_value() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fs = std::fs::canonicalize(root.join("stdlib/fs.vibra")).unwrap();
    let security = std::fs::canonicalize(root.join("stdlib/security.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("exists.txt");
    std::fs::write(&data, "present").unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"fs:
  $import: "{fs}"
security:
  $import: "{security}"
main:
  $function:
    grants: $security.grants
  return: $void
  do:
    - $let:
        p:
          $fs.path.new:
            s: "{path}"
    - $match: $args.grants.fs-read
      when:
        - case:
            $security.grant-status.granted:
              $bind: read-grant
          do:
            - $let:
                exists:
                  $fs.exists:
                    p: $p
                    grant: $read-grant
            - $match: $exists
              when:
                - case: true
                  do: []
        - case:
            $security.grant-status.denied:
              $bind: denied
          do: []
"#,
            fs = fs.display().to_string().replace('\\', "/"),
            security = security.display().to_string().replace('\\', "/"),
            path = data.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).expect("fs.exists bool match should lower");
    vibra::execute::run_lowered(
        &lowered,
        &vibra::runtime::RunConfig {
            program_name: "vibra-test".to_string(),
            argv: Vec::new(),
            preopen_host_dirs: vec![dir.path().to_path_buf()],
            ..vibra::runtime::RunConfig::default()
        },
    )
    .expect("fs.exists should return a bool runtime value");
}

#[test]
fn non_generic_multi_arg_call_rejects_unknown_key() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"join-ish:
  $function:
    left: $str
  args:
    right: $str
  return: $void
  do:
    - $wasm:
        import:
          module: wasi_snapshot_preview1
          name: fd_sync
        args:
          - $const.1
main:
  $function: $void
  return: $void
  do:
      - $join-ish:
          left: "a"
          right: "b"
          typo: "ignored"
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("unexpected key `typo` in call `$join-ish`"),
        "expected unexpected key rejection, got: {err}"
    );
}

#[test]
fn non_generic_single_arg_named_call_rejects_unknown_key() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"take-text:
  $function:
    x: $str
  return: $void
  do:
    - $wasm:
        import:
          module: wasi_snapshot_preview1
          name: fd_sync
        args:
          - $const.1
main:
  $function: $void
  return: $void
  do:
      - $take-text:
          x: "ok"
          typo: "ignored"
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("unexpected key `typo` in call `$take-text`"),
        "expected unexpected key rejection, got: {err}"
    );
}

#[test]
fn single_arg_constructor_shorthand_still_lowers() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"maybe:
  $enum:
    some: $str
    none: $void
take-maybe:
  $function:
    x: $maybe
  return: $void
  do:
    - $wasm:
        import:
          module: wasi_snapshot_preview1
          name: fd_sync
        args:
          - $const.1
main:
  $function: $void
  return: $void
  do:
      - $take-maybe:
          $maybe.some: "value"
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog);
    assert!(
        lowered.is_ok(),
        "expected single-arg constructor shorthand to keep lowering"
    );
}

#[test]
fn generic_call_value_arg_must_unify_with_substituted_type() {
    let dir = tempfile::tempdir().unwrap();
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"io:
  $import: "{io}"
identity:
  $function:
    input: $t
  return: $t
  do:
      - $return: $args.input
  =where: {{t: []}}
main:
  $function: $void
  return: $void
  do:
      - $identity: "hi"
        t: $int64
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    assert!(vibra::lower::lower_program(&prog).is_err());
}

#[test]
fn user_fn_non_void_return_requires_return_statement() {
    let dir = tempfile::tempdir().unwrap();
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"io:
  $import: "{io}"
bad:
  $function:
    input: $int64
  return: $int64
  do:
      - $io.println: "nope"
main:
  $function: $void
  return: $void
  do:
      - $io.println: "x"
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("non-void function must end with `$return`"),
        "unexpected error: {err}"
    );
}

#[test]
fn user_fn_imported_with_user_body_runs() {
    let dir = tempfile::tempdir().unwrap();
    let helper = dir.path().join("helper.vibra");
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &helper,
        r#"echo-int:
  $function:
    input: $int64
  return: $int64
  do:
      - $return: $args.input
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"h:
  $import: "{h}"
io:
  $import: "{io}"
main:
  $function: $void
  return: $void
  do:
      - $let:
          v:
            $h.echo-int: 42
      - $io.println: "z"
"#,
            h = helper.display().to_string().replace('\\', "/"),
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).unwrap();
    vibra::execute::run_lowered(&lowered, &vibra::runtime::RunConfig::default()).unwrap();
}

#[test]
fn generic_stdlib_wasm_wrapper_lowers() {
    let dir = tempfile::tempdir().unwrap();
    let lib = dir.path().join("lib.vibra");
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &lib,
        r#"flush-generic:
  $function:
    _: $t
  return: $void
  do:
      - $wasm:
          import:
            module: wasi_snapshot_preview1
            name: fd_sync
          args:
            - $const.1
  =where: {t: []}
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"lg:
  $import: "{lg}"
main:
  $function: $void
  return: $void
  do:
      - $lg.flush-generic: 0
        t: $int64
"#,
            lg = lib.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    assert!(vibra::lower::lower_program(&prog).is_ok());
}

// ===== New annotation + uniform-generics tests =====

/// A `=where` bound that is not an interface (here: `$int64`) is now
/// rejected with `E-WHERE-002`. `E-WHERE-001` was retired in Phase 5.
#[test]
fn where_with_non_interface_bound_is_rejected_with_e_where_002() {
    let dir = tempfile::tempdir().unwrap();
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"io:
  $import: "{io}"
box:
  $record:
    value: $t
  =where:
    t: [$int64]
main:
  $function: $void
  return: $void
  do:
      - $io.println: "ok"
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(err.contains("E-WHERE-002"), "unexpected error: {err}");
}

#[test]
fn self_type_is_allowed_inside_interface_body() {
    let dir = tempfile::tempdir().unwrap();
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"io:
  $import: "{io}"
display:
  $interface:
    fmt:
      $fn-type:
        args:
          $record:
            x: $self
        return: $str
main:
  $function: $void
  return: $void
  do:
      - $io.println: "ok"
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog);
    assert!(
        lowered.is_ok(),
        "expected `$self` inside `$interface` body to lower: {:?}",
        lowered.err()
    );
}

#[test]
fn self_type_is_rejected_in_top_level_record_field() {
    let dir = tempfile::tempdir().unwrap();
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"io:
  $import: "{io}"
node:
  $record:
    next: $self
main:
  $function: $void
  return: $void
  do:
      - $io.println: "ok"
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("E-SELF-001"),
        "expected E-SELF-001 for `$self` in top-level record field, got: {err}"
    );
}

#[test]
fn self_type_is_rejected_in_free_standing_function_signature() {
    let dir = tempfile::tempdir().unwrap();
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"io:
  $import: "{io}"
identity:
  $function:
    x: $self
  return: $self
  do:
      - $return: $args.x
main:
  $function: $void
  return: $void
  do:
      - $io.println: "ok"
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("E-SELF-001"),
        "expected E-SELF-001 for `$self` in free-standing function args, got: {err}"
    );
}

#[test]
fn self_type_is_allowed_in_nested_interface_inside_record() {
    // Even when wrapped in a `$record` (which itself forbids `$self`), an
    // inner `$interface` body re-opens the `$self` binding scope.
    let dir = tempfile::tempdir().unwrap();
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"io:
  $import: "{io}"
holder:
  $record:
    iface:
      $interface:
        fmt:
          $fn-type:
            args:
              $record:
                x: $self
            return: $str
main:
  $function: $void
  return: $void
  do:
      - $io.println: "ok"
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog);
    assert!(
        lowered.is_ok(),
        "expected `$self` inside a nested $interface body to lower: {:?}",
        lowered.err()
    );
}

#[test]
fn legacy_unprefixed_where_is_rejected_with_e_anno_002() {
    let dir = tempfile::tempdir().unwrap();
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"io:
  $import: "{io}"
pair:
  $tuple: [$a, $b]
  where: {{a: [], b: []}}
main:
  $function: $void
  return: $void
  do:
      - $io.println: "ok"
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("E-ANNO-002") && err.contains("=where"),
        "expected E-ANNO-002 with `=where` migration hint, got: {err}"
    );
}

#[test]
fn legacy_unprefixed_doc_is_rejected_with_e_anno_002() {
    let dir = tempfile::tempdir().unwrap();
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"io:
  $import: "{io}"
greeting:
  $literal: "hi"
  doc: "the greeting"
main:
  $function: $void
  return: $void
  do:
      - $io.println: "ok"
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("E-ANNO-002") && err.contains("=doc"),
        "expected E-ANNO-002 with `=doc` migration hint, got: {err}"
    );
}

#[test]
fn unknown_annotation_key_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"io:
  $import: "{io}"
foo:
  $function: $void
  return: $void
  do:
      - $io.println: "x"
  bogus: 1
main:
  $function: $void
  return: $void
  do:
      - $io.println: "ok"
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(err.contains("E-ANNO-001"), "unexpected error: {err}");
}

#[test]
fn doc_string_lowers_on_function_and_type_decls() {
    let dir = tempfile::tempdir().unwrap();
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"io:
  $import: "{io}"
greeting:
  $literal: "hi"
  =doc: |
    # `greeting`
    A literal type pinning the greeting string.
echo:
  $function:
    msg: $str
  return: $void
  do:
      - $io.println: $args.msg
  =doc: "Echo a message to stdout."
main:
  $function: $void
  return: $void
  do:
      - $echo: "hi"
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).unwrap();
    let echo = lowered.functions.get("echo").expect("echo registered");
    assert_eq!(echo.doc.as_deref(), Some("Echo a message to stdout."));
}

#[test]
fn where_key_order_defines_positional_type_param_order() {
    // Same fields, swapped `=where` key order. Only the second one accepts
    // (a -> Int, b -> Str) at the call site; the first one expects the reverse.
    let dir = tempfile::tempdir().unwrap();
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let mod_ab = dir.path().join("ab.vibra");
    let mod_ba = dir.path().join("ba.vibra");
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &mod_ab,
        r#"pair:
  $tuple: [$a, $b]
  =where: {a: [], b: []}
"#,
    )
    .unwrap();
    std::fs::write(
        &mod_ba,
        r#"pair:
  $tuple: [$a, $b]
  =where: {b: [], a: []}
"#,
    )
    .unwrap();

    let entry_src = |modpath: String, io: String| -> String {
        format!(
            r#"m:
  $import: "{m}"
io:
  $import: "{io}"
take:
  $function:
    input:
      $m.pair:
        a: $int64
        b: $str
  return: $void
  do:
      - $io.println: "ok"
main:
  $function: $void
  return: $void
  do:
      - $io.println: "ok"
"#,
            m = modpath,
            io = io,
        )
    };
    std::fs::write(
        &entry,
        entry_src(
            mod_ab.display().to_string().replace('\\', "/"),
            io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered_ab = vibra::lower::lower_program(&prog).unwrap();
    let take_ab = lowered_ab.functions.get("take").expect("take registered");
    let vibra::lower::TypeRef::Instantiated { type_args, .. } = &take_ab.arg_types[0] else {
        panic!(
            "expected instantiated tuple alias, got {:?}",
            take_ab.arg_types[0]
        );
    };
    assert_eq!(type_args.len(), 2);
    assert_eq!(type_args[0], vibra::lower::TypeRef::Int64);
    assert_eq!(type_args[1], vibra::lower::TypeRef::Str);

    std::fs::write(
        &entry,
        entry_src(
            mod_ba.display().to_string().replace('\\', "/"),
            io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered_ba = vibra::lower::lower_program(&prog).unwrap();
    let take_ba = lowered_ba.functions.get("take").expect("take registered");
    let vibra::lower::TypeRef::Instantiated { type_args, .. } = &take_ba.arg_types[0] else {
        panic!(
            "expected instantiated tuple alias, got {:?}",
            take_ba.arg_types[0]
        );
    };
    assert_eq!(type_args.len(), 2);
    assert_eq!(type_args[0], vibra::lower::TypeRef::Str);
    assert_eq!(type_args[1], vibra::lower::TypeRef::Int64);
}

#[test]
fn record_type_alias_lowers_and_is_usable_in_signature() {
    let dir = tempfile::tempdir().unwrap();
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let entry = dir.path().join("entry.vibra");
    // io.vibra defines ciovec as a non-generic $record. Function takes $io.ciovec
    // by bare reference (no instantiation) since it's non-generic.
    std::fs::write(
        &entry,
        format!(
            r#"io:
  $import: "{io}"
take-vec:
  $function:
    input: $io.ciovec
  return: $void
  do:
      - $io.println: "ok"
main:
  $function: $void
  return: $void
  do:
      - $io.println: "ok"
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).unwrap();
    let take_vec = lowered
        .functions
        .get("take-vec")
        .expect("take-vec registered");
    assert_eq!(
        take_vec.arg_types[0],
        vibra::lower::TypeRef::Named("io.ciovec".to_string())
    );
}

#[test]
fn tuple_type_alias_with_where_lowers() {
    let dir = tempfile::tempdir().unwrap();
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"io:
  $import: "{io}"
pair:
  $tuple: [$a, $b]
  =where: {{a: [], b: []}}
take:
  $function:
    input:
      $pair:
        a: $int64
        b: $str
  return: $void
  do:
      - $io.println: "ok"
main:
  $function: $void
  return: $void
  do:
      - $io.println: "ok"
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog);
    assert!(
        lowered.is_ok(),
        "expected tuple alias with `where` to lower: {:?}",
        lowered.err()
    );
}

#[test]
fn map_type_alias_with_where_lowers() {
    let dir = tempfile::tempdir().unwrap();
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"io:
  $import: "{io}"
dict:
  $map: {{key: $k, value: $v}}
  =where: {{k: [], v: []}}
take:
  $function:
    input:
      $dict:
        k: $str
        v: $int64
  return: $void
  do:
      - $io.println: "ok"
main:
  $function: $void
  return: $void
  do:
      - $io.println: "ok"
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog);
    assert!(
        lowered.is_ok(),
        "expected map alias with `where` to lower: {:?}",
        lowered.err()
    );
}

#[test]
fn interface_type_alias_with_where_lowers() {
    let dir = tempfile::tempdir().unwrap();
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"io:
  $import: "{io}"
container:
  $interface:
    value: $t
  =where: {{t: []}}
main:
  $function: $void
  return: $void
  do:
      - $io.println: "ok"
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog);
    assert!(
        lowered.is_ok(),
        "expected interface alias with `where` to lower: {:?}",
        lowered.err()
    );
}

#[test]
fn bare_generic_alias_in_signature_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"io:
  $import: "{io}"
pair:
  $tuple: [$a, $b]
  =where: {{a: [], b: []}}
take:
  $function:
    input: $pair
  return: $void
  do:
      - $io.println: "ok"
main:
  $function: $void
  return: $void
  do:
      - $io.println: "ok"
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(err.contains("E-GEN-001"), "unexpected error: {err}");
}

#[test]
fn instantiation_arity_mismatch_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"io:
  $import: "{io}"
pair:
  $tuple: [$a, $b]
  =where: {{a: [], b: []}}
take:
  $function:
    input:
      $pair:
        a: $int64
  return: $void
  do:
      - $io.println: "ok"
main:
  $function: $void
  return: $void
  do:
      - $io.println: "ok"
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(err.contains("E-GEN-002"), "unexpected error: {err}");
}

#[test]
fn instantiated_record_field_type_mismatch_is_caught() {
    let dir = tempfile::tempdir().unwrap();
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/io.vibra"))
        .unwrap();
    let entry = dir.path().join("entry.vibra");
    // Pass a $str where the function expects an int through an instantiated
    // generic record alias.
    std::fs::write(
        &entry,
        format!(
            r#"io:
  $import: "{io}"
box:
  $record:
    value: $t
  =where: {{t: []}}
take-int-box:
  $function:
    input:
      $box:
        t: $int64
  return: $void
  do:
      - $io.println: "ok"
make-str-box:
  $function: $void
  return:
    $box:
      t: $str
  do:
      - $return:
          value: "s"
main:
  $function: $void
  return: $void
  do:
      - $let:
          sb: {{$make-str-box: null}}
      - $take-int-box: $sb
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let res = vibra::lower::lower_program(&prog);
    assert!(
        res.is_err(),
        "expected mismatched generic record alias to be caught"
    );
}

#[test]
fn forall_keyword_is_no_longer_recognised() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"id:
  $forall:
    types: [t]
    in:
      $function:
        x: $t
      return: $t
      do:
          - $return: $args.x
main:
  $function: $void
  return: $void
  do: []
"#,
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    assert!(
        vibra::lower::lower_program(&prog).is_err(),
        "$forall should no longer be a recognised form"
    );
}

#[test]
fn list_and_dict_keywords_are_no_longer_recognised() {
    let dir = tempfile::tempdir().unwrap();
    let entry_list = dir.path().join("list.vibra");
    let entry_dict = dir.path().join("dict.vibra");
    std::fs::write(
        &entry_list,
        r#"my-list:
  $list: $int64
main:
  $function: $void
  return: $void
  do: []
"#,
    )
    .unwrap();
    std::fs::write(
        &entry_dict,
        r#"my-dict:
  $dict:
    a: $int64
main:
  $function: $void
  return: $void
  do: []
"#,
    )
    .unwrap();
    let prog_list = vibra::load::load_program(&entry_list).unwrap();
    assert!(
        vibra::lower::lower_program(&prog_list).is_err(),
        "$list should no longer be a recognised form"
    );
    let prog_dict = vibra::load::load_program(&entry_dict).unwrap();
    assert!(
        vibra::lower::lower_program(&prog_dict).is_err(),
        "$dict should no longer be a recognised form"
    );
}

// ---------------------------------------------------------------------------
// Phase 3: `=defs` (inherent ops) registration and `$self` substitution.
// ---------------------------------------------------------------------------

/// A non-generic record carrying `=defs` should register each inherent op
/// under its qualified name (`mod.type.op`), and `$self` inside `=defs`
/// must resolve to the enclosing type's named reference.
#[test]
fn defs_inherent_op_on_non_generic_type_registers_with_self_substituted() {
    let dir = tempfile::tempdir().unwrap();
    let model = dir.path().join("model.vibra");
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &model,
        r#"box:
  $record:
    value: $int64
  =defs:
    identity:
      $function: $self
      return: $self
      do:
          - $return: $args.self
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"m:
  $import: "{m}"
main:
  $function: $void
  return: $void
  do: []
"#,
            m = model.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered =
        vibra::lower::lower_program(&prog).expect("non-generic =defs program should lower");
    let dump = format!("{:?}", lowered);
    // The inherent op is registered under `m.box.identity` (sig key shape
    // matches what `parse_qualified_call`'s first-dot split will produce).
    assert!(
        dump.contains("m.box.identity"),
        "expected sig `m.box.identity` to be registered; got: {dump}"
    );
    // For a non-generic enclosing type, `$self` substitutes to the bare
    // `Named` reference -- no `Instantiated`, no leftover `SelfType`.
    assert!(
        dump.contains("Named(\"m.box\")"),
        "expected `$self` to substitute to `Named(\"m.box\")`; got: {dump}"
    );
    assert!(
        !dump.contains("SelfType"),
        "expected no leftover `SelfType` after substitution; dump: {dump}"
    );
}

/// A generic ADT carrying `=defs` should register inherent ops where
/// `$self` is substituted by the *instantiated* enclosing type
/// (so generic params remain in scope inside the op).
#[test]
fn defs_inherent_op_on_generic_type_substitutes_self_with_instantiation() {
    let dir = tempfile::tempdir().unwrap();
    let model = dir.path().join("res.vibra");
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &model,
        r#"result:
  $enum:
    err: $e
    ok: $t
  =where: {t: [], e: []}
  =defs:
    passthrough:
      $function: $self
      return: $self
      do:
          - $return: $args.self
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"r:
  $import: "{m}"
main:
  $function: $void
  return: $void
  do: []
"#,
            m = model.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).expect("generic =defs program should lower");
    let dump = format!("{:?}", lowered);
    assert!(
        dump.contains("r.result.passthrough"),
        "expected sig `r.result.passthrough` to be registered; got: {dump}"
    );
    // The substituted `$self` should carry the enclosing type's params,
    // so we expect to see an `Instantiated` reference to `r.result` in the
    // signature -- not a bare `SelfType` or unqualified `result`.
    assert!(
        dump.contains("Instantiated") && dump.contains("r.result"),
        "expected `$self` to be substituted by the instantiated enclosing type; dump: {dump}"
    );
    assert!(
        !dump.contains("SelfType"),
        "expected no leftover `SelfType` after substitution; dump: {dump}"
    );
}

/// `=defs` is only valid alongside a *type* definition. Putting it on
/// a `$function` must be rejected with `E-DEFS-001`.
#[test]
fn defs_on_a_function_definition_is_rejected_with_e_defs_001() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"main:
  $function: $void
  return: $void
  do: []
  =defs:
    nope:
      $function: $void
      return: $void
      do: []
"#,
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let err = vibra::lower::lower_program(&prog).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("E-DEFS-001"),
        "expected E-DEFS-001 for `=defs` on a `$function`, got: {msg}"
    );
}

/// Each entry of an `=defs` block must itself be a `$function` envelope.
#[test]
fn defs_entry_that_is_not_a_function_is_rejected_with_e_defs_001() {
    let dir = tempfile::tempdir().unwrap();
    let model = dir.path().join("model.vibra");
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &model,
        r#"thing:
  $record:
    value: $int64
  =defs:
    bad:
      $record:
        x: $int64
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"m:
  $import: "{m}"
main:
  $function: $void
  return: $void
  do: []
"#,
            m = model.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let err = vibra::lower::lower_program(&prog).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("E-DEFS-001"),
        "expected E-DEFS-001 for non-`$function` entry inside `=defs`, got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// Phase 5: lifted `=where` bounds (interface bounds checked at call sites
// and at type-position instantiations).
// ---------------------------------------------------------------------------

/// A non-empty `=where` bound that *is* an interface alias is now accepted
/// (Phase 5 lifted the legacy E-WHERE-001 restriction). Calling such a
/// generic function with a type that has the matching `=impl` succeeds.
/// Uses an enum (`box`) since v1 has no record-construction syntax.
#[test]
fn where_with_interface_bound_is_satisfied_at_call_site() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"display:
  $interface:
    fmt:
      $fn-type:
        args:
          $record:
            x: $self
        return: $str
box:
  $enum:
    boxed: $int64
  =impl:
    $display:
      fmt:
        $function: $self
        return: $str
        do:
            - $return: "boxed"
identity-displayable:
  $function:
    x: $t
  return: $t
  do:
      - $return: $args.x
  =where:
    t: [$display]
main:
  $function: $void
  return: $void
  do:
      - $let:
          b: { $box.boxed: 1 }
      - $let:
          c:
            $identity-displayable: $b
            t: $box
"#,
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    vibra::lower::lower_program(&prog)
        .expect("$box should satisfy `t: [$display]` via its `=impl` block");
}

/// Calling a `t: [$display]`-bounded function with a type that has *no*
/// `=impl: { $display: ... }` block (here: a plain primitive) is rejected
/// with `E-BOUND-001`.
#[test]
fn where_bound_violation_at_call_site_is_rejected_with_e_bound_001() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"display:
  $interface:
    fmt:
      $fn-type:
        args:
          $record:
            x: $self
        return: $str
identity-displayable:
  $function:
    x: $t
  return: $t
  do:
      - $return: $args.x
  =where:
    t: [$display]
main:
  $function: $void
  return: $void
  do:
      - $let:
          v:
            $identity-displayable: 7
            t: $int64
"#,
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("E-BOUND-001"),
        "expected E-BOUND-001 for primitive failing iface bound; got: {err}"
    );
}

#[test]
fn let_expr_nested_generic_bound_violations_are_rejected_with_e_bound_001() {
    fn program_with_let_expr(expr: &str) -> String {
        let indented_expr = expr
            .lines()
            .map(|line| format!("            {line}\n"))
            .collect::<String>();

        format!(
            r#"display:
  $interface:
    fmt:
      $fn-type:
        args:
          $record:
            x: $self
        return: $str
needs-display:
  $function:
    x: $t
  return: $t
  do:
      - $return: $args.x
  =where:
    t: [$display]
meter:
  $newtype: $int64
main:
  $function: $void
  return: $void
  do:
      - $let:
          result:
{indented_expr}"#
        )
    }

    let cases = [
        (
            "record field",
            r#"$record:
  y:
    $needs-display: 1
    t: $int64"#,
        ),
        (
            "array item",
            r#"$array:
  - $needs-display: 1
    t: $int64"#,
        ),
        (
            "map key",
            r#"$map:
  - key:
      $needs-display: 1
      t: $int64
    value: bad"#,
        ),
        (
            "map value",
            r#"$map:
  - key: bad
    value:
      $needs-display: 1
      t: $int64"#,
        ),
        (
            "cast subject",
            r#"$cast:
  $needs-display: 1
  t: $int64
into: $meter"#,
        ),
        (
            "if branch",
            r#"$if: true
then:
  $needs-display: 1
  t: $int64
else: 0"#,
        ),
    ];

    for (case, expr) in cases {
        let dir = tempfile::tempdir().unwrap();
        let entry = dir.path().join("entry.vibra");
        std::fs::write(&entry, program_with_let_expr(expr)).unwrap();

        let prog = vibra::load::load_program(&entry).unwrap();
        let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
        assert!(
            err.contains("E-BOUND-001"),
            "expected E-BOUND-001 for nested generic call in {case}; got: {err}"
        );
    }
}

#[test]
fn call_argument_nested_generic_bound_violations_are_rejected_with_e_bound_001() {
    fn program_with_main_statement(statement: &str) -> String {
        let indented_statement = statement
            .lines()
            .map(|line| format!("      {line}\n"))
            .collect::<String>();

        format!(
            r#"display:
  $interface:
    fmt:
      $fn-type:
        args:
          $record:
            x: $self
        return: $str
needs-display:
  $function:
    x: $t
  return: $t
  do:
      - $return: $args.x
  =where:
    t: [$display]
takes-record:
  $function:
    rec:
      $record:
        y: $int64
  return: $void
  do:
      - $let:
          ignored: 0
wrap-record:
  $function:
    rec:
      $record:
        y: $int64
  return:
    $record:
      y: $int64
  do:
      - $return: $args.rec
main:
  $function: $void
  return: $void
  do:
{indented_statement}"#
        )
    }

    let cases = [
        (
            "statement call argument",
            r#"  - $takes-record:
      rec:
        $record:
          y:
            $needs-display: 1
            t: $int64"#,
        ),
        (
            "let call argument",
            r#"  - $let:
      result:
        $wrap-record:
          rec:
            $record:
              y:
                $needs-display: 1
                t: $int64"#,
        ),
    ];

    for (case, statement) in cases {
        let dir = tempfile::tempdir().unwrap();
        let entry = dir.path().join("entry.vibra");
        std::fs::write(&entry, program_with_main_statement(statement)).unwrap();

        let prog = vibra::load::load_program(&entry).unwrap();
        let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
        assert!(
            err.contains("E-BOUND-001"),
            "expected E-BOUND-001 for nested generic call in {case}; got: {err}"
        );
    }
}

/// A type-position instantiation of a generic alias with a bound also
/// triggers bound-checking. Here `bag` declares `t: [$display]`; using
/// `$bag: { t: $int64 }` as a return-type annotation on another alias is
/// rejected. The annotation lives in pure type position (no value
/// constructed), so this exercises the type-walking branch of the
/// instantiation-bound sweep.
#[test]
fn where_bound_violation_at_type_position_is_rejected_with_e_bound_001() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"display:
  $interface:
    fmt:
      $fn-type:
        args:
          $record:
            x: $self
        return: $str
bag:
  $record:
    item: $t
  =where:
    t: [$display]
holds-bag:
  $record:
    inner:
      $bag:
        t: $int64
main:
  $function: $void
  return: $void
  do: []
"#,
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("E-BOUND-001"),
        "expected E-BOUND-001 for `$bag: {{ t: $int64 }}` at type position; got: {err}"
    );
}

/// A `$intersect` of two interfaces requires *both* to have `=impl`s.
/// Uses an enum for `half-impl` so we can construct a value of it.
#[test]
fn where_bound_intersect_requires_both_interfaces() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"display:
  $interface:
    fmt:
      $fn-type:
        args:
          $record:
            x: $self
        return: $str
debug:
  $interface:
    show:
      $fn-type:
        args:
          $record:
            x: $self
        return: $str
half-impl:
  $enum:
    wrap: $int64
  =impl:
    $display:
      fmt:
        $function: $self
        return: $str
        do:
            - $return: "half"
both-iface:
  $function:
    x: $t
  return: $t
  do:
      - $return: $args.x
  =where:
    t:
      - $intersect: [$display, $debug]
main:
  $function: $void
  return: $void
  do:
      - $let:
          v: { $half-impl.wrap: 1 }
      - $let:
          r:
            $both-iface: $v
            t: $half-impl
"#,
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("E-BOUND-001"),
        "expected E-BOUND-001: half-impl satisfies $display but not $debug; got: {err}"
    );
}

/// A generic param re-passed to another generic call must declare bounds
/// at least as strong as the callee's. Missing bounds in the caller scope
/// produce E-BOUND-001.
#[test]
fn where_bound_chain_requires_caller_to_declare_bound() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"display:
  $interface:
    fmt:
      $fn-type:
        args:
          $record:
            x: $self
        return: $str
needs-display:
  $function:
    x: $t
  return: $t
  do:
      - $return: $args.x
  =where:
    t: [$display]
forwarder:
  $function:
    x: $u
  return: $u
  do:
      - $let:
          y:
            $needs-display: $args.x
            t: $u
      - $return: $y
  =where:
    u: []
main:
  $function: $void
  return: $void
  do: []
"#,
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("E-BOUND-001"),
        "expected E-BOUND-001: forwarder's `u` has no bound but is passed to `t: [$display]`; got: {err}"
    );
}

// ---------------------------------------------------------------------------
// Phase 6: interface-qualified call dispatch (`$iface.method: { ... }`).
// ---------------------------------------------------------------------------

/// `$display.fmt: $b` resolves to the impl method registered for
/// `box`'s `=impl: { $display: ... }` block. The lowered `Statement::Call`
/// must point at the impl's sig key (here `box.display.fmt`).
#[test]
fn iface_qualified_call_dispatches_to_impl_method() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"display:
  $interface:
    fmt:
      $fn-type:
        args:
          $record:
            x: $self
        return: $str
box:
  $enum:
    boxed: $int64
  =impl:
    $display:
      fmt:
        $function: $self
        return: $str
        do:
            - $return: "boxed"
main:
  $function: $void
  return: $void
  do:
      - $let:
          b: { $box.boxed: 1 }
      - $let:
          s: { $display.fmt: $b }
"#,
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let lp = vibra::lower::lower_program(&prog)
        .expect("interface-qualified call should resolve to box.display.fmt");
    let last = lp.statements.last().expect("main statements present");
    let vibra::lower::Statement::Let { value, .. } = last else {
        panic!("expected $let as last main statement, got {last:?}");
    };
    let vibra::lower::LetValue::Call(call) = value else {
        panic!("expected Call let-value, got {value:?}");
    };
    assert_eq!(call.callee_key, "box.display.fmt");
}

/// Interface-qualified call to a method that has *no* `$self` argument is
/// rejected with `E-CALL-IFACE-NOSELF`. The user is told to use the
/// type-qualified form (`$<type>.<iface>.<method>`) instead.
#[test]
fn iface_qualified_call_without_self_arg_is_rejected_with_e_call_iface_noself() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"from-iface:
  $interface:
    from:
      $fn-type:
        args:
          $record:
            x: $int64
        return: $void
box:
  $enum:
    boxed: $int64
  =impl:
    $from-iface:
      from:
        $function:
          x: $int64
        return: $void
        do:
            - $let:
                unused: $args.x
main:
  $function: $void
  return: $void
  do:
      - $from-iface.from: 5
"#,
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("E-CALL-IFACE-NOSELF"),
        "expected E-CALL-IFACE-NOSELF; got: {err}"
    );
}

/// Interface-qualified call where the dispatch arg's type has *no* `=impl`
/// for the interface is rejected with `E-BOUND-001`. (`$int64` vs.
/// `$display`.)
#[test]
fn iface_qualified_call_unimplemented_type_is_rejected_with_e_bound_001() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"display:
  $interface:
    fmt:
      $fn-type:
        args:
          $record:
            x: $self
        return: $str
main:
  $function: $void
  return: $void
  do:
      - $let:
          s: { $display.fmt: 7 }
"#,
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("E-BOUND-001") || err.contains("cannot dispatch on dispatch-arg type"),
        "expected E-BOUND-001 / dispatch-type rejection; got: {err}"
    );
}

/// Interface-qualified call where the dispatch arg has a *generic* static
/// type (e.g. an `$args.x: $t` of an enclosing function) is rejected with
/// `E-DISPATCH-001` -- proper monomorphisation is deferred.
#[test]
fn iface_qualified_call_on_generic_value_is_rejected_with_e_dispatch_001() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"display:
  $interface:
    fmt:
      $fn-type:
        args:
          $record:
            x: $self
        return: $str
fmt-via-bound:
  $function:
    x: $t
  return: $str
  do:
      - $let:
          s: { $display.fmt: $args.x }
      - $return: $s
  =where:
    t: [$display]
main:
  $function: $void
  return: $void
  do: []
"#,
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("E-DISPATCH-001"),
        "expected E-DISPATCH-001 for generic-typed dispatch arg; got: {err}"
    );
}

// ---------------------------------------------------------------------------
// Phase 4: `=impl` (interface implementations) registration and validation.
// ---------------------------------------------------------------------------

/// A non-generic `$record` implementing a single-method `$interface` should
/// register under `mod.type.iface.method` and produce an `ImplKey ->
/// ImplBody` entry in `lowered.impls`.
#[test]
fn impl_basic_interface_lowers_and_registers_method() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"display:
  $interface:
    fmt:
      $fn-type:
        args:
          $record:
            x: $self
        return: $str
box:
  $record:
    value: $int64
  =impl:
    $display:
      fmt:
        $function: $self
        return: $str
        do:
            - $return: "boxed"
main:
  $function: $void
  return: $void
  do: []
"#,
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).expect("basic =impl should lower");
    let dump = format!("{:?}", lowered);
    assert!(
        dump.contains("box.display.fmt"),
        "expected sig `box.display.fmt` to be registered; got: {dump}"
    );
    assert_eq!(
        lowered.impls.len(),
        1,
        "expected exactly one entry in `impls`; got {}: {:?}",
        lowered.impls.len(),
        lowered.impls
    );
    let key = vibra::lower::ImplKey {
        implementing_type: "box".to_string(),
        interface: "display".to_string(),
    };
    let body = lowered
        .impls
        .get(&key)
        .expect("ImplKey {box, display} should be present");
    assert!(
        matches!(
            body.methods.get("fmt"),
            Some(vibra::lower::ImplMethodBinding::Fresh(s)) if s == "box.display.fmt"
        ),
        "expected Fresh(\"box.display.fmt\"); got {:?}",
        body.methods.get("fmt")
    );
}

/// An impl method binding can be a string ref to an existing inherent op
/// declared via `=defs`. The impl table records `Ref(<sig-key>)` and
/// the signatures must match.
#[test]
fn impl_method_as_ref_to_existing_defs_op_works() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"display:
  $interface:
    fmt:
      $fn-type:
        args:
          $record:
            x: $self
        return: $str
box:
  $record:
    value: $int64
  =defs:
    show:
      $function:
        x: $self
      return: $str
      do:
          - $return: "shown"
  =impl:
    $display:
      fmt: $box.show
main:
  $function: $void
  return: $void
  do: []
"#,
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).expect("=impl with method-ref should lower");
    let key = vibra::lower::ImplKey {
        implementing_type: "box".to_string(),
        interface: "display".to_string(),
    };
    let body = lowered.impls.get(&key).expect("impl entry missing");
    match body.methods.get("fmt") {
        Some(vibra::lower::ImplMethodBinding::Alias(s)) => {
            assert_eq!(s, "box.show", "ref should target the =defs op key");
        }
        other => panic!("expected Ref(\"box.show\"); got {other:?}"),
    }
}

/// `=impl` is only valid alongside a *type* definition.
#[test]
fn impl_on_a_function_definition_is_rejected_with_e_impl_001() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"main:
  $function: $void
  return: $void
  do: []
  =impl:
    $display:
      fmt: $whatever
"#,
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let err = vibra::lower::lower_program(&prog).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("E-IMPL-001"),
        "expected E-IMPL-001 for `=impl` on `$function`; got: {msg}"
    );
}

/// An `=impl` block keyed by an unknown interface alias is rejected.
#[test]
fn impl_unknown_interface_alias_is_rejected_with_e_impl_002() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"box:
  $record:
    value: $int64
  =impl:
    $no-such-iface:
      fmt: $whatever
main:
  $function: $void
  return: $void
  do: []
"#,
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let err = vibra::lower::lower_program(&prog).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("E-IMPL-002"),
        "expected E-IMPL-002 for unknown iface alias; got: {msg}"
    );
}

/// An impl block is rejected if it is missing one of the iface's methods.
#[test]
fn impl_missing_method_is_rejected_with_e_impl_003() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"display:
  $interface:
    fmt:
      $fn-type:
        args:
          $record:
            x: $self
        return: $str
    debug:
      $fn-type:
        args:
          $record:
            x: $self
        return: $str
box:
  $record:
    value: $int64
  =impl:
    $display:
      fmt:
        $function: $self
        return: $str
        do:
            - $return: "ok"
main:
  $function: $void
  return: $void
  do: []
"#,
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let err = vibra::lower::lower_program(&prog).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("E-IMPL-003"),
        "expected E-IMPL-003 for missing method; got: {msg}"
    );
}

/// An impl block is rejected if it carries a key that is neither an iface
/// type-arg, an iface method, nor `=where`.
#[test]
fn impl_extra_key_in_impl_is_rejected_with_e_impl_004() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"display:
  $interface:
    fmt:
      $fn-type:
        args:
          $record:
            x: $self
        return: $str
box:
  $record:
    value: $int64
  =impl:
    $display:
      fmt:
        $function: $self
        return: $str
        do:
            - $return: "ok"
      bonus-stuff: 1
main:
  $function: $void
  return: $void
  do: []
"#,
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let err = vibra::lower::lower_program(&prog).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("E-IMPL-004"),
        "expected E-IMPL-004 for extraneous payload key; got: {msg}"
    );
}

/// An impl method whose signature does not match the iface declaration
/// (after `$self` substitution) is rejected.
#[test]
fn impl_method_signature_mismatch_is_rejected_with_e_impl_005() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"display:
  $interface:
    fmt:
      $fn-type:
        args:
          $record:
            x: $self
        return: $str
box:
  $record:
    value: $int64
  =impl:
    $display:
      fmt:
        $function: $self
        return: $int64
        do:
            - $return: 1
main:
  $function: $void
  return: $void
  do: []
"#,
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let err = vibra::lower::lower_program(&prog).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("E-IMPL-005"),
        "expected E-IMPL-005 for signature mismatch; got: {msg}"
    );
}

#[test]
fn impl_method_return_type_can_be_covariant() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"display:
  $interface:
    fmt:
      $fn-type:
        args:
          $record:
            x: $self
        return:
          $union: [$int64, $str]
box:
  $record:
    value: $int64
  =impl:
    $display:
      fmt:
        $function: $self
        return: $str
        do:
            - $return: "boxed"
main:
  $function: $void
  return: $void
  do: []
"#,
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    vibra::lower::lower_program(&prog).expect("narrower impl return should satisfy iface");
}

#[test]
fn impl_method_argument_types_remain_invariant() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"display:
  $interface:
    fmt:
      $fn-type:
        args:
          $record:
            x:
              $union: [$int64, $str]
        return: $str
box:
  $record:
    value: $int64
  =impl:
    $display:
      fmt:
        $function:
          x: $str
        return: $str
        do:
            - $return: "boxed"
main:
  $function: $void
  return: $void
  do: []
"#,
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let err = vibra::lower::lower_program(&prog).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("E-IMPL-005"),
        "expected E-IMPL-005 for non-invariant args; got: {msg}"
    );
}

#[test]
fn impl_method_return_type_cannot_be_wider_than_interface() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"display:
  $interface:
    fmt:
      $fn-type:
        args:
          $record:
            x: $self
        return: $str
box:
  $record:
    value: $int64
  =impl:
    $display:
      fmt:
        $function: $self
        return:
          $union: [$int64, $str]
        do:
            - $return: "boxed"
main:
  $function: $void
  return: $void
  do: []
"#,
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let err = vibra::lower::lower_program(&prog).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("E-IMPL-005"),
        "expected E-IMPL-005 for wider impl return; got: {msg}"
    );
}

/// A parametric interface `from { t -> ... }` should accept a concrete
/// binding `t: $int64` and substitute it correctly into the method
/// signature. The function body uses a `$wasm` import so we exercise the
/// signature-substitution path without depending on record-construction
/// (a feature that does not yet exist in v1).
#[test]
fn impl_with_parametric_interface_binds_iface_type_arg() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"from-iface:
  $interface:
    from:
      $fn-type:
        args:
          $record:
            x: $t
        return: $int64
  =where: {t: []}
box:
  $record:
    value: $int64
  =impl:
    $from-iface:
      t: $int64
      from:
        $function:
          x: $t
        return: $int64
        do:
            - $wasm:
                import:
                  module: wasi_snapshot_preview1
                  name: fd_sync
                args:
                  - $args.x
main:
  $function: $void
  return: $void
  do: []
"#,
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).expect("parametric `=impl` should lower");
    let key = vibra::lower::ImplKey {
        implementing_type: "box".to_string(),
        interface: "from-iface".to_string(),
    };
    let body = lowered.impls.get(&key).expect("impl entry missing");
    assert_eq!(
        body.interface_args.len(),
        1,
        "expected one iface type-arg binding; got {:?}",
        body.interface_args
    );
    assert!(
        matches!(body.interface_args[0], vibra::lower::TypeRef::Int64),
        "expected `t -> Int64`; got {:?}",
        body.interface_args[0]
    );
    let dump = format!("{:?}", lowered);
    assert!(
        dump.contains("box.from-iface.from"),
        "expected sig `box.from-iface.from` to be registered; got: {dump}"
    );
    // The registered sig's arg type should be the substituted `Int64`,
    // *not* `Generic("t")` -- iface type-params are synthetic during parsing.
    let sig = lowered
        .functions
        .get("box.from-iface.from")
        .expect("sig missing");
    assert!(
        sig.type_params.is_empty(),
        "sig should have no free type-params; got {:?}",
        sig.type_params
    );
    assert!(
        matches!(sig.arg_types[0], vibra::lower::TypeRef::Int64),
        "expected substituted arg type Int64; got {:?}",
        sig.arg_types[0]
    );
}

#[test]
fn into_interface_registers_target_type_param() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"into-iface:
  $interface:
    into:
      $fn-type:
        args:
          $record:
            self: $self
        return: $t
  =where: {t: []}
box:
  $record:
    value: $int64
  =impl:
    $into-iface:
      t: $int64
      into:
        $function: $self
        return: $t
        do:
            - $wasm:
                import:
                  module: wasi_snapshot_preview1
                  name: fd_sync
                args:
                  - $const.1
main:
  $function: $void
  return: $void
  do: []
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).expect("parametric `into` impl should lower");
    let key = vibra::lower::ImplKey {
        implementing_type: "box".to_string(),
        interface: "into-iface".to_string(),
    };
    let body = lowered.impls.get(&key).expect("impl entry missing");
    assert_eq!(body.interface_args.len(), 1);
    assert!(matches!(
        body.interface_args[0],
        vibra::lower::TypeRef::Int64
    ));
}

/// Method-as-ref to an unknown function is rejected.
#[test]
fn impl_unknown_ref_target_is_rejected_with_e_impl_006() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"display:
  $interface:
    fmt:
      $fn-type:
        args:
          $record:
            x: $self
        return: $str
box:
  $record:
    value: $int64
  =impl:
    $display:
      fmt: $no.such.function
main:
  $function: $void
  return: $void
  do: []
"#,
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let err = vibra::lower::lower_program(&prog).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("E-IMPL-006"),
        "expected E-IMPL-006 for unknown ref target; got: {msg}"
    );
}

/// Inherent ops cannot redeclare a type parameter that is already in
/// scope from the enclosing generic type.
#[test]
fn defs_inherent_op_cannot_shadow_enclosing_type_param() {
    let dir = tempfile::tempdir().unwrap();
    let model = dir.path().join("model.vibra");
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &model,
        r#"holder:
  $record:
    value: $t
  =where: {t: []}
  =defs:
    bad:
      $function: $self
      return: $self
      do:
          - $return: $args.self
      =where: {t: []}
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"m:
  $import: "{m}"
main:
  $function: $void
  return: $void
  do: []
"#,
            m = model.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    let err = vibra::lower::lower_program(&prog).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("redeclares type parameter"),
        "expected shadowing of enclosing type param to be rejected, got: {msg}"
    );
}

fn vibra_cmd() -> std::process::Command {
    std::process::Command::new(env!("CARGO_BIN_EXE_vibra"))
}

fn path_str(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

#[test]
fn project_init_bin_template_creates_valid_project() {
    let dir = tempfile::tempdir().unwrap();

    let output = vibra_cmd()
        .current_dir(dir.path())
        .args(["init", "hello"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let project = dir.path().join("hello");
    let manifest = std::fs::read_to_string(project.join("project.vibra")).unwrap();
    let main = std::fs::read_to_string(project.join("src/hello/main.vibra")).unwrap();
    assert!(manifest.contains("manifest-version: 1"));
    assert!(main.contains("@std/io.vibra"));
    assert!(project.join("src/hello/main.vibra").exists());
    assert!(project.join("dep/std/io.vibra").exists());

    let check = vibra_cmd()
        .current_dir(dir.path())
        .args(["check", "hello"])
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "check failed: {}",
        String::from_utf8_lossy(&check.stderr)
    );

    let run = vibra_cmd()
        .current_dir(&project)
        .args(["run", "src/hello/main.vibra"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "run failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
}

#[test]
fn project_init_lib_and_workspace_templates_check() {
    let dir = tempfile::tempdir().unwrap();
    for (name, template, expected_entry) in [
        ("mylib", "lib", "src/mylib/lib.vibra"),
        ("myapp", "workspace", "src/core/lib.vibra"),
    ] {
        let init = vibra_cmd()
            .current_dir(dir.path())
            .args(["init", name, "--template", template])
            .output()
            .unwrap();
        assert!(
            init.status.success(),
            "{template} init failed: {}",
            String::from_utf8_lossy(&init.stderr)
        );
        assert!(dir.path().join(name).join(expected_entry).exists());

        let check = vibra_cmd()
            .current_dir(dir.path())
            .args(["check", name])
            .output()
            .unwrap();
        assert!(
            check.status.success(),
            "{template} check failed: {}",
            String::from_utf8_lossy(&check.stderr)
        );
    }
}

#[test]
fn project_check_rejects_invalid_manifest_shapes() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("bad");
    std::fs::create_dir_all(project.join("src/a")).unwrap();
    std::fs::write(
        project.join("src/a/main.vibra"),
        "main:\n  $function: $void\n  return: $void\n  do: []\n",
    )
    .unwrap();
    std::fs::write(
        project.join("project.vibra"),
        r#"manifest-version: 1
package:
  name: bad
  version: 0.1.0
targets:
  libs:
    - name: dup
      root: src/a
      entry: main.vibra
  bins:
    - name: dup
      root: /tmp/outside
      entry: main.vibra
dependencies:
  remote:
    git: https://example.com/remote.git
"#,
    )
    .unwrap();

    let check = vibra_cmd()
        .current_dir(dir.path())
        .args(["check", "bad"])
        .output()
        .unwrap();
    assert!(!check.status.success());
    let stderr = String::from_utf8_lossy(&check.stderr);
    assert!(
        stderr.contains("duplicate target or dependency name `dup`")
            || stderr.contains("must be relative")
            || stderr.contains("git dependency `remote` must pin `rev`"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn project_check_resolves_local_dependency_without_copying_it() {
    let dir = tempfile::tempdir().unwrap();
    let dep = dir.path().join("local-utils");
    std::fs::create_dir_all(&dep).unwrap();
    std::fs::write(
        dep.join("util.vibra"),
        "io:\n  $import: \"@std/io.vibra\"\nanswer: 42\n",
    )
    .unwrap();
    let stdlib = Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib");

    let project = dir.path().join("app");
    std::fs::create_dir_all(project.join("src/app")).unwrap();
    std::fs::write(
        project.join("src/app/main.vibra"),
        "utils:\n  $import: \"@local-utils/util.vibra\"\nmain:\n  $function: $void\n  return: $void\n  do: []\n",
    )
    .unwrap();
    std::fs::write(
        project.join("project.vibra"),
        format!(
            r#"manifest-version: 1
package:
  name: app
  version: 0.1.0
targets:
  bins:
    - name: app
      root: src/app
      entry: main.vibra
dependencies:
  std:
    path: {}
  local-utils:
    path: {}
"#,
            path_str(&stdlib),
            path_str(&dep)
        ),
    )
    .unwrap();

    let check = vibra_cmd()
        .current_dir(dir.path())
        .args(["check", "app"])
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "check failed: {}",
        String::from_utf8_lossy(&check.stderr)
    );
    assert!(!project.join("dep/local-utils").exists());

    let run = vibra_cmd()
        .current_dir(&project)
        .args(["run", "src/app/main.vibra"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "run failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
}

#[test]
fn project_sync_clones_git_dependency_at_pinned_rev() {
    let dir = tempfile::tempdir().unwrap();
    let remote = dir.path().join("remote-math");
    std::fs::create_dir_all(&remote).unwrap();
    std::fs::write(remote.join("math.vibra"), "pi: 3\n").unwrap();
    assert!(std::process::Command::new("git")
        .current_dir(&remote)
        .args(["init"])
        .output()
        .unwrap()
        .status
        .success());
    assert!(std::process::Command::new("git")
        .current_dir(&remote)
        .args(["add", "."])
        .output()
        .unwrap()
        .status
        .success());
    assert!(std::process::Command::new("git")
        .current_dir(&remote)
        .args([
            "-c",
            "user.name=Vibra Test",
            "-c",
            "user.email=vibra@example.test",
            "commit",
            "-m",
            "seed",
        ])
        .output()
        .unwrap()
        .status
        .success());
    let rev = std::process::Command::new("git")
        .current_dir(&remote)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(rev.status.success());
    let rev = String::from_utf8(rev.stdout).unwrap().trim().to_string();

    let project = dir.path().join("app");
    std::fs::create_dir_all(project.join("src/app")).unwrap();
    std::fs::write(
        project.join("src/app/main.vibra"),
        "math:\n  $import: \"@math/math.vibra\"\nmain:\n  $function: $void\n  return: $void\n  do: []\n",
    )
    .unwrap();
    std::fs::write(
        project.join("project.vibra"),
        format!(
            r#"manifest-version: 1
package:
  name: app
  version: 0.1.0
targets:
  bins:
    - name: app
      root: src/app
      entry: main.vibra
dependencies:
  math:
    git: {}
    rev: {}
"#,
            path_str(&remote),
            rev
        ),
    )
    .unwrap();

    let sync = vibra_cmd()
        .current_dir(dir.path())
        .args(["sync", "app"])
        .output()
        .unwrap();
    assert!(
        sync.status.success(),
        "sync failed: {}",
        String::from_utf8_lossy(&sync.stderr)
    );
    assert!(project.join("dep/math/math.vibra").exists());
    std::fs::write(project.join("dep/math/math.vibra"), "dirty: 0\n").unwrap();

    let resync = vibra_cmd()
        .current_dir(dir.path())
        .args(["sync", "app"])
        .output()
        .unwrap();
    assert!(
        resync.status.success(),
        "resync failed: {}",
        String::from_utf8_lossy(&resync.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(project.join("dep/math/math.vibra")).unwrap(),
        "pi: 3\n"
    );

    let check = vibra_cmd()
        .current_dir(dir.path())
        .args(["check", "app"])
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "check failed: {}",
        String::from_utf8_lossy(&check.stderr)
    );
}

#[test]
fn vibra_test_runs_top_level_test_declarations_without_main() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("app");
    let tests_dir = project.join("tests");
    std::fs::create_dir_all(&tests_dir).unwrap();
    std::fs::write(
        tests_dir.join("basic.vibra"),
        r#"test:
  $import: "@std/test.vibra"
passes:
  $test:
    do:
      - $test.assert: true
also-passes:
  $test:
    do:
      - $test.assert: true
"#,
    )
    .unwrap();
    std::fs::write(
        project.join("project.vibra"),
        r#"manifest-version: 1
package:
  name: app
  version: 0.1.0
targets:
  bins:
    - name: app
      root: tests
      entry: basic.vibra
dependencies:
  std:
    path: dep/std
"#,
    )
    .unwrap();
    copy_stdlib(&project.join("dep/std"));

    let output = vibra_cmd()
        .current_dir(dir.path())
        .args(["test", "app"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "test failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("2 passed"), "unexpected stdout: {stdout}");
}

#[test]
fn vibra_test_reports_assertion_failures() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("app");
    let tests_dir = project.join("tests");
    std::fs::create_dir_all(&tests_dir).unwrap();
    std::fs::write(
        tests_dir.join("fails.vibra"),
        r#"test:
  $import: "@std/test.vibra"
fails:
  $test:
    do:
      - $test.assert: false
"#,
    )
    .unwrap();
    std::fs::write(
        project.join("project.vibra"),
        r#"manifest-version: 1
package:
  name: app
  version: 0.1.0
targets:
  bins:
    - name: app
      root: tests
      entry: fails.vibra
dependencies:
  std:
    path: dep/std
"#,
    )
    .unwrap();
    copy_stdlib(&project.join("dep/std"));

    let output = vibra_cmd()
        .current_dir(dir.path())
        .args(["test", "app"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.contains("1 failed") || stderr.contains("assertion failed"),
        "unexpected stdout={stdout} stderr={stderr}"
    );
}

#[test]
fn vibra_test_writes_yaml_report_file() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("app");
    let tests_dir = project.join("tests");
    let report = dir.path().join("report.yaml");
    std::fs::create_dir_all(&tests_dir).unwrap();
    std::fs::write(
        tests_dir.join("basic.vibra"),
        r#"test:
  $import: "@std/test.vibra"
passes:
  $test:
    do:
      - $test.assert: true
"#,
    )
    .unwrap();
    std::fs::write(
        project.join("project.vibra"),
        r#"manifest-version: 1
package:
  name: app
  version: 0.1.0
targets:
  bins:
    - name: app
      root: tests
      entry: basic.vibra
dependencies:
  std:
    path: dep/std
"#,
    )
    .unwrap();
    copy_stdlib(&project.join("dep/std"));

    let output = vibra_cmd()
        .current_dir(dir.path())
        .args([
            "test",
            "app",
            "--report",
            "yaml",
            "--report-file",
            &path_str(&report),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "test failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let yaml = std::fs::read_to_string(report).unwrap();
    assert!(yaml.contains("total: 1"), "unexpected yaml: {yaml}");
    assert!(yaml.contains("passed: 1"), "unexpected yaml: {yaml}");
    assert!(yaml.contains("status: passed"), "unexpected yaml: {yaml}");
}

#[test]
fn module_part_test_file_shares_base_module_definitions() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("app");
    let tests_dir = project.join("tests");
    std::fs::create_dir_all(&tests_dir).unwrap();
    std::fs::write(
        tests_dir.join("math.vibra"),
        r#"is-ready:
  $function: $void
  return: $bool
  do:
    - $return: true
"#,
    )
    .unwrap();
    std::fs::write(
        tests_dir.join("math.test.vibra"),
        r#"test:
  $import: "@std/test.vibra"
uses-base-function:
  $test:
    do:
      - $let:
          ready:
            $is-ready: null
      - $test.assert: $ready
"#,
    )
    .unwrap();
    std::fs::write(
        project.join("project.vibra"),
        r#"manifest-version: 1
package:
  name: app
  version: 0.1.0
targets:
  bins:
    - name: app
      root: tests
      entry: math.vibra
dependencies:
  std:
    path: dep/std
"#,
    )
    .unwrap();
    copy_stdlib(&project.join("dep/std"));

    let output = vibra_cmd()
        .current_dir(dir.path())
        .args(["test", "app", "--filter", "uses-base-function"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "test failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn copy_stdlib(dest: &Path) {
    std::fs::create_dir_all(dest).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib");
    for entry in std::fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), dest.join(entry.file_name())).unwrap();
    }
}

#[test]
fn vibra_exec_prints_raw_string_expression() {
    let output = vibra_cmd()
        .args(["exec", "\"hello\"", "--format", "raw"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "exec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "hello");
}

#[test]
fn vibra_exec_reads_arg_file_and_gets_code_path() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.vibra");
    std::fs::write(
        &source,
        r#"io:
  $import: ./io.vibra
main:
  $function: $void
  return: $void
  do:
    - $io.println: "Hello"
"#,
    )
    .unwrap();

    let output = vibra_cmd()
        .args([
            "exec",
            "{$code.get: {$code.parse: $src}, path: \"/main/do/0/$io.println\"}",
            "--arg-file",
            &format!("src={}", path_str(&source)),
            "--format",
            "raw",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "exec failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "\"Hello\"");
}

#[test]
fn vibra_exec_sets_code_path_while_preserving_comments() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.vibra");
    std::fs::write(
        &source,
        r#"# keep module comment
io:
  $import: ./io.vibra

main:
  $function: $void
  return: $void
  do:
    # keep call comment
    - $io.println: "Hello"
"#,
    )
    .unwrap();

    let output = vibra_cmd()
        .args([
            "exec",
            "{$code.emit: {$code.set: {$code.parse: $src}, path: \"/main/do/0/$io.println\", value: \"\\\"Changed\\\"\"}}",
            "--arg-file",
            &format!("src={}", path_str(&source)),
            "--format",
            "raw",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "exec failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("# keep module comment"), "output: {stdout}");
    assert!(stdout.contains("# keep call comment"), "output: {stdout}");
    assert!(
        stdout.contains("$io.println: \"Changed\""),
        "output: {stdout}"
    );
    assert!(
        !stdout.contains("$io.println: \"Hello\""),
        "output: {stdout}"
    );
}

#[test]
fn vibra_exec_rejects_invalid_pointer_and_non_string_raw_output() {
    let missing = vibra_cmd()
        .args([
            "exec",
            "{$code.get: {$code.parse: \"main: 1\\n\"}, path: \"/missing\"}",
            "--format",
            "raw",
        ])
        .output()
        .unwrap();
    assert!(!missing.status.success());
    assert!(
        String::from_utf8_lossy(&missing.stderr).contains("JSON Pointer"),
        "stderr: {}",
        String::from_utf8_lossy(&missing.stderr)
    );

    let non_string = vibra_cmd()
        .args(["exec", "42", "--format", "raw"])
        .output()
        .unwrap();
    assert!(!non_string.status.success());
    assert!(
        String::from_utf8_lossy(&non_string.stderr).contains("raw output requires"),
        "stderr: {}",
        String::from_utf8_lossy(&non_string.stderr)
    );
}

#[test]
fn vibra_fmt_defaults_to_yaml_check_mode_and_write_is_explicit() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("messy.vibra");
    let original = "main:\n    $function: $void\n    return: $void\n    do: []\n";
    std::fs::write(&source, original).unwrap();

    let check = vibra_cmd()
        .args(["fmt", &path_str(&source)])
        .output()
        .unwrap();
    assert!(!check.status.success(), "fmt check should fail for drift");
    let stdout = String::from_utf8_lossy(&check.stdout);
    assert!(stdout.contains("files:"), "expected yaml output: {stdout}");
    assert!(
        stdout.contains("summary:"),
        "expected yaml summary: {stdout}"
    );
    assert!(
        stdout.contains("status: changed"),
        "expected changed status: {stdout}"
    );
    assert_eq!(std::fs::read_to_string(&source).unwrap(), original);

    let write = vibra_cmd()
        .args(["fmt", &path_str(&source), "--write"])
        .output()
        .unwrap();
    assert!(
        write.status.success(),
        "fmt --write failed: {}",
        String::from_utf8_lossy(&write.stderr)
    );
    assert_ne!(std::fs::read_to_string(&source).unwrap(), original);

    let recheck = vibra_cmd()
        .args(["fmt", &path_str(&source)])
        .output()
        .unwrap();
    assert!(
        recheck.status.success(),
        "formatted file should pass check: {}",
        String::from_utf8_lossy(&recheck.stdout)
    );
    let stdout = String::from_utf8_lossy(&recheck.stdout);
    assert!(
        stdout.contains("status: unchanged"),
        "expected unchanged status: {stdout}"
    );
}

#[test]
fn vibra_fmt_write_preserves_comments() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("commented.vibra");
    let original = "# important intent\nmain:\n  $function: $void\n  return: $void\n  do: []\n";
    std::fs::write(&source, original).unwrap();

    let write = vibra_cmd()
        .args(["fmt", &path_str(&source), "--write"])
        .output()
        .unwrap();
    assert!(
        write.status.success(),
        "fmt --write failed: {}",
        String::from_utf8_lossy(&write.stderr)
    );
    let formatted = std::fs::read_to_string(&source).unwrap();
    assert!(
        formatted.contains("# important intent"),
        "comment should survive fmt --write, got:\n{formatted}"
    );
}

#[test]
fn vibra_fmt_json_output_is_explicit() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("ok.vibra");
    std::fs::write(
        &source,
        "main:\n  $function: $void\n  return: $void\n  do: []\n",
    )
    .unwrap();

    let output = vibra_cmd()
        .args(["fmt", &path_str(&source), "--output", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "fmt json failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(report.get("files").is_some(), "json report: {report}");
    assert!(report.get("summary").is_some(), "json report: {report}");
}

#[test]
fn vibra_lint_defaults_to_yaml_and_reports_kebab_case_locations() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("style.vibra");
    std::fs::write(&source, "BadName: 1\n").unwrap();

    let output = vibra_cmd()
        .args(["lint", &path_str(&source), "--category", "style"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "warning-only lint should pass without --deny-warnings"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.starts_with("diagnostics:"),
        "default lint output should be yaml: {stdout}"
    );
    assert!(stdout.contains("code: W-STYLE-001"), "stdout: {stdout}");
    assert!(stdout.contains("line: 0"), "stdout: {stdout}");
    assert!(stdout.contains("column: 0"), "stdout: {stdout}");
    assert!(
        !stdout.contains("offset:"),
        "offset should be omitted when not guaranteed: {stdout}"
    );
}

#[test]
fn vibra_lint_suppression_and_deny_warnings_are_respected() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("style.vibra");
    std::fs::write(
        &source,
        "# vibra-lint-disable-next-line W-STYLE-001\nBadName: 1\nOtherBad: 2\n",
    )
    .unwrap();

    let suppressed = vibra_cmd()
        .args(["lint", &path_str(&source), "--category", "style"])
        .output()
        .unwrap();
    assert!(
        suppressed.status.success(),
        "lint failed: {}",
        String::from_utf8_lossy(&suppressed.stderr)
    );
    let stdout = String::from_utf8_lossy(&suppressed.stdout);
    assert!(
        !stdout.contains("BadName"),
        "suppressed diagnostic leaked: {stdout}"
    );
    assert!(
        stdout.contains("OtherBad"),
        "unsuppressed diagnostic missing: {stdout}"
    );

    let denied = vibra_cmd()
        .args([
            "lint",
            &path_str(&source),
            "--category",
            "style",
            "--deny-warnings",
        ])
        .output()
        .unwrap();
    assert!(!denied.status.success(), "--deny-warnings should fail");
}

#[test]
fn vibra_lint_json_and_sarif_outputs_are_explicit() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("style.vibra");
    std::fs::write(&source, "BadName: 1\n").unwrap();

    let json = vibra_cmd()
        .args([
            "lint",
            &path_str(&source),
            "--category",
            "style",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        json.status.success(),
        "json lint failed: {}",
        String::from_utf8_lossy(&json.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(report["diagnostics"][0]["code"], "W-STYLE-001");

    let sarif = vibra_cmd()
        .args([
            "lint",
            &path_str(&source),
            "--category",
            "style",
            "--format",
            "sarif",
        ])
        .output()
        .unwrap();
    assert!(
        sarif.status.success(),
        "sarif lint failed: {}",
        String::from_utf8_lossy(&sarif.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&sarif.stdout).unwrap();
    assert_eq!(report["version"], "2.1.0");
    assert_eq!(report["runs"][0]["results"][0]["ruleId"], "W-STYLE-001");
    assert_eq!(
        report["runs"][0]["tool"]["driver"]["rules"][0]["shortDescription"]["text"],
        "Symbol-like key is not kebab-case"
    );
    assert!(
        !report["runs"][0]["tool"]["driver"]["rules"][0]["shortDescription"]["text"]
            .as_str()
            .unwrap()
            .contains("BadName")
    );
}

#[test]
fn vibra_lint_reports_parse_and_compile_errors_as_structured_yaml() {
    let dir = tempfile::tempdir().unwrap();
    let bad_yaml = dir.path().join("bad-yaml.vibra");
    let bad_compile = dir.path().join("bad-compile.vibra");
    std::fs::write(&bad_yaml, "main:\n  -\n    nope: [\n").unwrap();
    std::fs::write(
        &bad_compile,
        "main:\n  $function: $void\n  return: $void\n  do:\n    - $missing: null\n",
    )
    .unwrap();

    let syntax = vibra_cmd()
        .args(["lint", &path_str(&bad_yaml), "--category", "syntax"])
        .output()
        .unwrap();
    assert!(!syntax.status.success());
    let stdout = String::from_utf8_lossy(&syntax.stdout);
    assert!(stdout.contains("code: E-YAML-001"), "stdout: {stdout}");
    assert!(stdout.contains("line:"), "stdout: {stdout}");

    let compile = vibra_cmd()
        .args(["lint", &path_str(&bad_compile), "--category", "compile"])
        .output()
        .unwrap();
    assert!(!compile.status.success());
    let stdout = String::from_utf8_lossy(&compile.stdout);
    assert!(stdout.contains("diagnostics:"), "stdout: {stdout}");
    assert!(
        stdout.contains("severity: error"),
        "expected compile error diagnostic: {stdout}"
    );
}

#[test]
fn vibra_lint_rejects_yaml_anchors_and_aliases() {
    let dir = tempfile::tempdir().unwrap();
    let anchored = dir.path().join("anchored.vibra");
    std::fs::write(&anchored, "a: &x 1\nb: *x\n").unwrap();

    let output = vibra_cmd()
        .args(["lint", &path_str(&anchored), "--category", "syntax"])
        .output()
        .unwrap();
    assert!(!output.status.success(), "anchors/aliases should fail lint");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("code: E-YAML-001"),
        "expected E-YAML-001 for anchors/aliases: {stdout}"
    );
    assert!(
        stdout.contains("anchor") || stdout.contains("alias"),
        "expected anchor/alias message: {stdout}"
    );
}

#[test]
fn vibra_lint_reports_hidden_transitive_import_alias() {
    let dir = tempfile::tempdir().unwrap();
    let leaf = dir.path().join("leaf.vibra");
    let helper = dir.path().join("helper.vibra");
    let entry = dir.path().join("entry.vibra");

    std::fs::write(
        &leaf,
        r#"value:
  $function: $void
  return: $str
  do:
    - $return: "hidden"
"#,
    )
    .unwrap();
    std::fs::write(
        &helper,
        r#"call:
  $function: $void
  return: $str
  do:
    - $return:
        $leaf.value: null
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"leaf:
  $import: "{leaf}"
helper:
  $import: "{helper}"
main:
  $function: $void
  return: $void
  do:
    - $helper.call: null
"#,
            leaf = leaf.display().to_string().replace('\\', "/"),
            helper = helper.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let output = vibra_cmd()
        .args(["lint", &path_str(&entry), "--category", "compile"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("code: E-MOD-004"), "stdout: {stdout}");
    assert!(stdout.contains("leaf"), "stdout: {stdout}");
}

#[test]
fn vibra_lint_compile_checks_library_files_without_main() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("library.vibra");
    std::fs::write(&source, "legacy:\n  $option: $str\n").unwrap();

    let output = vibra_cmd()
        .args(["lint", &path_str(&source), "--category", "compile"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("code: E-OPTION-001"), "stdout: {stdout}");
}

#[test]
fn vibra_lint_percent_encodes_file_uris() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("bad#name%25.vibra");
    std::fs::write(&source, "BadName: 1\n").unwrap();

    let output = vibra_cmd()
        .args(["lint", &path_str(&source), "--category", "style"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "lint failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("bad%23name%2525.vibra"), "stdout: {stdout}");
}

// ===== Issue #50: shared test context + single-named-test lowering =====

/// Write an entry module that imports the canonical `stdlib/test.vibra` and
/// contains `count` passing `$test` declarations plus a shared helper function
/// every test can call. Returns the temp dir (keep it alive) and entry path.
fn issue50_many_tests_entry(count: usize) -> (tempfile::TempDir, std::path::PathBuf) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let test_lib = std::fs::canonicalize(root.join("stdlib/test.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");

    let mut src = format!(
        "test:\n  $import: \"{lib}\"\nthe-answer:\n  $function: $void\n  return: $int64\n  do:\n    - $return: 42\n",
        lib = test_lib.display().to_string().replace('\\', "/"),
    );
    for i in 0..count {
        src.push_str(&format!(
            "many-{i}:\n  $test:\n    do:\n      - $let:\n          v:\n            $the-answer: null\n      - $match: $v\n        when:\n          - case: 42\n            do:\n              - $test.assert: true\n          - case:\n              $wildcard: null\n            do:\n              - $test.fail: not 42\n",
        ));
    }
    std::fs::write(&entry, src).unwrap();
    (dir, entry)
}

#[test]
fn issue50_many_tests_lower_and_run_all() {
    let count = 50;
    let (_dir, entry) = issue50_many_tests_entry(count);
    let program = vibra::load::load_program(&entry).unwrap();

    // Discovery must see every test by name without lowering any body.
    let names = vibra::lower::discover_test_names(&program).unwrap();
    assert_eq!(names.len(), count, "discovery should find all tests");

    // Each named test lowers on its own and runs green.
    for name in &names {
        let case = vibra::lower::lower_named_test(&program, name).unwrap();
        assert_eq!(&case.name, name);
        vibra::execute::run_lowered(&case.program, &vibra::runtime::RunConfig::default())
            .unwrap_or_else(|e| panic!("test `{name}` should pass: {e:#}"));
    }
}

#[test]
fn issue50_named_test_matches_lower_tests() {
    let (_dir, entry) = issue50_many_tests_entry(8);
    let program = vibra::load::load_program(&entry).unwrap();

    let all = vibra::lower::lower_tests(&program).unwrap();
    for reference in &all {
        let single = vibra::lower::lower_named_test(&program, &reference.name).unwrap();
        // The single-test path must produce an equivalent program: same body,
        // same shared context (functions/constants/impls).
        assert_eq!(
            format!("{:?}", single.program.statements),
            format!("{:?}", reference.program.statements),
            "statements differ for `{}`",
            reference.name
        );
        assert_eq!(
            single.program.functions.len(),
            reference.program.functions.len(),
            "function table size differs for `{}`",
            reference.name
        );
        assert!(
            single.program.functions.contains_key("the-answer"),
            "shared helper missing from `{}`",
            reference.name
        );
        // And both execute identically.
        vibra::execute::run_lowered(&single.program, &vibra::runtime::RunConfig::default())
            .expect("named-test run should match prior passing behavior");
    }
}

#[test]
fn issue50_failing_test_still_reported() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let test_lib = std::fs::canonicalize(root.join("stdlib/test.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"test:
  $import: "{lib}"
passes:
  $test:
    do:
      - $test.assert: true
fails:
  $test:
    do:
      - $test.assert: false
"#,
            lib = test_lib.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let program = vibra::load::load_program(&entry).unwrap();

    let passing = vibra::lower::lower_named_test(&program, "passes").unwrap();
    vibra::execute::run_lowered(&passing.program, &vibra::runtime::RunConfig::default())
        .expect("passing test should run cleanly");

    let failing = vibra::lower::lower_named_test(&program, "fails").unwrap();
    let err = vibra::execute::run_lowered(&failing.program, &vibra::runtime::RunConfig::default())
        .expect_err("failing test must surface an error");
    assert!(
        format!("{err:#}").contains("assertion failed"),
        "unexpected error: {err:#}"
    );

    // A name that does not exist is reported clearly.
    let missing = vibra::lower::lower_named_test(&program, "nope").unwrap_err();
    assert!(
        format!("{missing:#}").contains("test `nope` not found"),
        "unexpected error: {missing:#}"
    );
}

/// A writer that always fails as if the consuming pipe had been closed.
/// Used to prove guest stdout writes surface as a matchable `fs-error` rather
/// than panicking the runtime (the old `print!`/`eprint!` behavior).
struct BrokenPipeWriter;

impl std::io::Write for BrokenPipeWriter {
    fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "broken pipe",
        ))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Err(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "broken pipe",
        ))
    }
}

#[test]
fn guest_stdout_write_failure_yields_matchable_fs_error_io() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fs = std::fs::canonicalize(root.join("stdlib/fs.vibra")).unwrap();
    let result = std::fs::canonicalize(root.join("stdlib/result.vibra")).unwrap();
    let io = std::fs::canonicalize(root.join("stdlib/io.vibra")).unwrap();
    let test = std::fs::canonicalize(root.join("stdlib/test.vibra")).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"fs:
  $import: "{fs}"
result:
  $import: "{result}"
io:
  $import: "{io}"
test:
  $import: "{test}"
main:
  $function: $void
  return: $void
  do:
    - $let:
        r:
          $io.println: line that cannot be written
    - $match: $r
      when:
        - case:
            $result.result.ok: null
          do:
            - $test.fail: expected write failure but println returned ok
        - case:
            $result.result.err:
              $bind: e
          do:
            - $match: $e
              when:
                - case:
                    $fs.fs-error.io:
                      $bind: _msg
                  do:
                    - $test.assert: true
                - case: {{$wildcard: null}}
                  do:
                    - $test.fail: expected fs-error.io variant
"#,
            fs = path_str(&fs),
            result = path_str(&result),
            io = path_str(&io),
            test = path_str(&test),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).expect("broken-pipe io program should lower");

    // A failing stdout sink must NOT panic the runtime; instead the guest
    // should observe a matchable `fs-error.io(...)`. The guest asserts the
    // variant itself, so a clean `Ok(())` here proves the mapping worked.
    vibra::execute::run_lowered_with_io(
        &lowered,
        &vibra::runtime::RunConfig::default(),
        Box::new(BrokenPipeWriter),
        Box::new(BrokenPipeWriter),
    )
    .expect("guest should match fs-error.io rather than panic on broken pipe");
}

// --- Issue #47: cap user-controlled allocations (read-raw / random.bytes) ---

#[test]
fn checked_alloc_len_rejects_negative_length() {
    let config = vibra::runtime::RunConfig::default();
    let (tag, msg) = vibra::execute::checked_alloc_len(-1, &config).unwrap_err();
    assert_eq!(tag, "invalid-length");
    assert!(
        msg.contains("must not be negative"),
        "unexpected message: {msg}"
    );
}

#[test]
fn checked_alloc_len_rejects_over_limit_length() {
    let config = vibra::runtime::RunConfig {
        max_alloc_len: 8,
        ..vibra::runtime::RunConfig::default()
    };
    let (tag, msg) = vibra::execute::checked_alloc_len(9, &config).unwrap_err();
    assert_eq!(tag, "too-large");
    assert!(
        msg.contains("exceeds max-alloc-len"),
        "unexpected message: {msg}"
    );
}

#[test]
fn checked_alloc_len_accepts_in_bounds_length() {
    let config = vibra::runtime::RunConfig {
        max_alloc_len: 8,
        ..vibra::runtime::RunConfig::default()
    };
    assert_eq!(vibra::execute::checked_alloc_len(0, &config).unwrap(), 0);
    assert_eq!(vibra::execute::checked_alloc_len(8, &config).unwrap(), 8);
}

// NOTE: `read-raw` and `random.bytes` are not reachable from surface `.vibra`
// in the current codebase: their handlers gate on the legacy `=grants` grant
// token, which can only be seeded by a `main` grants block — and `main` grants
// blocks were removed in favor of `$policy` (see
// `main_function_grants_are_rejected_with_policy_migration_hint`). The shared
// allocation guard `checked_alloc_len` (exercised above) is the
// security-critical path both handlers funnel through, so it is covered at the
// unit level here.

#[test]
fn fs_open_handle_limit_is_enforced_and_freed_by_close() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fs = std::fs::canonicalize(root.join("stdlib/fs.vibra")).unwrap();
    let result = std::fs::canonicalize(root.join("stdlib/result.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    let c = dir.path().join("c.txt");

    // With `max_open_files: 2`, opening a third concurrent handle must fail with
    // the matchable `too-many-open-files` fs-error. Closing one handle frees a
    // slot so the subsequent reopen succeeds; only then is "freed-slot" written
    // to `c.txt`. If the cap were not enforced, the first open of `c` would
    // succeed (leaving the file empty) and the assertion below would fail.
    std::fs::write(
        &entry,
        format!(
            r#"fs:
  $import: "{fs}"
result:
  $import: "{result}"
main:
  $function:
    policy:
      $policy:
        fs-write:
          - requirement: mandatory
            scopes:
              - dir: "{dir}"
  return: $void
  do:
      - $let:
          pa:
            $fs.path.new: "{a}"
      - $let:
          pb:
            $fs.path.new: "{b}"
      - $let:
          pc:
            $fs.path.new: "{c}"
      - $let:
          oa:
            $fs.open-write: $pa
            policy: $args.policy
      - $match: $oa
        when:
            - case:
                $result.result.ok:
                  $bind: ha
              do:
                - $let:
                    ob:
                      $fs.open-write: $pb
                      policy: $args.policy
                - $match: $ob
                  when:
                      - case:
                          $result.result.ok:
                            $bind: hb
                        do:
                          - $let:
                              oc:
                                $fs.open-write: $pc
                                policy: $args.policy
                          - $match: $oc
                            when:
                                - case:
                                    $result.result.ok:
                                      $bind: hc-bad
                                  do: []
                                - case:
                                    $result.result.err:
                                      $bind: oc-err
                                  do:
                                    - $match: $oc-err
                                      when:
                                          - case:
                                              $fs.fs-error.too-many-open-files: null
                                            do:
                                              - $fs.closeable.close: $ha
                                              - $let:
                                                  oc2:
                                                    $fs.open-write: $pc
                                                    policy: $args.policy
                                              - $match: $oc2
                                                when:
                                                    - case:
                                                        $result.result.ok:
                                                          $bind: hc2
                                                      do:
                                                        - $fs.writable.write-string: $hc2
                                                          s: "freed-slot"
                                                        - $fs.closeable.close: $hc2
                                                    - case:
                                                        $result.result.err:
                                                          $bind: oc2-err
                                                      do: []
                                          - case:
                                              $wildcard: null
                                            do: []
                      - case:
                          $result.result.err:
                            $bind: ob-err
                        do: []
            - case:
                $result.result.err:
                  $bind: oa-err
              do: []
"#,
            fs = fs.display().to_string().replace('\\', "/"),
            result = result.display().to_string().replace('\\', "/"),
            dir = dir.path().display().to_string().replace('\\', "/"),
            a = a.display().to_string().replace('\\', "/"),
            b = b.display().to_string().replace('\\', "/"),
            c = c.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered =
        vibra::lower::lower_program(&prog).expect("open-handle-limit program should lower");
    vibra::execute::run_lowered(
        &lowered,
        &vibra::runtime::RunConfig {
            program_name: "vibra-test".to_string(),
            argv: Vec::new(),
            approved_policy: Some(vibra::lower::PolicyType {
                domains: std::collections::BTreeMap::from([(
                    "fs-write".to_string(),
                    vec![vibra::lower::PolicyGroup {
                        requirement: vibra::lower::GrantRequirement::Mandatory,
                        scopes: vec![vibra::lower::PolicyScope::Dir(
                            dir.path().display().to_string().replace('\\', "/"),
                        )],
                    }],
                )]),
            }),
            max_open_files: 2,
            ..vibra::runtime::RunConfig::default()
        },
    )
    .expect("open-handle-limit program should run");

    assert_eq!(
        std::fs::read_to_string(&c).unwrap(),
        "freed-slot",
        "third open should hit the cap with `too-many-open-files`, and closing a handle should free a slot for the reopen"
    );
}

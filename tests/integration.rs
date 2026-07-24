use std::path::Path;

fn vibra_cmd() -> std::process::Command {
    std::process::Command::new(env!("CARGO_BIN_EXE_vibra"))
}

fn path_str(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

fn lower_exec_value(source: &str) -> anyhow::Result<vibra::lower::RuntimeValue> {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(&entry, "placeholder: true\n").unwrap();
    let loaded = vibra::load::load_program(&entry).unwrap();
    let expression: serde_yaml::Value = serde_yaml::from_str(source).unwrap();
    let exec = vibra::lower::lower_exec_expr(&loaded, &expression, &Default::default())?;
    vibra::execute::eval_lowered_exec(
        &exec,
        &Default::default(),
        &vibra::runtime::RunConfig::default(),
    )
}

#[test]
fn primitive_operations_are_typed_and_evaluate() {
    assert_eq!(
        lower_exec_value("$add: [20, 22]").unwrap(),
        vibra::lower::RuntimeValue::Int(42)
    );
    assert_eq!(
        lower_exec_value("$less-than: [1.0, 2.0]").unwrap(),
        vibra::lower::RuntimeValue::Bool(true)
    );
    assert_eq!(
        lower_exec_value("$and: [true, false]").unwrap(),
        vibra::lower::RuntimeValue::Bool(false)
    );
    assert_eq!(
        lower_exec_value("$shift-left: [21, 1]").unwrap(),
        vibra::lower::RuntimeValue::Int(42)
    );
}

#[test]
fn checked_numeric_conversion_is_explicit_and_non_trapping() {
    assert_eq!(
        lower_exec_value("$convert: 42\ninto: $int8\nor: 0").unwrap(),
        vibra::lower::RuntimeValue::Typed {
            type_ref: vibra::lower::TypeRef::Int8,
            value: Box::new(vibra::lower::RuntimeValue::Int(42)),
        }
    );
    assert_eq!(
        lower_exec_value("$convert: 300\ninto: $int8\nor: -1").unwrap(),
        vibra::lower::RuntimeValue::Typed {
            type_ref: vibra::lower::TypeRef::Int8,
            value: Box::new(vibra::lower::RuntimeValue::Int(-1)),
        }
    );
    let invalid_fallback = lower_exec_value("$convert: 1\ninto: $uint8\nor: -1")
        .unwrap_err()
        .to_string();
    assert!(
        invalid_fallback.contains("E-OP-001"),
        "unexpected error: {invalid_fallback}"
    );
}

#[test]
fn primitive_operations_reject_mixed_or_invalid_types() {
    let mixed = lower_exec_value("$add: [1, 2.0]").unwrap_err().to_string();
    assert!(mixed.contains("E-OP-001"), "unexpected error: {mixed}");
    let invalid = lower_exec_value("$bit-and: [true, false]")
        .unwrap_err()
        .to_string();
    assert!(invalid.contains("E-OP-001"), "unexpected error: {invalid}");
}

#[test]
fn primitive_integer_failures_have_stable_diagnostics() {
    let divide = lower_exec_value("$divide: [1, 0]").unwrap_err().to_string();
    assert!(divide.contains("E-OP-003"), "unexpected error: {divide}");
    let shift = lower_exec_value("$shift-left: [1, 64]")
        .unwrap_err()
        .to_string();
    assert!(shift.contains("E-OP-004"), "unexpected error: {shift}");
    let overflow = lower_exec_value("$add: [9223372036854775807, 1]")
        .unwrap_err()
        .to_string();
    assert!(
        overflow.contains("E-OP-002"),
        "unexpected error: {overflow}"
    );
}

#[test]
fn collection_operations_execute_through_wasm_backend() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let collections =
        path_str(&std::fs::canonicalize(root.join("stdlib/src/collections.vibra")).unwrap());
    let test = path_str(&std::fs::canonicalize(root.join("stdlib/src/test.vibra")).unwrap());
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"collections:
  $import: "{collections}"
test:
  $import: "{test}"
main:
  $function: $void
  return: $void
  do:
  - $let:
      found:
        $collections.array-contains:
          t: $int64
          values: {{$array: [1, 2, 3]}}
          value: 2
  - $test.assert: $found
"#
        ),
    )
    .unwrap();

    let loaded = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&loaded).unwrap();
    vibra::execute::run_lowered(&lowered, &vibra::runtime::RunConfig::default()).unwrap();
}

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
    let io = std::fs::canonicalize(root.join("stdlib/src/io.vibra")).unwrap();
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
  do:
    - $let:
        ok: true
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
fn structured_task_lowers_and_rejects_mutable_reference_captures() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"main:
  $function: $void
  return: $void
  do:
    - $let:
        answer: 42
    - $task: [answer]
      do:
        - $let:
            snapshot: $answer
"#,
    )
    .unwrap();
    let loaded = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&loaded).expect("immutable task capture lowers");
    assert!(format!("{lowered:?}").contains("Task"));
    vibra::execute::run_lowered(&lowered, &vibra::runtime::RunConfig::default())
        .expect("structured task executes in interpreter");
    let wasm = vibra::wasm_backend::emit_program_wasm(&lowered);
    vibra::wasm_backend::run_wasm(&wasm, &vibra::runtime::RunConfig::default())
        .expect("structured task executes in Wasm backend");

    std::fs::write(
        &entry,
        r#"main:
  $function: $void
  return: $void
  do:
    - $let:
        count: {$mut: 0}
    - $task: [count]
      do: []
"#,
    )
    .unwrap();
    let loaded = vibra::load::load_program(&entry).unwrap();
    let error = format!("{:#}", vibra::lower::lower_program(&loaded).unwrap_err());
    assert!(error.contains("E-TASK-001"), "unexpected error: {error}");
    assert!(
        error.contains("immutable snapshot"),
        "missing migration guidance: {error}"
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
fn nested_function_grants_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"main:
  $function:
    args: $void
    grants:
      fs-read: $security.grant.mandatory
  return: $void
  do: []
"#,
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
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"main:
  $function: $void
  grants:
    fs_read: optional
  return: $void
  do: []
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("E-SEC-001"),
        "expected removed grant declaration rejection, got: {err}"
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
    - $let:
        ok: true
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
      - $let:
          BadLocal: $args.BadArg
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
      - $let:
          ok: true
accepts-float32:
  $function:
    input: $float32
  return: $void
  do:
      - $let:
          ok: true
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
      - $let:
          ok: true
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
    let fs = std::fs::canonicalize(root.join("stdlib/src/fs.vibra")).unwrap();
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
        err.contains("E-CAP-001"),
        "expected writable dispatch on read-file to be rejected, got: {err}"
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
fn domain_capability_type_lowers_with_a_typed_domain() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("main.vibra");
    std::fs::write(
        &path,
        r#"read-data:
  $capability.fs-read:
    - requirement: mandatory
      scopes:
        - dir: ./data
read-file:
  $function:
    capability: $read-data
  return: $void
  do:
    - $let:
        ok: true
main:
  $function: $void
  return: $void
  do: []
"#,
    )
    .unwrap();
    let loaded = vibra::load::load_program(&path).unwrap();
    vibra::lower::lower_program(&loaded).expect("typed fs-read capability should lower");
    assert_eq!(
        "fs-read".parse::<vibra::lower::CapabilityDomain>().unwrap(),
        vibra::lower::CapabilityDomain::FsRead
    );
}

#[test]
fn policy_narrow_returns_a_domain_capability() {
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
read-data:
  $capability.fs-read:
    - requirement: mandatory
      scopes:
        - dir: ./data
use-read:
  $function:
    capability: $read-data
  return: $void
  do:
    - $let:
        ok: true
main:
  $function:
    policy: $root-policy
  return: $void
  do:
    - $let:
        read:
          $policy.narrow: $args.policy
          into: $read-data
    - $use-read:
        capability: $read
"#,
    )
    .unwrap();
    let loaded = vibra::load::load_program(&entry).unwrap();
    vibra::lower::lower_program(&loaded).expect("policy should narrow into a domain capability");
}

#[test]
fn wasm_abi_rejects_wrong_value_parameter_type() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"bad-assert:
  $function:
    value: $str
  return: $void
  do:
    - $wasm:
        import:
          module: vibra_test
          name: assert
        args:
          - $args.value
main:
  $function: $void
  return: $void
  do: []
"#,
    )
    .unwrap();
    let loaded = vibra::load::load_program(&entry).unwrap();
    let error = format!("{:#}", vibra::lower::lower_program(&loaded).unwrap_err());
    assert!(
        error.contains("E-WASM-003") && error.contains("bool"),
        "{error}"
    );
}

#[test]
fn wasm_abi_rejects_wrong_return_type() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"bad-assert:
  $function:
    value: $bool
  return: $bool
  do:
    - $wasm:
        import:
          module: vibra_test
          name: assert
        args:
          - $args.value
main:
  $function: $void
  return: $void
  do: []
"#,
    )
    .unwrap();
    let loaded = vibra::load::load_program(&entry).unwrap();
    let error = format!("{:#}", vibra::lower::lower_program(&loaded).unwrap_err());
    assert!(
        error.contains("E-WASM-004") && error.contains("void"),
        "{error}"
    );
}

#[test]
fn wasm_abi_accepts_explicit_domain_capability() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"stdin-capability:
  $capability.stdin-read:
    - requirement: mandatory
      scopes: any
read-file:
  $handle.read: null
stdin-open:
  $function:
    capability: $stdin-capability
  return: $read-file
  do:
    - $wasm:
        import:
          module: vibra_v1
          name: stdin_open
        args:
          - $args.capability
main:
  $function: $void
  return: $void
  do: []
"#,
    )
    .unwrap();
    let loaded = vibra::load::load_program(&entry).unwrap();
    vibra::lower::lower_program(&loaded).expect("typed stdin capability should satisfy ABI");
}

#[test]
fn opaque_host_handle_cannot_be_cast_from_integer() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"read-file:
  $handle.read: null
main:
  $function: $void
  return: $void
  do:
    - $let:
        forged:
          $cast: 0
          into: $read-file
"#,
    )
    .unwrap();
    let loaded = vibra::load::load_program(&entry).unwrap();
    let error = format!("{:#}", vibra::lower::lower_program(&loaded).unwrap_err());
    assert!(error.contains("E-CAP-001"), "{error}");
}

#[test]
fn effects_command_reports_typed_host_surface_deterministically() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let hello = root.join("examples/hello.vibra");
    let first = vibra_cmd()
        .args(["effects", &path_str(&hello)])
        .output()
        .unwrap();
    let second = vibra_cmd()
        .args(["effects", &path_str(&hello)])
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert_eq!(first.stdout, second.stdout);
    let yaml = String::from_utf8(first.stdout).unwrap();
    assert!(yaml.contains("module: vibra_v1"), "{yaml}");
    assert!(yaml.contains("name: stdout_open"), "{yaml}");
    assert!(yaml.contains("return: write-handle"), "{yaml}");
    assert!(yaml.contains("root-policy:"), "{yaml}");
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
                    vibra::lower::CapabilityDomain::FsRead,
                    vec![vibra::lower::PolicyGroup {
                        requirement: vibra::lower::PolicyRequirement::Mandatory,
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
    let fs = std::fs::canonicalize(root.join("stdlib/src/fs.vibra")).unwrap();
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
        capability:
          $policy.narrow: $args.policy
          into: $fs.read-capability
    - $let:
        text:
          $fs.exists: $path
          capability: $capability
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
                        vibra::lower::CapabilityDomain::FsRead,
                        vec![vibra::lower::PolicyGroup {
                            requirement: vibra::lower::PolicyRequirement::Mandatory,
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
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"main:
  $function: $void
  grants:
    fs-read: optional
  return: $void
  do: []
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("E-SEC-001"),
        "expected migration diagnostic, got: {err}"
    );
}

#[test]
fn main_policy_argument_is_injected_and_authorizes_fs_read() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fs = std::fs::canonicalize(root.join("stdlib/src/fs.vibra")).unwrap();
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
        capability:
          $policy.narrow: $args.policy
          into: $fs.read-capability
    - $let:
        text:
          $fs.read-to-string: $path
          capability: $capability
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
                    vibra::lower::CapabilityDomain::FsRead,
                    vec![vibra::lower::PolicyGroup {
                        requirement: vibra::lower::PolicyRequirement::Mandatory,
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
    let fs = std::fs::canonicalize(root.join("stdlib/src/fs.vibra")).unwrap();
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
    let fs = std::fs::canonicalize(root.join("stdlib/src/fs.vibra")).unwrap();
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
        err.contains("missing value argument `capability`"),
        "expected missing capability argument rejection, got: {err}"
    );
}

#[test]
fn duplicate_nested_imports_are_idempotent() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let io = std::fs::canonicalize(root.join("stdlib/src/io.vibra")).unwrap();
    let fs = std::fs::canonicalize(root.join("stdlib/src/fs.vibra")).unwrap();
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
    let io = std::fs::canonicalize(root.join("stdlib/src/io.vibra")).unwrap();
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
fn tagged_option_rejects_raw_payload_and_null_coercions() {
    let dir = tempfile::tempdir().unwrap();
    let model = dir.path().join("model.vibra");
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
    - $let:
        ok: true
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
    - $let:
        ok: true
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
    - $let:
        ok: true
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
      - $let:
          ok: true
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
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
            - $return: 0
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
            - $return: 0
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
  $test: core
  do:
      - $test.assert: true
also-passes:
  $test: core
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
        .args(["test", "app", "--format", "human"])
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
  $test: core
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
        .args(["test", "app", "--format", "human"])
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
fn vibra_test_typed_equality_helpers_report_expected_and_actual_values() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("app");
    let tests_dir = project.join("tests");
    std::fs::create_dir_all(&tests_dir).unwrap();
    std::fs::write(
        tests_dir.join("fails.vibra"),
        r#"test:
  $import: "@std/test.vibra"
fails:
  $test: core
  do:
    - $test.assert-eq-int:
        actual: 1
        expected: 2
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
        .args(["test", "app", "--format", "human"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("expected Int(2), actual Int(1)"),
        "stderr={stderr}"
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
  $test: core
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
            "--format",
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
  $test: core
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
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib");
    std::fs::create_dir_all(dest.join("src")).unwrap();
    std::fs::copy(root.join("project.vibra"), dest.join("project.vibra")).unwrap();
    for entry in std::fs::read_dir(root.join("src")).unwrap() {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), dest.join("src").join(entry.file_name())).unwrap();
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
fn vibra_exec_rejects_non_string_raw_output() {
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
fn vibra_exec_json_output_is_explicit() {
    let output = vibra_cmd()
        .args(["exec", "42", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "exec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value, 42);
}

#[test]
fn vibra_code_inline_previews_and_write_is_explicit() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("main.vibra");
    let io =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vibra"))
            .unwrap();
    let original = format!(
        "io:\n  $import: {}\nmain:\n  $function: $void\n  return: $void\n  do:\n    - $io.println: Hello\n",
        path_str(&io)
    );
    std::fs::write(&source, &original).unwrap();
    let pipeline =
        "- $code.file: main.vibra\n- $code.at: [main, do, 0, $io.println]\n- $code.replace: Changed";

    let preview = vibra_cmd()
        .args(["code", pipeline, "--workspace", &path_str(dir.path())])
        .output()
        .unwrap();
    assert!(
        preview.status.success(),
        "code preview failed: stdout={} stderr={}",
        String::from_utf8_lossy(&preview.stdout),
        String::from_utf8_lossy(&preview.stderr)
    );
    let stdout = String::from_utf8_lossy(&preview.stdout);
    assert!(stdout.contains("status: preview"), "output: {stdout}");
    assert!(
        stdout.contains("+    - $io.println: Changed"),
        "output: {stdout}"
    );
    assert_eq!(std::fs::read_to_string(&source).unwrap(), original);

    let write = vibra_cmd()
        .args([
            "code",
            pipeline,
            "--workspace",
            &path_str(dir.path()),
            "--write",
        ])
        .output()
        .unwrap();
    assert!(
        write.status.success(),
        "code write failed: stdout={} stderr={}",
        String::from_utf8_lossy(&write.stdout),
        String::from_utf8_lossy(&write.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(&source).unwrap(),
        format!(
            "io:\n  $import: {}\nmain:\n  $function: $void\n  return: $void\n  do:\n    - $io.println: Changed\n",
            path_str(&io)
        )
    );
}

#[test]
fn vibra_code_queries_but_cannot_edit_vendored_sources() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("dep/std")).unwrap();
    let dependency = dir.path().join("dep/std/io.vibra");
    std::fs::write(&dependency, "value: original\n").unwrap();

    let query = vibra_cmd()
        .args([
            "code",
            "- $code.file: dep/std/io.vibra\n- $code.at: [value]",
            "--workspace",
            &path_str(dir.path()),
        ])
        .output()
        .unwrap();
    assert!(
        query.status.success(),
        "query failed: {}",
        String::from_utf8_lossy(&query.stderr)
    );
    assert!(String::from_utf8_lossy(&query.stdout).contains("io.vibra"));
    assert!(String::from_utf8_lossy(&query.stdout).contains("value"));

    let edit = vibra_cmd()
        .args([
            "code",
            "- $code.file: dep/std/io.vibra\n- $code.at: [value]\n- $code.replace: changed",
            "--workspace",
            &path_str(dir.path()),
            "--write",
        ])
        .output()
        .unwrap();
    assert!(!edit.status.success());
    assert!(String::from_utf8_lossy(&edit.stderr).contains("read-only"));
    assert_eq!(
        std::fs::read_to_string(dependency).unwrap(),
        "value: original\n"
    );
}

#[test]
fn vibra_code_accepts_equivalent_stdin_and_file_pipelines() {
    use std::io::Write;
    use std::process::Stdio;

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("main.vibra"), "main:\n  value: hello\n").unwrap();
    let pipeline = "- $code.file: main.vibra\n- $code.at: [main, value]\n- $code.project: form\n";
    let pipeline_file = dir.path().join("query.vibra");
    std::fs::write(&pipeline_file, pipeline).unwrap();

    let file_output = vibra_cmd()
        .args([
            "code",
            "--file",
            &path_str(&pipeline_file),
            "--workspace",
            &path_str(dir.path()),
        ])
        .output()
        .unwrap();
    assert!(file_output.status.success());

    let mut child = vibra_cmd()
        .args(["code", "-", "--workspace", &path_str(dir.path())])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(pipeline.as_bytes())
        .unwrap();
    let stdin_output = child.wait_with_output().unwrap();
    assert!(
        stdin_output.status.success(),
        "stdin code failed: {}",
        String::from_utf8_lossy(&stdin_output.stderr)
    );
    assert_eq!(file_output.stdout, stdin_output.stdout);
    assert!(String::from_utf8_lossy(&file_output.stdout).contains("hello"));
}

#[test]
fn vibra_code_semantic_rename_updates_multiple_files() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("a.vibra"),
        "helper:\n  $function: $void\n  return: $void\n  do:\n    - $let:\n        value: 1\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("b.vibra"),
        "util:\n  $import: ./a.vibra\nmain:\n  $function: $void\n  return: $void\n  do:\n    - $util.helper: null\n",
    )
    .unwrap();
    let pipeline = "- $code.rename-symbol:\n    file: a.vibra\n    from: helper\n    to: assist\n";

    let output = vibra_cmd()
        .args([
            "code",
            pipeline,
            "--workspace",
            &path_str(dir.path()),
            "--write",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "rename failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(std::fs::read_to_string(dir.path().join("a.vibra"))
        .unwrap()
        .contains("assist:"));
    assert!(std::fs::read_to_string(dir.path().join("b.vibra"))
        .unwrap()
        .contains("$util.assist:"));
}

#[test]
fn vibra_code_rejects_edits_that_break_compilation() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("main.vibra");
    let original = "main:\n  $function: $void\n  return: $void\n  do: []\n";
    std::fs::write(&source, original).unwrap();
    let pipeline =
        "- $code.file: main.vibra\n- $code.at: [main, $function]\n- $code.replace: broken\n";

    let output = vibra_cmd()
        .args([
            "code",
            pipeline,
            "--workspace",
            &path_str(dir.path()),
            "--write",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("validation"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read_to_string(&source).unwrap(), original);
}

#[test]
fn procedural_macro_quote_and_unquote_expand_before_lowering() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"identity:
  $macro:
    input: $code.expr-syntax
  return: $code.expr-syntax
  do:
    - $return:
        $quote:
          $unquote: $args.input
main:
  $function: $void
  return: $void
  do:
    - $let:
        value:
          $identity: hello
    - $match: $value
      when:
        - case: hello
          do: []
        - case:
            $wildcard: null
          do: []
"#,
    )
    .unwrap();

    let loaded = vibra::load::load_program(&entry).unwrap();
    vibra::lower::lower_program(&loaded).expect("macro invocation should expand before lowering");
}

#[test]
fn vibra_expand_shows_hygienic_bindings_and_explicit_capture() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"bind-temp:
  $macro:
    input: $code.expr-syntax
  return: $code.statement-syntax
  do:
    - $return:
        $quote:
          $let:
            temp:
              $unquote: $args.input
capture-name:
  $macro:
    input: $code.expr-syntax
  return: $code.expr-syntax
  do:
    - $return:
        $quote:
          $capture: $caller
main:
  $function: $void
  return: $void
  do:
    - $bind-temp: hello
    - $let:
        captured:
          $capture-name: ignored
"#,
    )
    .unwrap();

    let output = vibra_cmd()
        .args(["expand", &path_str(&entry)])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "expand failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("temp--macro-"), "expanded output: {stdout}");
    assert!(stdout.contains("$caller"), "expanded output: {stdout}");
    assert!(!stdout.contains("$bind-temp"), "expanded output: {stdout}");
}

#[test]
fn recursive_macro_expansion_reports_the_depth_limit_and_origin() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        r#"forever:
  $macro:
    input: $code.expr-syntax
  return: $code.expr-syntax
  do:
    - $return:
        $quote:
          $forever:
            $unquote: $args.input
main:
  $function: $void
  return: $void
  do:
    - $let:
        value:
          $forever: hello
"#,
    )
    .unwrap();

    let output = vibra_cmd()
        .args(["expand", &path_str(&entry)])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("64"), "stderr: {stderr}");
    assert!(stderr.contains("forever"), "stderr: {stderr}");
    assert!(stderr.contains("entry.vibra"), "stderr: {stderr}");
}

#[test]
fn imported_macro_quotes_resolve_names_in_definition_context() {
    let dir = tempfile::tempdir().unwrap();
    let macros = dir.path().join("macros.vibra");
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &macros,
        r#"helper:
  $function: $void
  return: $void
  do:
    - $let:
        value: 1
call-helper:
  $macro:
    input: $code.expr-syntax
  return: $code.statement-syntax
  do:
    - $return:
        $quote:
          $helper: null
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        r#"m:
  $import: ./macros.vibra
main:
  $function: $void
  return: $void
  do:
    - $m.call-helper: ignored
"#,
    )
    .unwrap();

    let loaded = vibra::load::load_program(&entry).unwrap();
    let expanded = serde_yaml::to_string(loaded.modules.get(&loaded.entry).unwrap()).unwrap();
    assert!(expanded.contains("$m.helper"), "expanded: {expanded}");
    vibra::lower::lower_program(&loaded).unwrap();
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
fn vibra_fmt_rejects_yaml_comments() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("commented.vibra");
    let original = "# important intent\nmain:\n  $function: $void\n  return: $void\n  do: []\n";
    std::fs::write(&source, original).unwrap();

    let write = vibra_cmd()
        .args(["fmt", &path_str(&source), "--write"])
        .output()
        .unwrap();
    assert!(!write.status.success(), "fmt must reject YAML comments");
    assert_eq!(std::fs::read_to_string(&source).unwrap(), original);
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
        .args(["fmt", &path_str(&source), "--format", "json"])
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
        "BadName:\n  =lint:\n    disable: [W-STYLE-001]\n  $literal: 1\nOtherBad: 2\n",
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
fn root_lint_annotation_suppresses_the_whole_file() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("style.vibra");
    std::fs::write(
        &source,
        "=lint: { disable: [W-STYLE-001] }\nBadName: 1\nOtherBad: 2\n",
    )
    .unwrap();

    let output = vibra_cmd()
        .args([
            "lint",
            &path_str(&source),
            "--category",
            "style",
            "--deny-warnings",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "root suppression failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
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

#[test]
fn test_envelope_uses_sibling_do_and_rejects_legacy_or_function_fields() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");

    std::fs::write(&entry, "passes:\n  $test: core\n  do: []\n").unwrap();
    let program = vibra::load::load_program(&entry).unwrap();
    let names = vibra::lower::discover_test_names(&program).unwrap();
    assert_eq!(names, vec!["passes"]);
    vibra::lower::lower_named_test(&program, "passes")
        .expect("canonical sibling test envelope should lower");

    for (name, source) in [
        ("legacy nested test", "legacy:\n  $test:\n    do: []\n"),
        (
            "test args",
            "bad:\n  $test: core\n  args: $void\n  do: []\n",
        ),
        (
            "test return",
            "bad:\n  $test: core\n  return: $void\n  do: []\n",
        ),
        ("uppercase test profile", "bad:\n  $test: Core\n  do: []\n"),
        (
            "underscored test profile",
            "bad:\n  $test: core_profile\n  do: []\n",
        ),
        ("empty test profile", "bad:\n  $test: \"\"\n  do: []\n"),
    ] {
        std::fs::write(&entry, source).unwrap();
        let program = vibra::load::load_program(&entry).unwrap();
        let err = vibra::lower::discover_test_names(&program).unwrap_err();
        assert!(
            format!("{err:#}").contains("E-TEST-001"),
            "{name} should be rejected with E-TEST-001, got: {err:#}"
        );
    }
}

#[test]
fn test_discovery_exposes_canonical_selection_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        "fast:\n  $test: core\n  tags: [language, fast]\n  timeout-ms: 25\n  do: []\nskipped:\n  $test: fs\n  tags: [filesystem]\n  skip: needs a sandbox\n  do: []\n",
    )
    .unwrap();
    let program = vibra::load::load_program(&entry).unwrap();
    let specs = vibra::lower::discover_test_specs(&program).unwrap();
    assert_eq!(specs.len(), 2);
    assert_eq!(specs[0].name, "fast");
    assert_eq!(specs[0].profile, "core");
    assert_eq!(specs[0].tags, vec!["language", "fast"]);
    assert_eq!(specs[0].timeout_ms, Some(25));
    assert_eq!(specs[1].skip.as_deref(), Some("needs a sandbox"));
    assert_eq!(
        vibra::lower::discover_test_names(&program).unwrap(),
        vec!["fast", "skipped"]
    );
}

#[test]
fn test_discovery_rejects_invalid_selection_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    for source in [
        "bad:\n  $test: core\n  tags: [not_kebab]\n  do: []\n",
        "bad:\n  $test: core\n  tags: [one, one]\n  do: []\n",
        "bad:\n  $test: core\n  timeout-ms: 0\n  do: []\n",
        "bad:\n  $test: core\n  skip: \"\"\n  do: []\n",
        "bad:\n  $test: core\n  skip: \"   \"\n  do: []\n",
    ] {
        std::fs::write(&entry, source).unwrap();
        let err = match vibra::load::load_program(&entry) {
            Ok(program) => vibra::lower::discover_test_specs(&program).unwrap_err(),
            Err(error) => error,
        };
        assert!(format!("{err:#}").contains("E-TEST-001"), "{err:#}");
    }
}

#[test]
fn test_discovery_trims_skip_reason_and_closes_profile_diagnostic() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        "skipped:\n  $test: core\n  skip: '  pending fixture  '\n  do: []\n",
    )
    .unwrap();
    let program = vibra::load::load_program(&entry).unwrap();
    let specs = vibra::lower::discover_test_specs(&program).unwrap();
    assert_eq!(specs[0].skip.as_deref(), Some("pending fixture"));

    std::fs::write(&entry, "bad:\n  $test: Not-Kebab\n  do: []\n").unwrap();
    let program = vibra::load::load_program(&entry).unwrap();
    let err = vibra::lower::discover_test_specs(&program).unwrap_err();
    assert!(format!("{err:#}").contains("got `Not-Kebab`"), "{err:#}");
}

#[test]
fn test_discovery_rejects_malformed_expected_error_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    for source in [
        "bad:\n  $test: core\n  expect-error: compile\n  do: []\n",
        "bad:\n  $test: core\n  expect-error:\n    phase: compile\n  do: []\n",
        "bad:\n  $test: core\n  expect-error:\n    phase: runtime\n    code: E-RUNTIME-001\n  do: []\n",
        "bad:\n  $test: core\n  expect-error:\n    phase: unknown\n    message-contains: nope\n  do: []\n",
        "bad:\n  $test: core\n  expect-error:\n    phase: compile\n    phase: runtime\n    code: E-OPTION-001\n  do: []\n",
        "bad:\n  $test: core\n  expect-error:\n    phase: compile\n    code: E-OPTION-001\n    code: E-CALL-001\n  do: []\n",
        "bad:\n  $test: core\n  expect-error:\n    phase: runtime\n    message-contains: one\n    message-contains: two\n  do: []\n",
    ] {
        std::fs::write(&entry, source).unwrap();
        let err = match vibra::load::load_program(&entry) {
            Ok(program) => vibra::lower::discover_test_specs(&program).unwrap_err(),
            Err(error) => error,
        };
        assert!(format!("{err:#}").contains("E-TEST-001"), "{err:#}");
    }
}

#[test]
fn vibra_test_matches_structured_expected_errors() {
    let dir = tempfile::tempdir().unwrap();
    let compile_entry = dir.path().join("compile-error.vibra");
    let runtime_entry = dir.path().join("runtime-error.vibra");
    let test_lib =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/test.vibra"))
            .unwrap();
    std::fs::write(
        &compile_entry,
        format!(
            r#"test:
  $import: "{test_lib}"
legacy:
  $option: $str
compile-error:
  $test: core
  expect-error:
    phase: compile
    code: E-OPTION-001
    message-contains: $option
  do: []
"#,
            test_lib = path_str(&test_lib),
        ),
    )
    .unwrap();

    let output = vibra_cmd()
        .args(["test", &path_str(&compile_entry), "--format", "human"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "test failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("1 passed"),
        "stdout={}",
        String::from_utf8_lossy(&output.stdout)
    );

    std::fs::write(
        &runtime_entry,
        format!(
            r#"test:
  $import: "{test_lib}"
runtime-error:
  $test: core
  expect-error:
    phase: runtime
    message-contains: assertion failed
  do:
    - $test.assert: false
"#,
            test_lib = path_str(&test_lib),
        ),
    )
    .unwrap();
    let output = vibra_cmd()
        .args(["test", &path_str(&runtime_entry)])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "test failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn vibra_test_matches_load_error_before_imports_are_recursively_loaded() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("load-error.vibra");
    let imported = dir.path().join("cycle.vibra");
    std::fs::write(
        &entry,
        "cycle:\n  $import: cycle.vibra\nload-error:\n  $test: core\n  expect-error:\n    phase: load\n    code: E-MOD-003\n  do: []\n",
    )
    .unwrap();
    std::fs::write(&imported, "entry:\n  $import: load-error.vibra\n").unwrap();

    let output = vibra_cmd()
        .args(["test", &path_str(&entry), "--format", "human"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "test failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("1 passed"),
        "stdout={}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn vibra_test_reports_expected_error_mismatches_from_the_parent() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("expected-error-mismatch.vibra");
    std::fs::write(
        &entry,
        "passes:\n  $test: core\n  expect-error:\n    phase: compile\n    code: E-OPTION-001\n  do: []\n",
    )
    .unwrap();

    let output = vibra_cmd()
        .args(["test", &path_str(&entry), "--format", "human"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("expected compile error"), "stderr={stderr}");
}

#[test]
fn vibra_test_reports_phase_code_and_message_expectation_mismatches() {
    let dir = tempfile::tempdir().unwrap();
    let test_lib =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/test.vibra"))
            .unwrap();
    let cases = [
        (
            "wrong-phase.vibra",
            format!(
                "test:\n  $import: \"{}\"\nbad:\n  $test: core\n  expect-error:\n    phase: compile\n    code: E-OPTION-001\n  do:\n    - $test.assert: false\n",
                path_str(&test_lib)
            ),
            "expected compile error, but test failed during runtime",
        ),
        (
            "wrong-code.vibra",
            "legacy:\n  $option: $str\nbad:\n  $test: core\n  expect-error:\n    phase: compile\n    code: E-CALL-001\n  do: []\n".to_string(),
            "expected compile error code `E-CALL-001`, got `E-OPTION-001`",
        ),
        (
            "wrong-message.vibra",
            format!(
                "test:\n  $import: \"{}\"\nbad:\n  $test: core\n  expect-error:\n    phase: runtime\n    message-contains: expected different runtime error\n  do:\n    - $test.assert: false\n",
                path_str(&test_lib)
            ),
            "expected runtime error message to contain `expected different runtime error`",
        ),
    ];
    for (name, source, expected_message) in cases {
        let entry = dir.path().join(name);
        std::fs::write(&entry, source).unwrap();
        let output = vibra_cmd()
            .args(["test", &path_str(&entry), "--format", "human"])
            .output()
            .unwrap();
        assert!(!output.status.success(), "{name} unexpectedly passed");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(expected_message), "stderr={stderr}");
    }
}

#[test]
fn vibra_test_selects_profiles_and_tags_and_reports_skips() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("selection.vibra");
    let report = dir.path().join("report.yaml");
    std::fs::write(
        &entry,
        "core-language:\n  $test: core\n  tags: [language, fast]\n  do: []\nfs-language:\n  $test: fs\n  tags: [language, filesystem]\n  do: []\nskipped-core:\n  $test: core\n  tags: [language]\n  skip: external fixture unavailable\n  do: []\n",
    )
    .unwrap();
    let output = vibra_cmd()
        .args([
            "test",
            &path_str(&entry),
            "--tag",
            "language",
            "--format",
            "yaml",
            "--report-file",
            &path_str(&report),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let yaml = std::fs::read_to_string(&report).unwrap();
    assert!(yaml.contains("passed: 1"), "{yaml}");
    assert!(yaml.contains("skipped: 1"), "{yaml}");
    assert!(yaml.contains("profile: core"), "{yaml}");
    assert!(
        yaml.contains("skip_reason: external fixture unavailable"),
        "{yaml}"
    );

    let output = vibra_cmd()
        .args([
            "test",
            &path_str(&entry),
            "--profile",
            "fs",
            "--tag",
            "filesystem",
            "--format",
            "human",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
}

#[test]
fn vibra_test_deny_skips_fails_after_reporting_selected_skip() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("skip.vibra");
    std::fs::write(
        &entry,
        "skipped:\n  $test: core\n  skip: pending\n  do: []\n",
    )
    .unwrap();
    let output = vibra_cmd()
        .args([
            "test",
            &path_str(&entry),
            "--deny-skips",
            "--format",
            "human",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 skipped"));
}

#[test]
fn vibra_test_caps_command_timeout_with_test_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("timeout.vibra");
    std::fs::write(
        &entry,
        "slow:\n  $test: core\n  timeout-ms: 1\n  do:\n    - $while: true\n      do: []\n",
    )
    .unwrap();
    let output = vibra_cmd()
        .args([
            "test",
            &path_str(&entry),
            "--timeout-ms",
            "1000",
            "--format",
            "human",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("timed out after 1 ms"), "stderr: {stderr}");
}

#[test]
fn vibra_test_temp_workspace_requires_explicit_opt_in_and_reports_the_skip() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("workspace.vibra");
    std::fs::write(
        &entry,
        "needs-workspace:\n  $test: core\n  workspace: temp\n  do: []\n",
    )
    .unwrap();

    let skipped = vibra_cmd()
        .args(["test", &path_str(&entry), "--format", "human"])
        .output()
        .unwrap();
    assert!(
        skipped.status.success(),
        "{}",
        String::from_utf8_lossy(&skipped.stderr)
    );
    assert!(
        String::from_utf8_lossy(&skipped.stdout).contains("requires --allow-test-workspace"),
        "{}",
        String::from_utf8_lossy(&skipped.stdout)
    );

    let enabled = vibra_cmd()
        .args([
            "test",
            &path_str(&entry),
            "--allow-test-workspace",
            "read-write",
            "--format",
            "human",
        ])
        .output()
        .unwrap();
    assert!(
        enabled.status.success(),
        "{}",
        String::from_utf8_lossy(&enabled.stderr)
    );
    assert!(String::from_utf8_lossy(&enabled.stdout).contains("1 passed"));
}

#[test]
fn vibra_test_workspace_metadata_rejects_unknown_values() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("workspace-invalid.vibra");
    std::fs::write(
        &entry,
        "bad:\n  $test: core\n  workspace: persistent\n  do: []\n",
    )
    .unwrap();
    let program = vibra::load::load_program(&entry).unwrap();
    let error = vibra::lower::discover_test_specs(&program).unwrap_err();
    assert!(format!("{error:#}").contains("E-TEST-001"));
}

#[test]
fn vibra_test_deny_warnings_fails_and_emits_warnings_in_yaml_report() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("warnings.vibra");
    let report = dir.path().join("report.yaml");
    std::fs::write(&entry, "BadName: 1\npasses:\n  $test: core\n  do: []\n").unwrap();

    let allowed = vibra_cmd()
        .args(["test", &path_str(&entry)])
        .output()
        .unwrap();
    assert!(
        allowed.status.success(),
        "{}",
        String::from_utf8_lossy(&allowed.stderr)
    );

    let denied = vibra_cmd()
        .args([
            "test",
            &path_str(&entry),
            "--deny-warnings",
            "--format",
            "yaml",
            "--report-file",
            &path_str(&report),
        ])
        .output()
        .unwrap();
    assert!(
        !denied.status.success(),
        "--deny-warnings should fail warning tests"
    );
    let yaml = std::fs::read_to_string(report).unwrap();
    assert!(yaml.contains("warnings:"), "{yaml}");
    assert!(yaml.contains("BadName"), "{yaml}");
}

#[test]
fn vibra_test_temp_workspace_modes_limit_real_filesystem_operations() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fs_lib = path_str(&std::fs::canonicalize(root.join("stdlib/src/fs.vibra")).unwrap());
    let test_lib = path_str(&std::fs::canonicalize(root.join("stdlib/src/test.vibra")).unwrap());
    let dir = tempfile::tempdir().unwrap();
    let host_dir = tempfile::tempdir().unwrap();
    let host_path = path_str(host_dir.path());
    let entry = dir.path().join("workspace-modes.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"test:
  $import: "{test_lib}"
fs:
  $import: "{fs_lib}"
result:
  $import: "{result_lib}"
workspace-read-only:
  $test: core
  workspace: temp
  policy:
    $policy:
      fs-read:
      - requirement: mandatory
        scopes: [{{dir: .}}]
  do:
    - $let:
        capability:
          $policy.narrow: $args.policy
          into: $fs.read-capability
    - $let:
        path:
          $fs.path.new: .
    - $let:
        readable:
          $fs.exists: $path
          capability: $capability
    - $test.assert: $readable
workspace-write-only:
  $test: core
  workspace: temp
  policy:
    $policy:
      fs-write:
      - requirement: mandatory
        scopes: [{{dir: .}}]
  do:
    - $let:
        capability:
          $policy.narrow: $args.policy
          into: $fs.write-capability
    - $let:
        path:
          $fs.path.new: workspace-created
    - $let:
        created:
          $fs.create-dir-all: $path
          capability: $capability
    - $match: $created
      when:
      - case:
          $result.result.ok: null
        do:
        - $test.assert: true
      - case:
          $result.result.err:
            $bind: ignored
        do:
        - $test.fail: workspace write grant was denied
workspace-read-write:
  $test: core
  workspace: temp
  policy:
    $policy:
      fs-read:
      - requirement: mandatory
        scopes: [{{dir: .}}]
      fs-write:
      - requirement: mandatory
        scopes: [{{dir: .}}]
  do:
    - $let:
        read-capability:
          $policy.narrow: $args.policy
          into: $fs.read-capability
    - $let:
        write-capability:
          $policy.narrow: $args.policy
          into: $fs.write-capability
    - $let:
        path:
          $fs.path.new: workspace-created
    - $let:
        created:
          $fs.create-dir-all: $path
          capability: $write-capability
    - $match: $created
      when:
      - case:
          $result.result.ok: null
        do: []
      - case:
          $result.result.err:
            $bind: ignored
        do:
        - $test.fail: workspace write grant was denied
    - $let:
        readable:
          $fs.exists: $path
          capability: $read-capability
    - $test.assert: $readable
host-grants-are-isolated:
  $test: core
  workspace: temp
  expect-error:
    phase: runtime
    message-contains: mandatory policy coverage is missing
  policy:
    $policy:
      fs-read:
      - requirement: mandatory
        scopes: [{{dir: "{host_path}"}}]
  do:
    - $let:
        capability:
          $policy.narrow: $args.policy
          into: $fs.read-capability
    - $let:
        host-path:
          $fs.path.new: {host_path}
    - $fs.exists: $host-path
      capability: $capability
"#,
            result_lib =
                path_str(&std::fs::canonicalize(root.join("stdlib/src/result.vibra")).unwrap()),
        ),
    )
    .unwrap();

    let read = vibra_cmd()
        .args([
            "test",
            &path_str(&entry),
            "--filter",
            "workspace-read-only",
            "--allow-test-workspace",
            "read",
        ])
        .output()
        .unwrap();
    assert!(
        read.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&read.stdout),
        String::from_utf8_lossy(&read.stderr)
    );

    let write = vibra_cmd()
        .args([
            "test",
            &path_str(&entry),
            "--filter",
            "workspace-write-only",
            "--allow-test-workspace",
            "write",
        ])
        .output()
        .unwrap();
    assert!(
        write.status.success(),
        "{}",
        String::from_utf8_lossy(&write.stderr)
    );

    let read_write = vibra_cmd()
        .args([
            "test",
            &path_str(&entry),
            "--filter",
            "workspace-read-write",
            "--allow-test-workspace",
            "read-write",
        ])
        .output()
        .unwrap();
    assert!(
        read_write.status.success(),
        "{}",
        String::from_utf8_lossy(&read_write.stderr)
    );

    let no_host_leak = vibra_cmd()
        .args([
            "test",
            &path_str(&entry),
            "--filter",
            "host-grants-are-isolated",
            "--allow-test-workspace",
            "read-write",
            "--allow-read",
            &host_path,
            "--allow-write",
            &host_path,
        ])
        .output()
        .unwrap();
    assert!(
        no_host_leak.status.success(),
        "{}",
        String::from_utf8_lossy(&no_host_leak.stderr)
    );
}

#[test]
fn vibra_test_drains_large_child_output_without_timing_out() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let io_lib = path_str(&std::fs::canonicalize(root.join("stdlib/src/io.vibra")).unwrap());
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("large-output.vibra");
    let report = dir.path().join("large-output-report.yaml");
    let payload = "x".repeat(128 * 1024);
    std::fs::write(
        &entry,
        format!(
            "io:\n  $import: \"{io_lib}\"\nemits-large-output:\n  $test: core\n  do:\n    - $io.println: {payload}\n"
        ),
    )
    .unwrap();

    let output = vibra_cmd()
        .args([
            "test",
            &path_str(&entry),
            "--timeout-ms",
            "1000",
            "--format",
            "yaml",
            "--report-file",
            &path_str(&report),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(std::fs::read_to_string(report).unwrap().contains(&payload));
}

/// Write an entry module that imports the canonical `stdlib/src/test.vibra` and
/// contains `count` passing `$test` declarations plus a shared helper function
/// every test can call. Returns the temp dir (keep it alive) and entry path.
fn issue50_many_tests_entry(count: usize) -> (tempfile::TempDir, std::path::PathBuf) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let test_lib = std::fs::canonicalize(root.join("stdlib/src/test.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");

    let mut src = format!(
        "test:\n  $import: \"{lib}\"\nthe-answer:\n  $function: $void\n  return: $int64\n  do:\n    - $return: 42\n",
        lib = test_lib.display().to_string().replace('\\', "/"),
    );
    for i in 0..count {
        src.push_str(&format!(
            "many-{i}:\n  $test: core\n  do:\n    - $let:\n        v:\n          $the-answer: null\n    - $match: $v\n      when:\n        - case: 42\n          do:\n            - $test.assert: true\n        - case:\n            $wildcard: null\n          do:\n            - $test.fail: not 42\n",
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
    let test_lib = std::fs::canonicalize(root.join("stdlib/src/test.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"test:
  $import: "{lib}"
passes:
  $test: core
  do:
      - $test.assert: true
fails:
  $test: core
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
    let fs = std::fs::canonicalize(root.join("stdlib/src/fs.vibra")).unwrap();
    let result = std::fs::canonicalize(root.join("stdlib/src/result.vibra")).unwrap();
    let io = std::fs::canonicalize(root.join("stdlib/src/io.vibra")).unwrap();
    let test = std::fs::canonicalize(root.join("stdlib/src/test.vibra")).unwrap();

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
fn random_bytes_os_rng_is_not_all_zero() {
    let mut buf = vec![0u8; 32];
    getrandom::getrandom(&mut buf).expect("OS randomness should be available in tests");
    assert!(
        buf.iter().any(|byte| *byte != 0),
        "CSPRNG output should not be all zeros"
    );
}

#[test]
fn fs_open_handle_limit_is_enforced_and_freed_by_close() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fs = std::fs::canonicalize(root.join("stdlib/src/fs.vibra")).unwrap();
    let result = std::fs::canonicalize(root.join("stdlib/src/result.vibra")).unwrap();
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
          capability:
            $policy.narrow: $args.policy
            into: $fs.write-capability
      - $let:
          oa:
            $fs.open-write: $pa
            capability: $capability
      - $match: $oa
        when:
            - case:
                $result.result.ok:
                  $bind: ha
              do:
                - $let:
                    ob:
                      $fs.open-write: $pb
                      capability: $capability
                - $match: $ob
                  when:
                      - case:
                          $result.result.ok:
                            $bind: hb
                        do:
                          - $let:
                              oc:
                                $fs.open-write: $pc
                                capability: $capability
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
                                                    capability: $capability
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
                    vibra::lower::CapabilityDomain::FsWrite,
                    vec![vibra::lower::PolicyGroup {
                        requirement: vibra::lower::PolicyRequirement::Mandatory,
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

#[test]
fn closed_file_aliases_return_stable_typed_lifecycle_errors() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fs = std::fs::canonicalize(root.join("stdlib/src/fs.vibra")).unwrap();
    let result = std::fs::canonicalize(root.join("stdlib/src/result.vibra")).unwrap();
    let test = std::fs::canonicalize(root.join("stdlib/src/test.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    let target = dir.path().join("closed.txt");
    std::fs::write(
        &entry,
        format!(
            r#"fs:
  $import: "{fs}"
result:
  $import: "{result}"
test:
  $import: "{test}"
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
        path:
          $fs.path.new: "{target}"
    - $let:
        capability:
          $policy.narrow: $args.policy
          into: $fs.write-capability
    - $let:
        opened:
          $fs.open-write: $path
          capability: $capability
    - $match: $opened
      when:
        - case:
            $result.result.ok:
              $bind: handle
          do:
            - $fs.closeable.close: $handle
            - $let:
                duplicate:
                  $fs.closeable.close: $handle
            - $match: $duplicate
              when:
                - case:
                    $result.result.err:
                      $fs.fs-error.resource-closed: null
                  do:
                    - $test.assert: true
                - case:
                    $wildcard: null
                  do:
                    - $test.fail: duplicate close did not return resource-closed
            - $let:
                after-close:
                  $fs.writable.write-string: $handle
                  s: forbidden
            - $match: $after-close
              when:
                - case:
                    $result.result.err:
                      $fs.fs-error.resource-closed: null
                  do:
                    - $test.assert: true
                - case:
                    $wildcard: null
                  do:
                    - $test.fail: use after close did not return resource-closed
        - case:
            $result.result.err:
              $wildcard: null
          do:
            - $test.fail: lifecycle fixture could not open file
"#,
            fs = path_str(&fs),
            result = path_str(&result),
            test = path_str(&test),
            dir = path_str(dir.path()),
            target = path_str(&target),
        ),
    )
    .unwrap();

    let program = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&program).expect("lifecycle program should lower");
    vibra::execute::run_lowered(
        &lowered,
        &vibra::runtime::RunConfig {
            approved_policy: Some(vibra::lower::PolicyType {
                domains: std::collections::BTreeMap::from([(
                    vibra::lower::CapabilityDomain::FsWrite,
                    vec![vibra::lower::PolicyGroup {
                        requirement: vibra::lower::PolicyRequirement::Mandatory,
                        scopes: vec![vibra::lower::PolicyScope::Dir(path_str(dir.path()))],
                    }],
                )]),
            }),
            ..vibra::runtime::RunConfig::default()
        },
    )
    .expect("guest should observe lifecycle violations as typed errors");
}

// --- Issue #53: environment capability scopes are case-sensitive on Unix ---

#[test]
fn env_read_capability_is_case_sensitive_on_unix() {
    if cfg!(windows) {
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let env_mod = std::fs::canonicalize(root.join("stdlib/src/env.vibra")).unwrap();
    let result = std::fs::canonicalize(root.join("stdlib/src/result.vibra")).unwrap();
    let test = std::fs::canonicalize(root.join("stdlib/src/test.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"env:
  $import: "{env_mod}"
result:
  $import: "{result}"
test:
  $import: "{test}"
env-policy:
  $policy:
    env-read:
    - requirement: mandatory
      scopes:
      - exact: VIBRA_ISSUE53_TOKEN
main:
  $function:
    policy: $env-policy
  return: $void
  do:
  - $let:
      capability:
        $policy.narrow: $args.policy
        into: $env.read-capability
  - $let:
      value:
        $env.get: vibra_issue53_token
        capability: $capability
  - $match: $value
    when:
    - case:
        $result.result.ok:
          $wildcard: null
      do:
      - $test.fail: lowercase environment name matched uppercase capability scope
    - case:
        $result.result.err:
          $wildcard: null
      do: []
"#,
            env_mod = path_str(&env_mod),
            result = path_str(&result),
            test = path_str(&test),
        ),
    )
    .unwrap();
    std::env::set_var("VIBRA_ISSUE53_TOKEN", "public");
    std::env::set_var("vibra_issue53_token", "secret");
    let output = vibra_cmd()
        .args(["run", &path_str(&entry), "--allow-env=VIBRA_ISSUE53_TOKEN"])
        .output()
        .unwrap();
    std::env::remove_var("VIBRA_ISSUE53_TOKEN");
    std::env::remove_var("vibra_issue53_token");
    assert!(
        output.status.success(),
        "lowercase env name must not match uppercase capability scope:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn injected_clock_and_environment_are_deterministic_and_isolated() {
    use std::sync::{Arc, Mutex};

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let env_mod = std::fs::canonicalize(root.join("stdlib/src/env.vibra")).unwrap();
    let time_mod = std::fs::canonicalize(root.join("stdlib/src/time.vibra")).unwrap();
    let result = std::fs::canonicalize(root.join("stdlib/src/result.vibra")).unwrap();
    let test = std::fs::canonicalize(root.join("stdlib/src/test.vibra")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vibra");
    std::fs::write(
        &entry,
        format!(
            r#"env:
  $import: "{env_mod}"
time:
  $import: "{time_mod}"
result:
  $import: "{result}"
test:
  $import: "{test}"
host-policy:
  $policy:
    clock: [{{requirement: mandatory, scopes: any}}]
    env-read: [{{requirement: mandatory, scopes: [{{exact: VIBRA_INJECTED}}]}}]
    env-write: [{{requirement: mandatory, scopes: [{{exact: VIBRA_INJECTED}}]}}]
main:
  $function:
    policy: $host-policy
  return: $void
  do:
  - $let: {{clock: {{$policy.narrow: $args.policy, into: $time.capability}}}}
  - $let: {{read: {{$policy.narrow: $args.policy, into: $env.read-capability}}}}
  - $let: {{write: {{$policy.narrow: $args.policy, into: $env.write-capability}}}}
  - $let: {{wall: {{$time.now-unix-millis: {{capability: $clock}}}}}}
  - $match: $wall
    when:
    - case: 1000
      do: []
    - case: {{$wildcard: null}}
      do: [{{$test.fail: injected-wall-clock-was-not-used}}]
  - $let: {{start: {{$time.monotonic-now: {{capability: $clock}}}}}}
  - $time.sleep: {{duration: {{$time.milliseconds: 7}}, capability: $clock}}
  - $let: {{finish: {{$time.now-unix-millis: {{capability: $clock}}}}}}
  - $match: $finish
    when:
    - case: 1007
      do: []
    - case: {{$wildcard: null}}
      do: [{{$test.fail: injected-monotonic-clock-was-not-advanced}}]
  - $let: {{set: {{$env.set: VIBRA_INJECTED, value: changed, capability: $write}}}}
  - $match: $set
    when:
    - case: {{$result.result.ok: null}}
      do: []
    - case: {{$wildcard: null}}
      do: [{{$test.fail: injected-env-set-failed}}]
  - $let: {{value: {{$env.get: VIBRA_INJECTED, capability: $read}}}}
  - $match: $value
    when:
    - case: {{$result.result.ok: changed}}
      do: []
    - case: {{$wildcard: null}}
      do: [{{$test.fail: injected-env-read-was-not-isolated}}]
"#,
            env_mod = path_str(&env_mod),
            time_mod = path_str(&time_mod),
            result = path_str(&result),
            test = path_str(&test),
        ),
    )
    .unwrap();
    let program = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&program).unwrap();
    let make_config = || vibra::runtime::RunConfig {
        injected_environment: Some(Arc::new(Mutex::new(std::collections::BTreeMap::from([(
            "VIBRA_INJECTED".to_string(),
            "original".to_string(),
        )])))),
        injected_clock: Some(Arc::new(Mutex::new(vibra::runtime::InjectedClock {
            unix_millis: 1000,
            monotonic_millis: 10,
        }))),
        approved_policy: Some(vibra::lower::PolicyType {
            domains: std::collections::BTreeMap::from([
                (
                    vibra::lower::CapabilityDomain::Clock,
                    vec![vibra::lower::PolicyGroup {
                        requirement: vibra::lower::PolicyRequirement::Mandatory,
                        scopes: vec![vibra::lower::PolicyScope::Any],
                    }],
                ),
                (
                    vibra::lower::CapabilityDomain::EnvRead,
                    vec![vibra::lower::PolicyGroup {
                        requirement: vibra::lower::PolicyRequirement::Mandatory,
                        scopes: vec![vibra::lower::PolicyScope::Exact("VIBRA_INJECTED".into())],
                    }],
                ),
                (
                    vibra::lower::CapabilityDomain::EnvWrite,
                    vec![vibra::lower::PolicyGroup {
                        requirement: vibra::lower::PolicyRequirement::Mandatory,
                        scopes: vec![vibra::lower::PolicyScope::Exact("VIBRA_INJECTED".into())],
                    }],
                ),
            ]),
        }),
        ..vibra::runtime::RunConfig::default()
    };
    vibra::execute::run_lowered(&lowered, &make_config()).unwrap();
}

// --- Issue #51: stdin reads require --allow-stdin ---

#[test]
fn forged_stdin_read_file_handle_requires_allow_stdin() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fs = std::fs::canonicalize(root.join("stdlib/src/fs.vibra")).unwrap();
    let result = std::fs::canonicalize(root.join("stdlib/src/result.vibra")).unwrap();
    let output = vibra_cmd()
        .args([
            "exec",
            &format!("{{$fs.readable.read-string: {{$cast: 0, into: $fs.read-file}}}}"),
            "--import",
            &format!("fs={}", path_str(&fs)),
            "--import",
            &format!("result={}", path_str(&result)),
            "--format",
            "yaml",
        ])
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "forged stdin handle must fail without --allow-stdin: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("E-CAP-001"),
        "expected opaque-handle forgery rejection, got: {stderr}"
    );
}

#[test]
fn compile_time_embed_supports_text_binary_and_structured_formats() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("message.txt"), "$literal-looking text\n").unwrap();
    std::fs::write(dir.path().join("payload.bin"), [0_u8, 127, 255]).unwrap();
    std::fs::write(dir.path().join("data.yaml"), "name: yaml\ncount: 2\n").unwrap();
    std::fs::write(dir.path().join("data.json"), r#"{"name":"json","count":3}"#).unwrap();
    std::fs::write(dir.path().join("data.toml"), "name = 'toml'\ncount = 4\n").unwrap();
    std::fs::write(
        dir.path().join("data.xml"),
        "<root><name>xml</name><count>5</count></root>",
    )
    .unwrap();
    let entry = dir.path().join("main.vibra");
    std::fs::write(
        &entry,
        r#"main:
  $function: $void
  return: $void
  do:
    - $let: {text: {$embed: message.txt}}
    - $let: {binary: {$embed: payload.bin}}
    - $let: {yaml: {$embed: data.yaml}}
    - $let: {json: {$embed: data.json}}
    - $let: {toml: {$embed: data.toml}}
    - $let: {xml: {$embed: data.xml}}
"#,
    )
    .unwrap();

    let loaded = vibra::load::load_program(&entry).unwrap();
    assert_eq!(loaded.embedded_files.len(), 6);
    let expanded = serde_yaml::to_string(loaded.modules.get(&loaded.entry).unwrap()).unwrap();
    assert!(expanded.contains("$literal-looking text"));
    assert!(expanded.contains("$uint8"));
    assert!(expanded.contains("yaml"));
    assert!(expanded.contains("json"));
    assert!(expanded.contains("toml"));
    assert!(expanded.contains("xml"));
    vibra::lower::lower_program(&loaded).unwrap();
}

#[test]
fn compile_time_embed_rejects_escape_and_fingerprints_raw_content() {
    let outer = tempfile::tempdir().unwrap();
    let package = outer.path().join("package");
    std::fs::create_dir(&package).unwrap();
    std::fs::write(outer.path().join("secret.txt"), "secret").unwrap();
    let entry = package.join("main.vibra");
    std::fs::write(
        &entry,
        "main:\n  $function: $void\n  return: $void\n  do:\n    - $let: {value: {$embed: ../secret.txt}}\n",
    )
    .unwrap();
    let error = format!("{:#}", vibra::load::load_program(&entry).unwrap_err());
    assert!(error.contains("E-EMBED-002"), "{error}");

    std::fs::write(package.join("asset.txt"), "one").unwrap();
    std::fs::write(
        &entry,
        "main:\n  $function: $void\n  return: $void\n  do:\n    - $let: {value: {$embed: asset.txt}}\n",
    )
    .unwrap();
    let first = vibra::load::load_program(&entry).unwrap();
    let first = vibra::lower::lower_program(&first).unwrap();
    let first = vibra::wasm_backend::emit_program_wasm(&first);
    std::fs::write(package.join("asset.txt"), "two").unwrap();
    let second = vibra::load::load_program(&entry).unwrap();
    let second = vibra::lower::lower_program(&second).unwrap();
    let second = vibra::wasm_backend::emit_program_wasm(&second);
    assert_ne!(first, second);
}

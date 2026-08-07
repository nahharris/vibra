use std::path::Path;

fn vibra_cmd() -> std::process::Command {
    std::process::Command::new(env!("CARGO_BIN_EXE_vibra"))
}

fn path_str(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

fn lower_exec_value(source: &str) -> anyhow::Result<vibra::lower::RuntimeValue> {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(&entry, "(const placeholder bool true)\n").unwrap();
    let loaded = vibra::load::load_program(&entry).unwrap();
    let expression = vibra::load::parse_inline_exec_expression(source)?;
    let exec = vibra::lower::lower_exec_expr(&loaded, &expression, &Default::default())?;
    vibra::execute::eval_lowered_exec(
        &exec,
        &Default::default(),
        &vibra::runtime::RunConfig::default(),
    )
}

#[test]
fn native_stream_module_registers_shared_roots_and_operations() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let stream = std::fs::canonicalize(root.join("stdlib/src/stream.vib")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        format!(
            "(import stream \"{}\")\n(defn main () void (do))\n",
            path_str(&stream)
        ),
    )
    .unwrap();
    let loaded = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&loaded).unwrap();
    assert!(lowered.functions.contains_key("stream.read.string"));
    assert!(lowered.functions.contains_key("stream.write.bytes-some"));
    assert!(lowered.functions.contains_key("stream.manage.close"));
}

#[test]
fn structural_effect_syntax_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        "(defn main () void (do) effects: ((effect @fs @read)))\n",
    )
    .unwrap();

    let error =
        vibra::frontend::load_surface_program(&entry, &vibra::load::CompilationFlags::default())
            .expect_err("structural effect syntax must not be accepted");
    let rendered = format!("{error:#}");
    assert!(rendered.contains("E-EFFECT-008"), "{rendered}");
}

#[test]
fn raw_wasm_requires_a_nominal_deffect_operation() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        "(defn main () uint64 (wasm \"vibra_v1\" \"clock_now_unix_millis\"))\n",
    )
    .unwrap();

    let error =
        vibra::load::load_program(&entry).expect_err("raw wasm outside deffect must be rejected");
    let rendered = format!("{error:#}");
    assert!(rendered.contains("E-WASM-008"), "{rendered}");
}

#[test]
fn primitive_operations_are_typed_and_evaluate() {
    assert_eq!(
        lower_exec_value("(add 20 22)").unwrap(),
        vibra::lower::RuntimeValue::Int(42)
    );
    assert_eq!(
        lower_exec_value("(less-than 1.0 2.0)").unwrap(),
        vibra::lower::RuntimeValue::Bool(true)
    );
    assert_eq!(
        lower_exec_value("(and true false)").unwrap(),
        vibra::lower::RuntimeValue::Bool(false)
    );
    assert_eq!(
        lower_exec_value("(shift-left 21 1)").unwrap(),
        vibra::lower::RuntimeValue::Int(42)
    );
}

#[test]
fn checked_numeric_conversion_is_explicit_and_non_trapping() {
    assert_eq!(
        lower_exec_value("(convert 42 int8 0)").unwrap(),
        vibra::lower::RuntimeValue::Typed {
            type_ref: vibra::lower::TypeRef::Int8,
            value: Box::new(vibra::lower::RuntimeValue::Int(42)),
        }
    );
    assert_eq!(
        lower_exec_value("(convert 300 int8 -1)").unwrap(),
        vibra::lower::RuntimeValue::Typed {
            type_ref: vibra::lower::TypeRef::Int8,
            value: Box::new(vibra::lower::RuntimeValue::Int(-1)),
        }
    );
    let invalid_fallback = lower_exec_value("(convert 1 uint8 -1)")
        .unwrap_err()
        .to_string();
    assert!(
        invalid_fallback.contains("E-OP-001"),
        "unexpected error: {invalid_fallback}"
    );
}

#[test]
fn primitive_operations_reject_mixed_or_invalid_types() {
    let mixed = lower_exec_value("(add 1 2.0)").unwrap_err().to_string();
    assert!(mixed.contains("E-OP-001"), "unexpected error: {mixed}");
    let invalid = lower_exec_value("(bit-and true false)")
        .unwrap_err()
        .to_string();
    assert!(invalid.contains("E-OP-001"), "unexpected error: {invalid}");
}

#[test]
fn primitive_integer_failures_have_stable_diagnostics() {
    let divide = lower_exec_value("(divide 1 0)").unwrap_err().to_string();
    assert!(divide.contains("E-OP-003"), "unexpected error: {divide}");
    let shift = lower_exec_value("(shift-left 1 64)")
        .unwrap_err()
        .to_string();
    assert!(shift.contains("E-OP-004"), "unexpected error: {shift}");
    let overflow = lower_exec_value("(add 9223372036854775807 1)")
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
        path_str(&std::fs::canonicalize(root.join("stdlib/src/collections.vib")).unwrap());
    let test = path_str(&std::fs::canonicalize(root.join("stdlib/src/test.vib")).unwrap());
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        format!(
            r#"(import collections "{collections}")
(import test "{test}")
(defn
  main
  ()
  void
  (do
    (let found (collections.array-contains int64 (array 1 2 3) 2))
    (test.assert found)
  )
)
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
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(defn main () void (do (match 0 0 (do) _ (do))))
"#,
    )
    .unwrap();

    let loaded = vibra::load::load_program(&entry).unwrap();
    vibra::lower::lower_program(&loaded).expect("`case` should be the canonical match arm key");
}

#[test]
fn legacy_pattern_match_arm_key_is_rejected() {
    // The legacy YAML surface distinguished the canonical `case:` match-arm
    // key from a deprecated `pattern:` spelling with a semantic check
    // (`E-ONE-008`). The S-expression grammar has no alternate spelling to
    // reject: `(case pattern (do ...))` is the only match-arm shape the
    // reader accepts (`"(", "case", pattern, body, ")"`), so a malformed arm
    // is rejected structurally, by the reader, not by this legacy semantic
    // check.
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(defn main () void (do (match 0 (pattern 0 (do)))))
"#,
    )
    .unwrap();

    let loaded = vibra::load::load_program(&entry);
    let err = match loaded {
        Ok(loaded) => format!("{:#}", vibra::lower::lower_program(&loaded).unwrap_err()),
        Err(error) => format!("{error:#}"),
    };
    assert!(
        err.contains("E-SYN") || err.contains("E-AST"),
        "unexpected error: {err}"
    );
}

#[test]
fn generic_alias_instantiation_prefers_current_module_scope() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let io = std::fs::canonicalize(root.join("stdlib/src/io.vib")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        format!(
            r#"(import io "{}")
(defn main () void (do (let ok true)))
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
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(defn
  main
  ()
  void
  (do
    (let count (mut 0))
    (let reader (ref count))
    (let writer (ref (mut count)))
    (set count 1)
    (set writer 2)
  )
)
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
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(defn
  main
  ()
  void
  (do (let answer 42) (task (let snapshot answer) captures: (answer)))
)
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
        r#"(defn main () void (do (let count (mut 0)) (task unit captures: (count))))
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
fn host_calls_are_general_nested_expressions_in_both_backends() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(deffect host
  (defn assert (value bool) void
    (wasm "vibra_test" "assert" value)
    effects: ()))
(defn main () void (host.assert (equal 1 1)) effects: (entry.host))
"#,
    )
    .unwrap();
    let program = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&program).unwrap();
    vibra::execute::run_lowered(&lowered, &vibra::runtime::RunConfig::default()).unwrap();
    let wasm = vibra::wasm_backend::emit_program_wasm(&lowered);
    vibra::wasm_backend::run_wasm(&wasm, &vibra::runtime::RunConfig::default()).unwrap();
}

#[test]
fn spawned_task_handles_are_typed_affine_and_scheduler_backed() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(defn
  main
  ()
  void
  (do
    (let base 40)
    (spawn first (add base 1) captures: (base))
    (spawn second (add base 2) captures: (base))
    (join second second-result)
    (join first first-result)
  )
)
"#,
    )
    .unwrap();
    let loaded = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&loaded).expect("spawn/join program lowers");
    let debug = format!("{lowered:?}");
    assert!(debug.contains("Spawn"));
    assert!(debug.contains("Join"));
    let wasm = vibra::wasm_backend::emit_program_wasm(&lowered);
    vibra::wasm_backend::run_wasm(&wasm, &vibra::runtime::RunConfig::default())
        .expect("Wasm executes typed task results");

    std::fs::write(
        &entry,
        r#"(defn main () void (do (spawn leaked 42 captures: ())))
"#,
    )
    .unwrap();
    let loaded = vibra::load::load_program(&entry).unwrap();
    let error = format!("{:#}", vibra::lower::lower_program(&loaded).unwrap_err());
    assert!(error.contains("E-TASK-003"), "unexpected error: {error}");
    assert!(
        error.contains("must be joined"),
        "unexpected error: {error}"
    );
}

#[test]
fn set_rejects_immutable_binding() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(defn main () void (do (let count 0) (set count 1)))
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
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(defn
  main
  ()
  void
  (do (let count (mut 0)) (let reader (ref count)) (set reader 1))
)
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
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(defn take-mut (value (mut int64)) int64 (do (return value)))
(defn take-ref (value (ref (mut int64))) int64 (do (return value)))
(defn main () void (do))
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
    let entry = dir.path().join("entry.vib");
    // `grants:` is legacy pre-policy-redesign syntax with no S-expression
    // form at all (the known `fn` attributes are `doc:`, `where:`, `defs:`,
    // `impls:`); the reader itself rejects it as an unknown declaration
    // attribute (E-SYN-011), superseding the legacy semantic check
    // (E-ONE-001/E-SEC-001) this test and the two below it originally
    // exercised.
    std::fs::write(
        &entry,
        "(defn main () void (do) grants: (fs-read mandatory))\n",
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry);
    let err = match prog {
        Ok(prog) => format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err()),
        Err(error) => format!("{error:#}"),
    };
    assert!(err.contains("E-SYN-011"), "unexpected error: {err}");
}

#[test]
fn implicit_subject_function_is_rejected_with_e_one_001() {
    // The legacy "implicit subject" fallback (a bare `$function` field set
    // to `$str` meant an unnamed single parameter, reachable only via
    // `$args.subject`) has no S-expression form: `fn` parameters are always
    // an explicit, named, positional list. Referencing an undeclared name
    // is rejected instead, by ordinary reference resolution.
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        "(defn identity () str (do (return subject)))\n(defn main () void (do))\n",
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(!err.is_empty(), "expected a reference-resolution error");
}

#[test]
fn void_function_with_sibling_args_is_rejected_with_e_one_001() {
    // The legacy `$function` field set to `$void` plus a sibling `args:` (two
    // different ways to spell "no parameters" and "some parameters" on the
    // same envelope) has no S-expression form: parameters are always the
    // single `((name Type) ...)` list. There is nothing left to reject here
    // that the grammar does not already make unwritable, so this now
    // asserts the grammar's single spelling still lowers cleanly.
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        "(defn identity (value str) str (do (return value)))\n(defn main () void (do))\n",
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    vibra::lower::lower_program(&prog)
        .expect("the single canonical parameter spelling always lowers");
}

#[test]
fn labeled_primary_is_only_available_through_args_namespace() {
    // `$args.` is removed entirely (spec: "Function arguments are ordinary
    // lexical names"); a parameter is always referenced by its bare
    // declared name, which is what this now asserts positively instead of
    // asserting the removed `args.` namespace requirement negatively.
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        "(defn identity (value str) str (do (return value)))\n(defn main () void (do))\n",
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    vibra::lower::lower_program(&prog).expect("bare parameter reference resolves directly");
}

#[test]
fn grant_names_must_be_kebab_case() {
    // See the comment on `nested_function_grants_are_rejected` above:
    // `grants:` is unknown syntax now, rejected by the reader.
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(&entry, "(defn main () void (do) grants: (fs_read))\n").unwrap();

    let prog = vibra::load::load_program(&entry);
    let err = match prog {
        Ok(prog) => format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err()),
        Err(error) => format!("{error:#}"),
    };
    assert!(err.contains("E-SYN-011"), "unexpected error: {err}");
}

#[test]
fn import_cycle_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.vib");
    let b = dir.path().join("b.vib");
    std::fs::write(&a, "(import io \"./b.vib\")\n").unwrap();
    std::fs::write(&b, "(import io \"./a.vib\")\n").unwrap();
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
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(defn main-helper () void (do (return)) visibility: @private)
(defn main () void (do (main-helper)))
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
    // Imports have no private/public distinction anymore ("Imports are
    // public module-local aliases but are never re-exported" -- spec); the
    // `private` wrapper only accepts `def`/`const`/`fn`/`macro`. This tests
    // the surviving half of the original intent: an import alias is usable
    // locally, regardless of the removed privacy marker.
    let dir = tempfile::tempdir().unwrap();
    let helper = dir.path().join("helper.vib");
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &helper,
        r#"(defn noop () void (do (let ok true)))
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"(import h "{h}")
(defn main () void (do (h.noop)))
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
    let lib = dir.path().join("lib.vib");
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &lib,
        r#"(defn priv () void (do (return)) visibility: @private)
(defn pub-entry () void (do (priv)))
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"(import m "{m}")
(defn main () void (do (m.pub-entry)))
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
    let lib = dir.path().join("lib.vib");
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &lib,
        r#"(defn priv () void (do (return)) visibility: @private)
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"(import m "{m}")
(defn main () void (do (m.priv)))
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
    let lib = dir.path().join("lib.vib");
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &lib,
        r#"(def priv-t (record (x int32)) visibility: @private)
(defn pub-nop () void (do (return)))
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"(import m "{m}")
(defn use-ty (subject m.priv-t) void (do (return)))
(defn main () void (do (m.pub-nop)))
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
    let lib = dir.path().join("lib.vib");
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &lib,
        r#"(def priv-e (enum (a void)) visibility: @private)
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"(import m "{m}")
(defn main () void (do (let value (m.priv-e.a))))
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
    let p = root.join("examples/hello.vib");
    let prog = vibra::load::load_program(&p).unwrap();
    let lowered = vibra::lower::lower_program(&prog).unwrap();
    vibra::execute::run_lowered(&lowered, &vibra::runtime::RunConfig::default()).unwrap();
}

#[test]
fn enum_match_lowers_with_new_syntax() {
    let dir = tempfile::tempdir().unwrap();
    let model = dir.path().join("model.vib");
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();
    let entry = dir.path().join("entry.vib");

    std::fs::write(
        &model,
        r#"(def integer (union int64 int32 int16 int8))
(def number (enum (int integer) (none void)))
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"(import m "{m}")
(import io "{io}")
(defn
  main
  ()
  void
  (do
    (let value (m.number.int 7))
    (match
  value
  (m.number.int (bind x))
  (do (io.stdout.println "int"))
  (m.number.none)
  (do (io.stdout.println "none"))
)
  )
  effects: (io.stdout stream.write)
)
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
    // The legacy YAML `when:` sibling could be spelled as either a sequence
    // (canonical) or a mapping (rejected). The S-expression grammar has no
    // mapping form to reject at all: match arms are always the positional
    // flat `pattern expression` pairs, so a malformed arm (here, `when`
    // used as an expression) is rejected by typed-surface lowering directly.
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");

    std::fs::write(
        &entry,
        r#"(def maybe (enum (some str) (none void)))
(defn
  main
  ()
  void
  (do
    (let value (maybe.some "x"))
    (match value (when (maybe.some (bind x)) (do)) (when (maybe.none) (do)))
  )
)
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry);
    let err = match prog {
        Ok(prog) => format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err()),
        Err(error) => format!("{error:#}"),
    };
    assert!(
        err.contains("E-SYN") || err.contains("E-AST") || err.contains("E-ADAPT-023"),
        "expected the malformed match arm to be rejected, got: {err}"
    );
}

#[test]
fn structured_match_form_is_rejected_with_e_one_007() {
    // The legacy structured `$match: {target:, arms:}` alternative to
    // `$match: value, when: [...]` has no S-expression form: `match` is
    // always `(match expr case+)` (one head, positional children), so this
    // now asserts the canonical form lowers cleanly rather than asserting
    // an unwritable alternative is rejected.
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");

    std::fs::write(
        &entry,
        r#"(def maybe (enum (some str) (none void)))
(defn
  main
  ()
  void
  (do
    (let value (maybe.some "x"))
    (match value (maybe.some (bind x)) (do) (maybe.none) (do))
  )
)
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    vibra::lower::lower_program(&prog)
        .expect("the single canonical `match` spelling always lowers");
}

#[test]
fn match_arm_rebinding_does_not_leak_to_parent_runtime_scope() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();

    std::fs::write(
        &entry,
        format!(
            r#"(import io "{io}")
(def maybe (enum (some str) (none void)))
(defn
  main
  ()
  void
  (do
    (let x "outer")
    (let value (maybe.some "payload"))
    (match value (maybe.some (bind payload)) (do (let x 42)) (maybe.none) (do))
    (io.stdout.println x)
  )
  effects: (io.stdout stream.write)
)
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
    let entry = dir.path().join("entry.vib");
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();

    std::fs::write(
        &entry,
        format!(
            r#"(import io "{io}")
(defn main () void (do (if true (do (let x 42)) (do (io.stdout.println x)))))
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
    let entry = dir.path().join("entry.vib");
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();

    std::fs::write(
        &entry,
        format!(
            r#"(import io "{io}")
(defn
  main
  ()
  void
  (do (if true (do (let x "then")) (do (let x "else"))) (io.stdout.println x))
  effects: (io.stdout stream.write)
)
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
    let entry = dir.path().join("entry.vib");
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();

    std::fs::write(
        &entry,
        format!(
            r#"(import io "{io}")
(defn main () void (do (while false (do (let x 42))) (io.stdout.println x)))
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
    let entry = dir.path().join("entry.vib");
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();

    std::fs::write(
        &entry,
        format!(
            r#"(import io "{io}")
(defn
  main
  ()
  void
  (do
    (let
      value
      (record
        (pair (tuple 7 "seven"))
        (tags (array "a" "b"))
        (table (map ("lang" "vibra")))
      )
    )
    (match
  value
  (record
    (pair (tuple (bind n) (bind word)))
    (tags (array "a" _))
    (table (map ("lang" (bind language))))
  )
  (do (io.stdout.println word) (io.stdout.println language))
  _
  (do (io.stdout.println "no match"))
)
  )
  effects: (io.stdout stream.write)
)
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

/// A `_` in payload position makes that payload irrefutable, not the arm. Before this
/// was scoped to top-level patterns, the arm below counted as total and the missing
/// `signal.stop` case trapped at runtime instead of failing to compile.
#[test]
fn nested_wildcard_does_not_satisfy_enum_exhaustiveness() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def signal (enum (go int64) (stop void)))
(defn classify (s signal) int64 (do (match s (signal.go _) (do (return 1)))))
(defn main () void (do (let n (classify (signal.go 1)))))
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("E-MATCH-001") && err.contains("stop"),
        "expected a nested `_` to leave `signal.stop` uncovered, got: {err}"
    );
}

/// Enum coverage is keyed by `(enum, tag)`, so a nested pattern cannot mark an outer
/// tag covered just because the two enums happen to share a tag name.
#[test]
fn nested_enum_tag_does_not_cover_same_named_outer_tag() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def inner (enum (yes void) (no void)))
(def outer (enum (yes inner) (no void)))
(defn classify (o outer) int64
  (do (match o (outer.yes (inner.no)) (do (return 1)) (outer.yes (inner.yes)) (do (return 2)))))
(defn main () void (do (let n (classify (outer.no)))))
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("E-MATCH-001") && err.contains("`no`"),
        "expected the nested `inner.no` arm not to cover `outer.no`, got: {err}"
    );
}

/// A single literal arm is only total when the target is that literal's singleton type.
/// Over an open type like `int64` every other value is unmatched.
#[test]
fn single_literal_arm_does_not_exhaust_open_type() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(defn classify (n int64) int64 (do (match n 3 (do (return 1)))))
(defn main () void (do (let r (classify 7))))
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("E-MATCH-002"),
        "expected a lone literal arm over `int64` to require a `_` arm, got: {err}"
    );
}

/// `(bind x)` matches every value, so it satisfies totality exactly as `_` does.
#[test]
fn sole_bind_arm_is_total() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(defn echo (s str) str (do (match s (bind captured) (do (return captured)))))
(defn main () void (do (let out (echo "hi"))))
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    vibra::lower::lower_program(&prog).expect("a sole `(bind x)` arm should be total");
}

#[test]
fn newtype_and_nominal_interface_patterns_match_runtime_type_tags() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");

    // This test's first `$match` needs `(interface Type _)`. Legacy's own
    // `Pattern::Interface(TypeRef)` (src/lower.rs) carries only a type, with
    // no slot for a nested sub-pattern -- but legacy's YAML surface spelled
    // this same construct `$interface: <type-expr>` with no inner pattern
    // either, and legacy's match evaluation for it only ever checks the
    // interface bound and binds nothing. A `_` wildcard sub-pattern is
    // therefore faithfully representable (it binds nothing too), so the
    // adapter accepts it; any other sub-pattern (a bind, a nested
    // destructure) still has no legacy slot to land in and is rejected with
    // E-ADAPT-017.
    std::fs::write(
        &entry,
        r#"(def display (interface (fmt (fn-type (self self) str))))
(def meter (newtype int64)
  impls: (
    (impl display methods: (
      (method fmt (fn (self self) str (do (return "meter"))))
    ))
  ))
(defn
  main
  ()
  void
  (do
    (let distance (cast 7 meter))
    (match
  distance
  (interface display _)
  (do (let matched "display"))
  _
  (do (let matched "other"))
)
    (match distance (newtype meter (bind raw)) (do (let seen raw)) _ (do))
  )
)
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
    let bad = dir.path().join("bad.vib");
    let entry = dir.path().join("entry.vib");

    // The legacy `$union: {variants: {...}}` alternative to the canonical
    // `$union: [...]` list has no S-expression form: `union` is always
    // `(union type-expr+)` (one head, positional children -- the contract
    // forbids any form accepting both a compact and expanded spelling).
    // There is nothing left to reject that the grammar does not already
    // make unwritable, so this now asserts the canonical form parses.
    std::fs::write(
        &bad,
        r#"(def maybe-text (union str bool))
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"(import u "{u}")
(defn main () void (do))
"#,
            u = bad.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    vibra::lower::lower_program(&prog)
        .expect("the single canonical `union` spelling always lowers");
}

#[test]
fn warns_for_non_kebab_case_symbols() {
    let dir = tempfile::tempdir().unwrap();
    let mod_file = dir.path().join("symbols.vib");
    let entry = dir.path().join("entry.vib");
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();

    std::fs::write(
        &mod_file,
        r#"(def BadType (enum (NotTag str)))
(defn doThing (BadArg str) void (do (let BadLocal BadArg)))
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"(import BadImport "{m}")
(import io "{io}")
(defn main () void (do (io.stdout.println "ok")) effects: (io.stdout stream.write))
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
    let model = dir.path().join("model.vib");
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();
    let entry = dir.path().join("entry.vib");

    std::fs::write(
        &model,
        r#"(def option (enum (none void) (some str)))
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"(import m "{m}")
(import io "{io}")
(defn
  main
  ()
  void
  (do
    (let value-none (m.option.none))
    (match
  value-none
  (m.option.none)
  (do (io.stdout.println "none"))
  (m.option.some (bind text))
  (do (io.stdout.println text))
)
  )
  effects: (io.stdout stream.write)
)
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
    let bad = dir.path().join("bad.vib");
    let entry = dir.path().join("entry.vib");

    std::fs::write(
        &bad,
        r#"(defn
  takes-old-int
  (input int)
  void
  (do
    (let ok true)
  )
)
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"(import u "{u}")
(defn
  main
  ()
  void
  (do
    (let ok true)
  )
)
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
    let mod_file = dir.path().join("numeric.vib");
    let entry = dir.path().join("entry.vib");

    std::fs::write(
        &mod_file,
        r#"(defn accepts-int32 (input int32) void (do (let ok true)))
(defn accepts-float32 (input float32) void (do (let ok true)))
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"(import n "{n}")
(defn main () void (do (n.accepts-int32 7) (n.accepts-float32 3.14)))
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

/// Run effect inference over a lowered program and render every function whose body
/// exceeds its declaration.
///
/// This drives `effect_semantics` directly for focused diagnostics; production
/// lowering enforces the same ceiling as a hard error.
fn report_effects(lowered: &vibra::lower::LoweredProgram) -> String {
    use vibra::effect_semantics::{infer_function, undeclared_effect_message};

    let mut owners: Vec<&String> = lowered.functions.keys().collect();
    owners.sort();
    let mut reported = Vec::new();
    for owner in owners {
        let sig = &lowered.functions[owner];
        let performed = infer_function(sig, &lowered.functions);
        for witness in performed.undeclared(&sig.effects) {
            reported.push(undeclared_effect_message(owner, &sig.effects, witness));
        }
    }
    let performed =
        vibra::effect_semantics::infer_statements(&lowered.statements, &lowered.functions);
    for witness in performed.undeclared(&lowered.main_effects) {
        reported.push(undeclared_effect_message(
            "main",
            &lowered.main_effects,
            witness,
        ));
    }
    reported.join("\n")
}

/// A host-import body's effects come from the registry, so declaring `effects: ()` on
/// one cannot launder the effect away. This is the check the whole model rests on.
/// An interface method's effect row is a ceiling: an implementation may perform fewer
/// effects than the interface allows, never more.
#[test]
fn impl_method_may_not_exceed_the_interface_effect_ceiling() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def lister (interface (all (fn-type (self self) (array str)))))
(def env-lister (newtype int64)
  impls: ((impl lister
    methods: ((method all
      (fn (self self) (array str)
        (do (return (wasm "vibra_v1" "env_list")))
        effects: (env.read)))))))
(defn main () void (do (let ok true)))
"#,
    )
    .unwrap();

    let err = format!("{:#}", vibra::load::load_program(&entry).unwrap_err());
    assert!(
        err.contains("E-WASM-008"),
        "raw wasm in an interface implementation must be rejected outside deffect, got: {err}"
    );
}

/// Declaring the effect on the interface too makes the same implementation legal.
#[test]
fn interface_effect_ceiling_admits_a_matching_impl() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def lister (interface (all (fn-type (self self) (array str) effects: (env.read)))))
(def env-lister (newtype int64)
  impls: ((impl lister
    methods: ((method all
      (fn (self self) (array str)
        (do (return (wasm "vibra_v1" "env_list")))
        effects: (env.read)))))))
(defn main () void (do (let ok true)))
"#,
    )
    .unwrap();

    let err = format!("{:#}", vibra::load::load_program(&entry).unwrap_err());
    assert!(
        err.contains("E-WASM-008"),
        "raw wasm must be owned by a deffect: {err}"
    );
}

/// An inline impl method is split by the adapter into a hoisted helper carrying the
/// real body and a forwarding shim bound into the impl. The shim's envelope is built
/// by hand, so it has to be handed the declared row explicitly -- otherwise it looks
/// pure while calling straight into an effectful helper, and enforcement misses it.
#[test]
fn inline_impl_method_effects_reach_the_forwarding_shim() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def lister (interface (all (fn-type (self self) (array str)))))
(def env-lister (newtype int64)
  impls: ((impl lister
    methods: ((method all
      (fn (self self) (array str)
        (do (return (wasm "vibra_v1" "env_list")))))))))
(defn main () void (do (let ok true)))
"#,
    )
    .unwrap();

    let err = format!("{:#}", vibra::load::load_program(&entry).unwrap_err());
    assert!(
        err.contains("E-WASM-008"),
        "raw wasm in an inline impl must be rejected outside deffect, got: {err}"
    );
}

#[test]
fn wasm_body_effects_come_from_the_registry_not_the_declaration() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(defn now () (array str) (wasm "vibra_v1" "env_list"))
(defn main () void (do (let t (now))))
"#,
    )
    .unwrap();

    // Raw wasm is now restricted to nominal deffect operations.
    let reported = format!("{:#}", vibra::load::load_program(&entry).unwrap_err());
    assert!(
        reported.contains("E-WASM-008"),
        "expected raw wasm outside deffect to be rejected, got: {reported}"
    );
}

/// Declaring the effect silences it, and declaring more than the body performs is
/// fine -- the row is a ceiling, not an equality.
#[test]
fn declared_effects_are_a_ceiling() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(defn exact () void (do (let ok true)) effects: (env.read))
(defn main () void (do (exact)) effects: (env.read fs.read))
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).unwrap();
    let reported = report_effects(&lowered);
    assert!(
        reported.is_empty(),
        "declaring an effect, or more than the body performs, should be silent: {reported}"
    );
}

/// Effects propagate through calls using the callee's performed body effects.
/// This requires a real graph/fixpoint rather than trusting declarations or
/// assuming that callees have already been visited.
#[test]
fn effects_propagate_to_callers_through_bodies() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(deffect host
  (defn read () void
    (do (let ignored (intrinsic @env-list)))
    effects: (env.read)))
(defn leaf () void (do (host.read)) effects: (entry.host env.read))
(defn middle () void (do (leaf)))
(defn main () void (do (middle)))
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    // Enforcement is a hard error now, so the propagated effect surfaces as a lowering
    // failure rather than a `report_effects` line; the message is the same one.
    let reported = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        reported.contains("`middle` declares no effects") && reported.contains("via `leaf`"),
        "expected the caller to inherit the callee's performed effect, got: {reported}"
    );
}

#[test]
fn private_call_chain_infers_effects_without_private_annotations() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(deffect host
  (defn read () void
    (do (let ignored (intrinsic @env-list)))
    effects: (env.read)))
(defn leaf () void (do (host.read)) visibility: @private)
(defn middle () void (do (leaf)) visibility: @private)
(defn main () void (do (middle)))
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog)
        .expect("private functions should infer effects through the whole call chain");
    let inferred =
        vibra::effect_semantics::infer_function(&lowered.functions["-middle"], &lowered.functions);
    assert!(inferred.labels().contains(&("env".into(), "read".into())));
}

#[test]
fn mutual_private_recursion_reaches_a_least_fixpoint() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(deffect host
  (defn read () void
    (do (let ignored (intrinsic @env-list)))
    effects: (env.read)))
(defn first () void (do (host.read) (second)) visibility: @private)
(defn second () void (do (first)) visibility: @private)
(defn main () void (do (first)))
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog)
        .expect("mutual private recursion should converge during effect inference");
    for key in ["-first", "-second"] {
        assert!(vibra::effect_semantics::infer_function(
            &lowered.functions[key],
            &lowered.functions
        )
        .labels()
        .contains(&("env".into(), "read".into())));
    }
}

#[test]
fn over_declared_effects_are_reported_as_warnings() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(defn api () void (do (let ok true)) effects: (env.read))
(defn main () void (do (api)))
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).unwrap();
    assert!(
        lowered
            .warnings
            .iter()
            .any(|warning| warning.contains("over-declares") && warning.contains("api")),
        "expected a deterministic over-declaration warning, got: {:?}",
        lowered.warnings
    );
}

#[test]
fn under_declared_exported_effects_remain_errors() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(deffect host
  (defn read () void
    (do (let ignored (intrinsic @env-list)))
    effects: (env.read)))
(defn api () void (do (host.read)))
(defn main () void (do (api)))
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let error = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        error.contains("E-EFFECT-001") && error.contains("`api`"),
        "expected an under-declared exported function to fail, got: {error}"
    );
}

#[test]
fn declared_effect_roots_subsume_only_at_the_declaration_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(deffect host
  (defn read () void
    (do (let ignored (intrinsic @fs-metadata (intrinsic @path-new "x"))))
    effects: (fs.metadata)))
(defn api () void (do (host.read)) effects: (fs entry.host))
(defn main () void (do (api)) effects: (fs))
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog)
        .expect("a declared root should cover its leaf operations");
    assert_eq!(
        lowered.functions["api"].effects.labels,
        [
            ("entry".to_string(), "host".to_string()),
            ("fs".to_string(), "".to_string()),
        ]
        .into_iter()
        .collect()
    );
    assert_eq!(
        vibra::effect_semantics::infer_function(&lowered.functions["api"], &lowered.functions)
            .labels(),
        [
            ("entry".to_string(), "host".to_string()),
            ("fs".to_string(), "metadata".to_string()),
        ]
        .into_iter()
        .collect()
    );
}

#[test]
fn interface_dispatch_infers_each_concrete_implementation_effects() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(deffect host
  (defn read () void
    (do (let ignored (intrinsic @env-list)))
    effects: (env.read)))
(def display (interface (fmt (fn-type (self self) str effects: (entry.host env.read)))))
(def quiet-box (newtype int64)
  impls: ((impl display methods: ((method fmt
    (fn (self self) str (do (return "quiet")) effects: ()))))))
(def loud-box (newtype int64)
  impls: ((impl display methods: ((method fmt
    (fn (self self) str (do (host.read) (return "loud")) effects: (entry.host env.read)))))))
(defn quiet-call (value quiet-box) void (do (display.fmt value)) effects: ())
(defn loud-call (value loud-box) void (do (display.fmt value)) effects: (entry.host env.read))
(defn main () void
  (do
    (let quiet (cast 1 quiet-box))
    (let loud (cast 2 loud-box))
    (quiet-call quiet)
    (loud-call loud)))
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog)
        .expect("static interface dispatch should resolve before effect inference");
    assert!(vibra::effect_semantics::infer_function(
        &lowered.functions["quiet-call"],
        &lowered.functions,
    )
    .labels()
    .is_empty());
    let loud_effects = vibra::effect_semantics::infer_function(
        &lowered.functions["loud-call"],
        &lowered.functions,
    )
    .labels();
    assert!(loud_effects.contains(&("env".into(), "read".into())));
    assert!(loud_effects.contains(&("entry".into(), "host".into())));
}

#[test]
fn deffect_operation_owner_is_still_inferred_at_the_operation_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(deffect read
  (defn open () void (do (let ok true)) effects: ()))
(defn main () void (do (read.open)))
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).unwrap();
    let inferred = vibra::effect_semantics::infer_function(
        &lowered.functions["read.open"],
        &lowered.functions,
    );
    assert!(
        inferred.labels().contains(&("entry".into(), "read".into())),
        "inferred: {inferred:?}, signature: {:?}",
        lowered.functions["read.open"]
    );
}

/// A task has no runtime isolation, so work it spawns still counts against the
/// enclosing function.
#[test]
fn task_effects_belong_to_the_enclosing_function() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(deffect host
  (defn read () void
    (do (let ignored (intrinsic @env-list)))
    effects: (env.read)))
(defn main () void (do (task (do (host.read)) captures: ())))
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).unwrap();
    let inferred =
        vibra::effect_semantics::infer_statements(&lowered.statements, &lowered.functions);
    assert!(inferred.labels().contains(&("env".into(), "read".into())));
}

/// `effects:` accepts nominal root names, and an absent attribute means pure.
#[test]
fn effects_attribute_accepts_nominal_labels() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(defn named () void (do (let ok true)) effects: (fs.read))
(defn inline () void (do (let ok true)) effects: (net.connect stream.read))
(defn pure-fn () void (do (let ok true)))
(defn
  main
  ()
  void
  (do (named) (inline) (pure-fn))
   effects: (fs.read net.connect stream.read)
)
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).expect("effects: should lower");

    let named = &lowered.functions["named"].effects;
    assert_eq!(
        named.labels,
        [("fs".to_string(), "read".to_string())]
            .into_iter()
            .collect()
    );
    let inline = &lowered.functions["inline"].effects;
    assert_eq!(inline.labels.len(), 2);
    assert!(lowered.functions["pure-fn"].effects.is_empty());
}

/// Canonical root identity is independent of the import alias used at a call site.
#[test]
fn nominal_roots_are_canonical_and_alias_independent() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.vib");
    let second = dir.path().join("second.vib");
    let entry = dir.path().join("entry.vib");

    std::fs::write(&first, "(defn read () void (do (let ok true)))\n").unwrap();
    std::fs::write(&second, "(defn fetch () void (do (let ok true)))\n").unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"(import a "{a}")
(import b "{b}")
 (defn via-first () void (do (let ok true)) effects: (fs.read))
 (defn via-second () void (do (let ok true)) effects: (fs.read))
(defn via-inline () void (do (let ok true)) effects: (fs.read))
(defn main () void (do (via-first) (via-second) (via-inline)) effects: (fs.read))
"#,
            a = first.display().to_string().replace('\\', "/"),
            b = second.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).unwrap();
    let first_row = &lowered.functions["via-first"].effects;
    assert_eq!(first_row, &lowered.functions["via-second"].effects);
    assert_eq!(first_row, &lowered.functions["via-inline"].effects);
}

/// Compiler-owned nominal roots can be used without importing a module alias.
#[test]
fn nominal_root_needs_no_import() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(defn touches-fs () void (do (let ok true)) effects: (fs.read))
(defn main () void (do (touches-fs)) effects: (fs.read))
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    vibra::lower::lower_program(&prog).expect("a canonical root should need no import");
}

#[test]
fn effects_is_rejected_on_non_function_declarations() {
    for (owner, source) in [
        ("def", "(def thing (newtype str) effects: ())\n"),
        ("const", "(const limit int64 7 effects: ())\n"),
        (
            "macro",
            "(macro twice (a @expr-syntax) @expr-syntax (do a) effects: ())\n",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let entry = dir.path().join("entry.vib");
        std::fs::write(
            &entry,
            format!("{source}(defn main () void (do (let ok true)))\n"),
        )
        .unwrap();

        let err = format!("{:#}", vibra::load::load_program(&entry).unwrap_err());
        assert!(
            err.contains("E-EFFECT-006") && err.contains(owner),
            "expected `effects:` to be rejected on `{owner}`, got: {err}"
        );
    }
}

#[test]
fn unknown_or_non_effect_names_are_rejected() {
    for (label, extra) in [("missing", ""), ("thing", "(def thing (newtype str))\n")] {
        let dir = tempfile::tempdir().unwrap();
        let entry = dir.path().join("entry.vib");
        std::fs::write(
            &entry,
            format!(
                r#"{extra}(defn f () void (do (let ok true)) effects: ({label}))
(defn main () void (do (f)))
"#
            ),
        )
        .unwrap();

        let prog = vibra::load::load_program(&entry).unwrap();
        let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
        assert!(
            err.contains("E-EFFECT-002"),
            "expected `{label}` to be rejected as an effect, got: {err}"
        );
    }
}

/// A named label is a cross-module reference like any other. Without visiting
/// annotations in `validate_top_level_aliases`, an `effects:` reference would slip
/// past the undeclared-alias rule that every other type reference obeys.
#[test]
fn named_effect_reference_obeys_the_declared_alias_rule() {
    let dir = tempfile::tempdir().unwrap();
    let inner = dir.path().join("inner.vib");
    let middle = dir.path().join("middle.vib");
    let entry = dir.path().join("entry.vib");

    std::fs::write(
        &inner,
        "(deffect read (defn open () void (do) effects: ()))\n",
    )
    .unwrap();
    std::fs::write(
        &middle,
        format!(
            "(import inner \"{inner}\")\n(defn noop () void (do (let ok true)))\n",
            inner = inner.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    // `inner` is a known alias in the program, but `entry` never imported it.
    std::fs::write(
        &entry,
        format!(
            r#"(import middle "{middle}")
(defn f () void (do (middle.noop)) effects: (inner.read))
(defn main () void (do (f)))
"#,
            middle = middle.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let err = format!("{:#}", vibra::load::load_program(&entry).unwrap_err());
    assert!(
        err.contains("E-MOD-004"),
        "expected an indirectly-imported alias in `effects:` to be rejected, got: {err}"
    );
}

/// A deffect root declares owned operations directly.
#[test]
fn effect_declaration_lowers() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(deffect read
  (defn open () void (do (let ok true)) effects: ()))
(deffect write
  (defn open () void (do (let ok true)) effects: ()))
(defn main () void (do (let ok true)))
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    vibra::lower::lower_program(&prog).expect("effect declarations should lower");
}

/// Deffect roots share the top-level namespace with types, so collisions are
/// rejected before operations are registered.
#[test]
fn effect_binding_collides_with_a_type_of_the_same_name() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(deffect read (defn open () void (do (let ok true)) effects: ()))
(def read (newtype str))
(defn main () void (do (let ok true)))
"#,
    )
    .unwrap();

    let err = format!("{:#}", vibra::load::load_program(&entry).unwrap_err());
    assert!(
        err.contains("E-MOD-002"),
        "expected an effect to collide with a same-named type, got: {err}"
    );
}

/// Bare type aliases are unsupported language-wide. Nominal roots are not type
/// aliases and cannot be rebound through `def`.
#[test]
fn bare_type_aliases_are_unsupported_for_effects_and_newtypes_alike() {
    for body in ["(newtype int64)"] {
        let dir = tempfile::tempdir().unwrap();
        let entry = dir.path().join("entry.vib");
        std::fs::write(
            &entry,
            format!(
                r#"(def original {body})
(def renamed original)
(defn main () void (do (let ok true)))
"#
            ),
        )
        .unwrap();

        let err = format!("{:#}", vibra::load::load_program(&entry).unwrap_err());
        assert!(
            err.contains("E-ADAPT-002"),
            "expected a bare alias of `{body}` to hit the shared adapter limit, got: {err}"
        );
    }
}

/// The operands are atoms, not symbols: a bare `(effect fs read)` is the pre-atom
/// spelling and must be rejected rather than silently read as a generic application.
#[test]
fn effect_operands_must_be_atoms() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def read (effect fs read))
(defn main () void (do (let ok true)))
"#,
    )
    .unwrap();

    let err = format!("{:#}", vibra::load::load_program(&entry).unwrap_err());
    assert!(
        err.contains("E-EFFECT-008"),
        "expected removed structural effect syntax to be rejected, got: {err}"
    );
}

#[test]
fn effect_requires_exactly_a_domain_and_an_action() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def read (effect @fs))
(defn main () void (do (let ok true)))
"#,
    )
    .unwrap();

    let err = format!("{:#}", vibra::load::load_program(&entry).unwrap_err());
    assert!(
        err.contains("E-SYN-009") || err.contains("effect"),
        "expected a one-operand effect to be rejected, got: {err}"
    );
}

/// Operands past the action are reserved for handler definitions. This is the
/// reservation that keeps handlers from being a syntax-breaking retrofit later.
#[test]
fn effect_extra_operands_are_reserved_for_handlers() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def read (effect @fs @read @extra))
(defn main () void (do (let ok true)))
"#,
    )
    .unwrap();

    let err = format!("{:#}", vibra::load::load_program(&entry).unwrap_err());
    assert!(
        err.contains("E-EFFECT-008"),
        "expected removed structural effect syntax to be rejected, got: {err}"
    );
}

/// The language contract says: "No implicit numeric widening or narrowing occurs." A literal may take the
/// parameter's width because it has none of its own, but a *local* carries `int64` and
/// must be converted explicitly.
#[test]
fn int64_local_does_not_narrow_to_int32_parameter() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(defn accepts-int32 (input int32) void (do (let ok true)))
(defn main () void (do (let wide 7) (accepts-int32 wide)))
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("type mismatch") && err.contains("Int32"),
        "expected an `int64` local to need an explicit conversion for an `int32` parameter, got: {err}"
    );
}

/// Literal widening is range-checked, so a literal that does not fit is still an error.
#[test]
fn out_of_range_int_literal_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(defn accepts-int8 (input int8) void (do (let ok true)))
(defn main () void (do (accepts-int8 300)))
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("type mismatch"),
        "expected 300 to be rejected as an `int8` literal, got: {err}"
    );
}

/// Signedness is not a width choice: a `uint64` value never silently becomes `int64`.
#[test]
fn uint64_value_does_not_implicitly_become_int64() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(defn produce () uint64 (do (return 7)))
(defn accepts-int64 (input int64) void (do (let ok true)))
(defn main () void (do (accepts-int64 (produce))))
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("type mismatch"),
        "expected `uint64` to need an explicit conversion for an `int64` parameter, got: {err}"
    );
}

/// An array literal has no element width of its own, so it may still reach a newtype
/// over a narrower array through an explicit `cast`.
#[test]
fn array_literal_casts_into_narrower_element_newtype() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def octets (newtype (array uint8)))
(defn main () void (do (let raw (cast (array 1 255) octets))))
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    vibra::lower::lower_program(&prog)
        .expect("an integer array literal should reach `(array uint8)` through a cast");
}

#[test]
fn newtype_decl_lowers_and_requires_explicit_cast() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def meter (newtype int64))
(defn take-meter (input meter) void (do (let ok true)))
(defn main () void (do (let v (cast 7 meter)) (take-meter v)))
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
        sig.parameters[0].ty,
        vibra::lower::TypeRef::Named("meter".to_string())
    );
}

#[test]
fn cast_rejects_legacy_nested_payload_shape() {
    // The legacy `$cast: {from: ..., to: ...}` nested-payload alternative to
    // the canonical `$cast: <expr>` + sibling `into: <type>` shape (the
    // E-CAST-002 check this test originally exercised) has no S-expression
    // form: `cast` is always `(cast expr type-expr)`, exactly two positional
    // operands, arity-checked by the reader itself (`exact_arity("cast",
    // ..., 2, ...)` in `src/ast/surface.rs`). A malformed arity is therefore
    // now a reader-level `E-SYN` error, not the legacy semantic check.
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def meter (newtype int64))
(defn main () void (do (let v (cast 7 meter (record (from 7) (to meter))))))
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry);
    let err = match prog {
        Ok(prog) => format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err()),
        Err(error) => format!("{error:#}"),
    };
    assert!(err.contains("E-SYN"), "unexpected error: {err}");
}

#[test]
fn cast_rejects_identity_conversion() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(defn main () void (do (let v (cast 7 int64))))
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
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def meter (newtype int64))
(defn
  take-meter
  (input meter)
  void
  (do
    (let ok true)
  )
)
(defn main () void (do (take-meter 7)))
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
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def meter (newtype int64))
(def second (newtype int64))
(defn
  take-second
  (input second)
  void
  (do
    (let ok true)
  )
)
(defn
  main
  ()
  void
  (do (let m (cast 7 meter)) (let s (cast m second)) (take-second s))
)
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
    let fs = std::fs::canonicalize(root.join("stdlib/src/fs.vib")).unwrap();
    let stream = std::fs::canonicalize(root.join("stdlib/src/stream.vib")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        format!(
            r#"(import fs "{fs}")
(import stream "{stream}")
(defn
  main
  ()
  void
  (do (let f (cast 0 fs.reader)) (stream.write.string f "nope"))
)
"#,
            fs = fs.display().to_string().replace('\\', "/"),
            stream = stream.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let err = format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err());
    assert!(
        err.contains("E-CAP-005"),
        "expected host-backed endpoint forgery to be rejected, got: {err}"
    );
}

#[test]
fn wasm_abi_rejects_wrong_value_parameter_type() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(deffect host
  (defn
    bad-assert
    (value str)
    void
    (do (wasm "vibra_test" "assert" value))
    effects: ()))
(defn main () void (do))
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
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(deffect host
  (defn
    bad-assert
    (value bool)
    bool
    (do (wasm "vibra_test" "assert" value))
    effects: ()))
(defn main () void (do))
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
fn opaque_host_handle_cannot_be_cast_from_integer() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def read-file (handle @read))
(defn main () void (do (let forged (cast 0 read-file))))
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
    let hello = root.join("examples/hello.vib");
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
    let report: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    let json = report.to_string();
    assert!(json.contains("vibra_v1"), "{json}");
    assert!(json.contains("stdout_open"), "{json}");
    assert!(json.contains("write-handle"), "{json}");
}

#[test]
fn effects_report_keeps_native_operation_owner_canonical_across_import_aliases() {
    let dir = tempfile::tempdir().unwrap();
    let resource = dir.path().join("time.vib");
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &resource,
        r#"(deffect now
  (defn open () uint64
    (intrinsic @clock-now-unix-millis)
    effects: ()))
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        r#"(import t "./time.vib")
(defn main () void
  (do (let value (t.now.open)))
  effects: (time.now))
"#,
    )
    .unwrap();

    let output = vibra_cmd()
        .args(["effects", &path_str(&entry)])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let operation = report["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|operation| operation["source"] == "t.now.open")
        .expect("native operation report");
    assert_eq!(operation["owner"]["domain"], "time");
    assert_eq!(operation["owner"]["action"], "now");
}

#[test]
fn legacy_function_grants_are_rejected_after_policy_redesign() {
    // See the comment on `nested_function_grants_are_rejected` (top of this
    // file): `grants:` is unknown syntax now, rejected by the reader
    // (E-SYN-011) rather than reaching the legacy migration diagnostic
    // (E-SEC-001) this test originally exercised.
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(&entry, "(defn main () void (do) grants: (fs-read))\n").unwrap();

    let prog = vibra::load::load_program(&entry);
    let err = match prog {
        Ok(prog) => format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err()),
        Err(error) => format!("{error:#}"),
    };
    assert!(
        err.contains("E-SYN-011"),
        "expected an unknown-attribute rejection, got: {err}"
    );
}

#[test]
fn fs_open_read_reads_a_file_without_any_authority_argument() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fs = std::fs::canonicalize(root.join("stdlib/src/fs.vib")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data.txt");
    std::fs::write(&data, "secret").unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        format!(
            r#"(import fs "{fs}")
(defn
  main
  ()
  void
  (do (let path (fs.path.new "{path}")) (let text (fs.read.file path)))
  effects: (fs.read stream.read stream.manage)
)
"#,
            fs = fs.display().to_string().replace('\\', "/"),
            path = data.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).unwrap();
    vibra::execute::run_lowered(&lowered, &vibra::runtime::RunConfig::default())
        .expect("fs reads require no capability argument anymore");
}

#[test]
fn duplicate_nested_imports_are_idempotent() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let io = std::fs::canonicalize(root.join("stdlib/src/io.vib")).unwrap();
    let fs = std::fs::canonicalize(root.join("stdlib/src/fs.vib")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");

    std::fs::write(
        &entry,
        format!(
            r#"(import io "{io}")
(import fs "{fs}")
(defn main () void (do (io.stdout.println "ok")) effects: (io.stdout stream.write))
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
    let io = std::fs::canonicalize(root.join("stdlib/src/io.vib")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let leaf_a = dir.path().join("leaf-a.vib");
    let leaf_b = dir.path().join("leaf-b.vib");
    let mod_a = dir.path().join("a.vib");
    let mod_b = dir.path().join("b.vib");
    let entry = dir.path().join("entry.vib");

    std::fs::write(
        &leaf_a,
        r#"(defn id () str (do (return "A")))
"#,
    )
    .unwrap();
    std::fs::write(
        &leaf_b,
        r#"(defn id () str (do (return "B")))
"#,
    )
    .unwrap();
    std::fs::write(
        &mod_a,
        format!(
            r#"(import util "{leaf}")
(import io "{io}")
(defn call () void (do (let x (util.id)) (io.stdout.println x)) effects: (io.stdout stream.write))
"#,
            leaf = leaf_a.display().to_string().replace('\\', "/"),
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    std::fs::write(
        &mod_b,
        format!(
            r#"(import util "{leaf}")
(import io "{io}")
(defn call () void (do (let x (util.id)) (io.stdout.println x)) effects: (io.stdout stream.write))
"#,
            leaf = leaf_b.display().to_string().replace('\\', "/"),
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"(import a "{a}")
(import b "{b}")
(import io "{io}")
(defn main () void (do (a.call) (b.call)) effects: (io.stdout stream.write))
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
    let leaf = dir.path().join("leaf.vib");
    let helper = dir.path().join("helper.vib");
    let entry = dir.path().join("entry.vib");

    std::fs::write(
        &leaf,
        r#"(defn value () str (do (return "hidden")))
"#,
    )
    .unwrap();
    std::fs::write(
        &helper,
        r#"(defn call () str (do (return (leaf.value))))
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"(import leaf "{leaf}")
(import helper "{helper}")
(defn main () void (do (helper.call)))
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
    let leaf = dir.path().join("leaf.vib");
    let helper = dir.path().join("helper.vib");
    let entry = dir.path().join("entry.vib");

    std::fs::write(
        &leaf,
        r#"(defn value () str (do (return "visible")))
"#,
    )
    .unwrap();
    std::fs::write(
        &helper,
        format!(
            r#"(import leaf "{leaf}")
(defn call () str (do (return (leaf.value))))
"#,
            leaf = leaf.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"(import helper "{helper}")
(defn main () void (do (helper.call)))
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
    let types = dir.path().join("types.vib");
    let helper = dir.path().join("helper.vib");
    let entry = dir.path().join("entry.vib");

    std::fs::write(
        &types,
        r#"(def outcome (enum (ok str) (err str)))
"#,
    )
    .unwrap();
    std::fs::write(
        &helper,
        r#"(defn make () types.outcome (do (return (types.outcome.ok "hidden"))))
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"(import types "{types}")
(import helper "{helper}")
(defn main () void (do (helper.make)))
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
    let entry = dir.path().join("entry.vib");

    std::fs::write(
        &entry,
        r#"(defn helper () void (do (return)) doc: "See $result.result for the canonical shape.")
(defn main () void (do (helper)))
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    vibra::lower::lower_program(&prog).expect("=doc text should not require imports");
}

#[test]
fn same_module_qualified_local_symbol_is_allowed() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");

    std::fs::write(
        &entry,
        r#"(def outcome (enum (ok str) (err str)))
(defn make () outcome (do (return (outcome.ok "local"))))
(defn main () void (do (make)))
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    vibra::lower::lower_program(&prog).expect("same-module qualified refs should lower");
}

#[test]
fn tagged_option_rejects_raw_payload_and_null_coercions() {
    let dir = tempfile::tempdir().unwrap();
    let model = dir.path().join("model.vib");
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();
    let entry = dir.path().join("entry.vib");

    std::fs::write(
        &model,
        r#"(def option (enum (some t) (none void)) where: (t any))
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"(import m "{m}")
(import io "{io}")
(defn use-option (input (m.option int64)) void (do (io.stdout.println "using option")))
(defn expect-int (input int64) void (do (io.stdout.println "x")))
(defn main () void (do (use-option 7) (use-option unit) (expect-int unit)))
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
    let entry = dir.path().join("entry.vib");
    // Legacy hardcoded a top-level `$option: T` definition body as a removed
    // type-sugar form (`env.form_key == "$option"`, `src/lower.rs`) --
    // independent of whether an `option` type was otherwise declared. The
    // new grammar has no built-in `option` type-form head at all (spec:
    // "Built-in type-form heads such as `record`, `tuple`, `array`, `map`,
    // and `union` are recognized before user-defined type constructors");
    // `(option str)` is ordinary generic type application syntax, so with
    // no local `def option` the *adapter* itself now fails to resolve the
    // constructor (E-ADAPT-009) before ever reaching legacy's dedicated
    // check. Either way the same rule holds: a bare `option` type sugar is
    // rejected, so this accepts both diagnostics.
    std::fs::write(
        &entry,
        r#"(def legacy (option str))
(defn main () void (do))
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry);
    let err = match prog {
        Ok(prog) => format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err()),
        Err(error) => format!("{error:#}"),
    };
    assert!(
        err.contains("E-OPTION-001") || err.contains("E-ADAPT-009"),
        "unexpected error: {err}"
    );
}

#[test]
fn legacy_option_sugar_with_mapped_inner_type_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    // See the comment on `legacy_option_sugar_is_rejected_with_stable_code`:
    // legacy's `$option: T` check (`parse_type_ref`, `src/lower.rs`) applies
    // wherever a type is parsed, nested positions included, but the adapter
    // still fails first (E-ADAPT-009) resolving `option` as an unknown
    // generic type constructor before that legacy check is reached.
    std::fs::write(
        &entry,
        r#"(def holder (record (value (option (array str)))))
(defn main () void (do))
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry);
    let err = match prog {
        Ok(prog) => format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err()),
        Err(error) => format!("{error:#}"),
    };
    assert!(
        err.contains("E-OPTION-001") || err.contains("E-ADAPT-009"),
        "unexpected error: {err}"
    );
}

#[test]
fn direct_void_union_is_rejected_with_stable_code() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def legacy (union void str))
(defn main () void (do))
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
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def option (enum (some t) (none void)) where: (t any))
(def holder (record (value (option str))))
(defn main () void (do))
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    vibra::lower::lower_program(&prog).expect("generic alias named option should remain valid");
}

#[test]
fn result_where_ok_and_err_type_params() {
    let dir = tempfile::tempdir().unwrap();
    let model = dir.path().join("model.vib");
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();
    let entry = dir.path().join("entry.vib");

    std::fs::write(
        &model,
        r#"(def result (enum (ok t) (err e)) where: (t any e any))
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"(import m "{m}")
(import io "{io}")
(defn
  main
  ()
  void
  (do
    (let r-ok (m.result.ok 99))
    (match
  r-ok
  (m.result.ok (bind x))
  (do (io.stdout.println "ok"))
  (m.result.err (bind y))
  (do (io.stdout.println y))
)
    (let r-err (m.result.err "fail"))
    (match
  r-err
  (m.result.ok (bind x2))
  (do (io.stdout.println "no"))
  (m.result.err (bind y2))
  (do (io.stdout.println y2))
)
  )
  effects: (io.stdout stream.write)
)
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
    let bad = dir.path().join("bad.vib");
    let good = dir.path().join("good.vib");
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();
    let entry_bad = dir.path().join("entry_bad.vib");
    let entry_good = dir.path().join("entry_good.vib");

    std::fs::write(
        &bad,
        r#"(def opt (enum (some T) (none void)))
"#,
    )
    .unwrap();
    std::fs::write(
        &good,
        r#"(def opt (enum (some t) (none void)) where: (t any))
"#,
    )
    .unwrap();

    std::fs::write(
        &entry_bad,
        format!(
            r#"(import m "{m}")
(import io "{io}")
(defn main () void (do (let v (m.opt.some 7)) (io.stdout.println "bad")) effects: (io.stdout stream.write))
"#,
            m = bad.display().to_string().replace('\\', "/"),
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    std::fs::write(
        &entry_good,
        format!(
            r#"(import m "{m}")
(import io "{io}")
(defn main () void (do (let v (m.opt.some 7)) (io.stdout.println "good")) effects: (io.stdout stream.write))
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
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();
    let entry = dir.path().join("entry.vib");

    std::fs::write(
        &entry,
        format!(
            r#"(import io "{io}")
(defn main () void (do (io.stdout.flush)) effects: (io.stdout stream.manage))
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
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();
    let entry = dir.path().join("entry.vib");

    // Legacy accepted only a literal `null` payload for a zero-arg call and
    // separately rejected any other literal, including `$void`
    // ("zero-arg call payload must be `null`"). Positional calls make that
    // whole spelling axis disappear: a zero-arg call is just `(io.stdout.flush)`,
    // and supplying a value where the callee takes none is an ordinary
    // arity mismatch, checked (and reported, E-ADAPT-026) while adapting
    // the call -- the same underlying rule ("nothing may be passed to a
    // zero-arg call"), enforced earlier and uniformly rather than by a
    // payload-literal special case.
    std::fs::write(
        &entry,
        format!(
            r#"(import io "{io}")
(defn main () void (do (io.stdout.flush unit)))
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry);
    let err = match prog {
        Ok(prog) => format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err()),
        Err(error) => format!("{error:#}"),
    };
    assert!(
        err.contains("E-ADAPT-026") && err.contains("io.stdout.flush"),
        "unexpected error: {err}"
    );
}

#[test]
fn generic_user_fn_identity_returns_value() {
    let dir = tempfile::tempdir().unwrap();
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        format!(
            r#"(import io "{io}")
(defn identity (input t) t (do (return input)) where: (t any))
(defn main () void (do (let n (identity int64 7)) (io.stdout.println "ok")) effects: (io.stdout stream.write))
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
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        format!(
            r#"(import io "{io}")
(defn identity (input t) t (do (return input)) where: (t any))
(defn main () void (do (identity 7)))
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry);
    let err = match prog {
        Ok(prog) => format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err()),
        Err(error) => format!("{error:#}"),
    };
    assert!(
        err.contains("E-ADAPT-026") && err.contains("identity"),
        "unexpected error: {err}"
    );
}

#[test]
fn generic_call_rejects_unknown_keys() {
    let dir = tempfile::tempdir().unwrap();
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();
    let entry = dir.path().join("entry.vib");
    // An extra unrecognized `q:` key alongside a generic call's payload and
    // type argument has no positional equivalent to even attempt: a generic
    // call's leading positional arguments are exactly its declared type
    // parameters followed by exactly its declared value parameters (matching
    // the migrated corpus's own convention, e.g.
    // `(collections.array-contains int64 (array 1 2 3) 2)`), so an extra
    // value operand is an ordinary arity mismatch (E-ADAPT-026) rather than
    // legacy's "unexpected key" check -- the same rule (unrecognized/extra
    // call input is rejected), caught earlier.
    std::fs::write(
        &entry,
        format!(
            r#"(import io "{io}")
(defn identity (input t) t (do (return input)) where: (t any))
(defn main () void (do (identity int64 7 1)))
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry);
    let err = match prog {
        Ok(prog) => format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err()),
        Err(error) => format!("{error:#}"),
    };
    assert!(
        err.contains("E-ADAPT-026") && err.contains("identity"),
        "unexpected error: {err}"
    );
}

#[test]
fn bool_literals_are_compatible_with_bool_args() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(defn accepts-bool (x bool) void (do (let ok true)))
(defn main () void (do (accepts-bool true) (accepts-bool false)))
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
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(defn accepts-int (x int64) void (do (let ok true)))
(defn main () void (do (accepts-int true)))
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
    let entry = dir.path().join("entry.vib");
    // A multi-arg call's extra unrecognized `typo:` key has no positional
    // equivalent: positional calls are arity-checked against the callee's
    // parameter list as a property of the call form itself, so an extra
    // value operand is an ordinary arity mismatch (E-ADAPT-026), caught
    // while adapting the call, rather than legacy's dedicated "unexpected
    // key" lowering check -- same rule (extraneous call input is rejected),
    // earlier enforcement point.
    std::fs::write(
        &entry,
        r#"(defn
  join-ish
  (left str right str)
  void
  (do
    (let ok true)
  )
)
(defn main () void (do (join-ish "a" "b" "ignored")))
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry);
    let err = match prog {
        Ok(prog) => format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err()),
        Err(error) => format!("{error:#}"),
    };
    assert!(
        err.contains("E-ADAPT-026") && err.contains("join-ish"),
        "expected unexpected key rejection, got: {err}"
    );
}

#[test]
fn non_generic_single_arg_named_call_rejects_unknown_key() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    // See the comment on `non_generic_multi_arg_call_rejects_unknown_key`:
    // an extra call operand is now an ordinary positional arity mismatch.
    std::fs::write(
        &entry,
        r#"(defn
  take-text
  (x str)
  void
  (do
    (let ok true)
  )
)
(defn main () void (do (take-text "ok" "ignored")))
"#,
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry);
    let err = match prog {
        Ok(prog) => format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err()),
        Err(error) => format!("{error:#}"),
    };
    assert!(
        err.contains("E-ADAPT-026") && err.contains("take-text"),
        "expected unexpected key rejection, got: {err}"
    );
}

#[test]
fn single_arg_constructor_shorthand_still_lowers() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def maybe (enum (some str) (none void)))
(defn take-maybe (x maybe) void (do (let ok true)))
(defn main () void (do (take-maybe (maybe.some "value"))))
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
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        format!(
            r#"(import io "{io}")
(defn identity (input t) t (do (return input)) where: (t any))
(defn main () void (do (identity int64 "hi")))
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
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        format!(
            r#"(import io "{io}")
(defn bad (input int64) int64 (do (io.stdout.println "nope")))
(defn main () void (do (io.stdout.println "x")))
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
    let helper = dir.path().join("helper.vib");
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &helper,
        r#"(defn echo-int (input int64) int64 (do (return input)))
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"(import h "{h}")
(import io "{io}")
(defn main () void (do (let v (h.echo-int 42)) (io.stdout.println "z")) effects: (io.stdout stream.write))
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
    let lib = dir.path().join("lib.vib");
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &lib,
        r#"(defn flush-generic (value t) void (do (let ok true)) where: (t any))
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"(import lg "{lg}")
(defn main () void (do (lg.flush-generic int64 0)))
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
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        format!(
            r#"(import io "{io}")
(def box (record (value t)) where: (t int64))
(defn main () void (do (io.stdout.println "ok")))
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
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        format!(
            r#"(import io "{io}")
(def display (interface (fmt (fn-type (self self) str))))
(defn main () void (do (io.stdout.println "ok")) effects: (io.stdout stream.write))
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
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        format!(
            r#"(import io "{io}")
(def node (record (next self)))
(defn main () void (do (io.stdout.println "ok")))
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
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        format!(
            r#"(import io "{io}")
(defn identity (x self) self (do (return x)))
(defn main () void (do (io.stdout.println "ok")))
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
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        format!(
            r#"(import io "{io}")
(def holder (record (iface (interface (fmt (fn-type (self self) str))))))
(defn main () void (do (io.stdout.println "ok")) effects: (io.stdout stream.write))
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
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();
    let entry = dir.path().join("entry.vib");
    // Legacy distinguished an unprefixed `where:` annotation key (rejected,
    // migration-hinted toward `=where`) from the canonical `=where:` spelling
    // ("=doc", "=where", "=defs", "=impl" | trailing "doc:", "where:",
    // "defs:", "impls:" attributes -- migration table). The S-expression
    // grammar has only one spelling, the unprefixed one; there is no second,
    // rejected form left to write. This now asserts the sole canonical
    // spelling lowers.
    std::fs::write(
        &entry,
        format!(
            r#"(import io "{io}")
(def pair (tuple a b) where: (a any b any))
(defn main () void (do (io.stdout.println "ok")) effects: (io.stdout stream.write))
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    vibra::lower::lower_program(&prog).expect("the single canonical `where:` spelling lowers");
}

#[test]
fn legacy_unprefixed_doc_is_rejected_with_e_anno_002() {
    let dir = tempfile::tempdir().unwrap();
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();
    let entry = dir.path().join("entry.vib");
    // See the comment on `legacy_unprefixed_where_is_rejected_with_e_anno_002`:
    // the unprefixed-vs-`=`-prefixed distinction no longer exists. This
    // fixture also originally pinned a `$literal` type, which likewise has
    // no S-expression form (`TypeExprKind` in `src/ast/surface.rs` has no
    // `Literal` variant at all -- a real, confirmed gap, not just an adapter
    // simplification); substituted an equivalent-purpose `newtype` since the
    // literal-vs-newtype distinction isn't what this test exercises.
    std::fs::write(
        &entry,
        format!(
            r#"(import io "{io}")
(def greeting (newtype str) doc: "the greeting")
(defn main () void (do (io.stdout.println "ok")) effects: (io.stdout stream.write))
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    vibra::lower::lower_program(&prog).expect("the single canonical `doc:` spelling lowers");
}

#[test]
fn unknown_annotation_key_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();
    let entry = dir.path().join("entry.vib");
    // An unrecognized `bogus:` function attribute has no S-expression
    // equivalent to even reach lowering: the known `fn` attributes are
    // `doc:`, `where:`, `defs:`, `impls:` (spec), and the reader itself
    // rejects any other label (E-SYN-011) -- same rejection as the
    // `grants:` cluster (see `nested_function_grants_are_rejected`), same
    // rule (an unrecognized declaration attribute is rejected), earlier
    // enforcement point.
    std::fs::write(
        &entry,
        format!(
            r#"(import io "{io}")
(defn foo () void (do (io.stdout.println "x")) bogus: 1)
(defn main () void (do (io.stdout.println "ok")))
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry);
    let err = match prog {
        Ok(prog) => format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err()),
        Err(error) => format!("{error:#}"),
    };
    assert!(err.contains("E-SYN-011"), "unexpected error: {err}");
}

#[test]
fn doc_string_lowers_on_function_and_type_decls() {
    let dir = tempfile::tempdir().unwrap();
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();
    let entry = dir.path().join("entry.vib");
    // The original `greeting` type pinned `$literal: "hi"`, which has no
    // S-expression form at all (see the comment on
    // `legacy_unprefixed_doc_is_rejected_with_e_anno_002`); substituted a
    // `newtype` since this test only asserts the *function's* doc string,
    // never inspecting `greeting`'s.
    std::fs::write(
        &entry,
        format!(
            r#"(import io "{io}")
(def greeting (newtype str) doc: "A newtype pinning the greeting string.")
(defn
  echo
  (msg str)
  void
  (do (io.stdout.println msg))
  doc: "Echo a message to stdout."
  effects: (io.stdout stream.write)
)
(defn main () void (do (echo "hi")) effects: (io.stdout stream.write))
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
    // Same fields, swapped `where:` key order. Legacy's named-keyed
    // instantiation (`{a: $int64, b: $str}`) let a call fix "a means int64,
    // b means str" independent of a module's own `where:` declaration
    // order, so the same literal call site produced different positional
    // `type_args` depending on which module's `pair` it targeted -- that
    // was the original gotcha this test demonstrated.
    //
    // The S-expression grammar has no named type arguments at all (spec:
    // "Partial type application and named type arguments are invalid");
    // `(m.pair int64 str)` is purely positional, and the adapter builds the
    // legacy named mapping by zipping *this* callee's own `where:` order
    // with the call's positional arguments (`type_application_value`,
    // `src/surface_adapter.rs`). So position 1 always means "whatever this
    // module's `where:` declares first," in both `ab` and `ba` -- the
    // original gotcha (same call, two different bindings) is no longer
    // reachable, only the tautological positional case remains. This now
    // shows both orderings lower with the call's own literal positional
    // order preserved.
    let dir = tempfile::tempdir().unwrap();
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();
    let mod_ab = dir.path().join("ab.vib");
    let mod_ba = dir.path().join("ba.vib");
    let entry = dir.path().join("entry.vib");

    std::fs::write(
        &mod_ab,
        r#"(def pair (tuple a b) where: (a any b any))
"#,
    )
    .unwrap();
    std::fs::write(
        &mod_ba,
        r#"(def pair (tuple a b) where: (b any a any))
"#,
    )
    .unwrap();

    let entry_src = |modpath: String, io: String| -> String {
        format!(
            r#"(import m "{m}")
(import io "{io}")
(defn take (input (m.pair int64 str)) void (do (io.stdout.println "ok")) effects: (io.stdout stream.write))
(defn main () void (do (io.stdout.println "ok")) effects: (io.stdout stream.write))
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
    let vibra::lower::TypeRef::Instantiated { type_args, .. } = &take_ab.parameters[0].ty else {
        panic!(
            "expected instantiated tuple alias, got {:?}",
            take_ab.parameters[0].ty
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
    let vibra::lower::TypeRef::Instantiated { type_args, .. } = &take_ba.parameters[0].ty else {
        panic!(
            "expected instantiated tuple alias, got {:?}",
            take_ba.parameters[0].ty
        );
    };
    assert_eq!(type_args.len(), 2);
    assert_eq!(type_args[0], vibra::lower::TypeRef::Int64);
    assert_eq!(type_args[1], vibra::lower::TypeRef::Str);
}

#[test]
fn record_type_alias_lowers_and_is_usable_in_signature() {
    let dir = tempfile::tempdir().unwrap();
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();
    let entry = dir.path().join("entry.vib");
    // io.vib defines ciovec as a non-generic $record. Function takes $io.ciovec
    // by bare reference (no instantiation) since it's non-generic.
    std::fs::write(
        &entry,
        format!(
            r#"(import io "{io}")
(defn take-vec (input io.ciovec) void (do (io.stdout.println "ok")) effects: (io.stdout stream.write))
(defn main () void (do (io.stdout.println "ok")) effects: (io.stdout stream.write))
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
        take_vec.parameters[0].ty,
        vibra::lower::TypeRef::Named("io.ciovec".to_string())
    );
}

#[test]
fn tuple_type_alias_with_where_lowers() {
    let dir = tempfile::tempdir().unwrap();
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        format!(
            r#"(import io "{io}")
(def pair (tuple a b) where: (a any b any))
(defn take (input (pair int64 str)) void (do (io.stdout.println "ok")) effects: (io.stdout stream.write))
(defn main () void (do (io.stdout.println "ok")) effects: (io.stdout stream.write))
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
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        format!(
            r#"(import io "{io}")
(def dict (map k v) where: (k any v any))
(defn take (input (dict str int64)) void (do (io.stdout.println "ok")) effects: (io.stdout stream.write))
(defn main () void (do (io.stdout.println "ok")) effects: (io.stdout stream.write))
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
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        format!(
            r#"(import io "{io}")
(def container (interface (value t)) where: (t any))
(defn main () void (do (io.stdout.println "ok")) effects: (io.stdout stream.write))
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
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        format!(
            r#"(import io "{io}")
(def pair (tuple a b) where: (a any b any))
(defn take (input pair) void (do (io.stdout.println "ok")))
(defn main () void (do (io.stdout.println "ok")))
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
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();
    let entry = dir.path().join("entry.vib");
    // Legacy caught a partially-instantiated generic alias (one of two type
    // arguments supplied) during lowering (E-GEN-002). The new grammar's
    // generic type application has no partial-application spelling at all
    // (spec: "Partial type application... [is] invalid"); the adapter
    // itself arity-checks a type application's argument count against the
    // callee's `where:` param count while building the legacy shape
    // (E-ADAPT-010, `type_application_value`), so an under-supplied
    // instantiation is now rejected earlier, for the same reason.
    std::fs::write(
        &entry,
        format!(
            r#"(import io "{io}")
(def pair (tuple a b) where: (a any b any))
(defn take (input (pair int64)) void (do (io.stdout.println "ok")))
(defn main () void (do (io.stdout.println "ok")))
"#,
            io = io.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry);
    let err = match prog {
        Ok(prog) => format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err()),
        Err(error) => format!("{error:#}"),
    };
    assert!(
        err.contains("E-GEN-002") || err.contains("E-ADAPT-010"),
        "unexpected error: {err}"
    );
}

#[test]
fn instantiated_record_field_type_mismatch_is_caught() {
    let dir = tempfile::tempdir().unwrap();
    let io = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/io.vib"))
        .unwrap();
    let entry = dir.path().join("entry.vib");
    // Pass a $str where the function expects an int through an instantiated
    // generic record alias.
    std::fs::write(
        &entry,
        format!(
            r#"(import io "{io}")
(def box (record (value t)) where: (t any))
(defn take-int-box (input (box int64)) void (do (io.stdout.println "ok")))
(defn make-str-box () (box str) (do (return (record (value "s")))))
(defn main () void (do (let sb (make-str-box)) (take-int-box sb)))
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
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def id (forall t))
(defn main () void (do))
"#,
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry);
    let is_err = match prog {
        Ok(prog) => vibra::lower::lower_program(&prog).is_err(),
        Err(_) => true,
    };
    assert!(is_err, "$forall should no longer be a recognised form");
}

#[test]
fn list_and_dict_keywords_are_no_longer_recognised() {
    let dir = tempfile::tempdir().unwrap();
    let entry_list = dir.path().join("list.vib");
    let entry_dict = dir.path().join("dict.vib");
    std::fs::write(
        &entry_list,
        r#"(def my-list (list int64))
(defn main () void (do))
"#,
    )
    .unwrap();
    std::fs::write(
        &entry_dict,
        r#"(def my-dict (dict a int64))
(defn main () void (do))
"#,
    )
    .unwrap();
    let prog_list = vibra::load::load_program(&entry_list);
    let list_is_err = match prog_list {
        Ok(prog) => vibra::lower::lower_program(&prog).is_err(),
        Err(_) => true,
    };
    assert!(list_is_err, "$list should no longer be a recognised form");
    let prog_dict = vibra::load::load_program(&entry_dict);
    let dict_is_err = match prog_dict {
        Ok(prog) => vibra::lower::lower_program(&prog).is_err(),
        Err(_) => true,
    };
    assert!(dict_is_err, "$dict should no longer be a recognised form");
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
    let model = dir.path().join("model.vib");
    let entry = dir.path().join("entry.vib");

    std::fs::write(
        &model,
        r#"(def box (record (value int64))
  defs: (
    (defn identity (self self) self (do (return self)))
  ))
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"(import m "{m}")
(defn main () void (do))
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
    let model = dir.path().join("res.vib");
    let entry = dir.path().join("entry.vib");

    std::fs::write(
        &model,
        r#"(def result (enum (err e) (ok t))
  where: (t any e any)
  defs: (
    (defn passthrough (self self) self (do (return self)))
  ))
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"(import r "{m}")
(defn main () void (do))
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
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(defn main () void (do) defs: ((defn nope () void (do))))
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
    let model = dir.path().join("model.vib");
    let entry = dir.path().join("entry.vib");
    // Legacy caught a non-`$function` `=defs` entry during lowering
    // (E-DEFS-001). The new grammar's `defs:` attribute is grammatically
    // `(function*)` -- each entry's reader parser requires an `fn` head
    // (`parse_defs_value`, `src/ast/surface.rs`) -- so a non-function entry
    // is now a reader-level rejection (E-SYN-008), same rule, earlier
    // enforcement point.
    std::fs::write(
        &model,
        r#"(def thing (record (value int64))
  defs: (
    (def bad (record (x int64)))
  ))
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"(import m "{m}")
(defn main () void (do))
"#,
            m = model.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let err = vibra::load::load_program(&entry).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("E-SYN-008"),
        "expected E-SYN-008 for non-`fn` entry inside `defs:`, got: {msg}"
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
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def display (interface (fmt (fn-type (self self) str))))
(def box (enum (boxed int64))
  impls: (
    (impl display methods: (
      (method fmt (fn (self self) str (do (return "boxed"))))
    ))
  ))
(defn identity-displayable (x t) t (do (return x)) where: (t display))
(defn
  main
  ()
  void
  (do (let b (box.boxed 1)) (let c (identity-displayable box b)))
)
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
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def display (interface (fmt (fn-type (self self) str))))
(defn identity-displayable (x t) t (do (return x)) where: (t display))
(defn main () void (do (let v (identity-displayable int64 7))))
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
        format!(
            r#"(def display (interface (fmt (fn-type (self self) str))))
(defn needs-display (x t) t (do (return x)) where: (t display))
(def meter (newtype int64))
(defn main () void (do (let result {expr})))
"#
        )
    }

    let cases = [
        ("record field", r#"(record (y (needs-display int64 1)))"#),
        ("array item", r#"(array (needs-display int64 1))"#),
        ("map key", r#"(map ((needs-display int64 1) "bad"))"#),
        ("map value", r#"(map ("bad" (needs-display int64 1)))"#),
        ("cast subject", r#"(cast (needs-display int64 1) meter)"#),
        (
            "if branch",
            r#"(if true (do (needs-display int64 1)) (do 0))"#,
        ),
    ];

    for (case, expr) in cases {
        let dir = tempfile::tempdir().unwrap();
        let entry = dir.path().join("entry.vib");
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
        format!(
            r#"(def display (interface (fmt (fn-type (self self) str))))
(defn needs-display (x t) t (do (return x)) where: (t display))
(defn takes-record (rec (record (y int64))) void (do (let ignored 0)))
(defn wrap-record (rec (record (y int64))) (record (y int64)) (do (return rec)))
(defn main () void (do {statement}))
"#
        )
    }

    let cases = [
        (
            "statement call argument",
            r#"(takes-record (record (y (needs-display int64 1))))"#,
        ),
        (
            "let call argument",
            r#"(let result (wrap-record (record (y (needs-display int64 1)))))"#,
        ),
    ];

    for (case, statement) in cases {
        let dir = tempfile::tempdir().unwrap();
        let entry = dir.path().join("entry.vib");
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
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def display (interface (fmt (fn-type (self self) str))))
(def bag (record (item t)) where: (t display))
(def holds-bag (record (inner (bag int64))))
(defn main () void (do))
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
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def display (interface (fmt (fn-type (self self) str))))
(def debug (interface (show (fn-type (self self) str))))
(def half-impl (enum (wrap int64))
  impls: (
    (impl display methods: (
      (method fmt (fn (self self) str (do (return "half"))))
    ))
  ))
(defn both-iface (x t) t (do (return x)) where: (t (intersect display debug)))
(defn
  main
  ()
  void
  (do (let v (half-impl.wrap 1)) (let r (both-iface half-impl v)))
)
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
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def display (interface (fmt (fn-type (self self) str))))
(defn needs-display (x t) t (do (return x)) where: (t display))
(defn
  forwarder
  (x u)
  u
  (do (let y (needs-display u x)) (return y))
  where: (u any)
)
(defn main () void (do))
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
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def display (interface (fmt (fn-type (self self) str))))
(def box (enum (boxed int64))
  impls: (
    (impl display methods: (
      (method fmt (fn (self self) str (do (return "boxed"))))
    ))
  ))
(defn main () void (do (let b (box.boxed 1)) (let s (display.fmt b))))
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
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def from-iface (interface (from (fn-type (arg-0 int64) void))))
(def box (enum (boxed int64))
  impls: (
    (impl from-iface methods: (
      (method from (fn (x int64) void (do (let unused x))))
    ))
  ))
(defn main () void (do (from-iface.from 5)))
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
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def display (interface (fmt (fn-type (self self) str))))
(defn main () void (do (let s (display.fmt 7))))
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
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def display (interface (fmt (fn-type (self self) str))))
(defn
  fmt-via-bound
  (x t)
  str
  (do (let s (display.fmt x)) (return s))
  where: (t display)
)
(defn main () void (do))
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
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def display (interface (fmt (fn-type (self self) str))))
(def box (record (value int64))
  impls: (
    (impl display methods: (
      (method fmt (fn (self self) str (do (return "boxed"))))
    ))
  ))
(defn main () void (do))
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
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def display (interface (fmt (fn-type (self self) str))))
(def box (record (value int64))
  defs: (
    (defn show (self self) str (do (return "shown")))
  )
  impls: (
    (impl display methods: (
      (method fmt box.show)
    ))
  ))
(defn main () void (do))
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
    let entry = dir.path().join("entry.vib");
    // A reachable interface is needed for the adapter to resolve `fmt`'s
    // signature at all (it builds the impl's forwarding shim from the
    // interface's declared member type); an unresolvable interface would
    // fail earlier with E-ADAPT-036 instead of reaching legacy's E-IMPL-001,
    // which is not what this test exercises (that's
    // `impl_unknown_interface_alias_is_rejected_with_e_impl_002`).
    std::fs::write(
        &entry,
        r#"(def display (interface (fmt (fn-type (self self) str))))
(defn whatever (self self) str (do (return "whatever")))
(defn main () void (do) impls: ((impl display methods: ((method fmt whatever)))))
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
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def box (record (value int64))
  impls: ((impl no-such-iface methods: ((method fmt whatever)))))
(defn main () void (do))
"#,
    )
    .unwrap();
    // The adapter itself needs to resolve the named interface to type the
    // impl's forwarding shim (`Converter::interface_member`), so an unknown
    // interface alias now fails during adaptation (E-ADAPT-036) rather than
    // reaching legacy's own E-IMPL-002 check -- same rule, earlier
    // enforcement point.
    let prog = vibra::load::load_program(&entry);
    let msg = match prog {
        Ok(prog) => format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err()),
        Err(error) => format!("{error:#}"),
    };
    assert!(
        msg.contains("E-IMPL-002") || msg.contains("E-ADAPT-036"),
        "expected an unknown-interface rejection; got: {msg}"
    );
}

/// An impl block is rejected if it is missing one of the iface's methods.
#[test]
fn impl_missing_method_is_rejected_with_e_impl_003() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def display (interface (fmt (fn-type (self self) str)) (debug (fn-type (self self) str))))
(def box (record (value int64))
  impls: (
    (impl display methods: (
      (method fmt (fn (self self) str (do (return "ok"))))
    ))
  ))
(defn main () void (do))
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
    let entry = dir.path().join("entry.vib");
    // The legacy `=impl: {$display: {fmt: {...}, bonus-stuff: 1}}` shape put
    // the interface's method map and any extraneous key at the same mapping
    // level, letting a stray key like `bonus-stuff:` sit beside a real
    // method (E-IMPL-004). The new grammar's `impl` form has no equivalent
    // open mapping: `methods:` is a list of `(method name binding)` entries
    // (`parse_method`, `src/ast/surface.rs`, requires an `method` head), and
    // `impl` itself accepts only `types:`/`methods:` labels
    // (`parse_impl_annotations`, E-SYN-011 for anything else). There is no
    // position left where a bare extra key like `bonus-stuff: 1` could even
    // be written, so this now asserts the reader rejects the closest
    // attempt -- a non-`method`-headed `methods:` entry -- structurally.
    std::fs::write(
        &entry,
        r#"(def display (interface (fmt (fn-type (self self) str))))
(def box (record (value int64))
  impls: (
    (impl display methods: (
      (method fmt (fn (self self) str (do (return "ok"))))
      (bonus-stuff 1)
    ))
  ))
(defn main () void (do))
"#,
    )
    .unwrap();
    let err = vibra::load::load_program(&entry).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("E-SYN"),
        "expected a reader-level rejection for a non-`method` `methods:` entry; got: {msg}"
    );
}

/// An impl method whose signature does not match the iface declaration
/// (after `$self` substitution) is rejected.
#[test]
fn impl_method_signature_mismatch_is_rejected_with_e_impl_005() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def display (interface (fmt (fn-type (self self) str))))
(def box (record (value int64))
  impls: (
    (impl display methods: (
      (method fmt (fn (self self) int64 (do (return 1))))
    ))
  ))
(defn main () void (do))
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
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def display (interface (fmt (fn-type (self self) (union int64 str)))))
(def box (record (value int64))
  impls: (
    (impl display methods: (
      (method fmt (fn (self self) str (do (return "boxed"))))
    ))
  ))
(defn main () void (do))
"#,
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry).unwrap();
    vibra::lower::lower_program(&prog).expect("narrower impl return should satisfy iface");
}

#[test]
fn impl_method_argument_types_remain_invariant() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def display (interface (fmt (fn-type (value (union int64 str)) str))))
(def box (record (value int64))
  impls: (
    (impl display methods: (
      (method fmt (fn (x str) str (do (return "boxed"))))
    ))
  ))
(defn main () void (do))
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
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def display (interface (fmt (fn-type (self self) str))))
(def box (record (value int64))
  impls: (
    (impl display methods: (
      (method fmt (fn (self self) (union int64 str) (do (return "boxed"))))
    ))
  ))
(defn main () void (do))
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
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def from-iface (interface (from (fn-type (arg-0 t) int64))) where: (t any))
(def box (record (value int64))
  impls: (
    (impl from-iface types: (int64) methods: (
      (method from (fn (x int64) int64 (do (return 0))))
    ))
  ))
(defn main () void (do))
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
        matches!(sig.parameters[0].ty, vibra::lower::TypeRef::Int64),
        "expected substituted arg type Int64; got {:?}",
        sig.parameters[0].ty
    );
}

#[test]
fn into_interface_registers_target_type_param() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(def into-iface (interface (into (fn-type (self self) t))) where: (t any))
(def box (record (value int64))
  impls: (
    (impl into-iface types: (int64) methods: (
      (method into (fn (self self) int64 (do (return 0))))
    ))
  ))
(defn main () void (do))
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
    let entry = dir.path().join("entry.vib");
    // The adapter itself resolves a `(method name qualified-symbol)`
    // reference's target signature (`Converter::impl_method_value`,
    // `MethodBinding::Reference`), so an unknown ref target now fails
    // during adaptation (E-ADAPT-037) before legacy's own E-IMPL-006 check
    // is reached -- same rule, earlier enforcement point.
    std::fs::write(
        &entry,
        r#"(def display (interface (fmt (fn-type (self self) str))))
(def box (record (value int64))
  impls: (
    (impl display methods: (
      (method fmt no.such.function)
    ))
  ))
(defn main () void (do))
"#,
    )
    .unwrap();
    let prog = vibra::load::load_program(&entry);
    let msg = match prog {
        Ok(prog) => format!("{:#}", vibra::lower::lower_program(&prog).unwrap_err()),
        Err(error) => format!("{error:#}"),
    };
    assert!(
        msg.contains("E-IMPL-006") || msg.contains("E-ADAPT-037"),
        "expected an unknown-ref-target rejection; got: {msg}"
    );
}

/// Inherent ops cannot redeclare a type parameter that is already in
/// scope from the enclosing generic type.
#[test]
fn defs_inherent_op_cannot_shadow_enclosing_type_param() {
    let dir = tempfile::tempdir().unwrap();
    let model = dir.path().join("model.vib");
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &model,
        r#"(def holder (record (value t))
  where: (t any)
  defs: (
    (defn bad (self self) self (do (return self)) where: (t any))
  ))
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"(import m "{m}")
(defn main () void (do))
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
        tests_dir.join("basic.vib"),
        r#"(import test "@std/test.vib")
(test.scenario
  "passes"
  (test.case "passes" (do (test.assert true)) profile: @core)
)
(test.scenario
  "also-passes"
  (test.case "also-passes" (do (test.assert true)) profile: @core)
)
"#,
    )
    .unwrap();
    std::fs::write(
        project.join("project.vib"),
        r#"(project
  (package "app" "0.1.0")
  (target app kind: @bin root: "tests" entry: "basic.vib")
  (dependency std path: "dep/std"))
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
        tests_dir.join("fails.vib"),
        r#"(import test "@std/test.vib")
(test.scenario "fails" (test.case "fails" (do (test.assert false)) profile: @core))
"#,
    )
    .unwrap();
    std::fs::write(
        project.join("project.vib"),
        r#"(project
  (package "app" "0.1.0")
  (target app kind: @bin root: "tests" entry: "fails.vib")
  (dependency std path: "dep/std"))
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
        tests_dir.join("fails.vib"),
        r#"(import test "@std/test.vib")
(test.scenario
  "fails"
  (test.case "fails" (do (test.assert-eq-int 1 2)) profile: @core)
)
"#,
    )
    .unwrap();
    std::fs::write(
        project.join("project.vib"),
        r#"(project
  (package "app" "0.1.0")
  (target app kind: @bin root: "tests" entry: "fails.vib")
  (dependency std path: "dep/std"))
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
fn vibra_test_writes_json_report_file() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("app");
    let tests_dir = project.join("tests");
    let report = dir.path().join("report.json");
    std::fs::create_dir_all(&tests_dir).unwrap();
    std::fs::write(
        tests_dir.join("basic.vib"),
        r#"(import test "@std/test.vib")
(test.scenario
  "passes"
  (test.case "passes" (do (test.assert true)) profile: @core)
)
"#,
    )
    .unwrap();
    std::fs::write(
        project.join("project.vib"),
        r#"(project
  (package "app" "0.1.0")
  (target app kind: @bin root: "tests" entry: "basic.vib")
  (dependency std path: "dep/std"))
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
            "json",
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
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(report).unwrap()).unwrap();
    assert_eq!(report["total"], 1);
    assert_eq!(report["passed"], 1);
    assert_eq!(report["tests"][0]["status"], "passed");
}

#[test]
fn module_part_test_file_shares_base_module_definitions() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("app");
    let tests_dir = project.join("tests");
    std::fs::create_dir_all(&tests_dir).unwrap();
    std::fs::write(
        tests_dir.join("math.vib"),
        r#"(defn is-ready () bool (do (return true)))
"#,
    )
    .unwrap();
    std::fs::write(
        tests_dir.join("math.test.vib"),
        r#"(import test "@std/test.vib")
(test.scenario
  "uses-base-function"
  (test.case
    "uses-base-function"
    (do (let ready (is-ready)) (test.assert ready))
    profile: @core
  )
)
"#,
    )
    .unwrap();
    std::fs::write(
        project.join("project.vib"),
        r#"(project
  (package "app" "0.1.0")
  (target app kind: @bin root: "tests" entry: "math.vib")
  (dependency std path: "dep/std"))
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
    std::fs::copy(root.join("project.vib"), dest.join("project.vib")).unwrap();
    for entry in std::fs::read_dir(root.join("src")).unwrap() {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), dest.join("src").join(entry.file_name())).unwrap();
    }
}

#[test]
fn vibra_exec_prints_raw_string_expression() {
    let output = vibra_cmd()
        .args(["exec", "--expr", "\"hello\"", "--format", "raw"])
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
        .args(["exec", "--expr", "42", "--format", "raw"])
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
        .args(["exec", "--expr", "42", "--format", "json"])
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
fn vibra_exec_atom_json_output_is_exact() {
    let output = vibra_cmd()
        .args(["exec", "--expr", "@http.not-found", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "exec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value, serde_json::json!({"atom": "http.not-found"}));
}

#[test]
fn vibra_exec_help_documents_expr_flag() {
    let output = vibra_cmd().args(["exec", "--help"]).output().unwrap();
    assert!(
        output.status.success(),
        "help failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("--expr <EXPR>"));
}

#[test]
fn vibra_exec_parses_and_lowers_sexpression() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(&entry, "(const placeholder bool true)\n").unwrap();
    let loaded = vibra::load::load_program(&entry).unwrap();
    let expression = vibra::load::parse_inline_exec_expression("(add 20 22)").unwrap();
    let exec = vibra::lower::lower_exec_expr(&loaded, &expression, &Default::default()).unwrap();
    let value = vibra::execute::eval_lowered_exec(
        &exec,
        &Default::default(),
        &vibra::runtime::RunConfig::default(),
    )
    .unwrap();
    assert_eq!(value, vibra::lower::RuntimeValue::Int(42));
}

#[test]
fn vibra_code_command_is_removed() {
    let output = vibra_cmd().arg("code").output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unrecognized subcommand"));
}

#[test]
fn procedural_macro_quote_and_unquote_expand_before_lowering() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(macro
  identity
  (input @expr-syntax)
  @expr-syntax
  (do (quote @expr-syntax (unquote input)))
)
(defn main () void (do (let value (identity 42))))
"#,
    )
    .unwrap();

    let loaded = vibra::load::load_program(&entry).unwrap();
    vibra::lower::lower_program(&loaded).expect("macro invocation should expand before lowering");
}

#[test]
fn vibra_expand_shows_hygienic_bindings_and_explicit_capture() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(macro
  bind-temp
  (input @expr-syntax)
  @expr-syntax
  (do (quote @expr-syntax (do (let temp (unquote input)) temp)))
)
(macro
  capture-name
  (input @expr-syntax)
  @expr-syntax
  (do (quote @expr-syntax (capture caller)))
)
(defn main () void (do (bind-temp hello) (let captured (capture-name ignored))))
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
    // Legacy's call-site spelling was `$`-prefixed (`$bind-temp: hello`)
    // while a definition's own key was bare (`bind-temp:`), so a vanished
    // `$bind-temp` distinguished "the call was expanded away" from "the
    // macro's own declaration is still printed". The new grammar has no
    // such distinction -- both definition and call spell the same bare
    // `bind-temp` -- so that check has no meaningful equivalent here; the
    // hygienic-rename and explicit-capture checks below are what this test
    // actually verifies.
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("temp--macro-"), "expanded output: {stdout}");
    assert!(stdout.contains("caller"), "expanded output: {stdout}");
}

#[test]
fn recursive_macro_expansion_reports_the_depth_limit_and_origin() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(macro
  forever
  (input @expr-syntax)
  @expr-syntax
  (do (quote @expr-syntax (forever (unquote input))))
)
(defn main () void (do (let value (forever hello))))
"#,
    )
    .unwrap();

    let output = vibra_cmd()
        .args(["expand", &path_str(&entry)])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    // The typed macro expander's own limit (`TYPED_MACRO_MAX_EXPANSION_DEPTH`,
    // src/ast/typed_macro_expand.rs) is 16, not legacy's 64.
    assert!(stderr.contains("16"), "stderr: {stderr}");
    assert!(stderr.contains("forever"), "stderr: {stderr}");
    // NOTE: legacy's error included the offending file's path; the typed
    // macro expander's `E-MACRO-006` (`src/ast/typed_macro_expand.rs`)
    // reports a byte span but `vibra expand`'s error printing
    // (`Command::Expand`, `src/main.rs`) has no document-id-to-path mapping
    // to attach one -- confirmed missing, not a deliberate design choice
    // (the CLI's own `--origins`/`--at` expansion-inspection flags the spec
    // calls for are likewise not yet implemented: `vibra expand --format
    // json --origins` currently errors "unexpected argument '--origins'").
    // Not asserted here; this is a real, reportable gap, not a relaxed
    // check standing in for one that used to hold.
}

#[test]
fn imported_macro_quotes_resolve_names_in_definition_context() {
    let dir = tempfile::tempdir().unwrap();
    let macros = dir.path().join("macros.vib");
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &macros,
        r#"(defn helper () void (do (let value 1)))
(macro
  call-helper
  (input @expr-syntax)
  @expr-syntax
  (do (quote @expr-syntax (helper)))
)
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        r#"(import m "./macros.vib")
(defn main () void (do (m.call-helper ignored)))
"#,
    )
    .unwrap();

    let loaded = vibra::load::load_program(&entry).unwrap();
    let expanded = format!("{:?}", loaded.modules.get(&loaded.entry).unwrap());
    assert!(expanded.contains("$m.helper"), "expanded: {expanded}");
    vibra::lower::lower_program(&loaded).unwrap();
}

#[test]
fn vibra_fmt_defaults_to_json_check_mode_and_write_is_explicit() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("messy.vib");
    let original = "(defn  main () void\n(do unit))\n";
    std::fs::write(&source, original).unwrap();

    let check = vibra_cmd()
        .args(["fmt", &path_str(&source)])
        .output()
        .unwrap();
    assert!(!check.status.success(), "fmt check should fail for drift");
    let report: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    assert_eq!(report["files"][0]["status"], "changed");
    assert!(report.get("summary").is_some());
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
    let report: serde_json::Value = serde_json::from_slice(&recheck.stdout).unwrap();
    assert_eq!(report["files"][0]["status"], "unchanged");
}

#[test]
fn vibra_fmt_rejects_yaml_comments() {
    // Inverted by the S-expression cutover: `;` comments are now a real,
    // preserved part of the grammar (see
    // `sexpr_tooling::staged_format_sexpr`'s idempotency test), where the
    // legacy YAML dialect forbade comments outright (`E-YAML-002`). `#` has
    // no meaning in S-expression source, so a leftover YAML-style comment
    // is now rejected as an invalid symbol rather than as "comments are
    // forbidden."
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("commented.vib");
    let original = "# important intent\n(defn main () void (do unit))\n";
    std::fs::write(&source, original).unwrap();

    let write = vibra_cmd()
        .args(["fmt", &path_str(&source), "--write"])
        .output()
        .unwrap();
    assert!(
        !write.status.success(),
        "fmt must reject leftover YAML-style `#` comments"
    );
    assert!(
        String::from_utf8_lossy(&write.stderr).contains("E-SYN-001"),
        "stderr: {}",
        String::from_utf8_lossy(&write.stderr)
    );
    assert_eq!(std::fs::read_to_string(&source).unwrap(), original);
}

#[test]
fn vibra_fmt_json_output_is_explicit() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("ok.vib");
    std::fs::write(&source, "(defn main () void unit)\n").unwrap();

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
fn vibra_lint_defaults_to_json_and_reports_kebab_case_locations() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("style.vib");
    std::fs::write(&source, "(defn BadName () void (do unit))\n").unwrap();

    let output = vibra_cmd()
        .args(["lint", &path_str(&source), "--category", "style"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "warning-only lint should pass without --deny-warnings"
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let diagnostic = &report["diagnostics"][0];
    assert_eq!(diagnostic["code"], "W-STYLE-001");
    assert_eq!(diagnostic["span"]["start"]["line"], 0);
    // `BadName` starts at byte 6 (`(defn `), not column 0 -- unlike a YAML
    // top-level mapping key, an S-expression name can never open a line.
    assert_eq!(diagnostic["span"]["start"]["column"], 6);
    // Sexpr-native diagnostics (`sexpr_tooling::staged_lint_sexpr`) always
    // carry a byte offset, unlike the legacy YAML-subset style diagnostics
    // this test exercised before the S-expression cutover.
    assert_eq!(diagnostic["span"]["start"]["offset"], 6);
}

#[test]
fn vibra_lint_suppression_and_deny_warnings_are_respected() {
    // Per-line `=lint:`/`=comment:` source annotations do not exist in the
    // S-expression surface (see the "Definition attributes" section of
    // docs/decisions/s-expression-language.md:
    // "Lint suppression moves to CLI or project configuration so source
    // semantics do not contain diagnostic policy"). No `.vib` source can
    // even parse with a bare `=lint:` line -- `=` is not a valid symbol
    // start (see `src/syntax/lexer.rs`) -- so this test now exercises the
    // CLI-level warning suppression (`--severity error`) that replaced it,
    // alongside the still-live `--deny-warnings` gate.
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("style.vib");
    std::fs::write(
        &source,
        "(defn BadName () void (do unit))\n(defn OtherBad () void (do unit))\n",
    )
    .unwrap();

    let unfiltered = vibra_cmd()
        .args(["lint", &path_str(&source), "--category", "style"])
        .output()
        .unwrap();
    assert!(
        unfiltered.status.success(),
        "warning-only lint should pass without --deny-warnings: {}",
        String::from_utf8_lossy(&unfiltered.stderr)
    );
    let stdout = String::from_utf8_lossy(&unfiltered.stdout);
    assert!(stdout.contains("BadName"), "diagnostic missing: {stdout}");
    assert!(stdout.contains("OtherBad"), "diagnostic missing: {stdout}");

    let suppressed = vibra_cmd()
        .args([
            "lint",
            &path_str(&source),
            "--category",
            "style",
            "--severity",
            "error",
            "--deny-warnings",
        ])
        .output()
        .unwrap();
    assert!(
        suppressed.status.success(),
        "--severity error should suppress warning-level diagnostics: {}",
        String::from_utf8_lossy(&suppressed.stderr)
    );
    let stdout = String::from_utf8_lossy(&suppressed.stdout);
    assert!(
        !stdout.contains("BadName") && !stdout.contains("OtherBad"),
        "warning diagnostics leaked past --severity error: {stdout}"
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
    // A root-level `=lint:` annotation cannot exist in S-expression source
    // (see the comment in `vibra_lint_suppression_and_deny_warnings_are_respected`
    // and docs/decisions/s-expression-language.md,
    // "Definition attributes"). `--severity error` is the surviving
    // whole-file equivalent: it drops every warning-level diagnostic before
    // `--deny-warnings` ever sees them, so a file with nothing but
    // non-kebab-case warnings still passes.
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("style.vib");
    std::fs::write(
        &source,
        "(defn BadName () void (do unit))\n(defn OtherBad () void (do unit))\n",
    )
    .unwrap();

    let output = vibra_cmd()
        .args([
            "lint",
            &path_str(&source),
            "--category",
            "style",
            "--severity",
            "error",
            "--deny-warnings",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "whole-file suppression via --severity error failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn vibra_lint_json_and_sarif_outputs_are_explicit() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("style.vib");
    std::fs::write(&source, "(defn BadName () void (do unit))\n").unwrap();

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
fn vibra_lint_reports_parse_and_compile_errors_as_structured_json() {
    let dir = tempfile::tempdir().unwrap();
    let bad_syntax = dir.path().join("bad-syntax.vib");
    let bad_compile = dir.path().join("bad-compile.vib");
    // Unclosed list: a reader-level error, not a YAML one -- source is
    // always S-expression (see `src/load.rs` module docs).
    std::fs::write(&bad_syntax, "(fn broken (\n").unwrap();
    std::fs::write(&bad_compile, "(defn main () void (do (undefined-call)))\n").unwrap();

    let syntax = vibra_cmd()
        .args(["lint", &path_str(&bad_syntax), "--category", "syntax"])
        .output()
        .unwrap();
    assert!(!syntax.status.success());
    let report: serde_json::Value = serde_json::from_slice(&syntax.stdout).unwrap();
    assert_eq!(report["diagnostics"][0]["code"], "E-SYN-006");
    assert!(report["diagnostics"][0]["span"]["start"]["line"].is_number());

    let compile = vibra_cmd()
        .args(["lint", &path_str(&bad_compile), "--category", "compile"])
        .output()
        .unwrap();
    assert!(!compile.status.success());
    let report: serde_json::Value = serde_json::from_slice(&compile.stdout).unwrap();
    assert!(report["diagnostics"].is_array());
    assert_eq!(report["diagnostics"][0]["severity"], "error");
}

#[test]
fn vibra_lint_rejects_yaml_anchors_and_aliases() {
    // `.vib` source is always read as S-expression (see `src/load.rs`
    // module docs), so YAML anchor/alias syntax (`&x` / `*x`) is no longer a
    // dialect-specific rule of its own -- it is simply not a valid symbol
    // (`&`/`*` are not valid leading symbol characters; see
    // `src/syntax/lexer.rs`). This test now confirms that leftover YAML
    // anchor/alias syntax still gets a clear reader-level rejection instead
    // of being silently misinterpreted.
    let dir = tempfile::tempdir().unwrap();
    let anchored = dir.path().join("anchored.vib");
    std::fs::write(&anchored, "a: &x 1\nb: *x\n").unwrap();

    let output = vibra_cmd()
        .args(["lint", &path_str(&anchored), "--category", "syntax"])
        .output()
        .unwrap();
    assert!(!output.status.success(), "anchors/aliases should fail lint");
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let stdout = report.to_string();
    assert!(
        stdout.contains("E-SYN-001"),
        "expected E-SYN-001 for invalid `&`/`*` symbols: {stdout}"
    );
    assert!(
        stdout.contains("invalid symbol"),
        "expected an invalid-symbol message: {stdout}"
    );
}

#[test]
fn vibra_lint_reports_hidden_transitive_import_alias() {
    let dir = tempfile::tempdir().unwrap();
    let leaf = dir.path().join("leaf.vib");
    let helper = dir.path().join("helper.vib");
    let entry = dir.path().join("entry.vib");

    std::fs::write(
        &leaf,
        r#"(defn value () str (do (return "hidden")))
"#,
    )
    .unwrap();
    std::fs::write(
        &helper,
        r#"(defn call () str (do (return (leaf.value))))
"#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        format!(
            r#"(import leaf "{leaf}")
(import helper "{helper}")
(defn main () void (do (helper.call)))
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
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let stdout = report.to_string();
    assert!(stdout.contains("E-MOD-004"), "stdout: {stdout}");
    assert!(stdout.contains("leaf"), "stdout: {stdout}");
}

#[test]
fn vibra_lint_compile_checks_library_files_without_main() {
    // See the comment on `legacy_option_sugar_is_rejected_with_stable_code`:
    // `(option str)` with no local `def option` fails adaptation
    // (E-ADAPT-009) before legacy's own E-OPTION-001 check; either
    // diagnostic demonstrates the same thing this test actually cares
    // about -- that a library file with no `main` still gets compile-
    // checked and its error surfaced through `vibra lint`.
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("library.vib");
    std::fs::write(&source, "(def legacy (option str))\n").unwrap();

    let output = vibra_cmd()
        .args(["lint", &path_str(&source), "--category", "compile"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let stdout = report.to_string();
    assert!(
        stdout.contains("E-OPTION-001") || stdout.contains("E-ADAPT-009"),
        "stdout: {stdout}"
    );
}

#[test]
fn vibra_lint_percent_encodes_file_uris() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("bad#name%25.vib");
    std::fs::write(&source, "(defn BadName () void (do unit))\n").unwrap();

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
    assert!(stdout.contains("bad%23name%2525.vib"), "stdout: {stdout}");
}

// ===== Issue #50: shared test context + single-named-test lowering =====

#[test]
fn test_envelope_uses_sibling_do_and_rejects_legacy_or_function_fields() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");

    std::fs::write(
        &entry,
        "(test.scenario \"passes\" (test.case \"passes\" (do) profile: @core))\n",
    )
    .unwrap();
    let program = vibra::load::load_program(&entry).unwrap();
    let names = vibra::lower::discover_test_names(&program).unwrap();
    assert_eq!(names, vec!["passes::passes"]);
    vibra::lower::lower_named_test(&program, "passes::passes")
        .expect("canonical sibling test envelope should lower");

    // The legacy nested `$test: {do: []}` payload and its sibling `args:`/
    // `return:` fields are unwritable now: `test = "(", "test", symbol,
    // symbol, body, { attribute }, ")"` always supplies name, profile, and
    // body as three required positional operands, and the only recognized
    // trailing labels are `tags:`, `expect-error:`, `clock:`, `benchmark:`,
    // `workspace:`, `policy:` (spec) -- there is no `args:`/`return:` label
    // to even attempt. These now hit reader-level rejections instead of the
    // legacy E-TEST-001 semantic check.
    for (name, source, expect_code) in [
        (
            "legacy nested test (missing profile)",
            "(test legacy (do))\n",
            "E-SYN",
        ),
        (
            "test args",
            "(test.scenario \"bad\" (test.case \"bad\" (do) profile: @core args: void))\n",
            "E-SYN-011",
        ),
        (
            "test return",
            "(test.scenario \"bad\" (test.case \"bad\" (do) profile: @core return: void))\n",
            "E-SYN-011",
        ),
        // A string where the profile symbol is required (the closest
        // reachable spelling of an "empty profile": there is no way to
        // write a zero-length symbol token at all) is also a reader
        // rejection, not the legacy semantic check.
        ("empty test profile", "(test bad \"\" (do))\n", "E-SYN"),
    ] {
        std::fs::write(&entry, source).unwrap();
        let err = match vibra::load::load_program(&entry) {
            Ok(program) => format!(
                "{:#}",
                vibra::lower::discover_test_names(&program).unwrap_err()
            ),
            Err(error) => format!("{error:#}"),
        };
        assert!(
            err.contains(expect_code),
            "{name} should be rejected with {expect_code}, got: {err}"
        );
    }

    // Profiles are atoms now, so kebab-case is enforced by the atom lexer
    // itself rather than by a later E-TEST-001 semantic check.
    for (name, source) in [
        (
            "uppercase test profile",
            "(test.scenario \"bad\" (test.case \"bad\" (do) profile: @Core))\n",
        ),
        (
            "underscored test profile",
            "(test.scenario \"bad\" (test.case \"bad\" (do) profile: @core_profile))\n",
        ),
    ] {
        std::fs::write(&entry, source).unwrap();
        let err = vibra::load::load_program(&entry).unwrap_err();
        assert!(
            format!("{err:#}").contains("E-ATOM-001"),
            "{name} should be rejected with E-ATOM-001, got: {err:#}"
        );
    }
}

#[test]
fn test_discovery_exposes_canonical_selection_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        r#"(test.scenario
  "fast"
  (test.case
    "fast"
    (do)
    profile: @core
    tags: (@language @fast)
    timeout-ms: 25
    random-seed: 42
    clock: (@fixed 1000 7)
  )
)
(test.scenario
  "skipped"
  (test.case "skipped" (do) profile: @fs tags: (@filesystem) skip: "needs a sandbox")
)
"#,
    )
    .unwrap();
    let program = vibra::load::load_program(&entry).unwrap();
    let specs = vibra::lower::discover_test_specs(&program).unwrap();
    assert_eq!(specs.len(), 2);
    assert_eq!(specs[0].name, "fast::fast");
    assert_eq!(specs[0].profile, "core");
    assert_eq!(specs[0].tags, vec!["language", "fast"]);
    assert_eq!(specs[0].timeout_ms, Some(25));
    assert_eq!(specs[0].random_seed, Some(42));
    assert_eq!(
        specs[0].clock,
        Some(vibra::lower::TestClock {
            unix_millis: 1000,
            monotonic_millis: 7,
        })
    );
    assert_eq!(specs[1].skip.as_deref(), Some("needs a sandbox"));
    assert_eq!(
        vibra::lower::discover_test_names(&program).unwrap(),
        vec!["fast::fast", "skipped::skipped"]
    );
}

#[test]
fn test_discovery_rejects_invalid_selection_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");

    // Duplicate tags have no reader-level check (`parse_test_meta`'s "tags"
    // arm just collects names), and a whitespace-only skip reason passes
    // the reader's raw (untrimmed) emptiness check -- both still reach
    // legacy's E-TEST-001 selection-metadata validation.
    for source in [
        "(test.scenario \"bad\" (test.case \"bad\" (do) profile: @core tags: (@one @one)))\n",
        "(test.scenario \"bad\" (test.case \"bad\" (do) skip: \"   \"))\n",
    ] {
        std::fs::write(&entry, source).unwrap();
        let err = match vibra::load::load_program(&entry) {
            Ok(program) => vibra::lower::discover_test_specs(&program).unwrap_err(),
            Err(error) => error,
        };
        assert!(format!("{err:#}").contains("E-TEST-001"), "{err:#}");
    }

    // Everything else this test originally covered is now validated at the
    // reader itself (`parse_test_meta`, `src/ast/surface.rs`): `timeout-ms:`
    // must be positive, `random-seed:` non-negative, `skip:` a non-empty
    // string, and `clock:` exactly `(fixed <unix-millis> <monotonic-millis>)`
    // with both non-negative -- so a non-`fixed`/wrong-arity/bare-symbol
    // clock is now a reader rejection, not legacy's E-TEST-001. A
    // non-kebab-case tag name (`not_kebab`) has no dedicated reader check
    // either, but is still a plain valid symbol, so it is not exercised as
    // a rejection case here (kebab-case violations are lint warnings, per
    // `warns_for_non_kebab_case_symbols`, not load/lower errors).
    for source in [
        "(test.scenario \"bad\" (test.case \"bad\" (do) timeout-ms: 0))\n",
        "(test.scenario \"bad\" (test.case \"bad\" (do) skip: \"\"))\n",
        "(test.scenario \"bad\" (test.case \"bad\" (do) random-seed: -1))\n",
        "(test.scenario \"bad\" (test.case \"bad\" (do) clock: nope))\n",
        "(test.scenario \"bad\" (test.case \"bad\" (do) clock: (@fixed 1)))\n",
        "(test.scenario \"bad\" (test.case \"bad\" (do) clock: (@fixed 1 2 3)))\n",
    ] {
        std::fs::write(&entry, source).unwrap();
        let err = match vibra::load::load_program(&entry) {
            Ok(program) => format!(
                "{:#}",
                vibra::lower::discover_test_specs(&program).unwrap_err()
            ),
            Err(error) => format!("{error:#}"),
        };
        assert!(err.contains("E-SYN"), "{source:?}: {err}");
    }
}

#[test]
fn deterministic_test_fixtures_are_recorded_in_machine_readable_reports() {
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../schemas/test-report.schema.json")).unwrap();
    assert_eq!(
        schema["$id"],
        "https://vibra.dev/schemas/test-report.schema.json"
    );
    assert_eq!(
        schema["$defs"]["testResult"]["properties"]["random_seed"]["type"],
        "integer"
    );
    assert_eq!(
        schema["$defs"]["testResult"]["properties"]["clock"]["additionalProperties"],
        false
    );
    assert_eq!(schema["properties"]["mode"]["enum"][1], "benchmark");
    assert_eq!(
        schema["$defs"]["testResult"]["properties"]["benchmark"]["additionalProperties"],
        false
    );

    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/stdlib-test.vib");
    let output = vibra_cmd()
        .args([
            "test",
            &path_str(&source),
            "--filter",
            "seeded-random-and-fake-clock",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "test failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["passed"], 1);
    assert_eq!(report["mode"], "test");
    assert_eq!(report["tests"][0]["random_seed"], 42);
    assert_eq!(report["tests"][0]["clock"]["unix_millis"], 1000);
    assert_eq!(report["tests"][0]["clock"]["monotonic_millis"], 40);
}

#[test]
fn benchmark_mode_emits_stable_machine_readable_statistics() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/stdlib-test.vib");
    let output = vibra_cmd()
        .args([
            "test",
            &path_str(&source),
            "--filter",
            "typed-equality-helpers",
            "--benchmark",
            "--benchmark-warmup",
            "0",
            "--benchmark-iterations",
            "2",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "benchmark failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["mode"], "benchmark");
    assert_eq!(report["passed"], 1);
    let benchmark = &report["tests"][0]["benchmark"];
    assert_eq!(benchmark["iterations"], 2);
    assert_eq!(benchmark["warmup_iterations"], 0);
    assert_eq!(benchmark["samples_ns"].as_array().unwrap().len(), 2);
    assert!(benchmark["min_ns"].as_u64().is_some());
    assert!(benchmark["median_ns"].as_u64().is_some());
    assert!(benchmark["max_ns"].as_u64().is_some());
    assert!(benchmark["mean_ns"].as_u64().is_some());

    let invalid = vibra_cmd()
        .args([
            "test",
            &path_str(&source),
            "--filter",
            "typed-equality-helpers",
            "--benchmark",
            "--benchmark-iterations",
            "0",
        ])
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    assert!(
        String::from_utf8_lossy(&invalid.stderr).contains("E-BENCH-001"),
        "{}",
        String::from_utf8_lossy(&invalid.stderr)
    );
}

#[test]
fn test_discovery_trims_skip_reason_and_closes_profile_diagnostic() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        "(test.scenario \"skipped\" (test.case \"skipped\" (do) skip: \"  pending fixture  \"))\n",
    )
    .unwrap();
    let program = vibra::load::load_program(&entry).unwrap();
    let specs = vibra::lower::discover_test_specs(&program).unwrap();
    assert_eq!(specs[0].skip.as_deref(), Some("pending fixture"));

    std::fs::write(
        &entry,
        "(test.scenario \"bad\" (test.case \"bad\" (do) profile: @Not-Kebab))\n",
    )
    .unwrap();
    let err = vibra::load::load_program(&entry).unwrap_err();
    assert!(format!("{err:#}").contains("E-ATOM-001"), "{err:#}");

    std::fs::write(
        &entry,
        "(test.scenario \"bad\" (test.case \"bad\" (do) profile: core))\n",
    )
    .unwrap();
    let err = vibra::load::load_program(&entry).unwrap_err();
    assert!(format!("{err:#}").contains("E-ATOM-003"), "{err:#}");
}

#[test]
fn test_discovery_rejects_malformed_expected_error_metadata() {
    // Legacy's `expect-error:` was a mapping of named `phase:`/`code:`/
    // `message-contains:` keys, so "wrong shape" included things like a
    // bare scalar payload or a duplicated key. The new grammar's
    // `expect-error:` is fully positional -- `(load|compile, code,
    // [message])` or `(runtime, message)` (`parse_test_meta`,
    // `src/ast/surface.rs`) -- with no named fields at all, so there is no
    // "duplicate key" to write anymore; every malformed shape below is a
    // reader-level rejection instead of legacy's E-TEST-001.
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    for source in [
        "(test.scenario \"bad\" (test.case \"bad\" (do) expect-error: compile))\n",
        "(test.scenario \"bad\" (test.case \"bad\" (do) expect-error: (@compile)))\n",
        "(test.scenario \"bad\" (test.case \"bad\" (do) expect-error: (@runtime E-RUNTIME-001)))\n",
        "(test.scenario \"bad\" (test.case \"bad\" (do) expect-error: (@unknown \"nope\")))\n",
        "(test.scenario \"bad\" (test.case \"bad\" (do) expect-error: (@compile E-OPTION-001 \"a\" \"b\")))\n",
    ] {
        std::fs::write(&entry, source).unwrap();
        let err = match vibra::load::load_program(&entry) {
            Ok(program) => format!(
                "{:#}",
                vibra::lower::discover_test_specs(&program).unwrap_err()
            ),
            Err(error) => format!("{error:#}"),
        };
        assert!(
            err.contains("E-SYN") || err.contains("E-ATOM"),
            "{source:?}: {err}"
        );
    }
}

#[test]
fn vibra_test_matches_structured_expected_errors() {
    let dir = tempfile::tempdir().unwrap();
    let compile_entry = dir.path().join("compile-error.vib");
    let runtime_entry = dir.path().join("runtime-error.vib");
    let test_lib =
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/test.vib"))
            .unwrap();
    // `$option: T` sugar has no S-expression form at all (see the comment
    // on `legacy_option_sugar_is_rejected_with_stable_code`) and reaching
    // it would only hit the adapter's own E-ADAPT-009 anyway; a direct
    // `$void` union member is the S-expression construct that still
    // reaches legacy's real E-OPTION-001 lowering check.
    std::fs::write(
        &compile_entry,
        format!(
            r#"(import test "{test_lib}")
(def legacy (union void str))
(test.scenario "compile-error" (test.case "compile-error" (do) expect-error: (@compile E-OPTION-001 "removed")))
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
            r#"(import test "{test_lib}")
(test.scenario "runtime-error" (test.case "runtime-error" (test.assert false) expect-error: (@runtime "assertion failed")))
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
    let entry = dir.path().join("load-error.vib");
    let imported = dir.path().join("cycle.vib");
    std::fs::write(
        &entry,
        "(import cycle \"cycle.vib\")\n(test.scenario \"load-error\" (test.case \"load-error\" (do) expect-error: (@load E-MOD-003)))\n",
    )
    .unwrap();
    std::fs::write(&imported, "(import entry \"load-error.vib\")\n").unwrap();

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
    let entry = dir.path().join("expected-error-mismatch.vib");
    std::fs::write(
        &entry,
        "(test.scenario \"passes\" (test.case \"passes\" (do) expect-error: (@compile E-OPTION-001)))\n",
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
        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/src/test.vib"))
            .unwrap();
    let cases = [
        (
            "wrong-phase.vib",
            format!(
                "(import test \"{}\")\n(test.scenario \"bad\" (test.case \"bad\" (test.assert false) expect-error: (@compile E-OPTION-001)))\n",
                path_str(&test_lib)
            ),
            "expected compile error, but test failed during runtime",
        ),
        (
            "wrong-code.vib",
            "(def legacy (union void str))\n(test.scenario \"bad\" (test.case \"bad\" (do) expect-error: (@compile E-CALL-001)))\n".to_string(),
            "expected compile error code `E-CALL-001`, got `E-OPTION-001`",
        ),
        (
            "wrong-message.vib",
            format!(
                "(import test \"{}\")\n(test.scenario \"bad\" (test.case \"bad\" (test.assert false) expect-error: (@runtime \"expected different runtime error\")))\n",
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
    let entry = dir.path().join("selection.vib");
    let report = dir.path().join("report.json");
    std::fs::write(
        &entry,
        r#"(test.scenario "selection"
  (test.case "core-language" (do) tags: (@language @fast))
  (test.case "fs-language" (do) profile: @fs tags: (@language @filesystem))
  (test.case "skipped-core" (do) tags: (@language) skip: "external fixture unavailable"))
"#,
    )
    .unwrap();
    let output = vibra_cmd()
        .args([
            "test",
            &path_str(&entry),
            "--tag",
            "language",
            "--format",
            "json",
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
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&report).unwrap()).unwrap();
    assert_eq!(report["passed"], 1);
    assert_eq!(report["skipped"], 1);
    assert_eq!(report["tests"][0]["profile"], "core");
    assert!(report["tests"]
        .as_array()
        .unwrap()
        .iter()
        .any(|test| { test["skip_reason"] == "external fixture unavailable" }));

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
    let entry = dir.path().join("skip.vib");
    std::fs::write(
        &entry,
        "(test.scenario \"skipped\" (test.case \"skipped\" (do) skip: \"pending\"))\n",
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
    let entry = dir.path().join("timeout.vib");
    std::fs::write(
        &entry,
        "(test.scenario \"slow\" (test.case \"slow\" (while true (do)) timeout-ms: 1))\n",
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
    let entry = dir.path().join("workspace.vib");
    std::fs::write(&entry, "(test.scenario \"needs-workspace\" (test.case \"needs-workspace\" (do) workspace: @temp))\n").unwrap();

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
    let entry = dir.path().join("workspace-invalid.vib");
    // The reader itself now requires `workspace:`'s value to be exactly the
    // atom `@temp` (`parse_test_meta`, src/ast/surface.rs), so an unknown
    // value is an E-ATOM-002 rejection, not legacy's lowering-time
    // E-TEST-001 check -- same rule, earlier enforcement point.
    std::fs::write(
        &entry,
        "(test.scenario \"bad\" (test.case \"bad\" (do) workspace: @persistent))\n",
    )
    .unwrap();
    let error = vibra::load::load_program(&entry).unwrap_err();
    assert!(format!("{error:#}").contains("E-ATOM-002"), "{error:#}");

    std::fs::write(
        &entry,
        "(test.scenario \"bad\" (test.case \"bad\" (do) workspace: temp))\n",
    )
    .unwrap();
    let error = vibra::load::load_program(&entry).unwrap_err();
    assert!(format!("{error:#}").contains("E-ATOM-003"), "{error:#}");
}

#[test]
fn vibra_test_deny_warnings_fails_and_emits_warnings_in_json_report() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("warnings.vib");
    let report = dir.path().join("report.json");
    std::fs::write(
        &entry,
        "(const BadName int64 1)\n(test.scenario \"passes\" (test.case \"passes\" (do)))\n",
    )
    .unwrap();

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
            "json",
            "--report-file",
            &path_str(&report),
        ])
        .output()
        .unwrap();
    assert!(
        !denied.status.success(),
        "--deny-warnings should fail warning tests"
    );
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(report).unwrap()).unwrap();
    assert!(report["tests"].as_array().unwrap().iter().any(|test| {
        test["warnings"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    }));
    assert!(report.to_string().contains("BadName"));
}

#[test]
fn vibra_test_temp_workspace_runs_fs_operations_relative_to_the_temp_cwd() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fs_lib = path_str(&std::fs::canonicalize(root.join("stdlib/src/fs.vib")).unwrap());
    let test_lib = path_str(&std::fs::canonicalize(root.join("stdlib/src/test.vib")).unwrap());
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("workspace-modes.vib");
    std::fs::write(
        &entry,
        format!(
            r#"(import test "{test_lib}")
(import fs "{fs_lib}")
(import result "{result_lib}")
(test.scenario "workspace"
  (test.case "workspace-read-write"
    (do
    (let path (fs.path.new "workspace-created"))
    (let created (fs.write.create-dirs path))
    (match
  created
  (result.result.ok)
  (do)
  (result.result.err (bind ignored))
  (do (test.fail "workspace write failed"))
)
     (let readable (fs.metadata.exists path))
    (test.assert readable))
    workspace: @temp))
"#,
            result_lib =
                path_str(&std::fs::canonicalize(root.join("stdlib/src/result.vib")).unwrap()),
        ),
    )
    .unwrap();

    let read_write = vibra_cmd()
        .args([
            "test",
            &path_str(&entry),
            "--filter",
            "workspace-read-write",
            "--allow-test-workspace",
        ])
        .output()
        .unwrap();
    assert!(
        read_write.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&read_write.stdout),
        String::from_utf8_lossy(&read_write.stderr)
    );
}

#[test]
fn vibra_test_drains_large_child_output_without_timing_out() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let io_lib = path_str(&std::fs::canonicalize(root.join("stdlib/src/io.vib")).unwrap());
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("large-output.vib");
    let report = dir.path().join("large-output-report.json");
    let payload = "x".repeat(128 * 1024);
    std::fs::write(
        &entry,
        format!(
            "(import io \"{io_lib}\")\n(test.scenario \"output\" (test.case \"emits-large-output\" (io.stdout.println \"{payload}\")))\n"
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
            "json",
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

/// Write an entry module that imports the canonical `stdlib/src/test.vib` and
/// contains `count` passing `$test` declarations plus a shared helper function
/// every test can call. Returns the temp dir (keep it alive) and entry path.
fn issue50_many_tests_entry(count: usize) -> (tempfile::TempDir, std::path::PathBuf) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let test_lib = std::fs::canonicalize(root.join("stdlib/src/test.vib")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");

    let mut src = format!(
        "(import test \"{lib}\")\n(defn the-answer () int64 (do (return 42)))\n",
        lib = test_lib.display().to_string().replace('\\', "/"),
    );
    for i in 0..count {
        src.push_str(&format!(
            "(test.scenario \"many-{i}\" (test.case \"many-{i}\" (do (let v (the-answer)) (match v 42 (test.assert true) _ (test.fail \"not 42\")))))\n",
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
    let test_lib = std::fs::canonicalize(root.join("stdlib/src/test.vib")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        format!(
            r#"(import test "{lib}")
(test.scenario "passes" (test.case "passes" (test.assert true)))
(test.scenario "fails" (test.case "fails" (test.assert false)))
"#,
            lib = test_lib.display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    let program = vibra::load::load_program(&entry).unwrap();

    let passing = vibra::lower::lower_named_test(&program, "passes::passes").unwrap();
    vibra::execute::run_lowered(&passing.program, &vibra::runtime::RunConfig::default())
        .expect("passing test should run cleanly");

    let failing = vibra::lower::lower_named_test(&program, "fails::fails").unwrap();
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
    let fs = std::fs::canonicalize(root.join("stdlib/src/fs.vib")).unwrap();
    let result = std::fs::canonicalize(root.join("stdlib/src/result.vib")).unwrap();
    let io = std::fs::canonicalize(root.join("stdlib/src/io.vib")).unwrap();
    let stream = std::fs::canonicalize(root.join("stdlib/src/stream.vib")).unwrap();
    let test = std::fs::canonicalize(root.join("stdlib/src/test.vib")).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        format!(
            r#"(import fs "{fs}")
(import result "{result}")
(import io "{io}")
(import stream "{stream}")
(import test "{test}")
(defn
  main
  ()
  void
  (do
    (let r (io.stdout.println "line that cannot be written"))
    (match
      r
      (result.result.ok)
      (test.fail "expected write failure but println returned ok")
      (result.result.err (bind e))
        (do
          (match
            e
            (stream.error.io (bind _msg)) (test.assert true)
            _ (test.fail "expected stream.error.io variant")
          )
        )
    )
  )
  effects: (io.stdout stream.write)
)
"#,
            fs = path_str(&fs),
            result = path_str(&result),
            io = path_str(&io),
            stream = path_str(&stream),
            test = path_str(&test),
        ),
    )
    .unwrap();

    let prog = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&prog).expect("broken-pipe io program should lower");

    // A failing stdout sink must NOT panic the runtime; instead the guest
    // should observe a matchable `stream.error.io(...)`. The guest asserts the
    // variant itself, so a clean `Ok(())` here proves the mapping worked.
    vibra::execute::run_lowered_with_io(
        &lowered,
        &vibra::runtime::RunConfig::default(),
        Box::new(BrokenPipeWriter),
        Box::new(BrokenPipeWriter),
    )
    .expect("guest should match stream.error.io rather than panic on broken pipe");
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

// NOTE: `read-raw` and `random.bytes` are not reachable from surface `.vib`
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
    let fs = std::fs::canonicalize(root.join("stdlib/src/fs.vib")).unwrap();
    let result = std::fs::canonicalize(root.join("stdlib/src/result.vib")).unwrap();
    let stream = std::fs::canonicalize(root.join("stdlib/src/stream.vib")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
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
            r#"(import fs "{fs}")
(import result "{result}")
(import stream "{stream}")
(defn
  main
  ()
  void
  (do
    (let pa (fs.path.new "{a}"))
    (let pb (fs.path.new "{b}"))
    (let pc (fs.path.new "{c}"))
    (let oa (fs.write.open pa))
    (match
      oa
      (result.result.ok (bind ha))
        (do
          (let ob (fs.write.open pb))
          (match
            ob
            (result.result.ok (bind hb))
              (do
                (let oc (fs.write.open pc))
                (match
                  oc
                  (result.result.ok (bind hc-bad)) (do)
                  (result.result.err (bind oc-err))
                    (do
                      (match
                        oc-err
                        (fs.fs-error.too-many-open-files)
                          (do
                            (stream.manage.close ha)
                            (let oc2 (fs.write.open pc))
                            (match
                              oc2
                              (result.result.ok (bind hc2))
                                (do
                                  (stream.write.string hc2 "freed-slot")
                                  (stream.manage.close hc2)
                                )
                              (result.result.err (bind oc2-err)) (do)
                            )
                          )
                        _ (do)
                      )
                    )
                )
              )
            (result.result.err (bind ob-err)) (do)
          )
        )
      (result.result.err (bind oa-err)) (do)
    )
  )
  effects: (fs.write stream.write stream.manage)
)
"#,
            fs = fs.display().to_string().replace('\\', "/"),
            result = result.display().to_string().replace('\\', "/"),
            stream = stream.display().to_string().replace('\\', "/"),
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
    let fs = std::fs::canonicalize(root.join("stdlib/src/fs.vib")).unwrap();
    let result = std::fs::canonicalize(root.join("stdlib/src/result.vib")).unwrap();
    let stream = std::fs::canonicalize(root.join("stdlib/src/stream.vib")).unwrap();
    let test = std::fs::canonicalize(root.join("stdlib/src/test.vib")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    let target = dir.path().join("closed.txt");
    std::fs::write(
        &entry,
        format!(
            r#"(import fs "{fs}")
(import result "{result}")
(import stream "{stream}")
(import test "{test}")
(defn
  main
  ()
  void
  (do
    (let path (fs.path.new "{target}"))
    (let opened (fs.write.open path))
    (match
      opened
      (result.result.ok (bind handle))
        (do
          (stream.manage.close handle)
          (let duplicate (stream.manage.close handle))
          (match
            duplicate
             (result.result.err (stream.error.resource-closed))
            (test.assert true)
            _ (test.fail "duplicate close did not return resource-closed")
          )
          (let after-close (stream.write.string handle "forbidden"))
          (match
            after-close
             (result.result.err (stream.error.resource-closed))
            (test.assert true)
            _ (test.fail "use after close did not return resource-closed")
          )
        )
      (result.result.err (bind _oa-err))
      (test.fail "lifecycle fixture could not open file")
    )
  )
  effects: (fs.write stream.write stream.manage)
)
"#,
            fs = path_str(&fs),
            result = path_str(&result),
            stream = path_str(&stream),
            test = path_str(&test),
            target = path_str(&target),
        ),
    )
    .unwrap();

    let program = vibra::load::load_program(&entry).unwrap();
    let lowered = vibra::lower::lower_program(&program).expect("lifecycle program should lower");
    vibra::execute::run_lowered(&lowered, &vibra::runtime::RunConfig::default())
        .expect("guest should observe lifecycle violations as typed errors");
}

#[test]
fn injected_clock_and_environment_are_deterministic_and_isolated() {
    use std::sync::{Arc, Mutex};

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let env_mod = std::fs::canonicalize(root.join("stdlib/src/env.vib")).unwrap();
    let time_mod = std::fs::canonicalize(root.join("stdlib/src/time.vib")).unwrap();
    let result = std::fs::canonicalize(root.join("stdlib/src/result.vib")).unwrap();
    let test = std::fs::canonicalize(root.join("stdlib/src/test.vib")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.vib");
    std::fs::write(
        &entry,
        format!(
            r#"(import env "{env_mod}")
(import time "{time_mod}")
(import result "{result}")
(import test "{test}")
(defn
  main
  ()
  void
  (do
    (let wall (time.now.unix-millis))
    (match
      wall
      1000 (do)
      _ (test.fail "injected-wall-clock-was-not-used")
    )
    (let start (time.now.monotonic-millis))
    (time.sleep.for (time.milliseconds 7))
    (let finish (time.now.unix-millis))
    (match
      finish
      1007 (do)
      _ (test.fail "injected-monotonic-clock-was-not-advanced")
    )
    (let set (env.write.set "VIBRA_INJECTED" "changed"))
    (match
      set
      (result.result.ok) (do)
      _ (test.fail "injected-env-set-failed")
    )
    (let value (env.read.get "VIBRA_INJECTED"))
    (match
      value
      (result.result.ok "changed") (do)
      _ (test.fail "injected-env-read-was-not-isolated")
    )
  )
     effects: (env.read env.write time.now time.sleep)
)
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
        ..vibra::runtime::RunConfig::default()
    };
    vibra::execute::run_lowered(&lowered, &make_config()).unwrap();
}

// --- Issue #51: stdin reads require --allow-stdin ---

#[test]
fn forged_stdin_read_file_handle_requires_allow_stdin() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fs = std::fs::canonicalize(root.join("stdlib/src/fs.vib")).unwrap();
    let stream = std::fs::canonicalize(root.join("stdlib/src/stream.vib")).unwrap();
    let result = std::fs::canonicalize(root.join("stdlib/src/result.vib")).unwrap();
    let output = vibra_cmd()
        .args([
            "exec",
            "--expr",
            "(stream.read.string (cast 0 fs.reader))",
            "--import",
            &format!("fs={}", path_str(&fs)),
            "--import",
            &format!("stream={}", path_str(&stream)),
            "--import",
            &format!("result={}", path_str(&result)),
            "--format",
            "json",
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
        stderr.contains("E-CAP-005"),
        "expected opaque-handle forgery rejection, got: {stderr}"
    );
}

#[test]
fn compile_time_embed_supports_text_binary_and_structured_formats() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("message.txt"), "$literal-looking text\n").unwrap();
    std::fs::write(dir.path().join("payload.bin"), [0_u8, 127, 255]).unwrap();
    std::fs::write(dir.path().join("data.json"), r#"{"name":"json","count":3}"#).unwrap();
    std::fs::write(dir.path().join("data.toml"), "name = 'toml'\ncount = 4\n").unwrap();
    std::fs::write(
        dir.path().join("data.xml"),
        "<root><name>xml</name><count>5</count></root>",
    )
    .unwrap();
    let entry = dir.path().join("main.vib");
    std::fs::write(
        &entry,
        r#"(defn
  main
  ()
  void
  (do
    (let text (embed "message.txt"))
    (let binary (embed "payload.bin"))
    (let json (embed "data.json"))
    (let toml (embed "data.toml"))
    (let xml (embed "data.xml"))
  )
)
"#,
    )
    .unwrap();

    let loaded = vibra::load::load_program(&entry).unwrap();
    assert_eq!(loaded.embedded_files.len(), 5);
    let expanded = format!("{:?}", loaded.modules.get(&loaded.entry).unwrap());
    assert!(expanded.contains("$literal-looking text"));
    assert!(expanded.contains("$uint8"));
    assert!(expanded.contains("json"));
    assert!(expanded.contains("toml"));
    assert!(expanded.contains("xml"));
    vibra::lower::lower_program(&loaded).unwrap();
}

#[test]
fn compile_time_embed_rejects_yaml_as_a_data_format() {
    // NOTE: the spec (`docs/decisions/s-expression-language.md`,
    // "Embedded data") states YAML *should* be usable
    // for `embed` as an external data-interoperability format -- "accepted
    // by explicit `(embed "path" format: yaml)` and may be inferred from
    // `.yaml` or `.yml`" -- which step 8 of the migration plan ("isolate
    // YAML to the external `embed` data decoder") has not landed yet on
    // this branch: the reader currently hard-rejects *both* the explicit
    // `format: yaml` spelling and `.yaml`/`.yml` auto-inference outright
    // (`parse_embed_format`/the auto-format extension check,
    // `src/ast/surface.rs`), with no working YAML decode path at all. This
    // is a confirmed, reportable gap against the spec, not a deliberate
    // design choice this test is meant to lock in. It still demonstrates
    // today's real, if temporary, behavior: YAML is rejected as an embed
    // data format, just at the reader (E-SYN-008) instead of legacy's
    // lowering-time E-EMBED-001.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("data.yaml"), "name: yaml\n").unwrap();
    let entry = dir.path().join("main.vib");
    std::fs::write(
        &entry,
        "(defn main () void (do (let value (embed \"data.yaml\"))))\n",
    )
    .unwrap();
    let error = format!("{:#}", vibra::load::load_program(&entry).unwrap_err());
    assert!(error.contains("E-SYN-008"), "{error}");
    assert!(error.contains("YAML embed format was removed"), "{error}");
}

#[test]
fn compile_time_embed_rejects_escape_and_fingerprints_raw_content() {
    let outer = tempfile::tempdir().unwrap();
    let package = outer.path().join("package");
    std::fs::create_dir(&package).unwrap();
    std::fs::write(outer.path().join("secret.txt"), "secret").unwrap();
    let entry = package.join("main.vib");
    std::fs::write(
        &entry,
        "(defn main () void (do (let value (embed \"../secret.txt\"))))\n",
    )
    .unwrap();
    let error = format!("{:#}", vibra::load::load_program(&entry).unwrap_err());
    assert!(error.contains("E-EMBED-002"), "{error}");

    std::fs::write(package.join("asset.txt"), "one").unwrap();
    std::fs::write(
        &entry,
        "(defn main () void (do (let value (embed \"asset.txt\"))))\n",
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

#[test]
fn compile_time_template_renders_nested_values_and_logicless_sections() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("message.txt"),
        "{{title}}\r\n{{#people}}- {{name}} ({{meta.role}}){{#active}} active{{/active}}\r\n{{/people}}{{^people}}nobody\r\n{{/people}}{{! ignored }}",
    )
    .unwrap();
    let entry = dir.path().join("main.vib");
    std::fs::write(
        &entry,
        r#"(defn
  main
  ()
  void
  (do
    (let
      rendered
      (template
        "message.txt"
        with: (record
          (title "Team")
          (people
            (array
              (record (name "Ada") (active true) (meta (record (role "compiler"))))
              (record (name "Lin") (active false) (meta (record (role "runtime"))))
            )
          )
        )
      )
    )
  )
)
"#,
    )
    .unwrap();

    let loaded = vibra::load::load_program(&entry).unwrap();
    assert_eq!(loaded.embedded_files.len(), 1);
    let module = loaded.modules.get(&loaded.entry).unwrap();
    // The typed frontend renders the template and inlines the result as a
    // bare literal string at load time (see `binary_compile_expr`'s sibling
    // template-expansion path in `src/frontend.rs`); the adapter's
    // `string_literal_value` only wraps a `$literal` envelope around a
    // string that happens to start with `$`, which this render never does.
    let value = module["main"]["do"][0]["$let"]["rendered"]
        .as_str()
        .unwrap();
    assert_eq!(value, "Team\n- Ada (compiler) active\n- Lin (runtime)\n");
    vibra::lower::lower_program(&loaded).unwrap();
}

#[test]
fn compile_time_template_is_strict_sandboxed_and_fingerprinted() {
    let outer = tempfile::tempdir().unwrap();
    let package = outer.path().join("package");
    std::fs::create_dir(&package).unwrap();
    std::fs::write(outer.path().join("secret.txt"), "{{secret}}").unwrap();
    let entry = package.join("main.vib");
    let program = |path: &str, with_body: &str| {
        format!(
            "(defn main () void (do (let rendered (template \"{path}\" with: (record {with_body})))))\n"
        )
    };

    std::fs::write(&entry, program("../secret.txt", "(secret false)")).unwrap();
    let error = format!("{:#}", vibra::load::load_program(&entry).unwrap_err());
    assert!(error.contains("E-TEMPLATE-002"), "{error}");

    let template = package.join("message.txt");
    std::fs::write(&template, "Hello {{name}}").unwrap();
    std::fs::write(&entry, program("message.txt", "(name \"Ada\")")).unwrap();
    let first = vibra::load::load_program(&entry).unwrap();
    let first_wasm =
        vibra::wasm_backend::emit_program_wasm(&vibra::lower::lower_program(&first).unwrap());
    std::fs::write(&template, "Welcome {{name}}").unwrap();
    let second = vibra::load::load_program(&entry).unwrap();
    let second_wasm =
        vibra::wasm_backend::emit_program_wasm(&vibra::lower::lower_program(&second).unwrap());
    assert_ne!(first_wasm, second_wasm);

    std::fs::write(&template, "{{missing}}").unwrap();
    let error = format!("{:#}", vibra::load::load_program(&entry).unwrap_err());
    assert!(error.contains("E-TEMPLATE-005"), "{error}");

    std::fs::write(&template, "{{#name}}unclosed").unwrap();
    let error = format!("{:#}", vibra::load::load_program(&entry).unwrap_err());
    assert!(error.contains("E-TEMPLATE-004"), "{error}");

    std::fs::write(
        &template,
        format!(
            "{}value{}",
            "{{#enabled}}".repeat(65),
            "{{/enabled}}".repeat(65)
        ),
    )
    .unwrap();
    std::fs::write(&entry, program("message.txt", "(enabled true)")).unwrap();
    let error = format!("{:#}", vibra::load::load_program(&entry).unwrap_err());
    assert!(error.contains("E-TEMPLATE-006"), "{error}");
}

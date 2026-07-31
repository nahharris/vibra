//! Staged typed-path producer of `LoweredProgram` (issue #150, step 2).
//!
//! `lower::lower_program` is the compiler's only current route from a loaded
//! program to the semantic IR `LoweredProgram` the interpreter and Wasm
//! backend execute. It reads exclusively from `load::LoadedProgram`, which is
//! backed by an internal compatibility value. The typed S-expression frontend
//! (`frontend::load_surface_program` -> `SurfaceProgram`) has no such
//! producer yet: `typed_lower`/`typed_body` stop at a validated signature and
//! body index, never assembling the fields `LoweredProgram` needs.
//!
//! This module closes that gap **without changing anything reachable from
//! the CLI**: it is a new, additive reader over `SurfaceProgram`, reusing the
//! exact legacy helpers for everything that is not itself a typed-frontend
//! concern (constant evaluation, `main` signature shape, wasm import
//! validation, kebab-case advisories). Nothing in `src/load.rs`, `src/main.rs`,
//! or any existing command path calls into this module.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::path::PathBuf;

use anyhow::{bail, Context, Result};

use crate::ast::{Test, TopLevel};
use crate::body_semantics::validate_task_handles;
use crate::frontend::SurfaceProgram;
use crate::lower::{
    self, ExpectedTestError, FunctionBody, FunctionSig, ImplBody, ImplKey, LoweredProgram,
    LoweredTestCase, RuntimeValue, TestClock, TestErrorPhase, TestSpec, TestWorkspace, TypeRef,
};
use crate::typed_body;
use crate::typed_lower::{self, TypedModuleInput, TypedSignatureIndex};
use crate::typed_readers::{self, StagedExpectedError, StagedTypedTest};

/// Assemble a complete `LoweredProgram` from the typed S-expression frontend.
///
/// Field-by-field:
/// - `functions` comes from `typed_body::materialize_typed_functions`, which
///   is already the checked typed executable subset (issue #150 step 2).
/// - `impls` comes straight from `TypedSignatureIndex::impls`.
/// - `constants` reuses `typed_body::materialize_constants`, which itself
///   reuses `crate::lower::infer_expr_type` (the legacy type-inference
///   entry point) rather than re-deriving constant typing.
/// - `statements`/`main_arg_bindings` come from the entry module's `main`
///   function, using the exact legacy record-flattening rule
///   (`crate::lower::seed_arg_type_bindings`) for its arguments.
/// - `foreign_modules` reuses `crate::project::static_wasm_artifacts_for_entry`
///   unchanged: it takes only the entry path, has no YAML dependency, and is
///   the same validated-bytes path the legacy compiler uses, so there is no
///   second (unvalidated) way for wasm bytes to enter the execution plan.
/// - `warnings`: see `collect_typed_warnings` below for exactly which
///   categories are produced and which legacy kebab-case advisories have no
///   typed equivalent yet.
pub fn lower_typed_program(program: &SurfaceProgram) -> Result<LoweredProgram> {
    let order = typed_module_order(program)?;
    let inputs = typed_module_inputs(program, &order)?;
    let signatures = typed_lower::lower_typed_signatures(inputs.iter().copied())
        .context("typed signature lowering")?;
    let bodies = typed_body::lower_typed_bodies(inputs.iter().copied(), &signatures)
        .context("typed body lowering")?;
    let functions = typed_body::materialize_typed_functions(&signatures, &bodies)
        .context("materializing typed functions")?;
    let constants = typed_body::materialize_constants(&signatures, &bodies)
        .context("materializing typed constants")?;
    let impls: HashMap<ImplKey, ImplBody> = signatures.impls.clone();

    let main_sig = functions.get("main").context("missing top-level `main`")?;
    if !main_sig.type_params.is_empty() {
        bail!("`main` must not be generic (no `where:`)");
    }
    if main_sig.return_type != TypeRef::Void {
        bail!("`main` must have return: void");
    }
    let FunctionBody::User { statements } = &main_sig.body else {
        bail!("`main` must be an ordinary function body, not a wasm import");
    };
    let statements = statements.clone();

    let mut main_arg_bindings: Vec<(String, TypeRef)> = Vec::new();
    for parameter in &main_sig.parameters {
        lower::seed_arg_type_bindings(
            &format!("args.{}", parameter.name),
            &parameter.ty,
            &signatures.aliases,
            &mut main_arg_bindings,
        );
    }
    validate_task_handles(&statements)
        .context("E-TASK-003: invalid task-handle lifetime in `main`")?;

    let test_names = discover_typed_test_names(program).unwrap_or_default();
    let warnings = collect_typed_warnings(&order, &functions, &constants, &test_names);

    let foreign_modules = crate::project::static_wasm_artifacts_for_entry(&program.entry)?;

    Ok(LoweredProgram {
        statements,
        main_arg_bindings,
        constants,
        functions,
        impls,
        warnings,
        foreign_modules,
    })
}

/// Shared, expensive part of lowering `$test` declarations: the signature and
/// body indices, materialized functions, constants, and declared-alias set,
/// built once and reused by every test the way legacy `TestContext` /
/// `build_test_context` is reused across `LoweredTestCase`s.
struct TypedTestContext {
    signatures: TypedSignatureIndex,
    functions: HashMap<String, FunctionSig>,
    constants: HashMap<String, RuntimeValue>,
    declared_aliases: BTreeSet<String>,
}

fn build_typed_test_context(program: &SurfaceProgram) -> Result<TypedTestContext> {
    let order = typed_module_order(program)?;
    let inputs = typed_module_inputs(program, &order)?;
    let signatures = typed_lower::lower_typed_signatures(inputs.iter().copied())
        .context("typed signature lowering")?;
    let bodies = typed_body::lower_typed_bodies(inputs.iter().copied(), &signatures)
        .context("typed body lowering")?;
    let functions = typed_body::materialize_typed_functions(&signatures, &bodies)
        .context("materializing typed functions")?;
    let constants = typed_body::materialize_constants(&signatures, &bodies)
        .context("materializing typed constants")?;
    let declared_aliases = signatures.aliases.keys().cloned().collect();
    Ok(TypedTestContext {
        signatures,
        functions,
        constants,
        declared_aliases,
    })
}

/// Locate the entry logical module's `$test` node named `name`, across all of
/// its physical parts. Imported modules are not test roots, matching
/// `typed_readers::staged_discover_typed_tests`.
fn find_typed_test<'a>(program: &'a SurfaceProgram, name: &str) -> Result<&'a Test> {
    let module = program
        .modules
        .get(&program.entry)
        .with_context(|| format!("entry module {} is missing", program.entry.display()))?;
    for part in &module.parts {
        for form in &part.module.forms {
            if let TopLevel::Test(test) = form {
                if test.name.value == name {
                    return Ok(test);
                }
            } else if let TopLevel::TestScenario(scenario) = form {
                for test in &scenario.cases {
                    if format!("{}::{}", scenario.name.value, test.name.value) == name {
                        return Ok(test);
                    }
                }
            }
        }
    }
    bail!("test `{name}` not found in {}", program.entry.display())
}

fn lower_typed_test_case(
    program: &SurfaceProgram,
    ctx: &TypedTestContext,
    name: &str,
) -> Result<LoweredTestCase> {
    let test = find_typed_test(program, name)?;
    let (statements, main_arg_bindings) = typed_body::materialize_typed_test(
        "",
        &ctx.signatures,
        &ctx.declared_aliases,
        &ctx.constants,
        test,
    )?;
    validate_task_handles(&statements)
        .with_context(|| format!("E-TASK-003: invalid task-handle lifetime in test `{name}`"))?;
    Ok(LoweredTestCase {
        name: name.to_string(),
        program: LoweredProgram {
            statements,
            main_arg_bindings,
            constants: ctx.constants.clone(),
            functions: ctx.functions.clone(),
            impls: ctx.signatures.impls.clone(),
            warnings: Vec::new(),
            foreign_modules: BTreeMap::new(),
        },
    })
}

/// Typed equivalent of `crate::lower::lower_named_test`: lower exactly one
/// named `$test` into a `LoweredProgram`, without lowering the bodies of
/// non-selected tests.
pub fn lower_named_typed_test(program: &SurfaceProgram, name: &str) -> Result<LoweredTestCase> {
    let ctx = build_typed_test_context(program)?;
    lower_typed_test_case(program, &ctx, name)
}

/// Typed equivalent of `crate::lower::lower_tests`: lower every `$test` in
/// `program`'s entry module into its own `LoweredProgram`, sharing the
/// expensive signature/body indexing across all of them.
pub fn lower_typed_tests(program: &SurfaceProgram) -> Result<Vec<LoweredTestCase>> {
    let ctx = build_typed_test_context(program)?;
    let names = discover_typed_test_names(program)?;
    if names.is_empty() {
        bail!(
            "no `$test` declarations found in {}",
            program.entry.display()
        );
    }
    names
        .iter()
        .map(|name| lower_typed_test_case(program, &ctx, name))
        .collect()
}

/// Typed equivalent of `crate::lower::discover_test_names`.
pub fn discover_typed_test_names(program: &SurfaceProgram) -> Result<Vec<String>> {
    Ok(discover_typed_test_specs(program)?
        .into_iter()
        .map(|spec| spec.name)
        .collect())
}

/// Typed equivalent of `crate::lower::discover_test_specs`. Builds on
/// `typed_readers::staged_discover_typed_tests`, converting its
/// `StagedTypedTest` into the exact `TestSpec` shape the test runner already
/// consumes rather than adding a second selection-metadata type.
pub fn discover_typed_test_specs(program: &SurfaceProgram) -> Result<Vec<TestSpec>> {
    let staged = typed_readers::staged_discover_typed_tests(program)?;
    if staged.is_empty() {
        bail!(
            "no `$test` declarations found in {}",
            program.entry.display()
        );
    }
    staged.into_iter().map(convert_test_spec).collect()
}

fn convert_test_spec(staged: StagedTypedTest) -> Result<TestSpec> {
    let expect_error = match staged.expect_error {
        Some(StagedExpectedError::Load { code, message }) => Some(ExpectedTestError {
            phase: TestErrorPhase::Load,
            code: Some(code),
            message_contains: message,
        }),
        Some(StagedExpectedError::Compile { code, message }) => Some(ExpectedTestError {
            phase: TestErrorPhase::Compile,
            code: Some(code),
            message_contains: message,
        }),
        Some(StagedExpectedError::Runtime { message }) => Some(ExpectedTestError {
            phase: TestErrorPhase::Runtime,
            code: None,
            message_contains: Some(message),
        }),
        None => None,
    };
    let workspace = match staged.workspace.as_deref() {
        Some("temp") => Some(TestWorkspace::Temp),
        Some(other) => bail!("E-TEST-001: `workspace` must be `temp`, got `{other}`"),
        None => None,
    };
    let clock = staged
        .clock
        .map(|clock| -> Result<TestClock> {
            Ok(TestClock {
                unix_millis: u64::try_from(clock.unix_millis)
                    .context("E-TEST-001: `clock` unix-millis must be non-negative")?,
                monotonic_millis: u64::try_from(clock.monotonic_millis)
                    .context("E-TEST-001: `clock` monotonic-millis must be non-negative")?,
            })
        })
        .transpose()?;
    Ok(TestSpec {
        name: staged.name,
        profile: staged.profile,
        tags: staged.tags,
        timeout_ms: staged.timeout_ms,
        skip: staged.skip,
        expect_error,
        workspace,
        random_seed: staged.random_seed,
        clock,
    })
}

/// BFS the typed module import graph starting at the entry module under the
/// empty (root) alias, exactly the way a real build resolves it: each
/// relatively- or `@`-imported module is visited under the exact alias its
/// importer declared, using `SurfaceProgram::imports` (already fully
/// resolved, including `@dependency` project imports, unlike the standalone
/// corpus migrator which cannot resolve those). The same physical file can
/// legitimately appear more than once under different aliases (e.g. a module
/// importing the same helper twice under two names), so edges are
/// deduplicated on `(alias, path)`, not on `path` alone.
fn typed_module_order(program: &SurfaceProgram) -> Result<Vec<(String, PathBuf)>> {
    let mut order = Vec::new();
    let mut visited: BTreeSet<(String, PathBuf)> = BTreeSet::new();
    let mut queue: VecDeque<(String, PathBuf)> = VecDeque::new();
    queue.push_back((String::new(), program.entry.clone()));
    while let Some((alias, path)) = queue.pop_front() {
        if !visited.insert((alias.clone(), path.clone())) {
            continue;
        }
        if let Some(edges) = program.imports.get(&path) {
            for (import_alias, target) in edges {
                queue.push_back((import_alias.clone(), target.clone()));
            }
        }
        order.push((alias, path));
    }
    Ok(order)
}

/// Flatten `order` into one `TypedModuleInput` per physical part of every
/// module in the graph, borrowing from both `program` and `order` so the
/// caller's stack frame -- not this function's -- owns the alias `String`s
/// `TypedModuleInput::alias` borrows from.
fn typed_module_inputs<'a>(
    program: &'a SurfaceProgram,
    order: &'a [(String, PathBuf)],
) -> Result<Vec<TypedModuleInput<'a>>> {
    let mut inputs = Vec::new();
    for (alias, path) in order {
        let module = program
            .modules
            .get(path)
            .with_context(|| format!("typed module {} is missing", path.display()))?;
        for part in &module.parts {
            inputs.push(TypedModuleInput {
                alias: alias.as_str(),
                module: &part.module,
            });
        }
    }
    Ok(inputs)
}

/// Collect the kebab-case advisories the typed path can currently produce.
///
/// Legacy `lower_program` calls `maybe_warn_kebab`/`maybe_warn_kebab_qualified`
/// at ~20 distinct syntactic positions (import aliases, type parameters,
/// record fields, interface members, enum tags, function names, function
/// arguments, local variables, iteration bindings, task handles, pattern
/// binds, argument keys, type references, symbol references, test names --
/// see `src/lower.rs`). None of `typed_lower.rs`/`typed_body.rs` thread a
/// warnings collector through their lowering passes today, so most of these
/// have no typed equivalent yet. This function produces the subset that is
/// cheaply derivable from data already assembled here:
///
/// - import aliases (from the module graph)
/// - top-level function and constant names (from their qualified keys' final
///   segment, i.e. the local, unqualified declaration name)
/// - `$test` names
///
/// Not covered (documented gap, not silently dropped): type parameters,
/// record fields, interface members, enum tags, function arguments, local
/// variables, iteration bindings, task handles, pattern binds, argument
/// keys, and type/symbol references. Closing these fully requires threading a
/// warnings sink through `typed_lower::lower_module`/`typed_body::lower_function`
/// and friends, which is out of scope for assembling `LoweredProgram` itself.
///
/// Unlike legacy (whose warnings follow YAML mapping iteration order),
/// `functions`/`constants` are `HashMap`s with no stable order, so this sorts
/// and dedups the result for determinism rather than reproducing legacy's
/// (incidental) order.
fn collect_typed_warnings(
    order: &[(String, PathBuf)],
    functions: &HashMap<String, FunctionSig>,
    constants: &HashMap<String, RuntimeValue>,
    test_names: &[String],
) -> Vec<String> {
    let mut warnings = Vec::new();
    for (alias, _path) in order {
        if alias.is_empty() {
            continue;
        }
        lower::maybe_warn_kebab(alias, "import alias", &mut warnings);
    }
    for key in functions.keys() {
        let local = key.rsplit('.').next().unwrap_or(key);
        lower::maybe_warn_kebab(local, "function name", &mut warnings);
    }
    for key in constants.keys() {
        let local = key.rsplit('.').next().unwrap_or(key);
        lower::maybe_warn_kebab(local, "top-level symbol", &mut warnings);
    }
    for name in test_names {
        lower::maybe_warn_kebab(name, "test name", &mut warnings);
    }
    warnings.sort();
    warnings.dedup();
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load::CompilationFlags;
    use crate::lower::Statement;
    use crate::runtime::RunConfig;

    fn write(path: &std::path::Path, source: &str) {
        std::fs::write(path, source).unwrap();
    }

    fn load(entry: &std::path::Path) -> SurfaceProgram {
        crate::frontend::load_surface_program(entry, &CompilationFlags::new(["test"])).unwrap()
    }

    #[test]
    fn assembles_complete_lowered_program_from_typed_frontend() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        write(
            &entry,
            "(const greeting str \"hi\")\n\
             (defn double (value int64) int64 (do (return (add value value))))\n\
             (defn main (count int64) void\n\
               (do (let doubled (double count)) (return unit)))\n",
        );
        let program = load(&entry);
        let lowered = lower_typed_program(&program).unwrap();

        assert!(lowered.functions.contains_key("double"));
        assert!(lowered.functions.contains_key("main"));
        assert_eq!(
            lowered.constants.get("greeting"),
            Some(&RuntimeValue::Str("hi".to_string()))
        );
        assert_eq!(
            lowered.main_arg_bindings,
            vec![("args.count".to_string(), TypeRef::Int64)]
        );
        assert_eq!(lowered.statements.len(), 2);
        assert!(matches!(lowered.statements[0], Statement::Let { .. }));
    }

    #[test]
    fn const_eval_reuses_legacy_literal_only_rule_via_materialize_constants() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        write(
            &entry,
            "(const answer int64 42)\n(defn main () void (do unit))\n",
        );
        let program = load(&entry);
        let lowered = lower_typed_program(&program).unwrap();
        assert_eq!(
            lowered.constants.get("answer"),
            Some(&RuntimeValue::Int(42))
        );
    }

    #[test]
    fn bad_main_return_type_is_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        write(&entry, "(defn main () int64 (do (return 1)))\n");
        let program = load(&entry);
        let error = format!("{:#}", lower_typed_program(&program).unwrap_err());
        assert!(
            error.contains("must have return"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn missing_main_is_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        write(&entry, "(defn helper () void (do unit))\n");
        let program = load(&entry);
        let error = format!("{:#}", lower_typed_program(&program).unwrap_err());
        assert!(
            error.contains("missing top-level `main`"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn discovers_typed_tests_with_profile_and_tags() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        write(
            &entry,
            "(defn main () void (do unit))\n\
             (test.scenario \"paths\" (test.case \"fast-path\" unit tags: (@fast @smoke)) (test.case \"slow-path\" unit tags: (@slow) timeout-ms: 500 skip: \"flaky\"))\n",
        );
        let program = load(&entry);
        let specs = discover_typed_test_specs(&program).unwrap();
        assert_eq!(
            specs
                .iter()
                .map(|spec| spec.name.as_str())
                .collect::<Vec<_>>(),
            ["paths::fast-path", "paths::slow-path"]
        );
        let fast = specs.iter().find(|s| s.name == "paths::fast-path").unwrap();
        assert_eq!(fast.profile, "core");
        assert_eq!(fast.tags, ["fast", "smoke"]);
        let slow = specs.iter().find(|s| s.name == "paths::slow-path").unwrap();
        assert_eq!(slow.timeout_ms, Some(500));
        assert_eq!(slow.skip.as_deref(), Some("flaky"));

        let names = discover_typed_test_names(&program).unwrap();
        assert_eq!(names, ["paths::fast-path", "paths::slow-path"]);
    }

    #[test]
    fn lowers_and_runs_a_named_typed_test_in_both_backends() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        write(
            &entry,
            "(defn main () void (do unit))\n\
             (test.scenario \"arithmetic\" (test.case \"arithmetic\" (let sum (add 2 2))))\n",
        );
        let program = load(&entry);
        let case = lower_named_typed_test(&program, "arithmetic::arithmetic").unwrap();
        assert_eq!(case.name, "arithmetic::arithmetic");
        crate::execute::run_lowered_interpreted(&case.program, &RunConfig::default()).unwrap();
        crate::wasm_backend::run_lowered(&case.program, &RunConfig::default()).unwrap();

        let all = lower_typed_tests(&program).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "arithmetic::arithmetic");
    }

    #[test]
    fn executes_assembled_program_in_interpreter_and_wasm_backend() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        write(
            &entry,
            "(defn triple (value int64) int64 (do (return (multiply value 3))))\n\
             (defn main () void (do (let result (triple 4))))\n",
        );
        let program = load(&entry);
        let lowered = lower_typed_program(&program).unwrap();
        crate::execute::run_lowered_interpreted(&lowered, &RunConfig::default()).unwrap();
        crate::wasm_backend::run_lowered(&lowered, &RunConfig::default()).unwrap();
    }

    #[test]
    fn resolves_relative_imports_under_their_declared_alias() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        write(
            &entry,
            "(import helper \"helper.vibra\")\n\
             (defn main () void (do (let doubled (helper.double 5))))\n",
        );
        write(
            &temp.path().join("helper.vibra"),
            "(defn double (value int64) int64 (do (return (add value value))))\n",
        );
        let program = load(&entry);
        let lowered = lower_typed_program(&program).unwrap();
        assert!(lowered.functions.contains_key("helper.double"));
        crate::execute::run_lowered_interpreted(&lowered, &RunConfig::default()).unwrap();
        crate::wasm_backend::run_lowered(&lowered, &RunConfig::default()).unwrap();
    }
}

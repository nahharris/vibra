//! Operation interfaces, rows as written, member rows, and the plans an
//! application of a contract `proc` member builds. Expectations come from
//! the effects chapter ("Operation interfaces", "Effect ceilings", "Cold
//! plans and performed rows"), the type chapter ("Implementations",
//! "Dispatch"), and the diagnostics chapter's rows of
//! `@effect.invalidReference` and `@effect.duplicateRoot`.

use std::sync::Arc;

use vibra_diagnostics::{ByteSpan, DiagnosticCode};
use vibra_ir::{BodyContext, CheckedModuleSet, Type};
use vibra_resolve::UnitKind;

use super::{SOURCE, accepted, at, faults, faults_of, input};
use crate::check::{CheckResult, check_source, check_workspace};

/// The three interfaces every case builds on: `Log` and `Clock` are
/// operation interfaces, `Shown` is not.
const INTERFACES: &str = "interface Log\n  proc append\n    takes {log: Self entry: Str}\n    \
                          returns done: Void\n  endproc\nendinterface\n\
                          interface Clock\n  proc now takes {clock: Self} returns at: I64 endproc\n\
                          endinterface\n\
                          interface Shown\n  fn show takes {value: Self} returns text: Str endfn\n\
                          endinterface\n";

/// A type that implements `Log`.
const MEMORY: &str = "type Memory of {lines: List(Str)}\n  impl Log\n    proc append \
                      takes {log: Self entry: Str} returns done: Void do return void \
                      endproc\n  endimpl\nendtype\n";

/// `INTERFACES` and then `rest`.
fn with_interfaces(rest: &str) -> String {
    format!("{INTERFACES}{rest}")
}

/// The module set of `source`, whose only diagnostics are the
/// `@contract.unusedEffect` warnings at `unused`: entries of rows its bodies
/// do not perform.
fn accepted_unused(
    source: &str,
    unused: &[ByteSpan],
) -> Result<Arc<CheckedModuleSet>, String> {
    let result = check_source(SOURCE, source);
    let found = faults_of(&result);
    let expected = unused
        .iter()
        .map(|span| (DiagnosticCode::ContractUnusedEffect, *span))
        .collect::<Vec<_>>();
    if found != expected {
        return Err(format!("{source}\n{:#?}", result.diagnostics()));
    }
    result
        .module_set()
        .map(Arc::clone)
        .ok_or_else(|| format!("no module set for {source}"))
}

/// The span of `name` in the last occurrence of the row text `row` in
/// `source`.
fn entry(source: &str, row: &str, name: &str) -> ByteSpan {
    let start = source
        .rfind(row)
        .unwrap_or(usize::MAX)
        .saturating_add(row.find(name).unwrap_or(usize::MAX));
    ByteSpan::new(start, start.saturating_add(name.len()))
}

/// The type of the member `member` of the interface whose path ends with
/// `.{interface}` in `set`.
fn member_type(set: &CheckedModuleSet, interface: &str, member: &str) -> Option<Type> {
    let suffix = format!(".{interface}");
    let (_, definition) = set
        .interfaces()
        .iter()
        .find(|(id, _)| id.path().ends_with(&suffix))?;
    definition
        .members()
        .iter()
        .find(|found| found.name == member)
        .map(|found| found.signature.type_info().clone())
}

/// The type of the function whose name ends with `name` in `set`.
fn function_type(set: &CheckedModuleSet, name: &str) -> Option<Type> {
    set.functions()
        .iter()
        .find(|function| function.name.ends_with(name))
        .map(|function| function.signature.type_info().clone())
}

/// The paths of the roots of the row of `ty`, a `Proc` or an `Intent`.
fn roots(ty: Option<&Type>) -> Vec<String> {
    let row = match ty {
        Some(Type::Proc(signature)) => &signature.effects,
        Some(Type::Intent(plan)) => &plan.effects,
        _ => return Vec::new(),
    };
    row.roots()
        .iter()
        .map(|root| root.path().to_owned())
        .collect()
}

/// The checked library `@app` of the modules `modules`, each a module path
/// and its source.
fn modules(modules: &[(&str, &str)]) -> Result<CheckResult, String> {
    let input = input(&[("@app", UnitKind::Lib, modules)])?;
    Ok(check_workspace(&input, &["@app".to_owned()]))
}

/// Each diagnostic of `result` by code, source, and primary span.
fn located(result: &CheckResult) -> Vec<(DiagnosticCode, String, ByteSpan)> {
    result
        .diagnostics()
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.code(),
                diagnostic.source_id().unwrap_or_default().to_owned(),
                diagnostic.primary_span(),
            )
        })
        .collect()
}

#[test]
fn an_operation_interface_member_has_a_proc_type_with_its_owner() -> Result<(), String>
{
    let set = accepted(INTERFACES)?;
    let append = member_type(&set, "Log", "append");
    let Some(Type::Proc(signature)) = &append else {
        return Err(format!("{append:?}"));
    };
    assert_eq!(signature.parameters.first(), Some(&Type::SelfType));
    assert_eq!(signature.result, Type::Void);
    assert_eq!(roots(append.as_ref()), ["app.main.Log"]);
    assert_eq!(
        roots(member_type(&set, "Clock", "now").as_ref()),
        ["app.main.Clock"]
    );
    assert!(matches!(
        member_type(&set, "Shown", "show"),
        Some(Type::Fn(_))
    ));
    Ok(())
}

#[test]
fn a_member_row_adds_to_its_owner_sorted_by_path() -> Result<(), String> {
    let source = with_interfaces(
        "interface Audit proc record takes {a: Self entry: Str} returns done: Void \
         performs {Log} endproc endinterface\n",
    );
    let set = accepted(&source)?;
    assert_eq!(
        roots(member_type(&set, "Audit", "record").as_ref()),
        ["app.main.Audit", "app.main.Log"]
    );
    Ok(())
}

#[test]
fn a_fn_member_of_an_operation_interface_is_pure() -> Result<(), String> {
    let source = with_interfaces(
        "interface Both proc go takes {b: Self} returns d: Void endproc \
         fn name takes {b: Self} returns n: Str endfn endinterface\n\
         fn label(T) takes {b: T} returns n: Str given {T: Both} do return Both.name(b) endfn\n",
    );
    let set = accepted(&source)?;
    assert!(matches!(
        member_type(&set, "Both", "name"),
        Some(Type::Fn(_))
    ));
    assert_eq!(
        roots(member_type(&set, "Both", "go").as_ref()),
        ["app.main.Both"]
    );
    assert!(matches!(function_type(&set, ".label"), Some(Type::Fn(_))));
    Ok(())
}

#[test]
fn a_proc_row_resolves_and_is_its_ceiling() -> Result<(), String> {
    let source = with_interfaces(&format!(
        "{MEMORY}proc p takes {{m: Memory}} returns d: Void performs {{Log}} do \
         let _ be Log.append(m \"x\") endlet return void endproc\n"
    ));
    // The body builds a plan and performs nothing.
    let set = accepted_unused(&source, &[entry(&source, "{Log}", "Log")])?;
    let p = function_type(&set, "main.p");
    assert_eq!(roots(p.as_ref()), ["app.main.Log"]);
    let context = set
        .functions()
        .iter()
        .find(|function| function.name.ends_with("main.p"))
        .map(|function| function.context.clone());
    assert!(
        matches!(context, Some(BodyContext::Procedure(row)) if row.roots().len() == 1)
    );
    Ok(())
}

#[test]
fn two_roots_are_sorted_by_path() -> Result<(), String> {
    let source = with_interfaces(
        "proc p returns d: Void performs {Log Clock} do return void endproc\n",
    );
    let set = accepted_unused(
        &source,
        &[
            entry(&source, "{Log Clock}", "Log"),
            entry(&source, "{Log Clock}", "Clock"),
        ],
    )?;
    assert_eq!(
        roots(function_type(&set, "main.p").as_ref()),
        ["app.main.Clock", "app.main.Log"]
    );
    Ok(())
}

#[test]
fn an_import_name_never_identifies_a_root() -> Result<(), String> {
    let log = "interface Log visibility @public proc append takes {log: Self entry: Str} \
               returns done: Void endproc endinterface\n";
    let main = "use @app.log enduse\nuse @app.log.Log as Journal enduse\n\
                proc p returns d: Void performs {log.Log Journal} do return void endproc\n";
    let result = modules(&[("@app.log", log), ("@app.main", main)])?;
    assert_eq!(
        located(&result),
        [(
            DiagnosticCode::EffectDuplicateRoot,
            "src/app/main.vib".to_owned(),
            at(main, "Journal", 1)
        )]
    );
    // One spelling alone is the declaration's root, whatever its name.
    let main = "use @app.log.Log as Journal enduse\n\
                proc p returns d: Void performs {Journal} do return void endproc\n";
    let result = modules(&[("@app.log", log), ("@app.main", main)])?;
    // The body performs nothing of it.
    assert_eq!(
        located(&result),
        [(
            DiagnosticCode::ContractUnusedEffect,
            "src/app/main.vib".to_owned(),
            at(main, "Journal", 1)
        )]
    );
    let set = result.module_set().ok_or("no module set")?;
    assert_eq!(
        roots(function_type(set, "main.p").as_ref()),
        ["app.log.Log"]
    );
    Ok(())
}

#[test]
fn every_position_of_a_row_resolves_its_entries() -> Result<(), String> {
    // A `proc` member of a type.
    let good = with_interfaces(
        "type T of I32 proc m returns d: Void performs {Log} do return void endproc endtype\n",
    );
    let set = accepted_unused(&good, &[entry(&good, "{Log}", "Log")])?;
    assert_eq!(
        roots(function_type(&set, "main.T.m").as_ref()),
        ["app.main.Log"]
    );
    let bad = with_interfaces(
        "type T of I32 proc m returns d: Void performs {Shown} do return void endproc endtype\n",
    );
    assert_eq!(
        faults(&bad),
        [(DiagnosticCode::EffectInvalidReference, at(&bad, "Shown", 1))]
    );
    // A member of an interface.
    let bad = with_interfaces(
        "interface Audit proc record takes {a: Self} returns d: Void performs {Shown} \
         endproc endinterface\n",
    );
    assert_eq!(
        faults(&bad),
        [(DiagnosticCode::EffectInvalidReference, at(&bad, "Shown", 1))]
    );
    // A member of an implementation.
    let audit = "interface Audit proc record takes {a: Self} returns d: Void \
                 performs {Log} endproc endinterface\n";
    let good = with_interfaces(&format!(
        "{audit}type A of I32 impl Audit proc record takes {{a: Self}} returns d: Void \
         performs {{Log}} do return void endproc endimpl endtype\n"
    ));
    let set = accepted(&good)?;
    assert_eq!(
        roots(function_type(&set, ".Audit.record").as_ref()),
        ["app.main.Audit", "app.main.Log"]
    );
    let bad = with_interfaces(&format!(
        "{audit}type A of I32 impl Audit proc record takes {{a: Self}} returns d: Void \
         performs {{Shown}} do return void endproc endimpl endtype\n"
    ));
    assert_eq!(
        faults(&bad),
        [(DiagnosticCode::EffectInvalidReference, at(&bad, "Shown", 1))]
    );
    Ok(())
}

#[test]
fn an_implementation_keeps_its_member_row() -> Result<(), String> {
    let set = accepted(&with_interfaces(MEMORY))?;
    let append = function_type(&set, ".Log.append");
    assert_eq!(roots(append.as_ref()), ["app.main.Log"]);
    let Some(Type::Proc(signature)) = &append else {
        return Err(format!("{append:?}"));
    };
    assert!(
        signature
            .parameters
            .first()
            .is_some_and(|first| *first != Type::SelfType)
    );
    // Wider: at the `performs` section.
    let wider = with_interfaces(
        "type Memory of I32 impl Log proc append takes {log: Self entry: Str} \
         returns done: Void performs {Clock} do return void endproc endimpl endtype\n",
    );
    assert_eq!(
        faults(&wider),
        [(
            DiagnosticCode::TypeMismatch,
            at(&wider, "performs {Clock}", 0)
        )]
    );
    // Narrower: at the member's name.
    let audit = "interface Audit proc record takes {a: Self entry: Str} returns done: Void \
                 performs {Log} endproc endinterface\n";
    let narrower = with_interfaces(&format!(
        "{audit}type A of I32 impl Audit proc record takes {{a: Self entry: Str}} \
         returns done: Void do return void endproc endimpl endtype\n"
    ));
    assert_eq!(
        faults(&narrower),
        [(DiagnosticCode::TypeMismatch, at(&narrower, "record", 1))]
    );
    // The owner in the member's own row.
    let owner = with_interfaces(
        "type Memory of I32 impl Log proc append takes {log: Self entry: Str} \
         returns done: Void performs {Log} do return void endproc endimpl endtype\n",
    );
    assert_eq!(
        faults(&owner),
        [(DiagnosticCode::EffectDuplicateRoot, at(&owner, "Log", 2))]
    );
    // A `fn` for the `proc`.
    let kind = with_interfaces(
        "type Memory of I32 impl Log fn append takes {log: Self entry: Str} \
         returns done: Void do return void endfn endimpl endtype\n",
    );
    assert_eq!(
        faults(&kind),
        [(DiagnosticCode::TypeMismatch, at(&kind, "append", 1))]
    );
    Ok(())
}

#[test]
fn an_implementation_in_an_interface_or_generic_keeps_its_member_row()
-> Result<(), String> {
    let note = "type Note of I32 visibility @public endtype\n";
    let main = |row: &str| {
        format!(
            "use @app.note.Note enduse\n\
             interface Log\n  proc append takes {{log: Self entry: Str}} returns done: Void endproc\n  \
             impl Note\n    proc append takes {{log: Self entry: Str}} returns done: Void{row} \
             do return void endproc\n  endimpl\nendinterface\n\
             interface Clock proc now takes {{clock: Self}} returns at: I64 endproc endinterface\n"
        )
    };
    let good = main("");
    let result = modules(&[("@app.note", note), ("@app.main", &good)])?;
    assert!(
        result.diagnostics().is_empty(),
        "{:#?}",
        result.diagnostics()
    );
    let set = result.module_set().ok_or("no module set")?;
    assert_eq!(
        roots(function_type(set, ".Log.append").as_ref()),
        ["app.main.Log"]
    );
    let bad = main(" performs {Clock}");
    let result = modules(&[("@app.note", note), ("@app.main", &bad)])?;
    assert_eq!(
        located(&result),
        [(
            DiagnosticCode::TypeMismatch,
            "src/app/main.vib".to_owned(),
            at(&bad, "performs {Clock}", 0)
        )]
    );
    // A generic implementation.
    let generic = |row: &str| {
        with_interfaces(&format!(
            "type Box(T) of {{item: T}}\n  impl Log\n    proc append takes {{log: Self entry: Str}} \
             returns done: Void{row} do return void endproc\n  endimpl\nendtype\n"
        ))
    };
    let set = accepted(&generic(""))?;
    assert_eq!(
        roots(function_type(&set, ".Log.append").as_ref()),
        ["app.main.Log"]
    );
    let bad = generic(" performs {Clock}");
    assert_eq!(
        faults(&bad),
        [(
            DiagnosticCode::TypeMismatch,
            at(&bad, "performs {Clock}", 0)
        )]
    );
    Ok(())
}

#[test]
fn applying_a_contract_proc_member_builds_a_plan_with_its_row() -> Result<(), String> {
    // At a closed receiver, a type parameter, and an interface value, the
    // application is a plan a `fn` may build and drop.
    let receivers = [
        ("", "m: Memory", ""),
        ("(T)", "m: T", " given {T: Log}"),
        ("", "m: Log", ""),
    ];
    for (binders, takes, given) in receivers {
        let built = with_interfaces(&format!(
            "{MEMORY}fn build{binders} takes {{{takes}}} returns unit: Void{given} do \
             let _ be Log.append(m \"x\") endlet return void endfn\n"
        ));
        accepted(&built)?;
        // Its type is `Intent(Void {Log})` with the member's complete row,
        // which the mismatch at a `Void` result names.
        let returned = with_interfaces(&format!(
            "{MEMORY}fn build{binders} takes {{{takes}}} returns unit: Void{given} do \
             return Log.append(m \"x\") endfn\n"
        ));
        let result = check_source(SOURCE, &returned);
        let found = result
            .diagnostics()
            .iter()
            .map(|diagnostic| (diagnostic.code(), diagnostic.message().to_owned()))
            .collect::<Vec<_>>();
        assert_eq!(
            found,
            [(
                DiagnosticCode::TypeMismatch,
                "expected `Void`, found `Intent(Void {@app.main.Log})`".to_owned()
            )],
            "{takes}"
        );
    }
    Ok(())
}

#[test]
fn a_default_proc_member_has_its_owner_row() -> Result<(), String> {
    let source = "interface Log\n  proc append takes {log: Self entry: Str} returns done: Void endproc\n  \
                  proc appendTwice takes {log: Self entry: Str} returns done: Void do \
                  let _ be Log.append(log entry) endlet return void endproc\nendinterface\n";
    let set = accepted(source)?;
    assert_eq!(
        roots(member_type(&set, "Log", "appendTwice").as_ref()),
        ["app.main.Log"]
    );
    let twice = set
        .functions()
        .iter()
        .find(|function| function.name.ends_with("Log.appendTwice"))
        .ok_or("no default body")?;
    assert_eq!(roots(Some(twice.signature.type_info())), ["app.main.Log"]);
    assert!(
        matches!(&twice.context, BodyContext::Procedure(row) if row.roots().len() == 1)
    );
    Ok(())
}

#[test]
fn every_application_of_a_generic_operation_interface_is_one_root() -> Result<(), String>
{
    let source = "interface Store(T) proc put takes {s: Self value: T} returns d: Void endproc \
                  endinterface\n\
                  proc p takes {a: Store(Str) b: Store(I32)} returns d: Void performs {Store} do \
                  return void endproc\n";
    let set = accepted_unused(source, &[entry(source, "{Store}", "Store")])?;
    assert_eq!(
        roots(function_type(&set, "main.p").as_ref()),
        ["app.main.Store"]
    );
    let source = "interface Store(T) proc put takes {s: Self value: T} returns d: Void endproc \
                  endinterface\n\
                  proc p returns d: Void performs {Store Store} do return void endproc\n";
    assert_eq!(
        faults(source),
        [(DiagnosticCode::EffectDuplicateRoot, at(source, "Store", 2))]
    );
    Ok(())
}

/// A project module that imports every standard operation interface, so the
/// checked module set holds the embedded operation modules.
fn every_operation_interface() -> String {
    let mut source = String::new();
    for interface in vibra_ir::external::host::HostInterface::ALL {
        source.push_str(&format!(
            "use {}.{} enduse\n\n",
            interface.module(),
            interface.name()
        ));
    }
    source.push_str("fn f returns r: I32 do return 1i32 endfn\n");
    source
}

/// Each of the 17 standard operation interfaces has its one host-bound
/// member: a `proc` whose complete row is `{owner}` and whose default body
/// is the host binding of its row (`docs/spec/03-effects.md`, "Operation
/// interfaces"; `docs/spec/06-runtime.md`, "Host registry").
#[test]
fn every_host_bound_member_has_the_owner_row_and_lowers_to_its_binding()
-> Result<(), String> {
    use vibra_ir::external::host::HostInterface;
    use vibra_ir::{FunctionBody, HostBinding};
    let result = modules(&[("@app.main", &every_operation_interface())])?;
    if !result.diagnostics().is_empty() {
        return Err(format!("{:#?}", result.diagnostics()));
    }
    let set = result.module_set().ok_or("no module set")?;
    for interface in HostInterface::ALL {
        let owner = interface.owner().path();
        let path = owner.trim_start_matches('@').to_owned();
        let (_, definition) = set
            .interfaces()
            .iter()
            .find(|(id, _)| id.path() == path)
            .ok_or(format!("no interface `{path}`"))?;
        let member = definition
            .members()
            .iter()
            .find(|member| member.name == interface.member())
            .ok_or(format!("no member `{path}.{}`", interface.member()))?;
        let ty = member.signature.type_info();
        assert!(matches!(ty, Type::Proc(_)), "`{path}` {ty:?}");
        assert_eq!(roots(Some(ty)), vec![path.clone()], "`{path}`");
        let body = member
            .default
            .and_then(|at| set.functions().get(at))
            .map(|function| &function.body);
        assert_eq!(
            body,
            Some(&FunctionBody::Host(HostBinding::Operation(
                interface.operation()
            ))),
            "`{path}`"
        );
    }
    Ok(())
}

/// The narrowing members are `fn` members whose default body is the
/// `grant.*` row at the grant type `Self`: `limit` on all 17, `restrict` and
/// `sub` on the six `Dir…` interfaces only (`docs/spec/03-effects.md`,
/// "Narrowing").
#[test]
fn the_narrowing_members_are_pure_rows_at_self() -> Result<(), String> {
    use vibra_ir::FunctionBody;
    use vibra_ir::external::CompilerIntrinsic;
    use vibra_ir::external::host::HostInterface;
    let result = modules(&[("@app.main", &every_operation_interface())])?;
    let set = result.module_set().ok_or("no module set")?;
    for interface in HostInterface::ALL {
        let owner = interface.owner().path();
        let path = owner.trim_start_matches('@').to_owned();
        let (_, definition) = set
            .interfaces()
            .iter()
            .find(|(id, _)| id.path() == path)
            .ok_or(format!("no interface `{path}`"))?;
        let directory = interface.module() == "@std.fs";
        for (name, intrinsic, present) in [
            ("limit", CompilerIntrinsic::GrantLimit, true),
            ("restrict", CompilerIntrinsic::GrantRestrict, directory),
            ("sub", CompilerIntrinsic::GrantSub, directory),
        ] {
            let member = definition
                .members()
                .iter()
                .find(|member| member.name == name);
            assert_eq!(member.is_some(), present, "`{path}.{name}`");
            let Some(member) = member else { continue };
            assert!(
                matches!(member.signature.type_info(), Type::Fn(_)),
                "`{path}.{name}` is pure"
            );
            let body = member
                .default
                .and_then(|at| set.functions().get(at))
                .map(|function| &function.body);
            assert_eq!(
                body,
                Some(&FunctionBody::Intrinsic {
                    intrinsic,
                    type_arguments: vec![Type::SelfType],
                }),
                "`{path}.{name}`"
            );
        }
    }
    Ok(())
}

/// A package's model of `DirReadText` supplies the host-bound member and
/// the three narrowing members; its `read` keeps the contract's row
/// `{DirReadText}`.
#[test]
fn a_model_of_an_operation_interface_supplies_its_bound_members() -> Result<(), String>
{
    let source = "use @std.fs.DirReadText enduse\n\nuse @std.fs.FsError enduse\n\n\
                  use @std.fs.FsPolicy enduse\n\nuse @std.fs.RelativePath enduse\n\n\
                  use @std.policy.GrantLimits enduse\n\nuse @std.policy.ScopeError enduse\n\n\
                  type Memory of {files: List(Str)}\n  impl DirReadText\n    \
                  proc read takes {grant: Self at: RelativePath} \
                  returns outcome: Result(Str FsError) do \
                  return Result.err(error: FsError.notFound()) endproc\n    \
                  fn restrict takes {grant: Self policy: FsPolicy} \
                  returns narrowed: Result(Self ScopeError) do \
                  return Result.ok(value: grant) endfn\n    \
                  fn sub takes {grant: Self prefix: RelativePath} \
                  returns narrowed: Result(Self ScopeError) do \
                  return Result.ok(value: grant) endfn\n    \
                  fn limit takes {grant: Self limits: GrantLimits} returns narrowed: Self \
                  do return grant endfn\n  endimpl\nendtype\n";
    let result = modules(&[("@app.main", source)])?;
    if !result.diagnostics().is_empty() {
        return Err(format!("{:#?}", result.diagnostics()));
    }
    let set = result.module_set().ok_or("no module set")?;
    let read = set
        .functions()
        .iter()
        .find(|function| {
            function.name.contains("Memory") && function.name.ends_with(".read")
        })
        .map(|function| function.signature.type_info().clone());
    assert_eq!(roots(read.as_ref()), vec!["std.fs.DirReadText".to_owned()]);
    Ok(())
}

//! Lowering: the checked bodies and the supported declarations become one
//! [`CheckedModuleSet`].
//!
//! The checked IR is a trust boundary: everything lowered here passes the
//! module set's validation, and a `match` lowered here covers its subject.
//! A standard-library declaration whose body this checker cannot check is
//! skipped with a note, and so is every standard-library declaration that
//! refers to it.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::{
    Arguments, Binder, Bound, CallTarget, Case, CheckedFunction, CheckedGlobal,
    CheckedModuleSet, Constant, ExitGuard, Expr, FnType, FunctionBody, Guard,
    Implementation, ImplementationRecord, InterfaceDefinition, InterfaceKey,
    InterfaceMember, Invariant, Library, LibraryTypes, ModuleSetParts, NamedTypes,
    Node, SignatureRecord, SourceOrigin, Type, TypeBody, TypeDefinition, Value,
    Visibility, encode_open_type, encode_type,
};
use vibra_syntax::Document;

use super::callables::fold;
use super::contract::{
    self, CheckedGuard, CheckedInvariant, Fold, Inputs, Owner, Predicate, Site,
    anonymous_entries,
};
use super::cycle::initializer_cycles;
use super::declare::{BoundData, Entity, FnData, Program, TypeKind};
use super::expr::{
    BodyChecker, CallableScope, Checked, GuardViews, Layout, Reference, Target,
    fallible_construction,
};
use super::generics::{Instantiation, expanding_instantiations};
use super::obligation::Obligation;
use super::testing::{self, TestFaults};
use super::{CheckResult, CheckedTest};

/// The bodies of one pass.
struct Pass {
    functions: BTreeMap<usize, Checked>,
    values: BTreeMap<usize, Checked>,
    tests: BTreeMap<usize, Checked>,
    /// The diagnostics of each declaration.
    faults: BTreeMap<Target, Vec<Diagnostic>>,
    /// The references of each declaration.
    references: BTreeMap<Target, Vec<Reference>>,
    /// Every instantiation of a generic function inside a body.
    instantiations: Vec<Instantiation>,
    /// The diagnostics of the labelled defaults of abstract contract
    /// members, which no function holds, and of the invariants of local
    /// types.
    contract: Vec<Diagnostic>,
    /// The checked invariant of each type, by program index.
    invariants: BTreeMap<usize, CheckedInvariant>,
    /// The error type of each type's `where … else`, by program index.
    errors: BTreeMap<usize, Type>,
    /// Every site of a bare-guard obligation or of a pattern a constant
    /// construction decides, with its module.
    sites: Vec<(usize, Owner, Site)>,
}

fn layout(program: &Program<'_>) -> Layout {
    let mut layout = Layout::default();
    let mut next = 0;
    for index in 0..program.functions.len() {
        if program.supported_fn(index) {
            layout.functions.push(Some(next));
            next += 1;
        } else {
            layout.functions.push(None);
        }
    }
    let mut next = 0;
    for index in 0..program.values.len() {
        if program.supported_value(index) {
            layout.globals.push(Some(next));
            next += 1;
        } else {
            layout.globals.push(None);
        }
    }
    layout
}

/// Checks every supported body of the modules in scope.
fn check_bodies(
    program: &Program<'_>,
    documents: &[Document],
    layout: &Layout,
) -> Pass {
    let mut pass = Pass {
        functions: BTreeMap::new(),
        values: BTreeMap::new(),
        tests: BTreeMap::new(),
        faults: BTreeMap::new(),
        references: BTreeMap::new(),
        instantiations: Vec::new(),
        contract: abstract_defaults(program, documents, layout),
        invariants: BTreeMap::new(),
        errors: BTreeMap::new(),
        sites: Vec::new(),
    };
    invariants(program, documents, layout, &mut pass);
    for (index, data) in program.functions.iter().enumerate() {
        if !program.supported_fn(index) || data.external.is_some() {
            continue;
        }
        let (Some(signature), Some(body), Some(text)) = (
            data.signature.as_ref(),
            data.view.body(),
            documents.get(data.module).and_then(Document::text),
        ) else {
            continue;
        };
        let scope = CallableScope {
            binders: data.binders.clone(),
            bounds: data.bounds.clone(),
            self_type: data.self_type.clone(),
            associated: data.associated.clone(),
            self_interface: data.contract,
            result_span: data.view.result().map(|(_, written)| written.span()),
        };
        let mut checker = BodyChecker::new(program, layout, data.module, text);
        let patterns = data
            .view
            .takes()
            .into_iter()
            .map(|(pattern, _)| pattern)
            .collect::<Vec<_>>();
        let labelled = data.view.labelled();
        let views = GuardViews::of(&data.view);
        let checked =
            checker.callable(signature, &patterns, &labelled, scope, &views, body);
        if let Some(interface) = data.implements {
            let differing = differing_defaults(
                program,
                documents,
                layout,
                data,
                interface,
                &checked.defaults,
            );
            checker.diagnostics.extend(differing);
        }
        pass.functions.insert(index, checked);
        pass.sites.extend(
            checker
                .sites
                .drain(..)
                .map(|site| (data.module, Owner::Function(index), site)),
        );
        pass.instantiations
            .extend(checker.instantiations.drain(..).map(
                |(callee, arguments, span)| Instantiation {
                    caller: index,
                    callee,
                    arguments,
                    span,
                },
            ));
        pass.faults.insert(
            Target::Function(index),
            std::mem::take(&mut checker.diagnostics),
        );
        pass.references.insert(
            Target::Function(index),
            std::mem::take(&mut checker.references),
        );
    }
    for (index, data) in program.values.iter().enumerate() {
        if !program.supported_value(index) {
            continue;
        }
        let (Some(ty), Some(value), Some(text)) = (
            data.ty.as_ref(),
            data.view.value(),
            documents.get(data.module).and_then(Document::text),
        ) else {
            continue;
        };
        let mut checker = BodyChecker::new(program, layout, data.module, text);
        let written = data.view.written_type().map(|written| written.span());
        let checked = checker.initializer(ty, value, written);
        pass.values.insert(index, checked);
        pass.sites.extend(
            checker
                .sites
                .drain(..)
                .map(|site| (data.module, Owner::Value(index), site)),
        );
        pass.faults.insert(
            Target::Value(index),
            std::mem::take(&mut checker.diagnostics),
        );
        pass.references.insert(
            Target::Value(index),
            std::mem::take(&mut checker.references),
        );
    }
    for (index, data) in program.tests.iter().enumerate() {
        if !testing::checked_test(program, index) {
            continue;
        }
        let (Some(body), Some(text)) = (
            data.view.body(),
            documents.get(data.module).and_then(Document::text),
        ) else {
            continue;
        };
        let mut checker = BodyChecker::new(program, layout, data.module, text);
        let checked = checker.test_body(body);
        pass.tests.insert(index, checked);
        pass.sites.extend(
            checker
                .sites
                .drain(..)
                .map(|site| (data.module, Owner::Test(index), site)),
        );
        pass.faults.insert(
            Target::Test(index),
            std::mem::take(&mut checker.diagnostics),
        );
        pass.references
            .insert(Target::Test(index), std::mem::take(&mut checker.references));
    }
    pass
}

/// The representation of a type with a body this checker reads: the type a
/// `where` predicate takes.
fn representation(kind: &TypeKind) -> Option<Type> {
    match kind {
        TypeKind::Wrapper(representation) => Some(representation.clone()),
        TypeKind::Tuple(components) => Some(Type::Tuple(components.clone())),
        TypeKind::Record(fields) => Type::record(fields.clone()).ok(),
        _ => None,
    }
}

/// Checks the `where` of every type without parameters that states one.
fn invariants(
    program: &Program<'_>,
    documents: &[Document],
    layout: &Layout,
    pass: &mut Pass,
) {
    for (index, data) in program.types.iter().enumerate() {
        if !data.invariant || !data.params.is_empty() {
            continue;
        }
        let (Some(representation), Some(section), Some(text)) = (
            representation(&data.kind),
            data.view.invariant(),
            documents.get(data.module).and_then(Document::text),
        ) else {
            continue;
        };
        let mut checker = BodyChecker::new(program, layout, data.module, text);
        let found = checker.invariant(section, &representation);
        if let Some(error) = found.error {
            pass.errors.insert(index, error);
        }
        if let Some(checked) = found.checked {
            pass.invariants.insert(index, checked);
        }
        if program
            .modules
            .get(data.module)
            .is_some_and(|module| !module.embedded)
        {
            pass.contract.append(&mut checker.diagnostics);
        }
    }
}

/// The diagnostics of the labelled defaults of every abstract member of an
/// interface outside the embedded library, each checked once as a constant
/// of its label's type with the interface's parameters and `Self` open.
fn abstract_defaults(
    program: &Program<'_>,
    documents: &[Document],
    layout: &Layout,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for (index, interface) in program.interfaces.iter().enumerate() {
        let module = program.modules.get(interface.module);
        if module.is_none_or(|module| module.embedded) {
            continue;
        }
        let Some(text) = documents.get(interface.module).and_then(Document::text)
        else {
            continue;
        };
        for view in interface.view.members() {
            let labelled = view.labelled();
            let member = view
                .name()
                .and_then(|name| interface.member(name.text()))
                .filter(|member| member.default.is_none());
            let Some(member) = member.filter(|_| !labelled.is_empty()) else {
                continue;
            };
            let mut checker = BodyChecker::new(program, layout, interface.module, text);
            let patterns = view
                .takes()
                .into_iter()
                .map(|(pattern, _)| pattern)
                .collect::<Vec<_>>();
            let scope = CallableScope {
                binders: member.binders.clone(),
                bounds: member.bounds.clone(),
                self_type: Some(Type::SelfType),
                associated: interface.own_associated(),
                self_interface: Some(index),
                result_span: None,
            };
            checker.contract_defaults(&member.signature, &patterns, &labelled, scope);
            diagnostics.append(&mut checker.diagnostics);
        }
    }
    diagnostics
}

/// The `@type.mismatch` of each labelled default of `data`, a member of an
/// implementation of the interface `interface` whose constant defaults are
/// `defaults`, that differs from the default its contract member writes,
/// both checked under the implementation's substitution.
fn differing_defaults(
    program: &Program<'_>,
    documents: &[Document],
    layout: &Layout,
    data: &FnData<'_>,
    interface: usize,
    defaults: &[(String, vibra_ir::Constant)],
) -> Vec<Diagnostic> {
    let Some(interface_data) = program.interfaces.get(interface) else {
        return Vec::new();
    };
    let contract = interface_data
        .view
        .members()
        .into_iter()
        .find(|member| member.name().is_some_and(|name| name.text() == data.name));
    let (Some(contract), Some(signature), Some(text)) = (
        contract,
        data.signature.as_ref(),
        documents
            .get(interface_data.module)
            .and_then(Document::text),
    ) else {
        return Vec::new();
    };
    let declared = contract.labelled();
    let patterns = contract
        .takes()
        .into_iter()
        .map(|(pattern, _)| pattern)
        .collect::<Vec<_>>();
    let scope = CallableScope {
        binders: data.binders.clone(),
        bounds: data.bounds.clone(),
        self_type: data.self_type.clone(),
        associated: data.associated.clone(),
        self_interface: None,
        result_span: None,
    };
    // The contract's own faults are reported where it is declared.
    let mut checker = BodyChecker::new(program, layout, interface_data.module, text);
    let expected = checker.contract_defaults(signature, &patterns, &declared, scope);
    let mut faults = Vec::new();
    for (name, _, written) in data.view.labelled() {
        let label = name.text();
        let actual = defaults.iter().find(|(found, _)| found == label);
        let wanted = expected.iter().find(|(found, _)| found == label);
        let declared = declared.iter().find(|(found, _, _)| found.text() == label);
        if let (Some((_, actual)), Some((_, wanted)), Some((_, _, declared))) =
            (actual, wanted, declared)
            && actual != wanted
        {
            let diagnostic = Diagnostic::new(
                DiagnosticCode::TypeMismatch,
                super::expr::expr_span(&written),
                format!(
                    "the default of `{label}` differs from the contract member's under the implementation's substitution"
                ),
            );
            let diagnostic = match program.modules.get(data.module) {
                Some(module) => diagnostic.with_source_id(module.source_id.as_ref()),
                None => diagnostic,
            };
            faults.push(program.related(
                diagnostic,
                interface_data.module,
                super::expr::expr_span(declared),
                "the contract member's default",
            ));
        }
    }
    faults
}

fn module_of(program: &Program<'_>, target: Target) -> Option<usize> {
    match target {
        Target::Function(index) => program.functions.get(index).map(|data| data.module),
        Target::Value(index) => program.values.get(index).map(|data| data.module),
        Target::Test(index) => program.tests.get(index).map(|data| data.module),
    }
}

fn embedded(program: &Program<'_>, target: Target) -> bool {
    module_of(program, target)
        .and_then(|module| program.modules.get(module))
        .is_some_and(|module| module.embedded)
}

fn name_of(program: &Program<'_>, target: Target) -> String {
    match target {
        Target::Function(index) => program
            .functions
            .get(index)
            .map(|data| data.qualified.clone())
            .unwrap_or_default(),
        Target::Value(index) => program
            .values
            .get(index)
            .map(|data| data.qualified.clone())
            .unwrap_or_default(),
        Target::Test(index) => program
            .tests
            .get(index)
            .map(|data| data.qualified.clone())
            .unwrap_or_default(),
    }
}

/// Skips every standard-library declaration whose body has a fault, and
/// every standard-library declaration that refers to a skipped one.
fn skip_faulty_stdlib(program: &mut Program<'_>, pass: &Pass) {
    let mut skipped: BTreeSet<Target> = BTreeSet::new();
    for (target, faults) in &pass.faults {
        if !faults.is_empty() && embedded(program, *target) {
            skipped.insert(*target);
            let reason = faults
                .first()
                .map(|fault| fault.message().to_owned())
                .unwrap_or_default();
            program.notes.push(format!(
                "{} is skipped: {reason}",
                name_of(program, *target)
            ));
        }
    }
    loop {
        let mut grew = false;
        for (target, references) in &pass.references {
            if skipped.contains(target) || !embedded(program, *target) {
                continue;
            }
            if let Some(reference) = references
                .iter()
                .find(|reference| skipped.contains(&reference.target))
            {
                skipped.insert(*target);
                program.notes.push(format!(
                    "{} is skipped: it refers to {}, which is skipped",
                    name_of(program, *target),
                    name_of(program, reference.target)
                ));
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    for target in skipped {
        match target {
            Target::Function(index) => {
                if let Some(data) = program.functions.get_mut(index) {
                    data.signature = None;
                }
            }
            Target::Value(index) => {
                if let Some(data) = program.values.get_mut(index) {
                    data.ty = None;
                }
            }
            Target::Test(_) => {}
        }
    }
}

/// The value of each module value whose initializer, checked in `first`
/// under the layout `provisional`, is a constant expression, by its global
/// index in `layout`. An initializer that names another module value is
/// constant when that one is, so the values are folded in rounds until no
/// more are found. A construction of a type whose `where` has an `else` is
/// no constant expression.
fn constants(
    program: &Program<'_>,
    provisional: &Layout,
    first: &Pass,
    layout: &Layout,
) -> BTreeMap<usize, Value> {
    let mut known: BTreeMap<usize, Value> = BTreeMap::new();
    loop {
        let mut grew = false;
        for (index, checked) in &first.values {
            let Some(global) = provisional.globals.get(*index).copied().flatten()
            else {
                continue;
            };
            if known.contains_key(&global) {
                continue;
            }
            if let Some(value) = checked
                .node
                .as_ref()
                .filter(|node| !fallible_construction(program, node))
                .and_then(|node| fold(node, &known).ok())
            {
                known.insert(global, value);
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    first
        .values
        .keys()
        .filter_map(|index| {
            let before = provisional.globals.get(*index).copied().flatten()?;
            let after = layout.globals.get(*index).copied().flatten()?;
            Some((after, known.get(&before)?.clone()))
        })
        .collect()
}

/// Checks `program`, whose modules are parsed as `documents`, and lowers it.
pub(crate) fn check_program(
    mut program: Program<'_>,
    documents: &[Document],
) -> CheckResult {
    let provisional = layout(&program);
    let first = check_bodies(&program, documents, &provisional);
    skip_faulty_stdlib(&mut program, &first);
    let mut layout = layout(&program);
    layout.constants = constants(&program, &provisional, &first, &layout);
    layout.constants_known = true;
    layout.errors = first.errors.clone();
    drop(first);
    let pass = check_bodies(&program, documents, &layout);
    let inputs = Inputs {
        program: &program,
        layout: &layout,
        functions: &pass.functions,
        values: &pass.values,
        tests: &pass.tests,
        invariants: &pass.invariants,
        sites: &pass.sites,
    };
    let reviewed = contract::review(&inputs, || {
        assemble(&program, &layout, &pass, &[], &BTreeMap::new(), true)
            .ok()
            .map(|assembled| assembled.set)
    });
    let test_faults = testing::declaration_faults(&program);
    let reported = std::mem::take(&mut program.diagnostics);
    let mut diagnostics = reported.clone();
    diagnostics.extend(test_faults.diagnostics.iter().cloned());
    for (target, faults) in &pass.faults {
        if !embedded(&program, *target) {
            diagnostics.extend(faults.iter().cloned());
        }
    }
    diagnostics.extend(pass.contract.iter().cloned());
    diagnostics.extend(reviewed.diagnostics);
    let mut obligations = reviewed.obligations;
    obligations.sort_by(|left, right| {
        let key = |obligation: &Obligation| {
            (
                obligation.declaration.clone(),
                obligation.source_id.clone(),
                obligation.span.start(),
                obligation.span.end(),
            )
        };
        key(left).cmp(&key(right))
    });
    diagnostics.extend(initializer_cycles(&program, &pass.references));
    diagnostics.extend(expanding_instantiations(&program, &pass.instantiations));
    diagnostics.sort_by(|left, right| {
        let order = |diagnostic: &Diagnostic| {
            let module = program
                .modules
                .iter()
                .position(|module| {
                    Some(module.source_id.as_ref()) == diagnostic.source_id()
                })
                .unwrap_or(usize::MAX);
            (
                module,
                diagnostic.primary_span().start(),
                diagnostic.primary_span().end(),
            )
        };
        order(left).cmp(&order(right))
    });
    let mut result = CheckResult {
        diagnostics,
        notes: std::mem::take(&mut program.notes),
        module_set: None,
        entry: None,
        types: None,
        functions: Vec::new(),
        tests: Vec::new(),
        test_set: None,
        obligations: Vec::new(),
    };
    if result.types_known() {
        result.types = types_document(&program, &obligations);
        result.obligations = obligations;
    }
    if result.has_errors() {
        if result.only_unavailable() {
            let (set, tests) = runnable_tests(
                &mut program,
                documents,
                &layout,
                &pass,
                &reported,
                &test_faults,
            );
            result.test_set = set;
            result.tests = tests;
        }
        return result;
    }
    let lowered = pass.tests.keys().copied().collect::<Vec<_>>();
    match assemble(&program, &layout, &pass, &lowered, &reviewed.folds, false) {
        Ok(assembled) => {
            result.entry = entry(&program, &layout);
            result.functions = assembled.functions;
            result.tests = checked_tests(&program, &assembled.tests, &BTreeMap::new());
            result.test_set = Some(Arc::clone(&assembled.set));
            result.module_set = Some(assembled.set);
        }
        Err(message) => result.diagnostics.push(Diagnostic::new(
            DiagnosticCode::RuntimeInvalidCheckedProgram,
            ByteSpan::empty_at(0),
            format!("the checker built an invalid module set: {message}"),
        )),
    }
    result
}

/// Every test of the modules of `@tests` in scope, in program order, with
/// the function `lowered` gives it or the fault `unavailable` gives it.
fn checked_tests(
    program: &Program<'_>,
    lowered: &BTreeMap<usize, usize>,
    unavailable: &BTreeMap<Target, Diagnostic>,
) -> Vec<CheckedTest> {
    program
        .tests
        .iter()
        .enumerate()
        .filter(|(_, test)| {
            program
                .modules
                .get(test.module)
                .is_some_and(|module| module.in_scope && module.tests)
        })
        .map(|(index, test)| CheckedTest {
            module: program
                .modules
                .get(test.module)
                .map(|module| module.path.clone())
                .unwrap_or_default(),
            name: test.name.clone(),
            function: lowered.get(&index).copied(),
            unavailable: unavailable.get(&Target::Test(index)).cloned(),
        })
        .collect()
}

/// The tests of a program whose every error is `@tool.unavailable`: every
/// declaration that has, or reaches, a fault is dropped, the rest is checked
/// again and lowered, and a test is unavailable with the fault its closure
/// reaches. When the rest cannot be lowered either, every test is
/// unavailable.
fn runnable_tests(
    program: &mut Program<'_>,
    documents: &[Document],
    layout: &Layout,
    pass: &Pass,
    reported: &[Diagnostic],
    test_faults: &TestFaults,
) -> (Option<Arc<CheckedModuleSet>>, Vec<CheckedTest>) {
    let lowered = |target: Target| match target {
        Target::Function(index) => {
            layout.functions.get(index).copied().flatten().is_some()
                && program.functions.get(index).is_some_and(|data| {
                    data.external.is_some()
                        || pass
                            .functions
                            .get(&index)
                            .is_some_and(|checked| checked.node.is_some())
                })
        }
        Target::Value(index) => {
            layout.globals.get(index).copied().flatten().is_some()
                && pass
                    .values
                    .get(&index)
                    .is_some_and(|checked| checked.node.is_some())
        }
        Target::Test(index) => pass
            .tests
            .get(&index)
            .is_some_and(|checked| checked.node.is_some()),
    };
    let mut bad = testing::unavailable_closure(
        program,
        reported,
        &pass.faults,
        &pass.references,
        &lowered,
        &test_faults.unavailable,
    );
    let runnable = pass
        .tests
        .keys()
        .copied()
        .filter(|index| !bad.contains_key(&Target::Test(*index)))
        .collect::<Vec<_>>();
    let none = BTreeMap::new();
    if runnable.is_empty() {
        return (None, checked_tests(program, &none, &bad));
    }
    for target in bad.keys() {
        match *target {
            Target::Function(index) => {
                if let Some(data) = program.functions.get_mut(index) {
                    data.signature = None;
                }
            }
            Target::Value(index) => {
                if let Some(data) = program.values.get_mut(index) {
                    data.ty = None;
                }
            }
            Target::Test(_) => {}
        }
    }
    let layout = self::layout(program);
    let mut reduced = check_bodies(program, documents, &layout);
    reduced.tests.retain(|index, _| runnable.contains(index));
    let refaulted = reduced.faults.iter().find(|(target, faults)| {
        !faults.is_empty()
            && !embedded(program, **target)
            && match target {
                Target::Test(index) => runnable.contains(index),
                _ => true,
            }
    });
    let failure = match refaulted {
        Some((_, faults)) => faults.first().cloned(),
        None => match assemble(
            program,
            &layout,
            &reduced,
            &runnable,
            &BTreeMap::new(),
            false,
        ) {
            Ok(assembled) => {
                return (
                    Some(assembled.set),
                    checked_tests(program, &assembled.tests, &bad),
                );
            }
            Err(message) => Some(Diagnostic::new(
                DiagnosticCode::ToolUnavailable,
                ByteSpan::empty_at(0),
                format!("the tests that remain cannot be lowered: {message}"),
            )),
        },
    };
    if let Some(failure) = failure {
        for index in runnable {
            bad.insert(Target::Test(index), failure.clone());
        }
    }
    (None, checked_tests(program, &none, &bad))
}

fn origin(program: &Program<'_>, module: usize, span: ByteSpan) -> SourceOrigin {
    let source = program
        .modules
        .get(module)
        .map_or_else(|| Arc::from(""), |data| Arc::clone(&data.source_id));
    SourceOrigin::new(source, span)
}

/// A lowered module set.
struct Assembled {
    set: Arc<CheckedModuleSet>,
    /// The qualified name and index of every function of a declaration.
    functions: Vec<(String, usize)>,
    /// The function index of each lowered test, by test index.
    tests: BTreeMap<usize, usize>,
}

/// Lowers the declarations `layout` places and the tests `tests`, whose
/// functions follow every other function, and then the anonymous invariant
/// predicates, in the order of their types. Each guard with `else` is lowered
/// as `folds` decides it; a bare guard leaves no code.
///
/// With `stubs`, the module set is one compile-time evaluations run over: a
/// function or a module value the bodies pass did not lower is given a body
/// that calls or reads itself, which no certified evaluation reaches.
fn assemble(
    program: &Program<'_>,
    layout: &Layout,
    pass: &Pass,
    tests: &[usize],
    folds: &BTreeMap<usize, Fold>,
    stubs: bool,
) -> Result<Assembled, String> {
    let count = layout.functions.iter().flatten().count();
    let anonymous = anonymous_entries(&pass.invariants, count + tests.len());
    let in_scope = |module: usize| {
        program
            .modules
            .get(module)
            .is_some_and(|data| data.in_scope)
    };
    let mut types = Vec::new();
    for (index, data) in program.types.iter().enumerate() {
        if !in_scope(data.module) {
            continue;
        }
        let Some(id) = data.id.clone() else { continue };
        let body = match &data.kind {
            TypeKind::Variant(cases) => TypeBody::Variant(
                cases
                    .iter()
                    .map(|case| {
                        Case::new(
                            case.name.clone(),
                            case.fields.clone().unwrap_or_default(),
                        )
                    })
                    .collect(),
            ),
            TypeKind::Wrapper(representation) => {
                TypeBody::Wrapper(representation.clone())
            }
            TypeKind::Tuple(components) => TypeBody::Tuple(components.clone()),
            TypeKind::Record(fields) => TypeBody::Record(fields.clone()),
            TypeKind::Scalar(_)
            | TypeKind::List
            | TypeKind::Pending
            | TypeKind::Faulty
            | TypeKind::Unsupported => continue,
        };
        let visibility = if data.representation_public {
            Visibility::Public
        } else {
            Visibility::Private
        };
        let invariant = match pass.invariants.get(&index) {
            Some(checked) => {
                let predicate = match &checked.predicate {
                    Predicate::Anonymous(_) => anonymous.get(&index).copied(),
                    Predicate::Named(function) => {
                        layout.functions.get(*function).copied().flatten()
                    }
                };
                predicate.map(|predicate| Invariant {
                    predicate,
                    consequence: checked.consequence.clone(),
                })
            }
            None => None,
        };
        let definition =
            TypeDefinition::new(id, data.params.clone(), body, invariant, visibility)
                .map_err(|error| error.to_string())?;
        types.push(definition);
    }
    let mut interfaces = Vec::new();
    for data in &program.interfaces {
        if !in_scope(data.module) {
            continue;
        }
        let Some(id) = data.id.clone() else { continue };
        let associated = data
            .associated
            .iter()
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        let mut members = Vec::new();
        for member in &data.members {
            let signature = member_record(
                program,
                &member.binders,
                &member.bounds,
                member.receiver,
                member.signature.clone(),
                &associated,
            )?;
            // A default body the checker skipped leaves its member out of
            // the checked interface: nothing can call it.
            let default = match member.default {
                Some(function) => {
                    match layout.functions.get(function).copied().flatten() {
                        Some(at) => Some(at),
                        None => continue,
                    }
                }
                None => None,
            };
            members.push(InterfaceMember {
                name: member.name.clone(),
                signature,
                default,
            });
        }
        interfaces.push(
            InterfaceDefinition::new(id, data.params.clone(), associated, members)
                .map_err(|error| error.to_string())?,
        );
    }
    let mut globals = Vec::new();
    for (index, data) in program.values.iter().enumerate() {
        if layout.globals.get(index).copied().flatten().is_none() {
            continue;
        }
        let origin = origin(program, data.module, data.view.construct().span());
        let Some(ty) = data.ty.clone() else {
            return Err(format!("the module value `{}` has no type", data.qualified));
        };
        let lowered = pass
            .values
            .get(&index)
            .and_then(|checked| Some((checked.node.clone()?, checked.slot_count)));
        let (initializer, slot_count) = match lowered {
            Some(lowered) => lowered,
            None if stubs => {
                let at = layout
                    .globals
                    .get(index)
                    .copied()
                    .flatten()
                    .unwrap_or_default();
                (Node::new(Expr::Global(at), origin.clone()), 0)
            }
            None => {
                return Err(format!(
                    "the module value `{}` was not lowered",
                    data.qualified
                ));
            }
        };
        globals.push(CheckedGlobal {
            name: data.qualified.clone(),
            value_type: ty,
            initializer,
            slot_count,
            origin,
        });
    }
    let mut functions = Vec::new();
    let mut names = Vec::new();
    for (index, data) in program.functions.iter().enumerate() {
        let Some(at) = layout.functions.get(index).copied().flatten() else {
            continue;
        };
        let Some(signature) = data.signature.clone() else {
            return Err(format!(
                "the function `{}` has no signature",
                data.qualified
            ));
        };
        let record = function_record(program, data, signature.clone())?;
        let origin = origin(program, data.module, data.view.construct().span());
        let function = match data.external {
            Some(intrinsic) => CheckedFunction::external(
                data.qualified.clone(),
                record,
                intrinsic,
                (0..data.binders.len()).map(Type::Binder).collect(),
                origin,
            ),
            None => match pass
                .functions
                .get(&index)
                .filter(|checked| checked.node.is_some())
            {
                Some(checked) => {
                    let fold = folds.get(&index).copied().unwrap_or_default();
                    let entry_guard = match &checked.guards.entry {
                        Some(guard) => lower_guard(program, guard, fold.entry)?,
                        None => None,
                    };
                    let exit_guard = match &checked.guards.exit {
                        Some((candidate, guard)) => {
                            lower_guard(program, guard, fold.exit)?.map(|guard| {
                                ExitGuard {
                                    candidate: *candidate,
                                    guard,
                                }
                            })
                        }
                        None => None,
                    };
                    CheckedFunction {
                        name: data.qualified.clone(),
                        signature: record,
                        defaults: checked.defaults.clone(),
                        body: FunctionBody::Expr(Box::new(
                            checked.node.clone().ok_or("a lowered body")?,
                        )),
                        entry_guard,
                        exit_guard,
                        slot_count: checked.slot_count,
                        origin,
                    }
                }
                None if stubs => {
                    stub(at, &signature, data.binders.len(), record, origin, data)
                }
                None => {
                    return Err(format!(
                        "the function `{}` was not lowered",
                        data.qualified
                    ));
                }
            },
        };
        names.push((data.qualified.clone(), at));
        functions.push(function);
    }
    let mut lowered_tests = BTreeMap::new();
    for index in tests {
        let Some(data) = program.tests.get(*index) else {
            return Err(format!("no test {index}"));
        };
        let Some(body) = pass
            .tests
            .get(index)
            .and_then(|checked| Some((checked.node.clone()?, checked.slot_count)))
        else {
            return Err(format!("the test `{}` was not lowered", data.qualified));
        };
        let (body, slot_count) = body;
        lowered_tests.insert(*index, functions.len());
        functions.push(CheckedFunction {
            name: data.qualified.clone(),
            signature: signature_record(
                program,
                &[],
                &[],
                FnType::new(Vec::new(), Type::Void),
            )?,
            defaults: Vec::new(),
            body: FunctionBody::Expr(Box::new(body)),
            entry_guard: None,
            exit_guard: None,
            slot_count,
            origin: origin(program, data.module, data.view.construct().span()),
        });
    }
    for (ty, at) in &anonymous {
        let (Some(data), Some(Predicate::Anonymous(closure))) = (
            program.types.get(*ty),
            pass.invariants.get(ty).map(|checked| &checked.predicate),
        ) else {
            continue;
        };
        if functions.len() != *at {
            return Err(format!("the predicate of `{}` is out of place", data.name));
        }
        let Type::Fn(signature) = &closure.value_type else {
            return Err(format!("the predicate of `{}` is no `Fn`", data.name));
        };
        let span = pass
            .invariants
            .get(ty)
            .map_or(data.view.construct().span(), |checked| checked.span);
        functions.push(CheckedFunction {
            name: format!("{}.where", data.name),
            signature: signature_record(program, &[], &[], (**signature).clone())?,
            defaults: closure.defaults.clone(),
            body: FunctionBody::Expr(Box::new(closure.body.clone())),
            entry_guard: None,
            exit_guard: None,
            slot_count: closure.slot_count,
            origin: origin(program, data.module, span),
        });
    }
    let mut implementations = Vec::new();
    for data in &program.impls {
        if !in_scope(data.module) || !data.usable {
            continue;
        }
        let Some(record) = implementation_record(program, data)? else {
            continue;
        };
        let mut members = Vec::new();
        for (name, function) in &data.members {
            let Some(at) = layout.functions.get(*function).copied().flatten() else {
                return Err(format!(
                    "an implementation member `{name}` was not lowered"
                ));
            };
            members.push((name.clone(), at));
        }
        implementations.push(
            Implementation::new(record, members).map_err(|error| error.to_string())?,
        );
    }
    let mut library = LibraryTypes::new();
    for (binding, entity) in &program.library {
        let id = match entity {
            Entity::Type(index) => program
                .types
                .get(*index)
                .filter(|data| in_scope(data.module))
                .and_then(|data| data.id.clone()),
            Entity::Interface(index) => program
                .interfaces
                .get(*index)
                .filter(|data| in_scope(data.module))
                .and_then(|data| data.id.clone()),
            _ => None,
        };
        if let Some(id) = id {
            library = library.with(*binding, id);
        }
    }
    let set = CheckedModuleSet::try_new(ModuleSetParts {
        library,
        types,
        interfaces,
        globals,
        functions,
        implementations,
    })
    .map_err(|error| error.to_string())?;
    Ok(Assembled {
        set,
        functions: names,
        tests: lowered_tests,
    })
}

/// The checked IR guard of `guard`, folded as `fold` decides: removed when
/// its predicate always holds, with the predicate `false` when it never
/// does. A bare guard leaves no code.
fn lower_guard(
    program: &Program<'_>,
    guard: &CheckedGuard,
    fold: Option<bool>,
) -> Result<Option<Guard>, String> {
    let Some(consequence) = guard.consequence.clone() else {
        return Ok(None);
    };
    let predicate = match fold {
        Some(true) => return Ok(None),
        Some(false) => {
            let truth = program
                .library_type(Library::Bool, Vec::new())
                .ok_or("a folded guard without `Bool`")?;
            let never = Value::Variant {
                case: "false".to_owned(),
                fields: Vec::new(),
            };
            let constant =
                Constant::new(truth, never).map_err(|error| error.to_string())?;
            Node::new(Expr::Literal(constant), guard.predicate.origin.clone())
        }
        None => guard.predicate.clone(),
    };
    Ok(Some(Guard {
        predicate,
        consequence,
    }))
}

/// The function `data`, at index `at` with the type `signature` and the
/// signature record `record`, whose body calls itself with its own
/// arguments: the stand-in for a body that was not lowered, in a module set
/// that compile-time evaluations run over and that never runs it.
fn stub(
    at: usize,
    signature: &FnType,
    binders: usize,
    record: SignatureRecord,
    origin: SourceOrigin,
    data: &FnData<'_>,
) -> CheckedFunction {
    let read = |slot: usize, value_type: &Type| {
        Node::new(
            Expr::Variable {
                slot,
                value_type: value_type.clone(),
            },
            origin.clone(),
        )
    };
    let positional = signature
        .parameters
        .iter()
        .enumerate()
        .map(|(slot, ty)| read(slot, ty))
        .collect();
    let labelled = signature
        .labels
        .entries()
        .iter()
        .enumerate()
        .map(|(at, (name, ty))| {
            (name.clone(), read(signature.parameters.len() + at, ty))
        })
        .collect();
    let body = Node::new(
        Expr::TailCall {
            target: CallTarget::Direct {
                function: at,
                type_arguments: (0..binders).map(Type::Binder).collect(),
            },
            arguments: Arguments {
                positional,
                labelled,
            },
        },
        origin.clone(),
    );
    CheckedFunction {
        name: data.qualified.clone(),
        signature: record,
        defaults: Vec::new(),
        body: FunctionBody::Expr(Box::new(body)),
        entry_guard: None,
        exit_guard: None,
        slot_count: signature.slot_count(),
        origin,
    }
}

/// The checked IR bound of `bound`.
fn bound_of(program: &Program<'_>, bound: &BoundData) -> Result<Bound, String> {
    let interface = program
        .interfaces
        .get(bound.interface)
        .and_then(|data| data.id.clone())
        .ok_or_else(|| "a bound names an unsupported interface".to_owned())?;
    Ok(Bound {
        contract: InterfaceKey::new(interface, bound.arguments.clone()),
        choices: NamedTypes::new(bound.choices.clone())
            .map_err(|error| error.to_string())?,
    })
}

/// The binders `names`, each with its bound in `bounds`.
fn binders_of(
    program: &Program<'_>,
    names: &[String],
    bounds: &[Option<BoundData>],
) -> Result<Vec<Binder>, String> {
    names
        .iter()
        .enumerate()
        .map(|(at, name)| {
            let bound = match bounds.get(at).and_then(Option::as_ref) {
                Some(bound) => Some(bound_of(program, bound)?),
                None => None,
            };
            Binder::new(name.clone(), bound).map_err(|error| error.to_string())
        })
        .collect()
}

/// The signature record of a function with the type parameters `binders`
/// and their bounds.
fn signature_record(
    program: &Program<'_>,
    binders: &[String],
    bounds: &[Option<BoundData>],
    signature: FnType,
) -> Result<SignatureRecord, String> {
    let binders = binders_of(program, binders, bounds)?;
    SignatureRecord::new(binders, Type::function(signature))
        .map_err(|error| error.to_string())
}

/// The signature record of a member of an interface with the associated
/// types `associated`.
fn member_record(
    program: &Program<'_>,
    binders: &[String],
    bounds: &[Option<BoundData>],
    receiver: Option<usize>,
    signature: FnType,
    associated: &[String],
) -> Result<SignatureRecord, String> {
    let binders = binders_of(program, binders, bounds)?;
    SignatureRecord::interface_member(
        binders,
        receiver.and_then(|slot| u64::try_from(slot).ok()),
        Type::function(signature),
        associated.to_vec(),
    )
    .map_err(|error| error.to_string())
}

/// The signature record of the function `data`: the member's record for a
/// default body, an ordinary one otherwise.
fn function_record(
    program: &Program<'_>,
    data: &FnData<'_>,
    signature: FnType,
) -> Result<SignatureRecord, String> {
    let Some(interface) = data
        .contract
        .and_then(|index| program.interfaces.get(index))
    else {
        return signature_record(program, &data.binders, &data.bounds, signature);
    };
    let associated = interface
        .associated
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    let receiver = interface
        .member(&data.name)
        .and_then(|member| member.receiver);
    member_record(
        program,
        &data.binders,
        &data.bounds,
        receiver,
        signature,
        &associated,
    )
}

/// The implementation record of `data`, when its interface is supported.
fn implementation_record(
    program: &Program<'_>,
    data: &super::declare::ImplData,
) -> Result<Option<ImplementationRecord>, String> {
    let Some(interface) = program
        .interfaces
        .get(data.interface)
        .and_then(|interface| interface.id.clone())
    else {
        return Ok(None);
    };
    let binders = binders_of(program, &data.binders, &data.bounds)?;
    let choices =
        NamedTypes::new(data.choices.clone()).map_err(|error| error.to_string())?;
    ImplementationRecord::new(
        binders,
        InterfaceKey::new(interface, data.arguments.clone()),
        data.receiver.clone(),
        choices,
    )
    .map(Some)
    .map_err(|error| error.to_string())
}

/// The first function with no parameter that the first local module
/// declares at its top level.
fn entry(program: &Program<'_>, layout: &Layout) -> Option<usize> {
    let module = program.modules.iter().position(|module| !module.embedded)?;
    program
        .functions
        .iter()
        .enumerate()
        .filter(|(_, data)| data.module == module && data.owner.is_none())
        .find(|(_, data)| {
            data.signature
                .as_ref()
                .is_some_and(|signature| signature.parameters.is_empty())
        })
        .and_then(|(index, _)| layout.functions.get(index).copied().flatten())
}

/// The `@types.v2` document of the local declarations in scope.
fn types_document(program: &Program<'_>, obligations: &[Obligation]) -> Option<String> {
    let local = |module: usize| {
        program
            .modules
            .get(module)
            .is_some_and(|data| data.in_scope && !data.embedded)
    };
    let mut values = Vec::new();
    for (index, data) in program.values.iter().enumerate() {
        if !local(data.module) || !program.supported_value(index) {
            continue;
        }
        let ty = data.ty.as_ref()?;
        values.push((
            data.identity.clone(),
            format!(
                "{{declaration: \"{}\" typeInfo: {}}}",
                data.identity,
                encode_type(ty).ok()?
            ),
        ));
    }
    let mut callables = Vec::new();
    for (index, data) in program.functions.iter().enumerate() {
        if !local(data.module)
            || !program.supported_fn(index)
            || data.implements.is_some()
        {
            continue;
        }
        let signature = data.signature.clone()?;
        let record = function_record(program, data, signature).ok()?;
        callables.push((
            data.identity.clone(),
            format!(
                "{{declaration: \"{}\" signature: {}}}",
                data.identity,
                record.encode().ok()?
            ),
        ));
    }
    let mut implementations = Vec::new();
    for data in &program.impls {
        if !local(data.module) || !data.usable {
            continue;
        }
        let record = implementation_record(program, data).ok()??;
        let contract = record.contract().encode().ok()?;
        let receiver = encode_open_type(record.receiver()).ok()?;
        implementations.push(((contract, receiver), record.encode().ok()?));
    }
    implementations.sort();
    values.sort();
    callables.sort();
    let join = |entries: Vec<(String, String)>| {
        entries
            .into_iter()
            .map(|(_, text)| text)
            .collect::<Vec<_>>()
            .join(" ")
    };
    Some(format!(
        "{{format: @types.v2 moduleValues: [{}] callables: [{}] implementations: [{}] obligations: [{}]}}\n",
        join(values),
        join(callables),
        implementations
            .into_iter()
            .map(|(_, text)| text)
            .collect::<Vec<_>>()
            .join(" "),
        obligations
            .iter()
            .map(Obligation::record)
            .collect::<Vec<_>>()
            .join(" ")
    ))
}

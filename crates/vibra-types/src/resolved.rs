//! Type checking for one resolver-owned multi-module snapshot.

use std::collections::{BTreeMap, BTreeSet};

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode, Level};
use vibra_ir::{CheckedFunction, CheckedGlobal, CheckedProgram, IrError, SourceOrigin};
use vibra_resolve::{DeclarationId, ResolvedSnapshot};
use vibra_syntax::{ApplicationBinding, Declaration, SourceAst};

use crate::{
    CheckEnvironment, FunctionHeader, FunctionTargetSet, GlobalHeader,
    ResolvedReferenceTarget, check_expression, check_signature, compiler_intrinsic,
    has_deferred_attributes, unavailable,
};

/// The result of type checking a selected set of source modules from one
/// resolver snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedCheckResult {
    programs: BTreeMap<DeclarationId, CheckedProgram>,
    diagnostics: Vec<Diagnostic>,
    bindings: Vec<ApplicationBinding>,
    function_indices: BTreeMap<DeclarationId, usize>,
}

impl ResolvedCheckResult {
    /// Diagnostics emitted in deterministic source/declaration order.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Authoritative call binding facts for checked applications.
    #[must_use]
    pub fn application_bindings(&self) -> &[ApplicationBinding] {
        &self.bindings
    }

    /// Whether the selected modules crossed the checker boundary.
    #[must_use]
    pub fn accepted(&self) -> bool {
        self.diagnostics
            .iter()
            .all(|diagnostic| diagnostic.level() != Level::Error)
    }

    /// Checked program whose entry is the resolved declaration.
    #[must_use]
    pub fn program_for_function(
        &self,
        declaration: &DeclarationId,
    ) -> Option<&CheckedProgram> {
        self.programs.get(declaration)
    }

    /// The checked function slot for a resolved declaration identity.
    #[must_use]
    pub fn function_index(&self, declaration: &DeclarationId) -> Option<usize> {
        self.function_indices.get(declaration).copied()
    }
}

#[derive(Clone)]
struct Module<'a> {
    record: &'a vibra_resolve::ModuleRecord,
    ast: &'a SourceAst,
    declarations: BTreeMap<(String, ByteSpan), DeclarationId>,
    trusted_bootstrap: bool,
}

/// Checks the selected source modules in one already-resolved immutable graph.
///
/// The source IDs are the complete checking scope. The workspace layer owns
/// target selection and import-closure construction; this checker does not
/// inspect files or dependencies.
pub fn check_resolved(
    snapshot: &ResolvedSnapshot,
    source_ids: &[String],
    verification: Option<&crate::BootstrapVerification>,
) -> ResolvedCheckResult {
    let selected = source_ids.iter().cloned().collect::<BTreeSet<_>>();
    let mut seen_source_ids = BTreeSet::new();
    let mut duplicate_source_ids = BTreeSet::new();
    for module in snapshot
        .modules()
        .iter()
        .filter(|module| selected.contains(module.source_id()))
    {
        if !seen_source_ids.insert(module.source_id().to_owned()) {
            duplicate_source_ids.insert(module.source_id().to_owned());
        }
    }
    if !duplicate_source_ids.is_empty() {
        return ResolvedCheckResult {
            programs: BTreeMap::new(),
            diagnostics: duplicate_source_ids
                .into_iter()
                .map(|source_id| {
                    Diagnostic::new(
                        DiagnosticCode::ModuleSourceIdCollision,
                        ByteSpan::empty_at(0),
                        format!(
                            "source identity `{source_id}` belongs to more than one module"
                        ),
                    )
                    .with_source_id(source_id)
                })
                .collect(),
            bindings: Vec::new(),
            function_indices: BTreeMap::new(),
        };
    }
    let mut modules = snapshot
        .modules()
        .iter()
        .filter(|module| selected.contains(module.source_id()))
        .filter_map(|record| {
            let ast = record.ast()?;
            let declarations = snapshot
                .declarations()
                .iter()
                .filter(|declaration| declaration.source_id() == record.source_id())
                .map(|declaration| {
                    (
                        (declaration.source_id().to_owned(), declaration.span()),
                        declaration.id().clone(),
                    )
                })
                .collect();
            Some(Module {
                record,
                ast,
                declarations,
                trusted_bootstrap: verification
                    .is_some_and(|verification| verification.trusts_module(record)),
            })
        })
        .collect::<Vec<_>>();
    modules
        .sort_by(|left, right| left.record.source_id().cmp(right.record.source_id()));

    let mut diagnostics = Vec::new();
    let mut globals = Vec::<GlobalHeader>::new();
    let mut functions = Vec::<FunctionHeader>::new();
    let mut global_indices = BTreeMap::<DeclarationId, usize>::new();
    let mut function_indices = BTreeMap::<DeclarationId, usize>::new();

    for (module_index, module) in modules.iter().enumerate() {
        for (declaration_index, declaration) in
            module.ast.declarations().iter().enumerate()
        {
            match declaration {
                Declaration::Def(definition) => {
                    let Some(id) = module
                        .declarations
                        .get(&(module.record.source_id().to_owned(), definition.span()))
                        .cloned()
                    else {
                        continue;
                    };
                    let Some(value_type) =
                        crate::primitive_type(definition.value_type())
                    else {
                        unavailable(
                            &mut diagnostics,
                            module.record.source_id(),
                            definition.span(),
                            "only monomorphic module values are available in Step 12",
                        );
                        continue;
                    };
                    let index = globals.len();
                    global_indices.insert(id.clone(), index);
                    globals.push(GlobalHeader {
                        name: id.canonical(),
                        value_type,
                        expression: definition.expression().clone(),
                        span: definition.span(),
                        source_id: module.record.source_id().to_owned(),
                        module_index,
                        function_index: None,
                        function_targets: FunctionTargetSet::default(),
                    });
                }
                Declaration::Defn(function) => {
                    let Some(id) = module
                        .declarations
                        .get(&(module.record.source_id().to_owned(), function.span()))
                        .cloned()
                    else {
                        continue;
                    };
                    let Some(signature) = check_signature(
                        module.record.source_id(),
                        function,
                        &mut diagnostics,
                    ) else {
                        continue;
                    };
                    let index = functions.len();
                    function_indices.insert(id.clone(), index);
                    functions.push(FunctionHeader {
                        declaration_index,
                        module_index,
                        source_id: module.record.source_id().to_owned(),
                        name: id.canonical(),
                        signature,
                        external: compiler_intrinsic(
                            module.record.source_id(),
                            function,
                            &mut diagnostics,
                            module.trusted_bootstrap,
                        ),
                        external_declared: function.attributes().items().iter().any(
                            |attribute| {
                                matches!(
                                    attribute,
                                    vibra_syntax::Attribute::External(_)
                                )
                            },
                        ),
                    });
                }
                Declaration::Import(_) => {}
                _ => unavailable(
                    &mut diagnostics,
                    module.record.source_id(),
                    declaration.span(),
                    "this declaration family is outside the Step 12 monomorphic profile",
                ),
            }
        }
    }

    let mut resolved_targets = BTreeMap::new();
    for reference in snapshot
        .references()
        .iter()
        .filter(|reference| selected.contains(reference.source_id()))
    {
        let target = reference.target().and_then(|target| {
            global_indices
                .get(target)
                .copied()
                .map(ResolvedReferenceTarget::Global)
                .or_else(|| {
                    function_indices
                        .get(target)
                        .copied()
                        .map(ResolvedReferenceTarget::Function)
                })
        });
        resolved_targets.insert(
            (
                reference.source_id().to_owned(),
                reference.span().start(),
                reference.span().end(),
            ),
            target.unwrap_or(ResolvedReferenceTarget::Unresolved),
        );
    }

    let mut recursive_groups = vec![BTreeSet::new(); functions.len()];
    for reference in snapshot
        .references()
        .iter()
        .filter(|reference| selected.contains(reference.source_id()))
    {
        let (Some(from), Some(target)) =
            (function_indices.get(reference.from()), reference.target())
        else {
            continue;
        };
        if let Some(to) = function_indices.get(target)
            && let Some(group) = recursive_groups.get_mut(*from)
        {
            group.insert(*to);
        }
    }
    let recursive_groups =
        crate::find_recursive_groups(&recursive_groups, &vec![true; functions.len()]);

    let empty_indices = BTreeMap::new();
    let empty_names = BTreeMap::new();
    let mut bindings = Vec::new();
    let mut checked_globals = vec![None; globals.len()];
    for (index, header) in globals.iter().cloned().enumerate() {
        let Some(module) = modules.get(header.module_index) else {
            continue;
        };
        let mut environment = CheckEnvironment::new(
            &header.source_id,
            &mut diagnostics,
            &empty_indices,
            &globals,
            &functions,
            &empty_indices,
            &empty_names,
            &mut bindings,
            None,
            None,
        );
        environment.resolved_targets = Some(&resolved_targets);
        let Some(expression) = check_expression(
            &mut environment,
            &header.expression,
            Some(header.value_type.clone()),
        ) else {
            continue;
        };
        let origin = SourceOrigin::new(&header.source_id, header.span);
        match CheckedGlobal::new(header.name, header.value_type, expression, origin) {
            Ok(global) => {
                if let Some(slot) = checked_globals.get_mut(index) {
                    *slot = Some(global);
                }
            }
            Err(error) => unavailable(
                &mut diagnostics,
                &header.source_id,
                header.span,
                format!("checked IR construction failed: {error}"),
            ),
        }
        let _ = module;
    }

    let mut checked_functions = vec![None; functions.len()];
    for (index, header) in functions.iter().cloned().enumerate() {
        let Some(module) = modules.get(header.module_index) else {
            continue;
        };
        let Some(declaration) = module.ast.declarations().get(header.declaration_index)
        else {
            continue;
        };
        let Declaration::Defn(function) = declaration else {
            continue;
        };
        if !header.external_declared
            && has_deferred_attributes(function.attributes().items())
        {
            unavailable(
                &mut diagnostics,
                &header.source_id,
                function.span(),
                "variadic, generic, external, and nonempty-effect attributes are unavailable in Step 12",
            );
            continue;
        }
        if let Some(intrinsic) = header.external {
            let origin = SourceOrigin::new(&header.source_id, function.span());
            match CheckedFunction::new_external(
                header.name,
                header.signature,
                intrinsic,
                origin,
            ) {
                Ok(checked) => {
                    if let Some(slot) = checked_functions.get_mut(index) {
                        *slot = Some(checked);
                    }
                }
                Err(error) => unavailable(
                    &mut diagnostics,
                    &header.source_id,
                    function.span(),
                    format!("checked IR construction failed: {error}"),
                ),
            }
            continue;
        }
        if header.external_declared {
            continue;
        }
        let recursive_group = recursive_groups.get(index).cloned();
        let mut environment = CheckEnvironment::new(
            &header.source_id,
            &mut diagnostics,
            &empty_indices,
            &globals,
            &functions,
            &empty_indices,
            &empty_names,
            &mut bindings,
            Some(index),
            recursive_group,
        );
        environment.resolved_targets = Some(&resolved_targets);
        let mut parameters_valid = true;
        for (parameter_index, parameter) in function.parameters().iter().enumerate() {
            match parameter.parsed_pattern().kind() {
                vibra_syntax::PatternKind::Binding(name) if name.is_discard() => {}
                vibra_syntax::PatternKind::Binding(name) => {
                    if !environment.add_binding(
                        name.value(),
                        parameter.value_type(),
                        parameter.span(),
                    ) {
                        parameters_valid = false;
                    }
                }
                _ => {
                    parameters_valid = false;
                    unavailable(
                        environment.diagnostics,
                        environment.source_id,
                        parameter.span(),
                        "constructor and destructuring patterns are deferred until M3",
                    );
                }
            }
            environment.next_slot = parameter_index.saturating_add(1);
        }
        let mut labelled_index = 0;
        for attribute in function.attributes().items() {
            let vibra_syntax::Attribute::Labelled(entries) = attribute else {
                continue;
            };
            for entry in entries {
                let Some(labelled) = header.signature.labelled().get(labelled_index)
                else {
                    continue;
                };
                labelled_index = labelled_index.saturating_add(1);
                if !environment.add_binding_type(
                    labelled.name(),
                    labelled.value_type(),
                    entry.span(),
                ) {
                    parameters_valid = false;
                }
            }
        }
        if !parameters_valid {
            continue;
        }
        let Some(body) = crate::check_sequence(
            &mut environment,
            function.expressions(),
            Some(header.signature.result()),
            function.span(),
            true,
        ) else {
            continue;
        };
        let origin = SourceOrigin::new(&header.source_id, function.span());
        match CheckedFunction::with_slots(
            header.name,
            header.signature,
            body,
            origin,
            environment.next_slot,
        ) {
            Ok(checked) => {
                if let Some(slot) = checked_functions.get_mut(index) {
                    *slot = Some(checked);
                }
            }
            Err(error) => unavailable(
                &mut diagnostics,
                &header.source_id,
                function.span(),
                format!("checked IR construction failed: {error}"),
            ),
        }
    }

    let globals = checked_globals.into_iter().collect::<Option<Vec<_>>>();
    let functions = checked_functions.into_iter().collect::<Option<Vec<_>>>();
    if let (Some(globals), Some(functions)) = (&globals, &functions) {
        match vibra_ir::validate_global_initializer_cycles(globals, functions) {
            Ok(()) => {}
            Err(IrError::GlobalInitializerCycle(_)) => {
                if let Some(global) = globals.first() {
                    diagnostics.push(
                        Diagnostic::new(
                            vibra_diagnostics::DiagnosticCode::TypeInitializerCycle,
                            global.origin().span(),
                            "module value initializers form a cycle",
                        )
                        .with_source_id(global.origin().source_id()),
                    );
                }
            }
            Err(error) => {
                if let Some(module) = modules.first() {
                    unavailable(
                        &mut diagnostics,
                        module.record.source_id(),
                        module.ast.span(),
                        format!("checked IR construction failed: {error}"),
                    );
                }
            }
        }
    }
    let has_error = diagnostics
        .iter()
        .any(|diagnostic| diagnostic.level() == Level::Error);
    let mut programs = BTreeMap::new();
    if !has_error && let (Some(globals), Some(functions)) = (globals, functions) {
        for entry in snapshot.entries() {
            let Some(declaration) = entry.declaration() else {
                continue;
            };
            let Some(index) = function_indices.get(declaration).copied() else {
                continue;
            };
            match CheckedProgram::try_new_with_globals(
                globals.clone(),
                functions.clone(),
                index,
            ) {
                Ok(program) => {
                    programs.insert(declaration.clone(), program);
                }
                Err(IrError::GlobalInitializerCycle(_)) => {
                    if let Some(global) = globals.first() {
                        diagnostics.push(
                            Diagnostic::new(
                                vibra_diagnostics::DiagnosticCode::TypeInitializerCycle,
                                global.origin().span(),
                                "module value initializers form a cycle",
                            )
                            .with_source_id(global.origin().source_id()),
                        );
                    }
                    break;
                }
                Err(error) => {
                    if let Some(declaration) = snapshot
                        .declarations()
                        .iter()
                        .find(|candidate| candidate.id() == declaration)
                    {
                        unavailable(
                            &mut diagnostics,
                            declaration.source_id(),
                            declaration.span(),
                            format!("checked IR construction failed: {error}"),
                        );
                    }
                    break;
                }
            }
        }
    }

    diagnostics.sort_by_key(|diagnostic| {
        (
            diagnostic.source_id().unwrap_or_default().to_owned(),
            diagnostic.primary_span().start(),
            diagnostic.primary_span().end(),
            diagnostic.code(),
        )
    });
    ResolvedCheckResult {
        programs,
        diagnostics,
        bindings,
        function_indices,
    }
}

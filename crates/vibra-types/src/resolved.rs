//! Type checking for one resolver-owned multi-module snapshot.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode, Level};
use vibra_ir::{
    CheckedFunction, CheckedGlobal, CheckedModuleSet, CheckedProgram, Expr,
    FunctionSignature, IrError, SourceOrigin, TestAssertion, Type, Value,
};
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
    signatures: BTreeMap<DeclarationId, IndexedSignature>,
    implementations: Vec<IndexedImplementation>,
    modules: Option<Arc<CheckedModuleSet>>,
}

/// The checked type of one value, function, or method, for an index record
/// (`docs/spec/05-tooling.md`, "Index records").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexedSignature {
    signature: String,
    errors: Vec<String>,
}

impl IndexedSignature {
    /// The canonical type encoding of the checked type.
    #[must_use]
    pub fn signature(&self) -> &str {
        &self.signature
    }

    /// The canonical encodings of the error types its result can carry: `e`
    /// for a result type `(result t e)`.
    #[must_use]
    pub fn errors(&self) -> &[String] {
        &self.errors
    }
}

/// One `impl` block, keyed by its receiver and applied interface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexedImplementation {
    receiver: String,
    interface: String,
    interface_id: String,
    source_id: String,
    span: ByteSpan,
    members: Vec<IndexedMember>,
}

impl IndexedImplementation {
    /// The canonical type encoding of the receiver type.
    #[must_use]
    pub fn receiver(&self) -> &str {
        &self.receiver
    }

    /// The canonical type encoding of the applied interface target.
    #[must_use]
    pub fn interface(&self) -> &str {
        &self.interface
    }

    /// The canonical declaration identity of the interface.
    #[must_use]
    pub fn interface_id(&self) -> &str {
        &self.interface_id
    }

    /// The source identity of the block.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// The span of the block.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    /// The members the block writes, by contract member name.
    #[must_use]
    pub fn members(&self) -> &[IndexedMember] {
        &self.members
    }
}

/// One member written in an `impl` block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexedMember {
    contract: String,
    span: ByteSpan,
    signature: String,
}

impl IndexedMember {
    /// The name of the contract member it implements.
    #[must_use]
    pub fn contract(&self) -> &str {
        &self.contract
    }

    /// The span of the member.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    /// The canonical type encoding of its checked signature.
    #[must_use]
    pub fn signature(&self) -> &str {
        &self.signature
    }
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

    /// The checked type of a value, function, or method, when its header
    /// checked.
    #[must_use]
    pub fn signature(&self, declaration: &DeclarationId) -> Option<&IndexedSignature> {
        self.signatures.get(declaration)
    }

    /// The validated module set of the selected modules: every checked
    /// function, global, and type definition. Absent when a module did not
    /// check or the scope declares no function.
    #[must_use]
    pub fn modules(&self) -> Option<&CheckedModuleSet> {
        self.modules.as_deref()
    }

    /// Every `impl` block of the selected modules, in registration order.
    #[must_use]
    pub fn implementations(&self) -> &[IndexedImplementation] {
        &self.implementations
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
    verification: Option<&crate::Stdlib>,
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
            signatures: BTreeMap::new(),
            implementations: Vec::new(),
            modules: None,
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

    // Declared types come first: every signature below may name one, in its
    // own module or through an import alias.
    let mut types = crate::nominal::TypeNames::default();
    let mut type_declarations = Vec::new();
    for module in &modules {
        for declaration in module.ast.declarations() {
            if let Declaration::Defint(value) = declaration
                && let Some(id) = module
                    .declarations
                    .get(&(module.record.source_id().to_owned(), value.span()))
            {
                let path = std::iter::once(id.unit())
                    .chain(id.module().iter().map(String::as_str))
                    .chain(id.path().iter().map(String::as_str))
                    .collect::<Vec<_>>()
                    .join(".");
                types.declare_interface(
                    module.record.source_id(),
                    value,
                    vibra_ir::TypeId::new(id.canonical(), path),
                );
                continue;
            }
            let Declaration::Deftype(value) = declaration else {
                continue;
            };
            let Some(id) = module
                .declarations
                .get(&(module.record.source_id().to_owned(), value.span()))
            else {
                continue;
            };
            let path = std::iter::once(id.unit())
                .chain(id.module().iter().map(String::as_str))
                .chain(id.path().iter().map(String::as_str))
                .collect::<Vec<_>>()
                .join(".");
            let index = types.declare(
                module.record.source_id(),
                value,
                vibra_ir::TypeId::new(id.canonical(), path),
            );
            type_declarations.push((index, value));
        }
    }
    for import in snapshot
        .imports()
        .iter()
        .filter(|import| selected.contains(import.source_id()))
    {
        let Some(target) = import.module() else {
            continue;
        };
        if let Some(record) = snapshot.modules().iter().find(|record| {
            record.package() == target.package()
                && record.unit() == target.unit()
                && record.segments() == target.segments()
        }) {
            match import.declaration() {
                Some(declaration) => types.import_declaration(
                    import.source_id(),
                    import.alias(),
                    record.source_id(),
                    declaration,
                ),
                None => {
                    types.import(import.source_id(), import.alias(), record.source_id())
                }
            }
        }
    }
    crate::standard::declare_standard_types(&mut types, &mut type_declarations);
    types.lower_bodies(&type_declarations, &mut diagnostics);

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
                    let Some(value_type) = types.lower_or_report(
                        module.record.source_id(),
                        crate::nominal::Scope::NONE,
                        definition.value_type(),
                        definition.span(),
                        &mut diagnostics,
                    ) else {
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
                    let Some((generics, bounds)) = crate::nominal::function_generics(
                        &types,
                        &[],
                        function,
                        module.record.source_id(),
                        &mut diagnostics,
                    ) else {
                        continue;
                    };
                    let Some(signature) = check_signature(
                        module.record.source_id(),
                        function,
                        &mut diagnostics,
                        &types,
                        crate::nominal::Scope::new(None, &generics)
                            .with_bounds(&bounds),
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
                        external: compiler_intrinsic(
                            module.record.source_id(),
                            function,
                            &mut diagnostics,
                            module.trusted_bootstrap,
                            &signature,
                            &crate::standard::role_types(&types),
                        ),
                        signature,
                        external_declared: function.attributes().items().iter().any(
                            |attribute| {
                                matches!(
                                    attribute,
                                    vibra_syntax::Attribute::External(_)
                                )
                            },
                        ),
                        test: None,
                        test_assertion: None,
                        member_index: None,
                        self_type: None,
                        type_parameters: generics,
                        bounds,
                        impl_member: None,
                        implements: None,
                    });
                }
                Declaration::Test(test) => {
                    if module.record.unit() != "tests" {
                        unavailable(
                            &mut diagnostics,
                            module.record.source_id(),
                            test.span(),
                            "test declarations are available only in the reserved @tests unit",
                        );
                        continue;
                    }
                    let Some(id) = module
                        .declarations
                        .get(&(module.record.source_id().to_owned(), test.span()))
                        .cloned()
                    else {
                        continue;
                    };
                    if let Some(effects) = test.effects()
                        && !effects.references().is_empty()
                    {
                        unavailable(
                            &mut diagnostics,
                            module.record.source_id(),
                            effects.span(),
                            "nonempty test effect ceilings are unavailable in M2",
                        );
                    }
                    let index = functions.len();
                    function_indices.insert(id.clone(), index);
                    functions.push(FunctionHeader {
                        declaration_index,
                        module_index,
                        source_id: module.record.source_id().to_owned(),
                        name: id.canonical(),
                        signature: FunctionSignature::new(Vec::new(), Type::Void),
                        external: None,
                        external_declared: false,
                        test: Some(test.clone()),
                        test_assertion: None,
                        member_index: None,
                        self_type: None,
                        type_parameters: Vec::new(),
                        bounds: BTreeMap::new(),
                        impl_member: None,
                        implements: None,
                    });
                }
                Declaration::Deftype(value) => {
                    let Some((self_type, owner_generics, owner_bounds)) = types
                        .declared()
                        .iter()
                        .find(|declared| {
                            declared.span == value.span()
                                && declared.source_id == module.record.source_id()
                        })
                        .map(|declared| {
                            (
                                crate::nominal::declared_self_type(
                                    &declared.id,
                                    &declared.parameters,
                                ),
                                declared.parameters.clone(),
                                declared.bounds.clone(),
                            )
                        })
                    else {
                        continue;
                    };
                    for (member_index, member) in value.members().iter().enumerate() {
                        let vibra_syntax::TypeMember::Method(method) = member else {
                            continue;
                        };
                        let Some(id) = module
                            .declarations
                            .get(&(module.record.source_id().to_owned(), method.span()))
                            .cloned()
                        else {
                            continue;
                        };
                        let Some((generics, bounds)) =
                            crate::nominal::function_generics(
                                &types,
                                &owner_generics,
                                method,
                                module.record.source_id(),
                                &mut diagnostics,
                            )
                        else {
                            continue;
                        };
                        let bounds = owner_bounds
                            .clone()
                            .into_iter()
                            .chain(bounds)
                            .collect::<BTreeMap<_, _>>();
                        let Some(signature) = check_signature(
                            module.record.source_id(),
                            method,
                            &mut diagnostics,
                            &types,
                            crate::nominal::Scope::new(Some(&self_type), &generics)
                                .with_bounds(&bounds),
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
                            external: None,
                            external_declared: false,
                            test: None,
                            test_assertion: None,
                            member_index: Some(member_index),
                            self_type: Some(self_type.clone()),
                            type_parameters: generics,
                            bounds,
                            impl_member: None,
                            implements: None,
                        });
                    }
                }
                // Declared with the types above; contracts lower below.
                Declaration::Defint(_) => {}
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

    {
        let plan_modules = modules
            .iter()
            .map(|module| crate::interfaces::PlanModule {
                source_id: module.record.source_id(),
                declarations: module.ast.declarations(),
            })
            .collect::<Vec<_>>();
        crate::interfaces::materialize(
            &mut types,
            &plan_modules,
            &mut functions,
            &|module_index, span| {
                let module = modules.get(module_index)?;
                module
                    .declarations
                    .get(&(module.record.source_id().to_owned(), span))
                    .map(DeclarationId::canonical)
            },
            &mut diagnostics,
        );
    }
    for global in &globals {
        for error in
            types.unsatisfied_bounds(crate::nominal::Scope::NONE, &global.value_type)
        {
            crate::nominal::report_lower_error(
                &mut diagnostics,
                &global.source_id,
                global.span,
                &error,
            );
        }
    }

    for declaration in snapshot.declarations().iter().filter(|declaration| {
        selected.contains(declaration.source_id())
            && is_verified_assertion_declaration(
                snapshot,
                declaration.id(),
                verification,
            )
    }) {
        let Some(assertion) = TestAssertion::from_member(declaration.id().name())
        else {
            continue;
        };
        let index = functions.len();
        function_indices.insert(declaration.id().clone(), index);
        functions.push(FunctionHeader {
            declaration_index: usize::MAX,
            module_index: usize::MAX,
            source_id: declaration.source_id().to_owned(),
            name: declaration.id().canonical(),
            signature: assertion.signature(),
            external: None,
            external_declared: true,
            test: None,
            test_assertion: Some(assertion),
            member_index: None,
            self_type: None,
            type_parameters: assertion.type_parameters(),
            bounds: BTreeMap::new(),
            impl_member: None,
            implements: None,
        });
    }

    // Builtin static methods such as `array.of` have no module in the graph;
    // only the members a selected module names join its programs.
    for member in crate::standard::builtin_members(&types) {
        let id = vibra_resolve::builtin_member_id(&member.type_name, &member.member);
        let referenced = snapshot.references().iter().any(|reference| {
            selected.contains(reference.source_id()) && reference.target() == Some(&id)
        });
        if !referenced || function_indices.contains_key(&id) {
            continue;
        }
        let index = functions.len();
        function_indices.insert(id.clone(), index);
        functions.push(FunctionHeader {
            declaration_index: crate::IMPORTED_FUNCTION_DECLARATION,
            module_index: usize::MAX,
            source_id: crate::STDLIB_BUILTIN_SOURCE_ID.to_owned(),
            name: id.canonical(),
            signature: member.signature,
            external: Some(member.intrinsic),
            external_declared: true,
            test: None,
            test_assertion: None,
            member_index: None,
            self_type: None,
            type_parameters: member.type_parameters,
            bounds: BTreeMap::new(),
            impl_member: None,
            implements: None,
        });
    }

    let mut resolved_targets = BTreeMap::new();
    for reference in snapshot
        .references()
        .iter()
        .filter(|reference| selected.contains(reference.source_id()))
    {
        let assertion_reference = reference.target().is_some_and(|target| {
            is_verified_assertion_declaration(snapshot, target, verification)
        });
        let local_non_test_reference = snapshot
            .modules()
            .iter()
            .find(|module| module.source_id() == reference.source_id())
            .is_some_and(|module| {
                module.package() == snapshot.package() && module.unit() != "tests"
            });
        if assertion_reference && local_non_test_reference {
            diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::ToolUnavailable,
                    reference.span(),
                    "verified test assertions are available only in @tests declarations",
                )
                .with_source_id(reference.source_id()),
            );
        }
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
            &types,
        );
        environment.resolved_targets = Some(&resolved_targets);
        environment.reports_redeclarations = false;
        let Some(expression) = check_expression(
            &mut environment,
            &header.expression,
            Some(header.value_type.clone()),
        ) else {
            continue;
        };
        let origin = SourceOrigin::new(header.source_id.as_str(), header.span);
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
        if header.declaration_index == crate::IMPORTED_FUNCTION_DECLARATION
            && let Some(intrinsic) = header.external
        {
            let origin =
                SourceOrigin::new(header.source_id.as_str(), ByteSpan::empty_at(0));
            if let Ok(checked) = CheckedFunction::new_external(
                header.name,
                header.signature,
                intrinsic,
                origin,
            ) && let Some(slot) = checked_functions.get_mut(index)
            {
                *slot = Some(checked);
            }
            continue;
        }
        if let Some(assertion) = header.test_assertion {
            let origin =
                SourceOrigin::new(header.source_id.as_str(), ByteSpan::empty_at(0));
            match CheckedFunction::new_test_assertion(header.name, assertion, origin) {
                Ok(checked) => {
                    if let Some(slot) = checked_functions.get_mut(index) {
                        *slot = Some(checked);
                    }
                }
                Err(error) => unavailable(
                    &mut diagnostics,
                    &header.source_id,
                    ByteSpan::empty_at(0),
                    format!("checked assertion construction failed: {error}"),
                ),
            }
            continue;
        }
        if let Some(test) = header.test.as_ref() {
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
                &types,
            );
            environment.exit = Some((Type::Void, None));
            environment.resolved_targets = Some(&resolved_targets);
            environment.reports_redeclarations = false;
            let Some(body) = crate::check_sequence(
                &mut environment,
                test.expressions(),
                Some(Type::Void),
                test.span(),
                true,
            ) else {
                continue;
            };
            let origin = SourceOrigin::new(header.source_id.as_str(), test.span());
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
                    test.span(),
                    format!("checked test construction failed: {error}"),
                ),
            }
            continue;
        }
        let Some(module) = modules.get(header.module_index) else {
            continue;
        };
        let Some(function) = crate::header_function(module.ast.declarations(), &header)
        else {
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
            let origin = SourceOrigin::new(header.source_id.as_str(), function.span());
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
            &types,
        );
        environment.resolved_targets = Some(&resolved_targets);
        environment.self_type = header.self_type.clone();
        environment.generics = header.type_parameters.clone();
        environment.bounds = header.bounds.clone();
        environment.exit =
            Some((header.signature.result(), Some(function.result_span())));
        environment.reports_redeclarations = false;
        let mut parameters_valid = true;
        let mut pending = Vec::new();
        for (parameter_index, parameter) in function.parameters().iter().enumerate() {
            match parameter.parsed_pattern().kind() {
                vibra_syntax::PatternKind::Binding(name) if name.is_discard() => {}
                vibra_syntax::PatternKind::Binding(name) => {
                    if !environment.add_binding(
                        name.value(),
                        parameter.value_type(),
                        parameter.span(),
                        parameter.parsed_pattern().span(),
                    ) {
                        parameters_valid = false;
                    }
                }
                _ => match header.signature.parameters().get(parameter_index) {
                    Some(value_type) => pending.push((
                        parameter_index,
                        parameter.parsed_pattern(),
                        value_type.clone(),
                    )),
                    None => parameters_valid = false,
                },
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
                    entry.name_span(),
                ) {
                    parameters_valid = false;
                }
            }
        }
        if !environment.bind_variadic(function.attributes().items(), &header.signature)
        {
            parameters_valid = false;
        }
        let Some(destructured) = environment.bind_parameter_patterns(&pending) else {
            continue;
        };
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
        let origin = SourceOrigin::new(header.source_id.as_str(), function.span());
        let body = crate::wrap_destructured(body, destructured, &origin);
        let implements = header.implements.clone();
        match CheckedFunction::with_slots(
            header.name,
            header.signature,
            body,
            origin,
            environment.next_slot,
        )
        .map(|checked| match implements {
            Some(implements) => checked.with_implements(implements),
            None => checked,
        }) {
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

    let can_build_unavailable_stubs = diagnostics.iter().all(|diagnostic| {
        diagnostic.level() != Level::Error
            || diagnostic.code() == DiagnosticCode::ToolUnavailable
    });
    if can_build_unavailable_stubs {
        for (index, checked) in checked_globals.iter_mut().enumerate() {
            if checked.is_some() {
                continue;
            }
            let Some(header) = globals.get(index) else {
                continue;
            };
            let origin = SourceOrigin::new(header.source_id.as_str(), header.span);
            let Some(initializer) =
                default_expression(&header.value_type, origin.clone())
            else {
                continue;
            };
            *checked = CheckedGlobal::new(
                header.name.clone(),
                header.value_type.clone(),
                initializer,
                origin,
            )
            .ok();
        }
        for (index, checked) in checked_functions.iter_mut().enumerate() {
            if checked.is_some() {
                continue;
            }
            let Some(header) = functions.get(index) else {
                continue;
            };
            let span = header
                .test
                .as_ref()
                .map_or(ByteSpan::empty_at(0), vibra_syntax::TestDeclaration::span);
            let origin = SourceOrigin::new(header.source_id.as_str(), span);
            let body = if header.test.is_some() {
                Some(Expr::sequence(Vec::new(), origin.clone()))
            } else {
                default_expression(&header.signature.result(), origin.clone())
            };
            if let Some(body) = body {
                *checked = CheckedFunction::with_slots(
                    header.name.clone(),
                    header.signature.clone(),
                    body,
                    origin,
                    header.signature.fixed_parameter_count(),
                )
                .ok();
            }
        }
    }
    // Index facts: the checked type of every header, and every `impl` block.
    let result_id = types
        .role("result")
        .and_then(|index| types.get(index))
        .map(|declared| declared.id.clone());
    let describe = |value: &Type| IndexedSignature {
        signature: vibra_ir::canonical_type(value),
        errors: match value {
            Type::Function(signature) => match signature.result() {
                Type::Applied(id, arguments) if Some(&id) == result_id.as_ref() => {
                    arguments
                        .get(1)
                        .map(vibra_ir::canonical_type)
                        .into_iter()
                        .collect()
                }
                _ => Vec::new(),
            },
            _ => Vec::new(),
        },
    };
    let mut signatures = BTreeMap::new();
    for (id, index) in &function_indices {
        if let Some(header) = functions.get(*index) {
            signatures.insert(
                id.clone(),
                describe(&Type::Function(Box::new(header.signature.clone()))),
            );
        }
    }
    for (id, index) in &global_indices {
        if let Some(global) = globals.get(*index) {
            signatures.insert(id.clone(), describe(&global.value_type));
        }
    }
    // A contract member's signature is written over `self`.
    for interface in types.interfaces() {
        let Some(module) = modules
            .iter()
            .find(|module| module.record.source_id() == interface.source_id)
        else {
            continue;
        };
        for member in &interface.members {
            if let Some(id) = module
                .declarations
                .get(&(interface.source_id.clone(), member.span))
            {
                signatures.insert(
                    id.clone(),
                    describe(&Type::Function(Box::new(member.signature.clone()))),
                );
            }
        }
    }
    let implementations = types
        .implementations()
        .iter()
        .filter_map(|implementation| {
            let interface = types.interface(implementation.interface)?;
            Some(IndexedImplementation {
                receiver: vibra_ir::canonical_type(&implementation.receiver),
                interface: vibra_ir::canonical_type(&Type::Interface(
                    interface.id.clone(),
                    implementation.arguments.clone(),
                )),
                interface_id: interface.id.id().to_owned(),
                source_id: implementation.source_id.clone(),
                span: implementation.span,
                members: implementation
                    .members
                    .iter()
                    .filter_map(|(name, index)| {
                        let header = functions.get(*index)?;
                        header.impl_member?;
                        let module = modules.get(header.module_index)?;
                        let function =
                            crate::header_function(module.ast.declarations(), header)?;
                        Some(IndexedMember {
                            contract: name.clone(),
                            span: function.span(),
                            signature: vibra_ir::canonical_type(&Type::Function(
                                Box::new(header.signature.clone()),
                            )),
                        })
                    })
                    .collect(),
            })
        })
        .collect();
    let globals = checked_globals.into_iter().collect::<Option<Vec<_>>>();
    let functions = checked_functions.into_iter().collect::<Option<Vec<_>>>();
    // One validated module set serves every entry and test program; entries
    // add only their own call-flow analysis and share the checked IR.
    let mut module_set = None;
    if let (Some(globals), Some(functions)) = (globals, functions) {
        let global_origins = globals
            .iter()
            .map(|global| global.origin().clone())
            .collect::<Vec<_>>();
        let validated = if functions.is_empty() {
            vibra_ir::validate_global_initializer_cycles(&globals, &functions)
                .map(|()| None)
        } else {
            CheckedModuleSet::try_new_with_types(
                types.definitions(),
                globals,
                functions,
            )
            .map(Some)
        };
        match validated {
            Ok(set) => module_set = set,
            Err(IrError::GlobalInitializerCycle(global_index)) => {
                if let Some(origin) = global_origins.get(global_index) {
                    diagnostics.push(crate::initializer_cycle_diagnostic(origin));
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
    let has_blocking_error = diagnostics.iter().any(|diagnostic| {
        diagnostic.level() == Level::Error
            && diagnostic.code() != DiagnosticCode::ToolUnavailable
    });
    let mut programs = BTreeMap::new();
    if !has_blocking_error && let Some(set) = &module_set {
        let entries = snapshot
            .entries()
            .iter()
            .filter_map(|entry| entry.declaration())
            .map(|declaration| (declaration, "checked IR construction failed"));
        let tests = function_indices
            .keys()
            .filter(|declaration| declaration.kind() == vibra_resolve::EntityKind::Test)
            .map(|declaration| {
                (declaration, "checked test program construction failed")
            });
        let mut failed_entry = false;
        for (declaration, failure) in entries.chain(tests) {
            let is_test = declaration.kind() == vibra_resolve::EntityKind::Test;
            if failed_entry && !is_test {
                continue;
            }
            let Some(index) = function_indices.get(declaration).copied() else {
                continue;
            };
            match CheckedProgram::for_entry(Arc::clone(set), index) {
                Ok(program) => {
                    programs.insert(declaration.clone(), program);
                }
                Err(error) => {
                    if let IrError::GlobalInitializerCycle(global_index) = error
                        && !is_test
                    {
                        if let Some(global) = set.globals().get(global_index) {
                            diagnostics.push(crate::initializer_cycle_diagnostic(
                                global.origin(),
                            ));
                        }
                    } else if let Some(resolved) = snapshot
                        .declarations()
                        .iter()
                        .find(|candidate| candidate.id() == declaration)
                    {
                        unavailable(
                            &mut diagnostics,
                            resolved.source_id(),
                            resolved.span(),
                            format!("{failure}: {error}"),
                        );
                    }
                    // A test program fails on its own entry, so the tests after
                    // it are still built. One failed target entry describes
                    // the shared module set, so later ones are skipped.
                    if is_test {
                        continue;
                    }
                    failed_entry = true;
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
        signatures,
        implementations,
        modules: module_set,
    }
}

fn default_expression(value_type: &Type, origin: SourceOrigin) -> Option<Expr> {
    let value = match value_type {
        Type::Bool => Some(Value::Bool(false)),
        Type::Void => Some(Value::Void),
        Type::Char => Some(Value::Char('\0')),
        Type::Str => Some(Value::Str(String::new())),
        Type::Bytes => Some(Value::Bytes(Vec::new())),
        Type::Atom => Some(Value::Atom(String::new())),
        Type::I8 => Some(Value::I8(0)),
        Type::I16 => Some(Value::I16(0)),
        Type::I32 => Some(Value::I32(0)),
        Type::I64 => Some(Value::I64(0)),
        Type::U8 => Some(Value::U8(0)),
        Type::U16 => Some(Value::U16(0)),
        Type::U32 => Some(Value::U32(0)),
        Type::U64 => Some(Value::U64(0)),
        Type::F32 => Value::f32(0.0),
        Type::F64 => Value::f64(0.0),
        // Declared and structural types have no placeholder value.
        Type::Declared(_)
        | Type::Record(_)
        | Type::Enum(_)
        | Type::Param(_)
        | Type::Applied(_, _)
        | Type::Tuple(_)
        | Type::Union(_)
        | Type::AtomSingleton(_)
        | Type::Array(_)
        | Type::Dict(_, _)
        | Type::Interface(_, _)
        | Type::Any => None,
        Type::Function(signature) => {
            let body = default_expression(&signature.result(), origin.clone())?;
            return Some(Expr::closure(
                (**signature).clone(),
                signature.parameters().to_vec(),
                Vec::new(),
                body,
                signature.fixed_parameter_count(),
                origin,
            ));
        }
    }?;
    Some(Expr::literal(value, origin))
}

fn is_verified_assertion_declaration(
    snapshot: &ResolvedSnapshot,
    id: &DeclarationId,
    verification: Option<&crate::Stdlib>,
) -> bool {
    let Some(verification) = verification else {
        return false;
    };
    if id.package() != verification.package()
        || id.unit() != "std"
        || id.module() != ["assert"]
        || id.kind() != vibra_resolve::EntityKind::Function
        || id.path().len() != 1
        || TestAssertion::from_member(id.name()).is_none()
    {
        return false;
    }
    let Some(declaration) = snapshot
        .declarations()
        .iter()
        .find(|declaration| declaration.id() == id)
    else {
        return false;
    };
    let Some(module) = snapshot
        .modules()
        .iter()
        .find(|module| module.source_id() == declaration.source_id())
    else {
        return false;
    };
    module.package() == verification.package()
        && module.unit() == "std"
        && module.segments() == ["assert"]
        && verification.trusts_module(module)
}

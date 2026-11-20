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
    Binder, Case, CheckedFunction, CheckedGlobal, CheckedModuleSet, FnType,
    FunctionBody, Implementation, ImplementationRecord, InterfaceDefinition,
    InterfaceKey, InterfaceMember, LibraryTypes, ModuleSetParts, NamedTypes,
    SignatureRecord, SourceOrigin, Type, TypeBody, TypeDefinition, Value, Visibility,
    encode_type,
};
use vibra_syntax::Document;

use super::CheckResult;
use super::callables::fold;
use super::cycle::initializer_cycles;
use super::declare::{Entity, Program, TypeData, TypeKind};
use super::expr::{BodyChecker, Checked, Layout, Reference, Target};
use super::generics::{Instantiation, expanding_instantiations};

/// The bodies of one pass.
struct Pass {
    functions: BTreeMap<usize, Checked>,
    values: BTreeMap<usize, Checked>,
    /// The diagnostics of each declaration.
    faults: BTreeMap<Target, Vec<Diagnostic>>,
    /// The references of each declaration.
    references: BTreeMap<Target, Vec<Reference>>,
    /// Every instantiation of a generic function inside a body.
    instantiations: Vec<Instantiation>,
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
        faults: BTreeMap::new(),
        references: BTreeMap::new(),
        instantiations: Vec::new(),
    };
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
        let self_type = data
            .owner
            .and_then(|owner| program.types.get(owner))
            .and_then(TypeData::self_type);
        let mut checker = BodyChecker::new(program, layout, data.module, text);
        let patterns = data
            .view
            .takes()
            .into_iter()
            .map(|(pattern, _)| pattern)
            .collect::<Vec<_>>();
        let labelled = data.view.labelled();
        let checked = checker.callable(
            signature,
            &patterns,
            &labelled,
            &data.binders,
            self_type,
            body,
        );
        pass.functions.insert(index, checked);
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
        let checked = checker.initializer(ty, value);
        pass.values.insert(index, checked);
        pass.faults.insert(
            Target::Value(index),
            std::mem::take(&mut checker.diagnostics),
        );
        pass.references.insert(
            Target::Value(index),
            std::mem::take(&mut checker.references),
        );
    }
    pass
}

fn module_of(program: &Program<'_>, target: Target) -> Option<usize> {
    match target {
        Target::Function(index) => program.functions.get(index).map(|data| data.module),
        Target::Value(index) => program.values.get(index).map(|data| data.module),
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
        }
    }
}

/// The value of each module value whose initializer, checked in `first`
/// under the layout `provisional`, is a constant expression, by its global
/// index in `layout`. An initializer that names another module value is
/// constant when that one is, so the values are folded in rounds until no
/// more are found.
fn constants(
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
    layout.constants = constants(&provisional, &first, &layout);
    layout.constants_known = true;
    drop(first);
    let mut pass = check_bodies(&program, documents, &layout);
    let mut diagnostics = std::mem::take(&mut program.diagnostics);
    for (target, faults) in &mut pass.faults {
        if !embedded(&program, *target) {
            diagnostics.append(faults);
        }
    }
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
    };
    if result.has_errors() {
        if result.only_unavailable() {
            result.types = types_document(&program);
        }
        return result;
    }
    match assemble(&program, &layout, pass) {
        Ok((set, functions)) => {
            result.types = types_document(&program);
            result.entry = entry(&program, &layout);
            result.functions = functions;
            result.module_set = Some(set);
        }
        Err(message) => result.diagnostics.push(Diagnostic::new(
            DiagnosticCode::RuntimeInvalidCheckedProgram,
            ByteSpan::empty_at(0),
            format!("the checker built an invalid module set: {message}"),
        )),
    }
    result
}

fn origin(program: &Program<'_>, module: usize, span: ByteSpan) -> SourceOrigin {
    let source = program
        .modules
        .get(module)
        .map_or_else(|| Arc::from(""), |data| Arc::clone(&data.source_id));
    SourceOrigin::new(source, span)
}

type Assembled = (Arc<CheckedModuleSet>, Vec<(String, usize)>);

fn assemble(
    program: &Program<'_>,
    layout: &Layout,
    mut pass: Pass,
) -> Result<Assembled, String> {
    let in_scope = |module: usize| {
        program
            .modules
            .get(module)
            .is_some_and(|data| data.in_scope)
    };
    let mut types = Vec::new();
    for data in &program.types {
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
        let definition =
            TypeDefinition::new(id, data.params.clone(), body, None, visibility)
                .map_err(|error| error.to_string())?;
        types.push(definition);
    }
    let mut interfaces = Vec::new();
    for data in &program.interfaces {
        if !in_scope(data.module) {
            continue;
        }
        let Some(id) = data.id.clone() else { continue };
        let mut members = Vec::new();
        for member in &data.members {
            let signature = SignatureRecord::interface_member(
                Vec::new(),
                member.receiver.and_then(|slot| u64::try_from(slot).ok()),
                Type::function(member.signature.clone()),
                Vec::new(),
            )
            .map_err(|error| error.to_string())?;
            members.push(InterfaceMember {
                name: member.name.clone(),
                signature,
                default: None,
            });
        }
        interfaces.push(
            InterfaceDefinition::new(id, Vec::new(), Vec::new(), members)
                .map_err(|error| error.to_string())?,
        );
    }
    let mut globals = Vec::new();
    for (index, data) in program.values.iter().enumerate() {
        if layout.globals.get(index).copied().flatten().is_none() {
            continue;
        }
        let (Some(ty), Some(checked)) = (data.ty.clone(), pass.values.remove(&index))
        else {
            return Err(format!(
                "the module value `{}` was not checked",
                data.qualified
            ));
        };
        let Some(initializer) = checked.node else {
            return Err(format!(
                "the module value `{}` was not lowered",
                data.qualified
            ));
        };
        globals.push(CheckedGlobal {
            name: data.qualified.clone(),
            value_type: ty,
            initializer,
            slot_count: checked.slot_count,
            origin: origin(program, data.module, data.view.construct().span()),
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
        let record = signature_record(&data.binders, signature)?;
        let origin = origin(program, data.module, data.view.construct().span());
        let function = match data.external {
            Some(intrinsic) => CheckedFunction::external(
                data.qualified.clone(),
                record,
                intrinsic,
                (0..data.binders.len()).map(Type::Binder).collect(),
                origin,
            ),
            None => {
                let Some(checked) = pass.functions.remove(&index) else {
                    return Err(format!(
                        "the function `{}` was not checked",
                        data.qualified
                    ));
                };
                let Some(body) = checked.node else {
                    return Err(format!(
                        "the function `{}` was not lowered",
                        data.qualified
                    ));
                };
                CheckedFunction {
                    name: data.qualified.clone(),
                    signature: record,
                    defaults: checked.defaults,
                    body: FunctionBody::Expr(Box::new(body)),
                    entry_guard: None,
                    exit_guard: None,
                    slot_count: checked.slot_count,
                    origin,
                }
            }
        };
        names.push((data.qualified.clone(), at));
        functions.push(function);
    }
    let mut implementations = Vec::new();
    for data in &program.impls {
        let owner_module = program.types.get(data.owner).map(|owner| owner.module);
        if !owner_module.is_some_and(in_scope) {
            continue;
        }
        let Some(interface) = program
            .interfaces
            .get(data.interface)
            .and_then(|i| i.id.clone())
        else {
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
        let record = ImplementationRecord::new(
            Vec::new(),
            InterfaceKey::new(interface, Vec::new()),
            data.receiver.clone(),
            NamedTypes::empty(),
        )
        .map_err(|error| error.to_string())?;
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
    Ok((set, names))
}

/// The signature record of a function with the type parameters `binders`,
/// each bounded by `Any`.
fn signature_record(
    binders: &[String],
    signature: FnType,
) -> Result<SignatureRecord, String> {
    let binders = binders
        .iter()
        .map(|name| Binder::new(name.clone(), None))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    SignatureRecord::new(binders, Type::function(signature))
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
fn types_document(program: &Program<'_>) -> Option<String> {
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
        let record = signature_record(&data.binders, signature).ok()?;
        callables.push((
            data.identity.clone(),
            format!(
                "{{declaration: \"{}\" signature: {}}}",
                data.identity,
                record.encode().ok()?
            ),
        ));
    }
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
        "{{format: @types.v2 moduleValues: [{}] callables: [{}] implementations: [] obligations: []}}\n",
        join(values),
        join(callables)
    ))
}

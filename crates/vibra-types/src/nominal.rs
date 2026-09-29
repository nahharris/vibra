//! Declared types and type-expression lowering (M3 Step 2).
//!
//! `docs/spec/02-type-system.md` makes every `deftype` a new identity and
//! anonymous `record` and `enum` types structural and order-insensitive. This
//! module collects the declared types one checking run can see, resolves type
//! names against them, lowers type expressions to IR [`Type`]s, and rejects a
//! declared type whose expansion never passes through a variable-size
//! container.
//!
//! Type expressions carry no source spans, so a failure is reported at the
//! owning parameter, result, field, or declaration span the caller supplies.

use std::collections::{BTreeMap, BTreeSet};

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::{
    FunctionSignature, LabelledParameter, Type, TypeBody, TypeDefinition, TypeId,
};
use vibra_syntax::{
    Attribute, DeftypeBody, DeftypeDeclaration, Name, TypeExpr, TypeMember,
};

use crate::unavailable;

/// Why a type expression could not be lowered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LowerError {
    /// A valid form owned by a later M3 step.
    Unavailable(&'static str),
    /// A name that denotes no visible type.
    Unknown(String),
    /// A name that denotes a type the current module may not see.
    Private(String),
    /// A declared type written with the wrong number of type arguments.
    Arity {
        name: String,
        expected: usize,
        found: usize,
    },
}

/// What a type expression may name besides declared types: the receiver type
/// inside a nested method, and the generic names in scope.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Scope<'a> {
    pub(crate) self_type: Option<&'a Type>,
    pub(crate) generics: &'a [String],
}

impl<'a> Scope<'a> {
    /// A scope with neither a receiver nor generic names.
    pub(crate) const NONE: Self = Self {
        self_type: None,
        generics: &[],
    };

    /// A scope with a receiver type and generic names.
    pub(crate) const fn new(
        self_type: Option<&'a Type>,
        generics: &'a [String],
    ) -> Self {
        Self {
            self_type,
            generics,
        }
    }
}

/// One declared type visible to a checking run.
#[derive(Clone, Debug)]
pub(crate) struct DeclaredType {
    pub(crate) id: TypeId,
    pub(crate) name: String,
    pub(crate) source_id: String,
    pub(crate) span: ByteSpan,
    pub(crate) public: bool,
    /// The lowered body, or `None` when the body is outside Stage 3A Step 2 or
    /// failed to lower.
    pub(crate) body: Option<TypeBody>,
    /// Whether the type may be named. A declaration whose body did not lower,
    /// or whose body names such a type, is reported once at its declaration
    /// and is unavailable at every use.
    pub(crate) available: bool,
    /// Generic parameter names in `where:` order.
    pub(crate) parameters: Vec<String>,
}

/// Type names visible from one source module.
#[derive(Clone, Debug, Default)]
struct ModuleScope {
    local: BTreeMap<String, usize>,
    imports: BTreeMap<String, String>,
}

/// Every declared type of one checking run and the names each module sees.
#[derive(Clone, Debug, Default)]
pub(crate) struct TypeNames {
    declared: Vec<DeclaredType>,
    /// Scopes keyed by source ID.
    scopes: BTreeMap<String, ModuleScope>,
    /// Declared type indices keyed by the source ID of their module.
    by_module: BTreeMap<String, BTreeMap<String, usize>>,
}

/// A value path that names a type constructor or an enum variant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ConstructorTarget {
    /// A declared record or wrapper constructor.
    Type(usize),
    /// A declared enum variant constructor.
    Variant(usize, String),
}

impl TypeNames {
    /// Registers a declared type owned by `source_id`, returning its index.
    pub(crate) fn declare(
        &mut self,
        source_id: &str,
        declaration: &DeftypeDeclaration,
        id: TypeId,
    ) -> usize {
        let index = self.declared.len();
        let name = declaration.name().value().to_owned();
        self.declared.push(DeclaredType {
            id,
            name: name.clone(),
            source_id: source_id.to_owned(),
            span: declaration.span(),
            public: declaration.attributes().items().iter().any(|attribute| {
                matches!(attribute, Attribute::Visibility(name) if name.value() == "public")
            }),
            body: None,
            available: true,
            parameters: generic_names(declaration.attributes().items()),
        });
        self.scopes
            .entry(source_id.to_owned())
            .or_default()
            .local
            .insert(name.clone(), index);
        self.by_module
            .entry(source_id.to_owned())
            .or_default()
            .insert(name, index);
        index
    }

    /// Makes the declared types of `target_source_id` visible from
    /// `source_id` through `alias`.
    pub(crate) fn import(
        &mut self,
        source_id: &str,
        alias: &str,
        target_source_id: &str,
    ) {
        self.scopes
            .entry(source_id.to_owned())
            .or_default()
            .imports
            .insert(alias.to_owned(), target_source_id.to_owned());
    }

    /// Declared types in registration order.
    pub(crate) fn declared(&self) -> &[DeclaredType] {
        &self.declared
    }

    /// One declared type.
    pub(crate) fn get(&self, index: usize) -> Option<&DeclaredType> {
        self.declared.get(index)
    }

    /// The lowered definitions, for the checked program.
    pub(crate) fn definitions(&self) -> Vec<TypeDefinition> {
        self.declared
            .iter()
            .filter_map(|declared| {
                declared.body.clone().map(|body| {
                    TypeDefinition::new(declared.id.clone(), body)
                        .with_parameters(declared.parameters.clone())
                })
            })
            .collect()
    }

    /// The index of the declared type with `id`.
    pub(crate) fn index_of(&self, id: &TypeId) -> Option<usize> {
        self.declared.iter().position(|declared| &declared.id == id)
    }

    /// Resolves a type name written in `source_id`: `t` names a type of the
    /// same module and `alias.t` a public type of an imported module.
    pub(crate) fn resolve(
        &self,
        source_id: &str,
        name: &Name,
    ) -> Result<usize, LowerError> {
        let scope = self.scopes.get(source_id);
        match name.segments() {
            [local] => scope
                .and_then(|scope| scope.local.get(local))
                .copied()
                .ok_or_else(|| LowerError::Unknown(name.value().to_owned())),
            [alias, member] => {
                let target = scope
                    .and_then(|scope| scope.imports.get(alias))
                    .and_then(|target| self.by_module.get(target))
                    .and_then(|types| types.get(member))
                    .copied()
                    .ok_or_else(|| LowerError::Unknown(name.value().to_owned()))?;
                if self
                    .declared
                    .get(target)
                    .is_some_and(|declared| declared.public)
                {
                    Ok(target)
                } else {
                    Err(LowerError::Private(name.value().to_owned()))
                }
            }
            _ => Err(LowerError::Unknown(name.value().to_owned())),
        }
    }

    /// Resolves a value path that names a constructor: `t`, `t.variant`,
    /// `alias.t`, or `alias.t.variant`. Returns `None` for any other path so
    /// the caller can report its own unknown-name diagnostic.
    pub(crate) fn constructor(
        &self,
        source_id: &str,
        name: &Name,
    ) -> Option<ConstructorTarget> {
        let segments = name.segments();
        let scope = self.scopes.get(source_id)?;
        let local = |segment: &String| scope.local.get(segment).copied();
        let imported = |alias: &String, member: &String| {
            let target = scope.imports.get(alias)?;
            let index = self.by_module.get(target)?.get(member).copied()?;
            self.declared.get(index)?.public.then_some(index)
        };
        match segments {
            [type_name] => local(type_name).map(ConstructorTarget::Type),
            [first, second] => local(first)
                .map(|index| ConstructorTarget::Variant(index, second.clone()))
                .or_else(|| imported(first, second).map(ConstructorTarget::Type)),
            [alias, type_name, variant] => imported(alias, type_name)
                .map(|index| ConstructorTarget::Variant(index, variant.clone())),
            _ => None,
        }
    }

    /// Lowers a type expression written in `source_id`, seeing the receiver
    /// type and generic names of `scope`.
    pub(crate) fn lower(
        &self,
        source_id: &str,
        scope: Scope<'_>,
        value: &TypeExpr,
    ) -> Result<Type, LowerError> {
        match value {
            TypeExpr::Void => Ok(Type::Void),
            TypeExpr::Name(name) => {
                if let Some(primitive) = primitive_type(name.value()) {
                    return Ok(primitive);
                }
                if name.value() == "self" {
                    return scope
                        .self_type
                        .cloned()
                        .ok_or_else(|| LowerError::Unknown("self".to_owned()));
                }
                if scope.generics.iter().any(|generic| generic == name.value()) {
                    return Ok(Type::Param(name.value().to_owned()));
                }
                self.applied(source_id, name, Vec::new())
            }
            TypeExpr::Function(function) => {
                if !function.effects().is_empty() {
                    return Err(LowerError::Unavailable(
                        "nonempty function-type effect rows arrive in M4",
                    ));
                }
                if function.variadic().is_some() {
                    return Err(LowerError::Unavailable(
                        "variadic function types arrive in M3 Step 4",
                    ));
                }
                let parameters = function
                    .parameters()
                    .iter()
                    .map(|parameter| self.lower(source_id, scope, parameter))
                    .collect::<Result<Vec<_>, _>>()?;
                let labelled = function
                    .labelled()
                    .iter()
                    .map(|slot| {
                        Ok(LabelledParameter::new(
                            slot.name().value(),
                            self.lower(source_id, scope, slot.value_type())?,
                            None,
                        ))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let result = self.lower(source_id, scope, function.result())?;
                Ok(Type::Function(Box::new(FunctionSignature::with_labelled(
                    parameters, labelled, result,
                ))))
            }
            TypeExpr::Record(fields) => Ok(Type::Record(vibra_ir::canonical_members(
                self.lower_members(source_id, scope, fields)?,
            ))),
            TypeExpr::Enum(variants) => Ok(Type::Enum(vibra_ir::canonical_members(
                self.lower_members(source_id, scope, variants)?,
            ))),
            TypeExpr::Applied { head, arguments } => {
                if matches!(head.value(), "option" | "result")
                    && matches!(
                        self.resolve(source_id, head),
                        Err(LowerError::Unknown(_))
                    )
                {
                    return self.applied(source_id, head, Vec::new());
                }
                let arguments = arguments
                    .iter()
                    .map(|argument| self.lower(source_id, scope, argument))
                    .collect::<Result<Vec<_>, _>>()?;
                self.applied(source_id, head, arguments)
            }
            TypeExpr::Tuple(_) | TypeExpr::Array(_) | TypeExpr::Map(_, _) => {
                Err(LowerError::Unavailable(
                    "tuple, array, and map types arrive in M3 Step 4",
                ))
            }
            TypeExpr::Union(_) => {
                Err(LowerError::Unavailable("union types arrive in M3 Step 6"))
            }
        }
    }

    /// Lowers `value`, reporting a failure at the owning `span`.
    pub(crate) fn lower_or_report(
        &self,
        source_id: &str,
        scope: Scope<'_>,
        value: &TypeExpr,
        span: ByteSpan,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<Type> {
        match self.lower(source_id, scope, value) {
            Ok(value) => Some(value),
            Err(error) => {
                report_lower_error(diagnostics, source_id, span, &error);
                None
            }
        }
    }

    /// A declared type named by `name` and applied to `arguments`, which must
    /// match its complete generic parameter list.
    fn applied(
        &self,
        source_id: &str,
        name: &Name,
        arguments: Vec<Type>,
    ) -> Result<Type, LowerError> {
        let index = match self.resolve(source_id, name) {
            Err(LowerError::Unknown(_))
                if matches!(name.value(), "option" | "result") =>
            {
                return Err(LowerError::Unavailable(
                    "the option and result library types arrive in M3 Steps 4 and 7",
                ));
            }
            resolved => resolved?,
        };
        let declared = self
            .declared
            .get(index)
            .ok_or_else(|| LowerError::Unknown(name.value().to_owned()))?;
        if !declared.available {
            return Err(LowerError::Unavailable(
                "this declared type is outside the available M3 profile",
            ));
        }
        if arguments.len() != declared.parameters.len() {
            return Err(LowerError::Arity {
                name: name.value().to_owned(),
                expected: declared.parameters.len(),
                found: arguments.len(),
            });
        }
        Ok(if arguments.is_empty() {
            Type::Declared(declared.id.clone())
        } else {
            Type::Applied(declared.id.clone(), arguments)
        })
    }

    fn lower_members(
        &self,
        source_id: &str,
        scope: Scope<'_>,
        members: &[vibra_syntax::TypeField],
    ) -> Result<Vec<(String, Type)>, LowerError> {
        members
            .iter()
            .map(|member| {
                Ok((
                    member.name().value().to_owned(),
                    self.lower(source_id, scope, member.ty())?,
                ))
            })
            .collect()
    }

    /// Lowers the body of every registered declaration, reports bodies
    /// outside this step, and runs the finite-size check.
    pub(crate) fn lower_bodies(
        &mut self,
        declarations: &[(usize, &DeftypeDeclaration)],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let mut lowered = Vec::with_capacity(declarations.len());
        for (index, declaration) in declarations {
            let Some(declared) = self.declared.get(*index) else {
                continue;
            };
            let source_id = declared.source_id.clone();
            let parameters = declared.parameters.clone();
            let self_type = declared_self_type(&declared.id, &parameters);
            if !report_interface_bounds(
                declaration.attributes().items(),
                &source_id,
                diagnostics,
            ) {
                continue;
            }
            for member in declaration.members() {
                if let TypeMember::Implementation(implementation) = member {
                    unavailable(
                        diagnostics,
                        &source_id,
                        implementation.span(),
                        "impl blocks arrive in M3 Step 11",
                    );
                }
            }
            let body = match declaration.body() {
                DeftypeBody::Type(TypeExpr::Record(fields)) => self
                    .lower_members(
                        &source_id,
                        Scope::new(Some(&self_type), &parameters),
                        fields,
                    )
                    .map(TypeBody::Record),
                DeftypeBody::Type(TypeExpr::Enum(variants)) => self
                    .lower_members(
                        &source_id,
                        Scope::new(Some(&self_type), &parameters),
                        variants,
                    )
                    .map(TypeBody::Enum),
                DeftypeBody::Type(TypeExpr::Tuple(_)) => Err(LowerError::Unavailable(
                    "declared tuple types arrive in M3 Step 4",
                )),
                DeftypeBody::Type(TypeExpr::Union(_)) => Err(LowerError::Unavailable(
                    "declared union types arrive in M3 Step 6",
                )),
                DeftypeBody::Intrinsic(_) => Err(LowerError::Unavailable(
                    "intrinsic-type declarations arrive with the M3 standard library in Step 4",
                )),
                // Any other body declares a wrapper type over one representation.
                DeftypeBody::Type(representation) => self
                    .lower(
                        &source_id,
                        Scope::new(Some(&self_type), &parameters),
                        representation,
                    )
                    .map(TypeBody::Wrapper),
            };
            match body {
                Ok(body) => lowered.push((*index, body)),
                Err(error) => report_lower_error(
                    diagnostics,
                    &source_id,
                    declaration.span(),
                    &error,
                ),
            }
        }
        let mut available = vec![false; self.declared.len()];
        for (index, body) in lowered {
            if let Some(declared) = self.declared.get_mut(index) {
                declared.body = Some(body);
            }
            if let Some(slot) = available.get_mut(index) {
                *slot = true;
            }
        }
        for (declared, available) in self.declared.iter_mut().zip(&available) {
            declared.available = *available;
            if !declared.available {
                declared.body = None;
            }
        }
        self.check_finite_size(declarations, diagnostics);
        self.propagate_unavailability(declarations, diagnostics);
    }

    /// Marks every type whose body names an unavailable type as unavailable,
    /// to a fixed point, reporting each once at its declaration.
    fn propagate_unavailability(
        &mut self,
        declarations: &[(usize, &DeftypeDeclaration)],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        loop {
            let blocked: Vec<usize> = declarations
                .iter()
                .map(|(index, _)| *index)
                .filter(|index| {
                    self.declared.get(*index).is_some_and(|declared| {
                        declared.available
                            && declared
                                .body
                                .as_ref()
                                .is_some_and(|body| self.body_names_unavailable(body))
                    })
                })
                .collect();
            if blocked.is_empty() {
                return;
            }
            for index in blocked {
                if let Some(declared) = self.declared.get_mut(index) {
                    declared.available = false;
                    declared.body = None;
                    unavailable(
                        diagnostics,
                        &declared.source_id,
                        declared.span,
                        "this declared type names a type outside the available M3 profile",
                    );
                }
            }
        }
    }

    fn body_names_unavailable(&self, body: &TypeBody) -> bool {
        match body {
            TypeBody::Record(members) | TypeBody::Enum(members) => members
                .iter()
                .any(|(_, member)| self.names_unavailable(member)),
            TypeBody::Wrapper(representation) => self.names_unavailable(representation),
        }
    }

    fn names_unavailable(&self, value: &Type) -> bool {
        match value {
            Type::Declared(id) => self
                .index_of(id)
                .and_then(|index| self.declared.get(index))
                .is_none_or(|declared| !declared.available),
            Type::Record(members) | Type::Enum(members) => members
                .iter()
                .any(|(_, member)| self.names_unavailable(member)),
            Type::Function(signature) => {
                signature
                    .parameters()
                    .iter()
                    .any(|value| self.names_unavailable(value))
                    || signature.labelled().iter().any(|parameter| {
                        self.names_unavailable(&parameter.value_type())
                    })
                    || self.names_unavailable(&signature.result())
            }
            _ => false,
        }
    }

    /// Rejects declared types whose expansion repeats without passing through
    /// an array, map, or function. Reported once per cycle, at the first
    /// declaration of the cycle in registration order, relating the member
    /// through which the expansion repeats.
    fn check_finite_size(
        &mut self,
        declarations: &[(usize, &DeftypeDeclaration)],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let edges: Vec<Vec<(usize, String)>> = self
            .declared
            .iter()
            .map(|declared| {
                let mut edges = Vec::new();
                match &declared.body {
                    Some(TypeBody::Record(members) | TypeBody::Enum(members)) => {
                        for (name, member) in members {
                            self.direct_edges(member, name, &mut edges);
                        }
                    }
                    Some(TypeBody::Wrapper(representation)) => {
                        self.direct_edges(representation, "", &mut edges);
                    }
                    None => {}
                }
                edges
            })
            .collect();
        let mut reported = BTreeSet::new();
        for (start, _) in declarations {
            if reported.contains(start) {
                continue;
            }
            let Some(cycle) = find_cycle(*start, &edges) else {
                continue;
            };
            let Some(declared) = self.declared.get(*start) else {
                continue;
            };
            let member = cycle
                .first()
                .map(|(_, member)| member.clone())
                .unwrap_or_default();
            let span = declarations
                .iter()
                .find(|(index, _)| index == start)
                .and_then(|(_, declaration)| member_span(declaration, &member))
                .unwrap_or(declared.span);
            diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::TypeInfiniteSize,
                    declared.span,
                    format!(
                        "`{}` contains itself without an array, map, or function between",
                        declared.name
                    ),
                )
                .with_source_id(&declared.source_id)
                .with_related(span, format!("the expansion repeats through `{member}`")),
            );
            for (index, _) in &cycle {
                reported.insert(*index);
            }
            reported.insert(*start);
        }
        for index in reported {
            if let Some(declared) = self.declared.get_mut(index) {
                declared.body = None;
                declared.available = false;
            }
        }
    }

    /// Declared types reachable from `value` without passing through a
    /// variable-size container or a function.
    fn direct_edges(
        &self,
        value: &Type,
        member: &str,
        edges: &mut Vec<(usize, String)>,
    ) {
        match value {
            Type::Declared(id) => {
                if let Some(index) = self.index_of(id) {
                    edges.push((index, member.to_owned()));
                }
            }
            Type::Applied(id, arguments) => {
                let Some(index) = self.index_of(id) else {
                    return;
                };
                edges.push((index, member.to_owned()));
                for (position, argument) in arguments.iter().enumerate() {
                    if self.parameter_is_direct(index, position, &mut BTreeSet::new()) {
                        self.direct_edges(argument, member, edges);
                    }
                }
            }
            Type::Record(members) | Type::Enum(members) => {
                for (_, nested) in members {
                    self.direct_edges(nested, member, edges);
                }
            }
            _ => {}
        }
    }

    /// Whether the generic parameter at `position` of declaration `index`
    /// is stored directly in its body, so an argument there is part of the
    /// applied type's own size. A declaration already being visited adds
    /// nothing: its own recursion is an edge of the size graph.
    fn parameter_is_direct(
        &self,
        index: usize,
        position: usize,
        visiting: &mut BTreeSet<usize>,
    ) -> bool {
        let Some(declared) = self.declared.get(index) else {
            return false;
        };
        let Some(name) = declared.parameters.get(position) else {
            return false;
        };
        if !visiting.insert(index) {
            return false;
        }
        let direct = match &declared.body {
            Some(TypeBody::Record(members) | TypeBody::Enum(members)) => members
                .iter()
                .any(|(_, member)| self.contains_directly(member, name, visiting)),
            Some(TypeBody::Wrapper(representation)) => {
                self.contains_directly(representation, name, visiting)
            }
            None => false,
        };
        visiting.remove(&index);
        direct
    }

    /// Whether `value` stores the generic parameter `name` without passing
    /// through a variable-size container or a function.
    fn contains_directly(
        &self,
        value: &Type,
        name: &str,
        visiting: &mut BTreeSet<usize>,
    ) -> bool {
        match value {
            Type::Param(parameter) => parameter == name,
            Type::Record(members) | Type::Enum(members) => members
                .iter()
                .any(|(_, member)| self.contains_directly(member, name, visiting)),
            Type::Applied(id, arguments) => {
                let Some(index) = self.index_of(id) else {
                    return false;
                };
                arguments.iter().enumerate().any(|(position, argument)| {
                    self.contains_directly(argument, name, visiting)
                        && self.parameter_is_direct(index, position, visiting)
                })
            }
            _ => false,
        }
    }
}

/// A cycle through `start`, as the list of `(target, member)` edges taken
/// from `start` until the expansion returns to it.
fn find_cycle(
    start: usize,
    edges: &[Vec<(usize, String)>],
) -> Option<Vec<(usize, String)>> {
    let mut stack = vec![(start, Vec::<(usize, String)>::new())];
    let mut seen = BTreeSet::new();
    while let Some((node, path)) = stack.pop() {
        for (target, member) in edges.get(node).into_iter().flatten() {
            let mut next = path.clone();
            next.push((*target, member.clone()));
            if *target == start {
                return Some(next);
            }
            if seen.insert(*target) {
                stack.push((*target, next));
            }
        }
    }
    None
}

/// The span of a record field or enum variant of a declaration.
fn member_span(declaration: &DeftypeDeclaration, member: &str) -> Option<ByteSpan> {
    match declaration.body() {
        DeftypeBody::Type(TypeExpr::Record(fields) | TypeExpr::Enum(fields)) => fields
            .iter()
            .find(|field| field.name().value() == member)
            .map(vibra_syntax::TypeField::span),
        _ => None,
    }
}

/// The IR type of a primitive type name.
pub(crate) fn primitive_type(name: &str) -> Option<Type> {
    Some(match name {
        "bool" => Type::Bool,
        "char" => Type::Char,
        "str" => Type::Str,
        "bytes" => Type::Bytes,
        "atom" => Type::Atom,
        "i8" => Type::I8,
        "i16" => Type::I16,
        "i32" => Type::I32,
        "i64" => Type::I64,
        "u8" => Type::U8,
        "u16" => Type::U16,
        "u32" => Type::U32,
        "u64" => Type::U64,
        "f32" => Type::F32,
        "f64" => Type::F64,
        _ => return None,
    })
}

/// Reports a lowering failure at the owning `span`.
pub(crate) fn report_lower_error(
    diagnostics: &mut Vec<Diagnostic>,
    source_id: &str,
    span: ByteSpan,
    error: &LowerError,
) {
    match error {
        LowerError::Unavailable(message) => {
            unavailable(diagnostics, source_id, span, *message);
        }
        LowerError::Unknown(name) => diagnostics.push(
            Diagnostic::new(
                DiagnosticCode::NameUnknownSymbol,
                span,
                format!("`{name}` does not name a visible type"),
            )
            .with_source_id(source_id),
        ),
        LowerError::Private(name) => diagnostics.push(
            Diagnostic::new(
                DiagnosticCode::NamePrivateAccess,
                span,
                format!("`{name}` names a private type of another module"),
            )
            .with_source_id(source_id),
        ),
        LowerError::Arity {
            name,
            expected,
            found,
        } => diagnostics.push(
            Diagnostic::new(
                DiagnosticCode::TypeTypeArgumentMismatch,
                span,
                format!(
                    "`{name}` takes {expected} type argument{}, not {found}",
                    if *expected == 1 { "" } else { "s" }
                ),
            )
            .with_source_id(source_id),
        ),
    }
}

/// The generic names a declaration's `where:` clause introduces, in order.
pub(crate) fn generic_names(attributes: &[Attribute]) -> Vec<String> {
    attributes
        .iter()
        .filter_map(|attribute| match attribute {
            Attribute::Where(bindings) => Some(bindings),
            _ => None,
        })
        .flatten()
        .map(|binding| binding.name().value().to_owned())
        .collect()
}

/// Reports every `where:` bound other than the predeclared `any`, which
/// Stage 3A does not admit. Returns whether every bound is `any`.
pub(crate) fn report_interface_bounds(
    attributes: &[Attribute],
    source_id: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    let mut all_any = true;
    for attribute in attributes {
        let Attribute::Where(bindings) = attribute else {
            continue;
        };
        for binding in bindings {
            if binding.bound().value() != "any" {
                all_any = false;
                unavailable(
                    diagnostics,
                    source_id,
                    binding.span(),
                    "interface bounds arrive in M3 Step 11",
                );
            }
        }
    }
    all_any
}

/// The receiver type of a declaration: the declared type, applied to its own
/// generic parameters when it has any.
pub(crate) fn declared_self_type(id: &vibra_ir::TypeId, parameters: &[String]) -> Type {
    if parameters.is_empty() {
        Type::Declared(id.clone())
    } else {
        Type::Applied(
            id.clone(),
            parameters
                .iter()
                .map(|name| Type::Param(name.clone()))
                .collect(),
        )
    }
}

/// The complete generic parameter list of a function: its owner's, then its
/// own. Returns `None`, after reporting, when a bound names an interface.
pub(crate) fn function_generics(
    owner: &[String],
    function: &vibra_syntax::FunctionDeclaration,
    source_id: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Vec<String>> {
    let items = function.attributes().items();
    if !report_interface_bounds(items, source_id, diagnostics) {
        return None;
    }
    let mut generics = owner.to_vec();
    generics.extend(generic_names(items));
    Some(generics)
}

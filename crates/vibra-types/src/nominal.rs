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
    /// A map key type outside the admissible key types.
    InvalidMapKey(Type),
    /// A map key type that is or contains a function type.
    FunctionMapKey(Type),
    /// Two union members that some substitution makes equal.
    UnionOverlap(Box<(Type, Type)>),
    /// A union member that is a union, an interface, or a generic name.
    UnionNotConcrete(Type),
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
    /// Declaration imports: alias to the target module's source ID and the
    /// declaration name.
    declarations: BTreeMap<String, (String, String)>,
}

/// Every declared type of one checking run and the names each module sees.
#[derive(Clone, Debug, Default)]
pub(crate) struct TypeNames {
    declared: Vec<DeclaredType>,
    /// Scopes keyed by source ID.
    scopes: BTreeMap<String, ModuleScope>,
    /// Declared type indices keyed by the source ID of their module.
    by_module: BTreeMap<String, BTreeMap<String, usize>>,
    /// The standard-library type playing each language role, keyed by the
    /// role atom without `@`.
    roles: BTreeMap<String, usize>,
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
        // Only the embedded standard library claims a role; a project
        // declaration writing `role:` is reported when bodies lower.
        if is_stdlib_source(source_id) {
            for attribute in declaration.attributes().items() {
                if let Attribute::Role(role) = attribute {
                    self.roles.entry(role.value().to_owned()).or_insert(index);
                }
            }
        }
        index
    }

    /// The standard-library type playing `role`, such as `option`.
    pub(crate) fn role(&self, role: &str) -> Option<usize> {
        self.roles.get(role).copied()
    }

    /// A type that plays a role, named by its spelling: the role vocabulary
    /// needs no import.
    fn role_type_named(&self, name: &str) -> Option<usize> {
        self.roles.values().copied().find(|index| {
            self.declared
                .get(*index)
                .is_some_and(|declared| declared.name == name)
        })
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

    /// Makes the declared type `name` of `target_source_id` visible from
    /// `source_id` as `alias`, for a declaration import.
    pub(crate) fn import_declaration(
        &mut self,
        source_id: &str,
        alias: &str,
        target_source_id: &str,
        name: &str,
    ) {
        self.scopes
            .entry(source_id.to_owned())
            .or_default()
            .declarations
            .insert(
                alias.to_owned(),
                (target_source_id.to_owned(), name.to_owned()),
            );
    }

    /// The public declared type a declaration alias of `source_id` names.
    fn declaration_alias(&self, source_id: &str, alias: &str) -> Option<usize> {
        let (target, name) = self.scopes.get(source_id)?.declarations.get(alias)?;
        let index = self.by_module.get(target)?.get(name).copied()?;
        self.declared.get(index)?.public.then_some(index)
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
                .or_else(|| self.declaration_alias(source_id, local))
                .or_else(|| self.role_type_named(local))
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
        let scope = self.scopes.get(source_id);
        let local = |segment: &String| scope?.local.get(segment).copied();
        let imported = |alias: &String, member: &String| {
            let target = scope?.imports.get(alias)?;
            let index = self.by_module.get(target)?.get(member).copied()?;
            self.declared.get(index)?.public.then_some(index)
        };
        match segments {
            [type_name] => local(type_name)
                .or_else(|| self.declaration_alias(source_id, type_name))
                .or_else(|| self.role_type_named(type_name))
                .map(ConstructorTarget::Type),
            [first, second] => local(first)
                .or_else(|| self.declaration_alias(source_id, first))
                .map(|index| ConstructorTarget::Variant(index, second.clone()))
                .or_else(|| imported(first, second).map(ConstructorTarget::Type))
                .or_else(|| {
                    self.role_type_named(first)
                        .map(|index| ConstructorTarget::Variant(index, second.clone()))
                }),
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
                let variadic = function
                    .variadic()
                    .map(|tail| self.lower_variadic(source_id, scope, tail))
                    .transpose()?;
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
                let signature =
                    FunctionSignature::with_labelled(parameters, labelled, result);
                Ok(Type::Function(Box::new(match variadic {
                    Some(tail) => signature.with_variadic(tail),
                    None => signature,
                })))
            }
            TypeExpr::Record(fields) => Ok(Type::Record(vibra_ir::canonical_members(
                self.lower_members(source_id, scope, fields)?,
            ))),
            TypeExpr::Enum(variants) => Ok(Type::Enum(vibra_ir::canonical_members(
                self.lower_members(source_id, scope, variants)?,
            ))),
            TypeExpr::Applied { head, arguments } => {
                if head.value() == "result"
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
            TypeExpr::Tuple(components) => Ok(Type::Tuple(
                components
                    .iter()
                    .map(|component| self.lower(source_id, scope, component))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            TypeExpr::Array(element) => Ok(Type::Array(Box::new(
                self.lower(source_id, scope, element)?,
            ))),
            TypeExpr::Map(key, value) => {
                let key = self.lower(source_id, scope, key)?;
                let value = self.lower(source_id, scope, value)?;
                match map_key(&key) {
                    KeyVerdict::Admissible => {}
                    // The embedded standard library declares the generic
                    // `map` itself; a user map keyed by a generic parameter
                    // needs interface bounds.
                    KeyVerdict::Generic if is_stdlib_source(source_id) => {}
                    KeyVerdict::Generic => {
                        return Err(LowerError::Unavailable(
                            "map types keyed by a generic parameter arrive with interfaces in M3 Step 11",
                        ));
                    }
                    KeyVerdict::Function => {
                        return Err(LowerError::FunctionMapKey(key));
                    }
                    KeyVerdict::Invalid => return Err(LowerError::InvalidMapKey(key)),
                }
                Ok(Type::Map(Box::new(key), Box::new(value)))
            }
            TypeExpr::Union(members) => {
                let members = members
                    .iter()
                    .map(|member| self.lower(source_id, scope, member))
                    .collect::<Result<Vec<_>, _>>()?;
                crate::union::check_members(self, &members)?;
                Ok(Type::Union(vibra_ir::canonical_union(members)))
            }
        }
    }

    /// Lowers a variadic tail type to its `(array t)` or `(map k v)` type,
    /// applying the map-key rules.
    pub(crate) fn lower_variadic(
        &self,
        source_id: &str,
        scope: Scope<'_>,
        tail: &vibra_syntax::VariadicType,
    ) -> Result<Type, LowerError> {
        let written = match tail {
            vibra_syntax::VariadicType::Array(element) => {
                TypeExpr::Array(element.clone())
            }
            vibra_syntax::VariadicType::Map(key, value) => {
                TypeExpr::Map(key.clone(), value.clone())
            }
        };
        self.lower(source_id, scope, &written)
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
            Err(LowerError::Unknown(_)) if name.value() == "result" => {
                return Err(LowerError::Unavailable(
                    "the result library type arrives in M3 Step 7",
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
            if !is_stdlib_source(&source_id)
                && declaration
                    .attributes()
                    .items()
                    .iter()
                    .any(|attribute| matches!(attribute, Attribute::Role(_)))
            {
                unavailable(
                    diagnostics,
                    &source_id,
                    declaration.span(),
                    "role: is admissible only in the embedded standard library",
                );
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
                DeftypeBody::Type(TypeExpr::Tuple(components)) => components
                    .iter()
                    .map(|component| {
                        self.lower(
                            &source_id,
                            Scope::new(Some(&self_type), &parameters),
                            component,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map(TypeBody::Tuple),
                DeftypeBody::Type(TypeExpr::Union(members)) => members
                    .iter()
                    .map(|member| {
                        self.lower(
                            &source_id,
                            Scope::new(Some(&self_type), &parameters),
                            member,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map(TypeBody::Union),
                DeftypeBody::Intrinsic(_) => Err(LowerError::Unavailable(
                    "intrinsic-type is admissible only in the embedded standard library",
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
        self.check_union_bodies(declarations, diagnostics);
        self.check_finite_size(declarations, diagnostics);
        self.propagate_unavailability(declarations, diagnostics);
    }

    /// Checks every union member list in the lowered bodies once all bodies
    /// exist, so a member naming another declared union is seen. A failing
    /// declaration becomes unavailable.
    fn check_union_bodies(
        &mut self,
        declarations: &[(usize, &DeftypeDeclaration)],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let mut failed = Vec::new();
        for (index, declaration) in declarations {
            let Some(declared) = self.declared.get(*index) else {
                continue;
            };
            let Some(body) = declared.body.as_ref() else {
                continue;
            };
            let mut lists = Vec::new();
            if let TypeBody::Union(members) = body {
                lists.push(members.clone());
            }
            let mut pending: Vec<Type> =
                body.slots().into_iter().map(|(_, value)| value).collect();
            while let Some(value) = pending.pop() {
                if let Type::Union(members) = &value {
                    lists.push(members.clone());
                }
                pending.extend(value.components());
            }
            if let Some(error) = lists
                .iter()
                .find_map(|members| crate::union::check_members(self, members).err())
            {
                report_lower_error(
                    diagnostics,
                    &declared.source_id,
                    declaration.span(),
                    &error,
                );
                failed.push(*index);
            }
        }
        for index in failed {
            if let Some(declared) = self.declared.get_mut(index) {
                declared.available = false;
                declared.body = None;
            }
        }
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
        body.slots()
            .iter()
            .any(|(_, slot)| self.names_unavailable(slot))
    }

    fn names_unavailable(&self, value: &Type) -> bool {
        let head_unavailable = match value {
            Type::Declared(id) | Type::Applied(id, _) => self
                .index_of(id)
                .and_then(|index| self.declared.get(index))
                .is_none_or(|declared| !declared.available),
            _ => false,
        };
        head_unavailable
            || value
                .components()
                .iter()
                .any(|component| self.names_unavailable(component))
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
                for (name, slot) in declared.body.iter().flat_map(TypeBody::slots) {
                    self.direct_edges(&slot, &name, &mut edges);
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
            // Tuple components, fields, and payloads are stored inline; array
            // and map elements and function types are not.
            Type::Record(_) | Type::Enum(_) | Type::Tuple(_) => {
                for nested in value.components() {
                    self.direct_edges(&nested, member, edges);
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
        let direct = declared
            .body
            .iter()
            .flat_map(TypeBody::slots)
            .any(|(_, slot)| self.contains_directly(&slot, name, visiting));
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
            Type::Record(_) | Type::Enum(_) | Type::Tuple(_) => value
                .components()
                .iter()
                .any(|component| self.contains_directly(component, name, visiting)),
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
        LowerError::InvalidMapKey(key) => diagnostics.push(
            Diagnostic::new(
                DiagnosticCode::TypeInvalidMapKey,
                span,
                format!("{key} is not an admissible map key type"),
            )
            .with_source_id(source_id),
        ),
        LowerError::FunctionMapKey(key) => diagnostics.push(
            Diagnostic::new(
                DiagnosticCode::TypeFunctionNotEquatable,
                span,
                format!("{key} contains a function type and cannot key a map"),
            )
            .with_source_id(source_id),
        ),
        LowerError::UnionOverlap(pair) => diagnostics.push(
            Diagnostic::new(
                DiagnosticCode::TypeUnionMemberOverlap,
                span,
                format!(
                    "union members {} and {} can be the same type",
                    pair.0, pair.1
                ),
            )
            .with_source_id(source_id),
        ),
        LowerError::UnionNotConcrete(member) => diagnostics.push(
            Diagnostic::new(
                DiagnosticCode::TypeUnionMemberNotConcrete,
                span,
                format!("union member {member} is not a concrete type"),
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

/// Whether a type may key a map (`docs/spec/02-type-system.md`, "Nominal
/// declarations").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeyVerdict {
    /// A closed conformance: a key primitive or an anonymous structure of them.
    Admissible,
    /// A generic parameter, admissible only through an interface bound.
    Generic,
    /// A function type, or a structure containing one.
    Function,
    /// Any other type.
    Invalid,
}

/// Classifies `key`. A function anywhere in the key wins, then a generic
/// parameter, then any other inadmissible component.
pub(crate) fn map_key(key: &Type) -> KeyVerdict {
    match key {
        Type::Bool
        | Type::Char
        | Type::Str
        | Type::Bytes
        | Type::Atom
        | Type::I8
        | Type::I16
        | Type::I32
        | Type::I64
        | Type::U8
        | Type::U16
        | Type::U32
        | Type::U64 => KeyVerdict::Admissible,
        Type::Function(_) => KeyVerdict::Function,
        Type::Param(_) => KeyVerdict::Generic,
        Type::Tuple(_) | Type::Record(_) | Type::Enum(_) => {
            // A `void` enum payload marks a nullary variant, not a component.
            let verdicts = key
                .components()
                .iter()
                .filter(|component| {
                    !(matches!(key, Type::Enum(_)) && **component == Type::Void)
                })
                .map(map_key)
                .collect::<Vec<_>>();
            [
                KeyVerdict::Function,
                KeyVerdict::Generic,
                KeyVerdict::Invalid,
            ]
            .into_iter()
            .find(|verdict| verdicts.contains(verdict))
            .unwrap_or(KeyVerdict::Admissible)
        }
        // `void`, floats, arrays, maps, and declared types, including
        // `option`, have no closed conformance; a declared type is
        // admissible only through its own implementations (Step 11).
        _ => KeyVerdict::Invalid,
    }
}

/// Whether `source_id` names a module of the embedded standard library.
pub(crate) fn is_stdlib_source(source_id: &str) -> bool {
    source_id.starts_with("stdlib/src/")
}

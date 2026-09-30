//! Declared type identities and definitions.
//!
//! `docs/spec/02-type-system.md` makes every `deftype` a new identity. The
//! checked program carries one [`TypeDefinition`] per declared type it uses, so
//! the interpreter and renderers never consult source or the resolver.

use std::collections::BTreeMap;

use crate::{Expr, FunctionSignature, IrError, Type};

/// The identity of one declared type.
///
/// `id` is the resolver's canonical declaration identity and decides equality;
/// `path` is the canonical atom path, without its leading `@`, that the
/// canonical value encoding renders.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeId {
    id: String,
    path: String,
}

impl TypeId {
    /// Creates a declared type identity.
    #[must_use]
    pub fn new(id: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            path: path.into(),
        }
    }

    /// The canonical declaration identity.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The canonical atom path without its leading `@`.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }
}

/// The body of a declared type, in declaration order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeBody {
    /// Closed, named fields.
    Record(Vec<(String, Type)>),
    /// Variants, each with one payload slot; `void` marks a nullary variant.
    Enum(Vec<(String, Type)>),
    /// A distinct identity over one representation type.
    Wrapper(Type),
    /// Positional components.
    Tuple(Vec<Type>),
    /// Member types, in declaration order, which fixes discriminants.
    Union(Vec<Type>),
}

impl TypeBody {
    /// Every slot of the body with its name: a field, a variant, a tuple
    /// component spelled by its index, or the unnamed representation.
    #[must_use]
    pub fn slots(&self) -> Vec<(String, Type)> {
        match self {
            Self::Record(members) | Self::Enum(members) => members.clone(),
            Self::Wrapper(representation) => {
                vec![(String::new(), representation.clone())]
            }
            Self::Tuple(components) | Self::Union(components) => components
                .iter()
                .enumerate()
                .map(|(index, value)| (index.to_string(), value.clone()))
                .collect(),
        }
    }
}

/// One declared type of a checked program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeDefinition {
    id: TypeId,
    parameters: Vec<String>,
    body: TypeBody,
}

impl TypeDefinition {
    /// Creates a declared type definition with no generic parameters.
    #[must_use]
    pub const fn new(id: TypeId, body: TypeBody) -> Self {
        Self {
            id,
            parameters: Vec::new(),
            body,
        }
    }

    /// The same definition with generic parameters, in `where:` order. The
    /// body names them as [`Type::Param`].
    #[must_use]
    pub fn with_parameters(mut self, parameters: Vec<String>) -> Self {
        self.parameters = parameters;
        self
    }

    /// The generic parameter names in `where:` order.
    #[must_use]
    pub fn parameters(&self) -> &[String] {
        &self.parameters
    }

    /// The body with its generic parameters replaced by `arguments`, or `None`
    /// when the argument count differs from the parameter list.
    #[must_use]
    pub fn instantiate(&self, arguments: &[Type]) -> Option<TypeBody> {
        if arguments.len() != self.parameters.len() {
            return None;
        }
        let map: std::collections::BTreeMap<String, Type> = self
            .parameters
            .iter()
            .cloned()
            .zip(arguments.iter().cloned())
            .collect();
        let substitute = |members: &[(String, Type)]| {
            members
                .iter()
                .map(|(name, value)| (name.clone(), value.substitute(&map)))
                .collect()
        };
        Some(match &self.body {
            TypeBody::Record(fields) => TypeBody::Record(substitute(fields)),
            TypeBody::Enum(variants) => TypeBody::Enum(substitute(variants)),
            TypeBody::Wrapper(representation) => {
                TypeBody::Wrapper(representation.substitute(&map))
            }
            TypeBody::Tuple(components) => TypeBody::Tuple(
                components
                    .iter()
                    .map(|value| value.substitute(&map))
                    .collect(),
            ),
            TypeBody::Union(members) => TypeBody::Union(
                members.iter().map(|value| value.substitute(&map)).collect(),
            ),
        })
    }

    /// The declared identity.
    #[must_use]
    pub const fn id(&self) -> &TypeId {
        &self.id
    }

    /// The declared body.
    #[must_use]
    pub const fn body(&self) -> &TypeBody {
        &self.body
    }

    /// The fields of a record body, in declaration order.
    #[must_use]
    pub fn record_fields(&self) -> Option<&[(String, Type)]> {
        match &self.body {
            TypeBody::Record(fields) => Some(fields),
            TypeBody::Enum(_)
            | TypeBody::Wrapper(_)
            | TypeBody::Tuple(_)
            | TypeBody::Union(_) => None,
        }
    }

    /// The variants of an enum body, in declaration order.
    #[must_use]
    pub fn enum_variants(&self) -> Option<&[(String, Type)]> {
        match &self.body {
            TypeBody::Enum(variants) => Some(variants),
            TypeBody::Record(_)
            | TypeBody::Wrapper(_)
            | TypeBody::Tuple(_)
            | TypeBody::Union(_) => None,
        }
    }
}

/// Orders anonymous record fields or enum variants canonically: by the UTF-8
/// bytes of their names. Anonymous structural identity ignores written order.
#[must_use]
pub fn canonical_members(mut members: Vec<(String, Type)>) -> Vec<(String, Type)> {
    members.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    members
}

/// Declared definitions keyed by identity.
pub(crate) type TypeTable<'a> = BTreeMap<&'a TypeId, &'a TypeDefinition>;

/// Indexes definitions, rejecting a repeated identity and any definition that
/// names an undefined declared type.
pub(crate) fn type_table(types: &[TypeDefinition]) -> Result<TypeTable<'_>, IrError> {
    let mut table = BTreeMap::new();
    for definition in types {
        if table.insert(definition.id(), definition).is_some() {
            return Err(invalid(format!(
                "declared type `{}` is defined twice",
                definition.id().id()
            )));
        }
    }
    for definition in types {
        for (_, slot) in definition.body().slots() {
            validate_declared_type(&slot, &table)?;
        }
    }
    Ok(table)
}

/// Rejects a declared type with no definition, and a declared type applied to
/// the wrong number of arguments.
pub(crate) fn validate_declared_type(
    value: &Type,
    table: &TypeTable<'_>,
) -> Result<(), IrError> {
    match value {
        Type::Declared(_) | Type::Applied(_, _) => {
            declared_body(table, value)?;
            if let Type::Applied(_, arguments) = value {
                for argument in arguments {
                    validate_declared_type(argument, table)?;
                }
            }
            Ok(())
        }
        _ => value
            .components()
            .iter()
            .try_for_each(|component| validate_declared_type(component, table)),
    }
}

/// Validates every type a signature names.
pub(crate) fn validate_declared_signature(
    signature: &FunctionSignature,
    table: &TypeTable<'_>,
) -> Result<(), IrError> {
    for parameter in signature.parameters() {
        validate_declared_type(parameter, table)?;
    }
    for parameter in signature.labelled() {
        validate_declared_type(&parameter.value_type(), table)?;
    }
    validate_declared_type(&signature.result(), table)
}

/// The instantiated body of a declared or applied type.
pub(crate) fn declared_body(
    table: &TypeTable<'_>,
    value: &Type,
) -> Result<TypeBody, IrError> {
    let (id, arguments): (&TypeId, &[Type]) = match value {
        Type::Declared(id) => (id, &[]),
        Type::Applied(id, arguments) => (id, arguments),
        _ => return Err(invalid(format!("{value} is not a declared type"))),
    };
    let definition = table.get(id).copied().ok_or_else(|| {
        invalid(format!("declared type `{}` has no definition", id.id()))
    })?;
    definition.instantiate(arguments).ok_or_else(|| {
        invalid(format!(
            "`{}` takes {} type arguments, not {}",
            id.id(),
            definition.parameters().len(),
            arguments.len()
        ))
    })
}

/// Checks every declared construction and projection in `expression` against
/// its instantiated definition. Shape validation has already typed each
/// operand; generic parameters are compared as written.
pub(crate) fn validate_declared_expr(
    expression: &Expr,
    table: &TypeTable<'_>,
) -> Result<(), IrError> {
    let mut pending = vec![expression];
    while let Some(expression) = pending.pop() {
        validate_declared_type(&expression.result_type(), table)?;
        match expression {
            Expr::Record {
                value_type: value_type @ (Type::Declared(_) | Type::Applied(_, _)),
                fields,
                ..
            } => {
                let TypeBody::Record(declared) = declared_body(table, value_type)?
                else {
                    return Err(invalid(format!("{value_type} is not a record")));
                };
                if declared.len() != fields.len() {
                    return Err(invalid(format!(
                        "{value_type} construction has the wrong fields"
                    )));
                }
                for (name, value) in fields {
                    let expected = member(&declared, name).ok_or_else(|| {
                        invalid(format!("{value_type} has no field `{name}`"))
                    })?;
                    if !expected.same_shape(&value.result_type()) {
                        return Err(invalid(format!(
                            "{value_type} field `{name}` has type {}, expected {expected}",
                            value.result_type()
                        )));
                    }
                }
            }
            Expr::Variant {
                value_type: value_type @ (Type::Declared(_) | Type::Applied(_, _)),
                variant,
                payload,
                ..
            } => {
                let TypeBody::Enum(declared) = declared_body(table, value_type)? else {
                    return Err(invalid(format!("{value_type} is not an enum")));
                };
                let expected = member(&declared, variant).ok_or_else(|| {
                    invalid(format!("{value_type} has no variant `{variant}`"))
                })?;
                let actual = payload.as_deref().map_or(Type::Void, Expr::result_type);
                if !expected.same_shape(&actual) {
                    return Err(invalid(format!(
                        "{value_type} variant `{variant}` payload has type {actual}, expected {expected}"
                    )));
                }
            }
            Expr::Wrap {
                value_type: value_type @ (Type::Declared(_) | Type::Applied(_, _)),
                value,
                ..
            } => {
                let TypeBody::Wrapper(expected) = declared_body(table, value_type)?
                else {
                    return Err(invalid(format!("{value_type} is not a wrapper type")));
                };
                if !expected.same_shape(&value.result_type()) {
                    return Err(invalid(format!(
                        "{value_type} wraps {expected}, not {}",
                        value.result_type()
                    )));
                }
            }
            Expr::Widen {
                value_type: value_type @ (Type::Declared(_) | Type::Applied(_, _)),
                value,
                member,
                ..
            } => {
                let TypeBody::Union(members) = declared_body(table, value_type)? else {
                    return Err(invalid(format!("{value_type} is not a union")));
                };
                let actual = value.result_type();
                if !member
                    .and_then(|index| members.get(index))
                    .is_some_and(|found| found.admits(&actual))
                {
                    return Err(invalid(format!(
                        "{actual} is not a member of {value_type}"
                    )));
                }
            }
            Expr::Tuple {
                value_type: value_type @ (Type::Declared(_) | Type::Applied(_, _)),
                components,
                ..
            } => {
                let TypeBody::Tuple(expected) = declared_body(table, value_type)?
                else {
                    return Err(invalid(format!("{value_type} is not a tuple")));
                };
                if expected.len() != components.len()
                    || !expected
                        .iter()
                        .zip(components)
                        .all(|(expected, component)| {
                            expected.admits(&component.result_type())
                        })
                {
                    return Err(invalid(format!(
                        "{value_type} construction has the wrong components"
                    )));
                }
            }
            Expr::TupleProject {
                tuple,
                index,
                value_type,
                ..
            } => {
                let tuple_type = tuple.result_type();
                if matches!(tuple_type, Type::Declared(_) | Type::Applied(_, _)) {
                    let TypeBody::Tuple(components) =
                        declared_body(table, &tuple_type)?
                    else {
                        return Err(invalid(format!(
                            "projection from non-tuple {tuple_type}"
                        )));
                    };
                    if !components
                        .get(*index)
                        .is_some_and(|found| found.admits(value_type))
                    {
                        return Err(invalid(format!(
                            "{tuple_type} has no component {index} of type {value_type}"
                        )));
                    }
                }
            }
            Expr::Project {
                record,
                field,
                value_type,
                ..
            } => {
                let record_type = record.result_type();
                if matches!(record_type, Type::Declared(_) | Type::Applied(_, _)) {
                    let TypeBody::Record(declared) =
                        declared_body(table, &record_type)?
                    else {
                        return Err(invalid(format!(
                            "projection from non-record {record_type}"
                        )));
                    };
                    if !member(&declared, field)
                        .is_some_and(|found| found.same_shape(value_type))
                    {
                        return Err(invalid(format!(
                            "{record_type} has no field `{field}` of type {value_type}"
                        )));
                    }
                }
            }
            _ => {}
        }
        pending.extend(children(expression));
    }
    Ok(())
}

fn member<'a>(members: &'a [(String, Type)], name: &str) -> Option<&'a Type> {
    members
        .iter()
        .find(|(member, _)| member == name)
        .map(|(_, value)| value)
}

fn invalid(message: String) -> IrError {
    IrError::InvalidExpression(message)
}

/// Every direct subexpression, including closure bodies and callees.
pub(crate) fn children(expression: &Expr) -> Vec<&Expr> {
    match expression {
        Expr::External { arguments, .. } => arguments.iter().collect(),
        Expr::Sequence { expressions, .. } => expressions.iter().collect(),
        Expr::Closure { captures, body, .. } => {
            let mut children: Vec<&Expr> = captures.iter().collect();
            children.push(body);
            children
        }
        Expr::Let { value, body, .. } => vec![value, body],
        Expr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => vec![condition, then_branch, else_branch],
        Expr::Call {
            target, arguments, ..
        } => {
            let mut children: Vec<&Expr> = target.callee().into_iter().collect();
            children.extend(arguments);
            children
        }
        _ => expression.data_operands(),
    }
}

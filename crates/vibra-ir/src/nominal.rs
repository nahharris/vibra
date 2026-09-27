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
    Newtype(Type),
}

/// One declared type of a checked program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeDefinition {
    id: TypeId,
    body: TypeBody,
}

impl TypeDefinition {
    /// Creates a declared type definition.
    #[must_use]
    pub const fn new(id: TypeId, body: TypeBody) -> Self {
        Self { id, body }
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
            TypeBody::Enum(_) | TypeBody::Newtype(_) => None,
        }
    }

    /// The variants of an enum body, in declaration order.
    #[must_use]
    pub fn enum_variants(&self) -> Option<&[(String, Type)]> {
        match &self.body {
            TypeBody::Enum(variants) => Some(variants),
            TypeBody::Record(_) | TypeBody::Newtype(_) => None,
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
        match definition.body() {
            TypeBody::Record(members) | TypeBody::Enum(members) => {
                for (_, member) in members {
                    validate_declared_type(member, &table)?;
                }
            }
            TypeBody::Newtype(representation) => {
                validate_declared_type(representation, &table)?;
            }
        }
    }
    Ok(table)
}

/// Rejects a declared type reference with no definition.
pub(crate) fn validate_declared_type(
    value: &Type,
    table: &TypeTable<'_>,
) -> Result<(), IrError> {
    match value {
        Type::Declared(id) if !table.contains_key(id) => Err(invalid(format!(
            "declared type `{}` has no definition",
            id.id()
        ))),
        Type::Record(members) | Type::Enum(members) => members
            .iter()
            .try_for_each(|(_, member)| validate_declared_type(member, table)),
        Type::Function(signature) => validate_declared_signature(signature, table),
        _ => Ok(()),
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

/// Checks every declared construction and projection in `expression` against
/// its definition. Shape validation has already typed each operand.
pub(crate) fn validate_declared_expr(
    expression: &Expr,
    table: &TypeTable<'_>,
) -> Result<(), IrError> {
    let mut pending = vec![expression];
    while let Some(expression) = pending.pop() {
        validate_declared_type(&expression.result_type(), table)?;
        match expression {
            Expr::Record {
                value_type: Type::Declared(id),
                fields,
                ..
            } => {
                let declared = definition(table, id)?
                    .record_fields()
                    .ok_or_else(|| invalid(format!("`{}` is not a record", id.id())))?;
                if declared.len() != fields.len() {
                    return Err(invalid(format!(
                        "`{}` construction has the wrong fields",
                        id.id()
                    )));
                }
                for (name, value) in fields {
                    let expected = member(declared, name).ok_or_else(|| {
                        invalid(format!("`{}` has no field `{name}`", id.id()))
                    })?;
                    if !expected.same_shape(&value.result_type()) {
                        return Err(invalid(format!(
                            "`{}` field `{name}` has type {}, expected {expected}",
                            id.id(),
                            value.result_type()
                        )));
                    }
                }
            }
            Expr::Variant {
                value_type: Type::Declared(id),
                variant,
                payload,
                ..
            } => {
                let declared = definition(table, id)?
                    .enum_variants()
                    .ok_or_else(|| invalid(format!("`{}` is not an enum", id.id())))?;
                let expected = member(declared, variant).ok_or_else(|| {
                    invalid(format!("`{}` has no variant `{variant}`", id.id()))
                })?;
                let actual = payload.as_deref().map_or(Type::Void, Expr::result_type);
                if !expected.same_shape(&actual) {
                    return Err(invalid(format!(
                        "`{}` variant `{variant}` payload has type {actual}, expected {expected}",
                        id.id()
                    )));
                }
            }
            Expr::Newtype {
                value_type, value, ..
            } => {
                let TypeBody::Newtype(expected) = definition(table, value_type)?.body()
                else {
                    return Err(invalid(format!(
                        "`{}` is not a newtype",
                        value_type.id()
                    )));
                };
                if !expected.same_shape(&value.result_type()) {
                    return Err(invalid(format!(
                        "`{}` wraps {expected}, not {}",
                        value_type.id(),
                        value.result_type()
                    )));
                }
            }
            Expr::Project {
                record,
                field,
                value_type,
                ..
            } => {
                if let Type::Declared(id) = record.result_type() {
                    let declared =
                        definition(table, &id)?.record_fields().ok_or_else(|| {
                            invalid(format!("projection from non-record `{}`", id.id()))
                        })?;
                    if !member(declared, field)
                        .is_some_and(|found| found.same_shape(value_type))
                    {
                        return Err(invalid(format!(
                            "`{}` has no field `{field}` of type {value_type}",
                            id.id()
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

fn definition<'a>(
    table: &TypeTable<'a>,
    id: &TypeId,
) -> Result<&'a TypeDefinition, IrError> {
    table.get(id).copied().ok_or_else(|| {
        invalid(format!("declared type `{}` has no definition", id.id()))
    })
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
fn children(expression: &Expr) -> Vec<&Expr> {
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

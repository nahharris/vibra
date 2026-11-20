//! Declared types, interfaces, and implementations
//! (`docs/spec/02-type-system.md`, "Declared types" and "Interfaces").
//!
//! A builtin declared `of Intrinsic(@tag)` has no definition here: its type
//! is a scalar case or the `List` case of [`Type`].

use std::collections::BTreeSet;

use crate::types::is_identifier;
use crate::{
    Binder, Constant, ImplementationRecord, InterfaceKey, IrError, NamedTypes,
    OpenScope, SignatureRecord, Type, TypeId,
};

/// The visibility of a type's representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Visibility {
    /// Exposed wherever the type is visible.
    Public,
    /// Hidden from every other module.
    Private,
}

/// A case of a variant type: its name and its named fields, in declaration
/// order, none for a case without fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Case {
    /// The case name.
    pub name: String,
    /// The fields, in declaration order.
    pub fields: Vec<(String, Type)>,
}

impl Case {
    /// The case `name` with `fields`.
    #[must_use]
    pub fn new(name: impl Into<String>, fields: Vec<(String, Type)>) -> Self {
        Self {
            name: name.into(),
            fields,
        }
    }
}

/// The body of a declared type. Declaration order fixes discriminants,
/// rendering, and comparison order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeBody {
    /// `of {name: T …}`: a declared record, fields in declaration order.
    Record(Vec<(String, Type)>),
    /// `of {T U}`: a declared tuple.
    Tuple(Vec<Type>),
    /// `of T` for any other `T`: a wrapper.
    Wrapper(Type),
    /// A `case` list: a variant type.
    Variant(Vec<Case>),
}

/// A type invariant: `where P`, or `where P else E`.
///
/// The predicate is the function at this index of the module set, of type
/// `Fn({R} Bool)` where `R` is the representation type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invariant {
    /// The predicate function's index in the module set.
    pub predicate: usize,
    /// The constant consequence `E`, `None` for a bare `where`.
    pub consequence: Option<Constant>,
}

/// One declared type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeDefinition {
    id: TypeId,
    parameters: Vec<Binder>,
    body: TypeBody,
    invariant: Option<Invariant>,
    representation: Visibility,
}

fn check_distinct<'a>(
    what: &'static str,
    names: impl IntoIterator<Item = &'a str>,
) -> Result<(), IrError> {
    let mut seen = BTreeSet::new();
    for name in names {
        if !is_identifier(name) {
            return Err(IrError::InvalidName {
                what,
                name: name.to_owned(),
            });
        }
        if !seen.insert(name) {
            return Err(IrError::Duplicate {
                what,
                name: name.to_owned(),
            });
        }
    }
    Ok(())
}

impl TypeDefinition {
    /// The definition of the type `id` with the type parameters `parameters`,
    /// each with its bound, whose body refers to them by binder position.
    ///
    /// # Errors
    ///
    /// A parameter, field, or case name is not an identifier or is repeated;
    /// a type in a bound or in the body refers to a binder out of range or
    /// holds `Self` or an association slot; a record or variant body is
    /// empty; or a variant type has an invariant.
    pub fn new(
        id: TypeId,
        parameters: Vec<Binder>,
        body: TypeBody,
        invariant: Option<Invariant>,
        representation: Visibility,
    ) -> Result<Self, IrError> {
        check_distinct("type parameter", parameters.iter().map(Binder::name))?;
        let scope = OpenScope::Binders(parameters.len());
        for bound in parameters.iter().filter_map(Binder::bound) {
            bound
                .contract
                .arguments
                .iter()
                .chain(bound.choices.entries().iter().map(|(_, ty)| ty))
                .try_for_each(|ty| ty.validate(scope))?;
        }
        match &body {
            TypeBody::Record(fields) => {
                if fields.is_empty() {
                    return Err(IrError::InvalidType(format!(
                        "the record `{}` has no field",
                        id.path()
                    )));
                }
                check_distinct("field", fields.iter().map(|(name, _)| name.as_str()))?;
                fields
                    .iter()
                    .try_for_each(|(_, value)| value.validate(scope))?;
            }
            TypeBody::Tuple(values) => {
                values.iter().try_for_each(|value| value.validate(scope))?
            }
            TypeBody::Wrapper(value) => value.validate(scope)?,
            TypeBody::Variant(cases) => {
                if cases.is_empty() {
                    return Err(IrError::InvalidType(format!(
                        "the variant type `{}` has no case",
                        id.path()
                    )));
                }
                if invariant.is_some() {
                    return Err(IrError::InvalidType(format!(
                        "the variant type `{}` has an invariant",
                        id.path()
                    )));
                }
                check_distinct("case", cases.iter().map(|case| case.name.as_str()))?;
                for case in cases {
                    check_distinct(
                        "field",
                        case.fields.iter().map(|(name, _)| name.as_str()),
                    )?;
                    case.fields
                        .iter()
                        .try_for_each(|(_, value)| value.validate(scope))?;
                }
            }
        }
        Ok(Self {
            id,
            parameters,
            body,
            invariant,
            representation,
        })
    }

    /// The type's identity.
    #[must_use]
    pub const fn id(&self) -> &TypeId {
        &self.id
    }

    /// The type parameters with their bounds, in written order. Every
    /// application of the type MUST satisfy them.
    #[must_use]
    pub fn parameters(&self) -> &[Binder] {
        &self.parameters
    }

    /// The body, over binder references to the parameters.
    #[must_use]
    pub const fn body(&self) -> &TypeBody {
        &self.body
    }

    /// The invariant, when the type has one.
    #[must_use]
    pub const fn invariant(&self) -> Option<&Invariant> {
        self.invariant.as_ref()
    }

    /// The representation's visibility.
    #[must_use]
    pub const fn representation(&self) -> Visibility {
        self.representation
    }

    /// The representation type `R` an invariant's predicate takes, open in
    /// the parameters; `None` for a variant type.
    #[must_use]
    pub fn representation_type(&self) -> Option<Type> {
        match &self.body {
            TypeBody::Record(fields) => NamedTypes::new(fields.iter().cloned())
                .ok()
                .map(Type::Record),
            TypeBody::Tuple(values) => Some(Type::Tuple(values.clone())),
            TypeBody::Wrapper(value) => Some(value.clone()),
            TypeBody::Variant(_) => None,
        }
    }

    /// The case named `name` of a variant type.
    #[must_use]
    pub fn case(&self, name: &str) -> Option<&Case> {
        match &self.body {
            TypeBody::Variant(cases) => cases.iter().find(|case| case.name == name),
            _ => None,
        }
    }

    /// The type this definition declares, applied to its own parameters.
    #[must_use]
    pub fn self_type(&self) -> Type {
        Type::Declared(
            self.id.clone(),
            (0..self.parameters.len()).map(Type::Binder).collect(),
        )
    }
}

/// A member of an interface: a callable with its signature record, and the
/// function holding its default body when it has one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterfaceMember {
    /// The member name.
    pub name: String,
    /// The member's signature record, a member of the interface.
    pub signature: SignatureRecord,
    /// The index of the function holding the default body, `None` for an
    /// abstract member.
    pub default: Option<usize>,
}

/// One interface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterfaceDefinition {
    id: TypeId,
    parameters: Vec<String>,
    associated: Vec<String>,
    members: Vec<InterfaceMember>,
}

impl InterfaceDefinition {
    /// The interface `id` with its parameters, its associated types, and its
    /// callable members.
    ///
    /// # Errors
    ///
    /// A name is not an identifier; the associated types and members share
    /// one namespace and a name repeats in it; a member's record is not a
    /// member of this interface; or a member's binders do not start with the
    /// interface's parameters.
    pub fn new(
        id: TypeId,
        parameters: Vec<String>,
        associated: Vec<String>,
        members: Vec<InterfaceMember>,
    ) -> Result<Self, IrError> {
        check_distinct("type parameter", parameters.iter().map(String::as_str))?;
        check_distinct(
            "interface member",
            associated
                .iter()
                .map(String::as_str)
                .chain(members.iter().map(|member| member.name.as_str())),
        )?;
        for member in &members {
            if member.signature.associated() != Some(associated.as_slice()) {
                return Err(IrError::InvalidType(format!(
                    "the member `{}` of `{}` is not recorded as one of its members",
                    member.name,
                    id.path()
                )));
            }
            let inherited = member
                .signature
                .binders()
                .iter()
                .map(Binder::name)
                .take(parameters.len());
            if !inherited.eq(parameters.iter().map(String::as_str)) {
                return Err(IrError::InvalidType(format!(
                    "the binders of `{}.{}` do not start with the interface's parameters",
                    id.path(),
                    member.name
                )));
            }
        }
        Ok(Self {
            id,
            parameters,
            associated,
            members,
        })
    }

    /// The interface's identity.
    #[must_use]
    pub const fn id(&self) -> &TypeId {
        &self.id
    }

    /// The type parameters, in written order.
    #[must_use]
    pub fn parameters(&self) -> &[String] {
        &self.parameters
    }

    /// The associated type names, in declaration order.
    #[must_use]
    pub fn associated(&self) -> &[String] {
        &self.associated
    }

    /// The callable members, in declaration order.
    #[must_use]
    pub fn members(&self) -> &[InterfaceMember] {
        &self.members
    }

    /// The member named `name`.
    #[must_use]
    pub fn member(&self, name: &str) -> Option<&InterfaceMember> {
        self.members.iter().find(|member| member.name == name)
    }
}

/// One implementation: its record and the function supplying each member.
///
/// The key is the interface, the receiver, and the positional arguments.
/// Associated types are not part of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Implementation {
    record: ImplementationRecord,
    members: Vec<(String, usize)>,
}

impl Implementation {
    /// The implementation `record`, whose members are the functions at the
    /// given indices of the module set.
    ///
    /// # Errors
    ///
    /// A member is supplied twice.
    pub fn new(
        record: ImplementationRecord,
        members: Vec<(String, usize)>,
    ) -> Result<Self, IrError> {
        check_distinct(
            "implementation member",
            members.iter().map(|(name, _)| name.as_str()),
        )?;
        Ok(Self { record, members })
    }

    /// The implementation record.
    #[must_use]
    pub const fn record(&self) -> &ImplementationRecord {
        &self.record
    }

    /// The binders the key is open in.
    #[must_use]
    pub fn binders(&self) -> &[Binder] {
        self.record.binders()
    }

    /// The interface key.
    #[must_use]
    pub const fn contract(&self) -> &InterfaceKey {
        self.record.contract()
    }

    /// The receiver type.
    #[must_use]
    pub const fn receiver(&self) -> &Type {
        self.record.receiver()
    }

    /// The choices, by associated type name.
    #[must_use]
    pub const fn choices(&self) -> &NamedTypes {
        self.record.choices()
    }

    /// The supplied members and the index of each one's function.
    #[must_use]
    pub fn members(&self) -> &[(String, usize)] {
        &self.members
    }

    /// The function supplying the member `name`, when this implementation
    /// supplies it.
    #[must_use]
    pub fn member(&self, name: &str) -> Option<usize> {
        self.members
            .iter()
            .find_map(|(member, function)| (member == name).then_some(*function))
    }
}

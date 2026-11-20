//! Open signatures (`docs/spec/06-runtime.md`, "Open signatures"): the
//! records that describe a generic declaration, an interface member, and an
//! implementation before any activation exists.

use crate::types::is_identifier;
use crate::{IrError, NamedTypes, OpenScope, Type, TypeId};

/// An interface named at its positional arguments and nothing else: the key
/// of a bound or of an implementation. It has no choices, so it is never
/// mistaken for an interface value type.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct InterfaceKey {
    /// The interface.
    pub interface: TypeId,
    /// The positional arguments, in parameter order.
    pub arguments: Vec<Type>,
}

impl InterfaceKey {
    /// The interface `interface` at `arguments`.
    #[must_use]
    pub const fn new(interface: TypeId, arguments: Vec<Type>) -> Self {
        Self {
            interface,
            arguments,
        }
    }

    pub(crate) fn validate(&self, scope: OpenScope<'_>) -> Result<(), IrError> {
        self.arguments
            .iter()
            .try_for_each(|argument| argument.validate(scope))
    }
}

/// A bound: the interface key of the bound and the associated types its
/// labels fix.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Bound {
    /// The interface key.
    pub contract: InterfaceKey,
    /// The labels the bound fixes, by associated type name.
    pub choices: NamedTypes,
}

/// A type parameter in scope: its name and its bound, absent for `Any`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Binder {
    name: String,
    bound: Option<Bound>,
}

impl Binder {
    /// The parameter `name` with the bound `bound`, `None` for `Any`.
    ///
    /// # Errors
    ///
    /// The name is not an identifier.
    pub fn new(name: impl Into<String>, bound: Option<Bound>) -> Result<Self, IrError> {
        let name = name.into();
        if !is_identifier(&name) {
            return Err(IrError::InvalidName {
                what: "type parameter",
                name,
            });
        }
        Ok(Self { name, bound })
    }

    /// The parameter's name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The bound, `None` for `Any`.
    #[must_use]
    pub const fn bound(&self) -> Option<&Bound> {
        self.bound.as_ref()
    }

    fn validate(&self, scope: OpenScope<'_>) -> Result<(), IrError> {
        if let Some(bound) = &self.bound {
            bound.contract.validate(scope)?;
            for (_, choice) in bound.choices.entries() {
                choice.validate(scope)?;
            }
        }
        Ok(())
    }
}

fn validate_binders(binders: &[Binder], scope: OpenScope<'_>) -> Result<(), IrError> {
    binders.iter().try_for_each(|binder| binder.validate(scope))
}

/// A signature record: the binders in scope, the receiver slot of a
/// contract member, and an open type.
///
/// The binders are those of each enclosing declaration, outermost first,
/// then the declaration's own. A record of a member of an interface also
/// knows that interface's associated type names, where `Self` and the
/// association slots are valid; that knowledge is not part of the encoding.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SignatureRecord {
    binders: Vec<Binder>,
    receiver: Option<u64>,
    type_info: Type,
    associated: Option<Vec<String>>,
}

impl SignatureRecord {
    /// The signature record of a declaration that is not a member of an
    /// interface: a function, a procedure, a type's representation, or a
    /// module value's type. It holds binder references only.
    ///
    /// # Errors
    ///
    /// A binder reference is out of range, or `Self` or an association slot
    /// occurs.
    pub fn new(binders: Vec<Binder>, type_info: Type) -> Result<Self, IrError> {
        let record = Self {
            binders,
            receiver: None,
            type_info,
            associated: None,
        };
        record.validate()?;
        Ok(record)
    }

    /// The signature record of a member of an interface that declares the
    /// associated types `associated`, with its receiver slot when it has one.
    ///
    /// # Errors
    ///
    /// A binder reference is out of range; an association slot names no
    /// associated type of the interface; the type is not a callable type; or
    /// the receiver is not the first positional parameter written `Self`.
    pub fn interface_member(
        binders: Vec<Binder>,
        receiver: Option<u64>,
        type_info: Type,
        associated: Vec<String>,
    ) -> Result<Self, IrError> {
        let record = Self {
            binders,
            receiver,
            type_info,
            associated: Some(associated),
        };
        record.validate()?;
        let parameters = match &record.type_info {
            Type::Fn(signature) => &signature.parameters,
            Type::Proc(signature) => &signature.parameters,
            other => {
                return Err(IrError::InvalidType(format!(
                    "an interface member has a callable type, not {other}"
                )));
            }
        };
        let first_self = parameters
            .iter()
            .position(|parameter| *parameter == Type::SelfType);
        let receiver = record
            .receiver
            .map(|receiver| usize::try_from(receiver).unwrap_or(usize::MAX));
        if receiver != first_self {
            return Err(IrError::InvalidType(format!(
                "the receiver slot is {receiver:?}, but the first positional `Self` is {first_self:?}"
            )));
        }
        Ok(record)
    }

    fn validate(&self) -> Result<(), IrError> {
        let scope = self.scope();
        validate_binders(&self.binders, scope)?;
        self.type_info.validate(scope)
    }

    /// Where open forms are valid inside this record.
    #[must_use]
    pub fn scope(&self) -> OpenScope<'_> {
        match &self.associated {
            Some(associated) => OpenScope::InterfaceMember {
                binders: self.binders.len(),
                associated,
            },
            None => OpenScope::Binders(self.binders.len()),
        }
    }

    /// The binders in scope.
    #[must_use]
    pub fn binders(&self) -> &[Binder] {
        &self.binders
    }

    /// The zero-based receiver slot of a contract member, when it has one.
    #[must_use]
    pub const fn receiver(&self) -> Option<u64> {
        self.receiver
    }

    /// The open type.
    #[must_use]
    pub const fn type_info(&self) -> &Type {
        &self.type_info
    }

    /// The associated type names of the interface this record is a member
    /// of, `None` when it is no interface member.
    #[must_use]
    pub fn associated(&self) -> Option<&[String]> {
        self.associated.as_deref()
    }
}

/// An implementation record: the binders its key is open in, its interface
/// key, its receiver, and one choice for every associated type.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ImplementationRecord {
    binders: Vec<Binder>,
    contract: InterfaceKey,
    receiver: Type,
    choices: NamedTypes,
}

impl ImplementationRecord {
    /// The record of an implementation of `contract` for `receiver`.
    ///
    /// # Errors
    ///
    /// A binder reference is out of range, or `Self` or an association slot
    /// occurs: an implementation's types are written under its substitution.
    pub fn new(
        binders: Vec<Binder>,
        contract: InterfaceKey,
        receiver: Type,
        choices: NamedTypes,
    ) -> Result<Self, IrError> {
        let record = Self {
            binders,
            contract,
            receiver,
            choices,
        };
        let scope = record.scope();
        validate_binders(&record.binders, scope)?;
        record.contract.validate(scope)?;
        record.receiver.validate(scope)?;
        for (_, choice) in record.choices.entries() {
            choice.validate(scope)?;
        }
        let mut fixed = vec![false; record.binders.len()];
        mark(&record.receiver, &mut fixed);
        for argument in &record.contract.arguments {
            mark(argument, &mut fixed);
        }
        let mut changed = true;
        while changed {
            changed = false;
            for (at, binder) in record.binders.iter().enumerate() {
                let Some(bound) = binder.bound() else {
                    continue;
                };
                if !fixed.get(at).copied().unwrap_or(false)
                    || !bound
                        .contract
                        .arguments
                        .iter()
                        .all(|argument| all_marked(argument, &fixed))
                {
                    continue;
                }
                for (_, choice) in bound.choices.entries() {
                    if !all_marked(choice, &fixed) {
                        mark(choice, &mut fixed);
                        changed = true;
                    }
                }
            }
        }
        if let Some(at) = fixed.iter().position(|fixed| !fixed) {
            return Err(IrError::InvalidType(format!(
                "the binder {at} of an implementation is fixed neither by its key nor by a label of the bound of a binder that is fixed"
            )));
        }
        Ok(record)
    }

    /// The first binder whose bound can fix another binder `assignment`
    /// leaves open: the binder is fixed, its bound's positional arguments
    /// mention only fixed binders, and a label of the bound mentions one
    /// that is not.
    #[must_use]
    pub fn open_bound(&self, assignment: &[Option<Type>]) -> Option<usize> {
        let fixed = assignment.iter().map(Option::is_some).collect::<Vec<_>>();
        self.binders.iter().enumerate().position(|(at, binder)| {
            binder.bound().is_some_and(|bound| {
                fixed.get(at).copied().unwrap_or(false)
                    && bound
                        .contract
                        .arguments
                        .iter()
                        .all(|argument| all_marked(argument, &fixed))
                    && bound
                        .choices
                        .entries()
                        .iter()
                        .any(|(_, choice)| !all_marked(choice, &fixed))
            })
        })
    }
}

/// Marks each binder `ty` mentions.
fn mark(ty: &Type, fixed: &mut [bool]) {
    ty.visit(&mut |inner| {
        if let Type::Binder(index) = inner
            && let Some(slot) = fixed.get_mut(*index)
        {
            *slot = true;
        }
    });
}

/// Whether every binder `ty` mentions is marked.
fn all_marked(ty: &Type, fixed: &[bool]) -> bool {
    let mut all = true;
    ty.visit(&mut |inner| {
        if let Type::Binder(index) = inner {
            all &= fixed.get(*index).copied().unwrap_or(false);
        }
    });
    all
}

impl ImplementationRecord {
    /// Where open forms are valid inside this record: its binders.
    #[must_use]
    pub const fn scope(&self) -> OpenScope<'static> {
        OpenScope::Binders(self.binders.len())
    }

    /// The binders the key is open in.
    #[must_use]
    pub fn binders(&self) -> &[Binder] {
        &self.binders
    }

    /// The interface key.
    #[must_use]
    pub const fn contract(&self) -> &InterfaceKey {
        &self.contract
    }

    /// The receiver type.
    #[must_use]
    pub const fn receiver(&self) -> &Type {
        &self.receiver
    }

    /// The choices, by associated type name.
    #[must_use]
    pub const fn choices(&self) -> &NamedTypes {
        &self.choices
    }
}

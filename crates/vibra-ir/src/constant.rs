//! Constants: the values of constant expressions (`docs/spec/02-type-system.md`,
//! "Constant expressions and constant patterns").
//!
//! A constant has a value encoding, and a grant and a plan have none
//! (`docs/spec/06-runtime.md`, "What has no encoding"): a type that is, or
//! holds, an operation interface value type, a `Proc`, or an `Intent` is no
//! constant type and no dict key type. [`crate::Value`] has no form for a
//! grant or a plan, so no constant, input, or decoded datum holds one.

use std::collections::BTreeMap;

use crate::effects::is_operation_interface;
use crate::{InterfaceDefinition, IrError, Type, TypeId, Value};

/// A typed constant: a closed type and a value that holds no callable.
///
/// A literal, a labelled default, a constant pattern, and the consequence of
/// a `where … else` are constants. Whether the value fits a declared type,
/// and whether an interface in the type is an operation interface, is
/// checked where the definitions are known, when a module set is built.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Constant {
    value_type: Type,
    value: Value,
}

/// Whether `ty` holds a `Proc` or an `Intent` at any depth.
fn holds_procedure(ty: &Type) -> bool {
    let mut found = false;
    ty.visit(&mut |inner| found |= matches!(inner, Type::Proc(_) | Type::Intent(_)));
    found
}

/// Checks that `ty`, the type of `what`, holds no operation interface value
/// type, no `Proc`, and no `Intent`.
pub(crate) fn check_encodable(
    what: &'static str,
    ty: &Type,
    interfaces: &BTreeMap<TypeId, InterfaceDefinition>,
) -> Result<(), IrError> {
    let mut found = false;
    ty.visit(&mut |inner| {
        found |= match inner {
            Type::Proc(_) | Type::Intent(_) => true,
            Type::Interface(id, _, _) => {
                interfaces.get(id).is_some_and(is_operation_interface)
            }
            _ => false,
        };
    });
    if found {
        Err(IrError::NoEncoding {
            what,
            ty: Box::new(ty.clone()),
        })
    } else {
        Ok(())
    }
}

impl Constant {
    /// The constant `value` of type `value_type`.
    ///
    /// # Errors
    ///
    /// The type is open, callable, or `Never`, or holds a `Proc` or an
    /// `Intent`; or the value holds a function or a closure.
    pub fn new(value_type: Type, value: Value) -> Result<Self, IrError> {
        value_type.validate(crate::OpenScope::Closed)?;
        if matches!(value_type, Type::Fn(_) | Type::Never)
            || holds_procedure(&value_type)
            || value.holds_callable()
        {
            return Err(IrError::InvalidExpression(format!(
                "a constant of type {value_type} cannot hold a callable"
            )));
        }
        if let Some(scalar) = value.scalar_type() {
            let fits = scalar == value_type
                || matches!((&value, &value_type), (Value::Atom(atom), Type::AtomSingleton(singleton)) if atom == singleton);
            if !fits {
                return Err(IrError::TypeMismatch {
                    expected: Box::new(value_type),
                    actual: Box::new(scalar),
                });
            }
        }
        Ok(Self { value_type, value })
    }

    /// The constant's type.
    #[must_use]
    pub const fn value_type(&self) -> &Type {
        &self.value_type
    }

    /// The constant's value.
    #[must_use]
    pub const fn value(&self) -> &Value {
        &self.value
    }
}

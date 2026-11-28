//! Constants: the values of constant expressions that hold data
//! (`docs/spec/02-type-system.md`, "Constant expressions and constant
//! patterns").
//!
//! A constant is a value with a value encoding. A function, a procedure, a
//! plan, and a grant have none (`docs/spec/06-runtime.md`, "What has no
//! encoding"): [`crate::Value`] has no form for a grant or a plan, and a
//! function or a closure in a value is no constant. A constant expression
//! that holds a function value, which the type chapter admits as a labelled
//! default, is kept as its checked [`crate::Node`] instead. A type that holds
//! a grant is no constant type; a type that merely mentions a `Proc` or an
//! `Intent`, as `Option(Proc({} I32))` does, is the type of the constant
//! `none`, which holds no procedure.

use std::collections::BTreeMap;

use crate::effects::is_operation_interface;
use crate::{InterfaceDefinition, IrError, Type, TypeId, Value};

/// A typed constant: a closed type and a value that holds no callable.
///
/// A literal, a labelled default that holds no function, a constant pattern,
/// and the consequence of a `where … else` are constants. Whether the value
/// fits a declared type, and whether an interface in the type is an
/// operation interface, is checked where the definitions are known, when a
/// module set is built.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Constant {
    value_type: Type,
    value: Value,
}

/// Checks that `ty`, the type of `what`, holds no operation interface value
/// type: a grant has no encoding, whatever value sits at the type.
pub(crate) fn check_encodable(
    what: &'static str,
    ty: &Type,
    interfaces: &BTreeMap<TypeId, InterfaceDefinition>,
) -> Result<(), IrError> {
    let mut found = false;
    ty.visit(&mut |inner| {
        found |= match inner {
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
    /// The type is open or `Never`, or the value holds a function or a
    /// closure.
    pub fn new(value_type: Type, value: Value) -> Result<Self, IrError> {
        value_type.validate(crate::OpenScope::Closed)?;
        if matches!(value_type, Type::Never) {
            return Err(IrError::InvalidExpression(
                "a constant of type Never, which has no value".to_owned(),
            ));
        }
        if value.holds_callable() {
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

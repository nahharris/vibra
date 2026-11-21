//! Constants: the values of constant expressions (`docs/spec/02-type-system.md`,
//! "Constant expressions and constant patterns").

use crate::{IrError, Type, Value};

/// A typed constant: a closed type and a value that holds no callable.
///
/// A literal, a labelled default, a constant pattern, and the consequence of
/// a `where … else` are constants. Whether the value fits a declared type is
/// checked where the type's definition is known, when a module set is built.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Constant {
    value_type: Type,
    value: Value,
}

impl Constant {
    /// The constant `value` of type `value_type`.
    ///
    /// # Errors
    ///
    /// The type is open or callable, or the value holds a function or a
    /// closure.
    pub fn new(value_type: Type, value: Value) -> Result<Self, IrError> {
        value_type.validate(crate::OpenScope::Closed)?;
        if matches!(
            value_type,
            Type::Fn(_) | Type::Proc(_) | Type::Intent(_) | Type::Never
        ) || value.holds_callable()
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

//! What the emitter reports when it cannot produce a module.
//!
//! The emitter lowers every form of a checked program, so none of these
//! errors is a program the language rejects: each is a toolchain defect or a
//! hand-built program. They are a program larger than the 32-bit indices of a
//! module, an entry that takes a parameter (a checked entry has none), and a
//! checked-IR shape that violates an invariant the checker guarantees, such as
//! a pattern over a type that has no shape, a contract call with no receiver,
//! or a registry row the closed tables do not know. An error is a value, never
//! a partial or approximate module. The `lowers_every_form` module of the
//! `vibra-conformance` test target
//! (`crates/vibra-conformance/tests/it/lowers_every_form.rs`) holds every
//! checked corpus program to emitting without one.

use std::fmt;

use vibra_ir::{SourceOrigin, Type};

use crate::types::class_of;

/// Why the emitter produced no module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EmitError {
    /// The program has more functions, values, or bytes than the 32-bit indices
    /// of a module admit.
    ModuleSize,
    /// The entry takes parameters, which a checked entry never does.
    EntryParameters {
        /// The origin of the entry.
        origin: SourceOrigin,
    },
    /// A checked-IR shape that breaks an invariant the checker guarantees.
    Malformed {
        /// The kind of type or the registry symbol the lowering found no row
        /// for, `contract` for a contract call with no receiver, `conversion`
        /// for a result conversion between shapes that cannot differ, and
        /// `user-key` for a dict key with no order.
        detail: &'static str,
        /// Where the program holds the shape, when it has an origin.
        origin: Option<SourceOrigin>,
    },
}

impl EmitError {
    pub(crate) const fn malformed(
        detail: &'static str,
        origin: Option<SourceOrigin>,
    ) -> Self {
        Self::Malformed { detail, origin }
    }

    /// A value of a type no lowered kind represents, named by its kind.
    pub(crate) fn of_type(ty: &Type, origin: &SourceOrigin) -> Self {
        Self::malformed(class_of(ty).err().unwrap_or("shape"), Some(origin.clone()))
    }
}

impl fmt::Display for EmitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let origin = match self {
            Self::ModuleSize => {
                return formatter.write_str(
                    "cannot emit: the program exceeds the module size limits",
                );
            }
            Self::EntryParameters { origin } => {
                formatter.write_str("cannot emit: the entry takes parameters")?;
                Some(origin)
            }
            Self::Malformed { detail, origin } => {
                write!(
                    formatter,
                    "cannot emit: malformed checked program (`{detail}`)"
                )?;
                origin.as_ref()
            }
        };
        if let Some(origin) = origin {
            write!(formatter, " at {}:{:?}", origin.source_id(), origin.span())?;
        }
        Ok(())
    }
}

impl std::error::Error for EmitError {}

//! Why [`crate::emit`] produces no object.

use std::fmt;

/// Why a program could not be turned into an object. Emission is all or
/// nothing: no variant leaves a partial object behind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EmitError {
    /// The target triple is not one this generator emits for.
    UnsupportedTarget(String),
    /// The program uses something the generator does not implement, named. The
    /// stepper rejects the same programs (`vibra_nexus_eval::EvalError`).
    Unsupported(&'static str),
    /// A count or a size of the program does not fit what the descriptor can
    /// state, named.
    TooLarge(&'static str),
    /// Cranelift, or the object writer, refused. The text is theirs.
    Backend(String),
    /// The generator and the bridge's contract disagree: a routine the
    /// generator needs is missing from, or differently shaped in,
    /// `vibra_nexus_bridge::ROUTINES`. A defect of the toolchain.
    Contract(String),
}

impl fmt::Display for EmitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedTarget(triple) => {
                write!(formatter, "cannot emit for the target `{triple}`")
            }
            Self::Unsupported(what) => {
                write!(formatter, "the code generator does not support {what}")
            }
            Self::TooLarge(what) => {
                write!(formatter, "{what} is too large for the program descriptor")
            }
            Self::Backend(message) => {
                write!(formatter, "code generation failed: {message}")
            }
            Self::Contract(message) => {
                write!(formatter, "the bridge contract is not met: {message}")
            }
        }
    }
}

impl std::error::Error for EmitError {}

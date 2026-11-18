//! The external registries the checked IR names. M1 has one, the closed
//! `@compiler` registry; the host and native registries arrive with the
//! runtime crates that use them.

pub mod compiler;
mod vectors;

pub use compiler::{
    CompilerIntrinsic, FloatOp, FloatType, IntegerOp, IntegerType, IntrinsicRow,
    REGISTRY_MAJOR, REGISTRY_MINOR, RowType, is_assertion,
};
pub use vectors::{CHARS, Outcome, SCALARS, Vector};

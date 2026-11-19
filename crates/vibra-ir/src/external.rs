//! The external registries the checked IR names: the closed `@compiler`
//! registry, the major-1 host registry, and the table of native rows.

pub mod compiler;
pub mod host;
mod native;
mod vectors;

pub use compiler::{
    CompilerIntrinsic, FloatOp, FloatType, IntegerOp, IntegerType, IntrinsicRow,
    REGISTRY_MAJOR, REGISTRY_MINOR, RowType, is_assertion,
};
pub use native::{
    Answer, NATIVE_ROWS, NativeRow, Numeric, ObjectKind, Operand, Scalar, Totality,
};
pub use vectors::{CHARS, Outcome, SCALARS, Vector};

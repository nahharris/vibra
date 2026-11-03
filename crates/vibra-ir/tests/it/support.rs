//! Helpers shared by the `vibra-ir` integration tests.

use vibra_diagnostics::ByteSpan;
use vibra_ir::SourceOrigin;

/// The origin every hand-built expression and function carries.
pub(crate) fn origin() -> SourceOrigin {
    SourceOrigin::new("case.vib", ByteSpan::new(0, 1))
}

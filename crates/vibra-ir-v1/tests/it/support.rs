//! Helpers shared by the `vibra-ir-v1` integration tests.

use vibra_diagnostics_v1::ByteSpan;
use vibra_ir_v1::SourceOrigin;

/// The origin every hand-built expression and function carries.
pub(crate) fn origin() -> SourceOrigin {
    SourceOrigin::new("case.vib", ByteSpan::new(0, 1))
}

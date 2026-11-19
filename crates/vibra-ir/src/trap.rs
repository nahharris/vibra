//! The two defect traps of the runtime chapter's "Traps".

use vibra_diagnostics::DiagnosticCode;

/// A defect trap. Both mean that the toolchain or the runtime is wrong; no
/// user data in supported checked source can raise either, and source cannot
/// catch one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Trap {
    /// Impossible typed IR, descriptor, or continuation, or another
    /// toolchain failure. An unrecognized trap is reported as this one.
    InvalidCheckedProgram,
    /// A boundary value that is invalid, stale, foreign, or of the wrong
    /// kind; an unsealed access; an illegal builder mutation; an invalid
    /// scalar; or a malformed trusted result.
    InvalidHostValue,
}

impl Trap {
    /// Both traps, in the order of the chapter's table.
    pub const ALL: [Self; 2] = [Self::InvalidCheckedProgram, Self::InvalidHostValue];

    /// The registered diagnostic code of the trap.
    #[must_use]
    pub const fn code(self) -> DiagnosticCode {
        match self {
            Self::InvalidCheckedProgram => DiagnosticCode::RuntimeInvalidCheckedProgram,
            Self::InvalidHostValue => DiagnosticCode::RuntimeInvalidHostValue,
        }
    }

    /// The trap's atom, such as `@runtime.invalidCheckedProgram`.
    #[must_use]
    pub const fn atom(self) -> &'static str {
        self.code().as_atom()
    }
}

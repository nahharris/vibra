//! The trap codes of the v1 runtime boundary.
//!
//! `docs/spec/06-runtime.md`, "Traps", fixes the closed set of traps. A
//! runtime records a trap by its numeric code and a host reads it back, so the
//! codes are written once here, beside the closed tables of the compiler
//! registry.

/// The closed trap codes of "Traps".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrapCode {
    /// `@runtime.invalid-checked-program`.
    InvalidCheckedProgram,
    /// `@runtime.unobservable-function`.
    UnobservableFunction,
    /// `@runtime.invalid-host-value`.
    InvalidHostValue,
    /// `@runtime.division-by-zero`: an unchecked `div` or `rem` had a zero
    /// divisor.
    DivisionByZero,
}

impl TrapCode {
    /// The code a runtime records for the trap.
    #[must_use]
    pub const fn code(self) -> i32 {
        match self {
            Self::InvalidCheckedProgram => 1,
            Self::UnobservableFunction => 2,
            Self::InvalidHostValue => 3,
            Self::DivisionByZero => 4,
        }
    }

    /// The trap a code names, or `None` for a code outside the table.
    #[must_use]
    pub const fn from_code(code: i32) -> Option<Self> {
        match code {
            1 => Some(Self::InvalidCheckedProgram),
            2 => Some(Self::UnobservableFunction),
            3 => Some(Self::InvalidHostValue),
            4 => Some(Self::DivisionByZero),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TestAssertion;

    #[test]
    fn codes_round_trip() {
        for trap in [
            TrapCode::InvalidCheckedProgram,
            TrapCode::UnobservableFunction,
            TrapCode::InvalidHostValue,
            TrapCode::DivisionByZero,
        ] {
            assert_eq!(TrapCode::from_code(trap.code()), Some(trap));
        }
        assert_eq!(
            TestAssertion::ALL.map(TestAssertion::code),
            [1, 2, 3],
            "the failure codes are part of the runtime boundary"
        );
        assert_eq!(TrapCode::from_code(0), None);
    }
}

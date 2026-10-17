//! The WebAssembly boundary names and codes of a v1 module.
//!
//! `docs/spec/06-runtime.md`, "WebAssembly boundary", fixes the export names, the
//! native import module, and the numeric codes a module records and a host
//! reads. They are written once here, beside the closed tables of the compiler
//! registry, so that the emitter that writes a module and the runner that
//! reads one cannot drift apart, and so that neither depends on the other. The
//! names carry the module's version: an incompatible change requires
//! `vibra_v2`, never an edit of these.

use vibra_diagnostics::DiagnosticCode;

/// The one export of linear memory, reserved for toolchain-owned native code.
pub const MEMORY_EXPORT: &str = "vibra_v1_memory";

/// The pure, versioned module of native imports. It is the only import module
/// of a Stage 4A module, and no source can name it.
pub const NATIVE_IMPORT_MODULE: &str = "vibra_native_v1";

/// `() -> ()`: runs the binary target's entry.
pub const ENTRY_EXPORT: &str = "vibra_v1_entry";
/// `(i32) -> ()`: runs the test at a zero-based discovery position.
pub const TEST_EXPORT: &str = "vibra_v1_test";
/// `() -> i32`: what the last call recorded, as a [`Status`] code.
pub const STATUS_EXPORT: &str = "vibra_v1_status";
/// `() -> i32`: after a trap, its [`TrapCode`].
pub const TRAP_CODE_EXPORT: &str = "vibra_v1_trap_code";
/// `() -> i32`: after a trap or a failed assertion, the origin ordinal.
pub const ORIGIN_EXPORT: &str = "vibra_v1_origin";
/// `() -> i32`: after a failed assertion, its [`Failure`] code.
pub const FAILURE_EXPORT: &str = "vibra_v1_failure";
/// `() -> i64`: after a failed `assert.equal`, the expected operand.
pub const FAILURE_EXPECTED_EXPORT: &str = "vibra_v1_failure_expected";
/// `() -> i64`: after a failed `assert.equal`, the actual operand.
pub const FAILURE_ACTUAL_EXPORT: &str = "vibra_v1_failure_actual";
/// `() -> i64`: after a completed entry, the ID of its result, `0` for `void`.
pub const RESULT_EXPORT: &str = "vibra_v1_result";
/// `() -> i64`: the live arena size in bytes.
pub const LIVE_SIZE_EXPORT: &str = "vibra_v1_live_size";
/// `(i64) -> ()`: the host drops its hold on a value ID.
pub const RELEASE_EXPORT: &str = "vibra_v1_release";
/// `(i64) -> i32`: the variant index of an enum or the member index of a union.
pub const VARIANT_EXPORT: &str = "vibra_v1_variant";
/// `(i64) -> i64`: the scalar, byte, element, entry, or component count.
pub const LENGTH_EXPORT: &str = "vibra_v1_length";
/// `(i64, i64) -> i32`: a scalar component.
pub const READ_I32_EXPORT: &str = "vibra_v1_read_i32";
/// `(i64, i64) -> i64`: a scalar component.
pub const READ_I64_EXPORT: &str = "vibra_v1_read_i64";
/// `(i64, i64) -> f32`: a scalar component.
pub const READ_F32_EXPORT: &str = "vibra_v1_read_f32";
/// `(i64, i64) -> f64`: a scalar component.
pub const READ_F64_EXPORT: &str = "vibra_v1_read_f64";
/// `(i64, i64) -> i64`: a compound component as a new ID the host releases.
pub const READ_ID_EXPORT: &str = "vibra_v1_read_id";

/// Every function export a Stage 4A module may carry, in the order of the
/// specification's table. A module exports exactly [`MEMORY_EXPORT`] and a
/// subset of these.
pub const FUNCTION_EXPORTS: &[&str] = &[
    ENTRY_EXPORT,
    TEST_EXPORT,
    STATUS_EXPORT,
    TRAP_CODE_EXPORT,
    ORIGIN_EXPORT,
    FAILURE_EXPORT,
    FAILURE_EXPECTED_EXPORT,
    FAILURE_ACTUAL_EXPORT,
    RESULT_EXPORT,
    LIVE_SIZE_EXPORT,
    RELEASE_EXPORT,
    VARIANT_EXPORT,
    LENGTH_EXPORT,
    READ_I32_EXPORT,
    READ_I64_EXPORT,
    READ_F32_EXPORT,
    READ_F64_EXPORT,
    READ_ID_EXPORT,
];

/// What the last call into a module recorded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Nothing was recorded.
    Nothing,
    /// A registered trap, with a [`TrapCode`] and an origin.
    Trap,
    /// The memory host event of "Activations and memory".
    MemoryExhausted,
    /// A failed assertion, with a [`Failure`] and an origin.
    FailedAssertion,
}

impl Status {
    /// The code a module returns from `vibra_v1_status`.
    #[must_use]
    pub const fn code(self) -> i32 {
        match self {
            Self::Nothing => 0,
            Self::Trap => 1,
            Self::MemoryExhausted => 2,
            Self::FailedAssertion => 3,
        }
    }

    /// The status a code names, or `None` for a code outside the table.
    #[must_use]
    pub const fn from_code(code: i32) -> Option<Self> {
        match code {
            0 => Some(Self::Nothing),
            1 => Some(Self::Trap),
            2 => Some(Self::MemoryExhausted),
            3 => Some(Self::FailedAssertion),
            _ => None,
        }
    }
}

/// The closed trap codes of "Traps".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrapCode {
    /// `@runtime.invalid-checked-program`.
    InvalidCheckedProgram,
    /// `@runtime.unobservable-function`.
    UnobservableFunction,
    /// `@runtime.invalid-host-value`.
    InvalidHostValue,
}

impl TrapCode {
    /// The code a module returns from `vibra_v1_trap_code`.
    #[must_use]
    pub const fn code(self) -> i32 {
        match self {
            Self::InvalidCheckedProgram => 1,
            Self::UnobservableFunction => 2,
            Self::InvalidHostValue => 3,
        }
    }

    /// The trap a code names, or `None` for a code outside the table.
    #[must_use]
    pub const fn from_code(code: i32) -> Option<Self> {
        match code {
            1 => Some(Self::InvalidCheckedProgram),
            2 => Some(Self::UnobservableFunction),
            3 => Some(Self::InvalidHostValue),
            _ => None,
        }
    }

    /// The registered diagnostic the trap is reported as.
    #[must_use]
    pub const fn diagnostic_code(self) -> DiagnosticCode {
        match self {
            Self::InvalidCheckedProgram => DiagnosticCode::RuntimeInvalidCheckedProgram,
            Self::UnobservableFunction => DiagnosticCode::RuntimeUnobservableFunction,
            Self::InvalidHostValue => DiagnosticCode::RuntimeInvalidHostValue,
        }
    }
}

/// The failed assertion a module recorded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// `assert.true`.
    True,
    /// `assert.false`.
    False,
    /// `assert.equal`.
    Equal,
}

impl Failure {
    /// The code a module returns from `vibra_v1_failure`.
    #[must_use]
    pub const fn code(self) -> i32 {
        match self {
            Self::True => 1,
            Self::False => 2,
            Self::Equal => 3,
        }
    }

    /// The assertion a code names, or `None` for a code outside the table.
    #[must_use]
    pub const fn from_code(code: i32) -> Option<Self> {
        match code {
            1 => Some(Self::True),
            2 => Some(Self::False),
            3 => Some(Self::Equal),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip() {
        for status in [
            Status::Nothing,
            Status::Trap,
            Status::MemoryExhausted,
            Status::FailedAssertion,
        ] {
            assert_eq!(Status::from_code(status.code()), Some(status));
        }
        for trap in [
            TrapCode::InvalidCheckedProgram,
            TrapCode::UnobservableFunction,
            TrapCode::InvalidHostValue,
        ] {
            assert_eq!(TrapCode::from_code(trap.code()), Some(trap));
        }
        for failure in [Failure::True, Failure::False, Failure::Equal] {
            assert_eq!(Failure::from_code(failure.code()), Some(failure));
        }
        assert_eq!(Status::from_code(4), None);
        assert_eq!(TrapCode::from_code(0), None);
        assert_eq!(Failure::from_code(0), None);
    }

    #[test]
    fn every_export_is_versioned_and_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for name in FUNCTION_EXPORTS {
            assert!(name.starts_with("vibra_v1_"), "{name}");
            assert!(seen.insert(*name), "{name} is repeated");
        }
        assert!(MEMORY_EXPORT.starts_with("vibra_v1_"));
        assert!(!FUNCTION_EXPORTS.contains(&MEMORY_EXPORT));
    }
}

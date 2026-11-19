//! The names and codes of a transition module's boundary.
//!
//! `docs/spec/06-runtime.md`, "Wasm transition", fixes the export names, the
//! native import module, and the numeric codes a module records and a host
//! reads. They are written once here, beside the closed tables of the
//! compiler registry, so that the emitter that writes a module and the runner
//! that reads one cannot drift apart, and so that neither depends on the
//! other. The names carry the module's version. The codes are constants of
//! this boundary alone.

use crate::Trap;

/// The one export of linear memory, reserved for toolchain-owned native code.
pub const MEMORY_EXPORT: &str = "vibra_v1_memory";

/// The pure, versioned module of native imports. It is the only import module
/// of a transition module, and no source can name it.
pub const NATIVE_IMPORT_MODULE: &str = "vibra_native_v1";

/// `() -> ()`: calls the module's first function with no parameters, as the
/// `interpret` operation does. Only an evaluation module has it.
pub const EVAL_EXPORT: &str = "vibra_v1_eval";
/// `(i32) -> ()`: runs one test without `takes`, at a zero-based position,
/// in a fresh instance. Only a test module has it.
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
/// `() -> i64`: after a completed evaluation, its result: the scalar bits, or
/// the ID of a held value.
pub const RESULT_EXPORT: &str = "vibra_v1_result";
/// `() -> i64`: the live arena and handle-table size in bytes.
pub const LIVE_SIZE_EXPORT: &str = "vibra_v1_live_size";
/// `(i64) -> ()`: the host drops its hold on a value ID.
pub const RELEASE_EXPORT: &str = "vibra_v1_release";
/// `(i64) -> i32`: the case index of a variant value.
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

/// The function exports every transition module carries beside its entry
/// export, in the order of the chapter's table.
pub const ACCESSOR_EXPORTS: &[&str] = &[
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

/// The entry export of a module: [`EVAL_EXPORT`] or [`TEST_EXPORT`], never
/// both.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    /// An evaluation module.
    Eval,
    /// A test module.
    Test,
}

impl Entry {
    /// The export name.
    #[must_use]
    pub const fn export(self) -> &'static str {
        match self {
            Self::Eval => EVAL_EXPORT,
            Self::Test => TEST_EXPORT,
        }
    }
}

/// Exactly the export names of a module with `entry`, memory included, in
/// the order of the chapter's table: the closed table of "Wasm transition".
#[must_use]
pub fn exports(entry: Entry) -> Vec<&'static str> {
    let mut names = vec![entry.export()];
    names.extend_from_slice(ACCESSOR_EXPORTS);
    names.push(MEMORY_EXPORT);
    names
}

/// What the last call into a module recorded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Nothing was recorded.
    Nothing,
    /// A defect trap, with a [`TrapCode`] and an origin.
    Trap,
    /// The memory host event.
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

/// The trap codes of the boundary: `1` for `invalidCheckedProgram` and `3`
/// for `invalidHostValue`. `2` and `4` are unassigned and never returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrapCode(Trap);

impl TrapCode {
    /// The code of `trap`.
    #[must_use]
    pub const fn of(trap: Trap) -> Self {
        Self(trap)
    }

    /// The trap.
    #[must_use]
    pub const fn trap(self) -> Trap {
        self.0
    }

    /// The code a module returns from `vibra_v1_trap_code`.
    #[must_use]
    pub const fn code(self) -> i32 {
        match self.0 {
            Trap::InvalidCheckedProgram => 1,
            Trap::InvalidHostValue => 3,
        }
    }

    /// The trap a code names, or `None` for a code outside the table, `2`
    /// and `4` among them.
    #[must_use]
    pub const fn from_code(code: i32) -> Option<Self> {
        match code {
            1 => Some(Self(Trap::InvalidCheckedProgram)),
            3 => Some(Self(Trap::InvalidHostValue)),
            _ => None,
        }
    }
}

/// The assertion row a failed assertion names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// `assert.true`.
    AssertTrue,
    /// `assert.false`.
    AssertFalse,
    /// `assert.equal`.
    AssertEqual,
}

impl Failure {
    /// The three rows, in code order.
    pub const ALL: [Self; 3] = [Self::AssertTrue, Self::AssertFalse, Self::AssertEqual];

    /// The code a module returns from `vibra_v1_failure`.
    #[must_use]
    pub const fn code(self) -> i32 {
        match self {
            Self::AssertTrue => 1,
            Self::AssertFalse => 2,
            Self::AssertEqual => 3,
        }
    }

    /// The row a code names, or `None` for a code outside the table.
    #[must_use]
    pub const fn from_code(code: i32) -> Option<Self> {
        match code {
            1 => Some(Self::AssertTrue),
            2 => Some(Self::AssertFalse),
            3 => Some(Self::AssertEqual),
            _ => None,
        }
    }

    /// The row's wire symbol, such as `assert.equal`.
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::AssertTrue => "assert.true",
            Self::AssertFalse => "assert.false",
            Self::AssertEqual => "assert.equal",
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
        for trap in Trap::ALL {
            let code = TrapCode::of(trap);
            assert_eq!(TrapCode::from_code(code.code()), Some(code));
        }
        assert_eq!(TrapCode::of(Trap::InvalidCheckedProgram).code(), 1);
        assert_eq!(TrapCode::of(Trap::InvalidHostValue).code(), 3);
        assert_eq!(TrapCode::from_code(2), None);
        assert_eq!(TrapCode::from_code(4), None);
        for failure in Failure::ALL {
            assert_eq!(Failure::from_code(failure.code()), Some(failure));
        }
        assert_eq!(Failure::ALL.map(Failure::code), [1, 2, 3]);
        assert_eq!(Status::from_code(4), None);
        assert_eq!(TrapCode::from_code(0), None);
        assert_eq!(Failure::from_code(0), None);
    }

    #[test]
    fn every_export_is_versioned_and_the_entries_are_exclusive() {
        for entry in [Entry::Eval, Entry::Test] {
            let names = exports(entry);
            let mut seen = std::collections::BTreeSet::new();
            for name in &names {
                assert!(name.starts_with("vibra_v1_"), "{name}");
                assert!(seen.insert(*name), "{name} is repeated");
            }
            assert_eq!(names.len(), 18);
        }
        assert!(exports(Entry::Eval).contains(&EVAL_EXPORT));
        assert!(!exports(Entry::Eval).contains(&TEST_EXPORT));
        assert!(!exports(Entry::Test).contains(&EVAL_EXPORT));
    }
}

//! The `test` payload and its records.
//!
//! `docs/spec/05-tooling.md`, "Command payloads": the `test` payload is
//! `selected`, `passed`, `failed`, and `tests`, one `TestItem` per selected
//! test. Each field of a `TestItem` is the JSON projection of the test record
//! of the `@testRun.v2` observation
//! (`docs/spec/07-diagnostics-and-conformance.md`, "Persistent
//! observations"), with `null` for an absent optional field or an
//! unavailable fact.

use serde::Serialize;

use crate::diagnostic::SpanDocument;
use crate::envelope::Status;

/// The `@test.*` result of one item.
///
/// `docs/spec/05-tooling.md`, "Tests and selection", the item result table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum TestResult {
    /// The body completed with no failed assertion.
    #[serde(rename = "@test.passed")]
    Passed,
    /// An assertion was false, or an operand could not be observed.
    #[serde(rename = "@test.assertionFailed")]
    AssertionFailed,
    /// A toolchain defect trap.
    #[serde(rename = "@test.trap")]
    Trap,
    /// The selection had static errors.
    #[serde(rename = "@test.invalid")]
    Invalid,
    /// The item needs an unsupported construct.
    #[serde(rename = "@test.unavailable")]
    Unavailable,
}

impl TestResult {
    /// The exact atom spelling.
    #[must_use]
    pub const fn as_atom(self) -> &'static str {
        match self {
            Self::Passed => "@test.passed",
            Self::AssertionFailed => "@test.assertionFailed",
            Self::Trap => "@test.trap",
            Self::Invalid => "@test.invalid",
            Self::Unavailable => "@test.unavailable",
        }
    }
}

/// Whether a failure is a false assertion or an operand that could not be
/// observed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TestFailureKind {
    /// The assertion did not hold.
    Assertion,
    /// An operand of the assertion could not be observed.
    Observation,
}

/// The code of an observation error.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ObservationErrorCode {
    /// The value holds a function, a plan, or a grant, which has no
    /// observation.
    UnsupportedValue,
}

/// `ObservationError`: why an operand could not be observed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ObservationError {
    /// `unsupportedValue`.
    pub code: ObservationErrorCode,
    /// The canonical `@type.v2` text of the operand's type, or `null`.
    #[serde(rename = "type")]
    pub value_type: Option<String>,
    /// The source-origin ordinal of the observed expression, or `null`.
    pub origin: Option<u64>,
}

/// `TestItem.failure`: the failed assertion that ended a test.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestFailure {
    /// `assertion` or `observation`.
    pub kind: TestFailureKind,
    /// The assertion's wire symbol, such as `assert.equal`, or `null`.
    pub assertion: Option<String>,
    /// The canonical value text of the expected operand, or `null`.
    pub expected: Option<String>,
    /// The canonical value text of the actual operand, or `null`.
    pub actual: Option<String>,
    /// Where the failing call was written.
    pub primary_span: SpanDocument,
    /// The observation error; `null` for an assertion.
    pub observation_error: Option<ObservationError>,
}

/// `TrapRecord`: the defect trap that ended a test.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrapRecord {
    /// The trap's atom, such as `@runtime.invalidCheckedProgram`.
    pub trap_code: String,
    /// The source-origin ordinal of the trapping operation, or `null`.
    pub origin: Option<u64>,
}

/// One event of a test's audit trace.
///
/// This version publishes no event kind: a test in the pure language
/// completes no host call, so every trace is empty.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum AuditEvent {}

/// `TestItem`: one selected test and how it ended.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestItem {
    /// The test's selector, such as `@tests.math::"adds"`.
    pub name: String,
    /// The `@test.*` result; `null` when the test has none, because its
    /// execution was stopped by a host event.
    pub result: Option<TestResult>,
    /// The failed assertion, for `@test.assertionFailed`.
    pub failure: Option<TestFailure>,
    /// The defect trap, for `@test.trap`.
    pub trap: Option<TrapRecord>,
    /// The audit events the test completed.
    pub audit_trace: Vec<AuditEvent>,
    /// The code of every envelope diagnostic attributed to the item, in the
    /// envelope's order.
    pub diagnostics: Vec<String>,
    /// The observation the item reports.
    pub status: Status,
}

/// The `test` payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TestPayload {
    /// How many tests were selected.
    pub selected: u64,
    /// How many selected tests are `@test.passed`.
    pub passed: u64,
    /// `selected` minus `passed`.
    pub failed: u64,
    /// Every selected test, in discovery order.
    pub tests: Vec<TestItem>,
}

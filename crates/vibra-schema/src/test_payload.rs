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
    /// The test exhausted its memory budget: the host event
    /// `@runtime.memoryExhausted` stopped it.
    #[serde(rename = "@test.exhausted")]
    Exhausted,
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
            Self::Exhausted => "@test.exhausted",
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

/// The code of an observation error, spelled in JSON as its full atom text,
/// `@` included, as every code is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum ObservationErrorCode {
    /// `@tool.unsupportedValue`: the value holds a function, a plan, or a
    /// grant, which has no observation.
    #[serde(rename = "@tool.unsupportedValue")]
    UnsupportedValue,
}

/// `ObservationError`: why an operand could not be observed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ObservationError {
    /// `@tool.unsupportedValue`.
    pub code: ObservationErrorCode,
    /// The canonical `@type.v2` text of the operand's type, or `null`.
    #[serde(rename = "type")]
    pub value_type: Option<String>,
    /// The source origin of the observed expression, or `null`.
    pub origin: Option<SpanDocument>,
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

/// `TrapRecord`: the defect trap that ended a test or a run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrapRecord {
    /// The trap's atom, such as `@runtime.invalidCheckedProgram`.
    pub trap_code: String,
    /// The source origin of the trapping operation, or `null` when none is
    /// known.
    pub origin: Option<SpanDocument>,
}

/// One event of an audit trace, as the JSON projection of the runtime
/// chapter (`docs/spec/06-runtime.md`, "Audit trace") gives it: each field of
/// the event its own member, an atom a string keeping its `@`, and a VIBON
/// value such as an argument or a result its canonical text inside a string.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct AuditEvent(pub serde_json::Value);

/// `TestItem`: one selected test and how it ended.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestItem {
    /// The test's selector, such as `@tests.math::"adds"`.
    pub name: String,
    /// The `@test.*` result.
    pub result: TestResult,
    /// The failed assertion, for `@test.assertionFailed`.
    pub failure: Option<TestFailure>,
    /// The defect trap, for `@test.trap`.
    pub trap: Option<TrapRecord>,
    /// The events of the test's `@auditTrace.v2` document, in completion
    /// order.
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

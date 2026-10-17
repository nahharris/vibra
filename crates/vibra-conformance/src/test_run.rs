//! The `@test-run.v1` observation, from the rows either backend produces.
//!
//! `docs/spec/07-diagnostics-and-conformance.md`, "workspace-test": the
//! snapshot is a record of the suite's command result and, for each selected
//! test in canonical discovery order, its name, its outcome, the structured
//! failure or trap it ended with, and its isolated `@audit-trace.v1`. Both
//! backends build their rows and this module writes them, so the two
//! observations differ only where the backends do.

use std::fmt::Write as _;

use vibra_workspace::semantic::{TestItem, TestItemStatus};

/// Where a failed assertion or a trap is: a source and a span.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowOrigin {
    /// The source identity.
    pub source_id: String,
    /// The span's start.
    pub start: usize,
    /// The span's end.
    pub end: usize,
}

/// A false assertion, as the snapshot records it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowFailure {
    /// The assertion member atom.
    pub assertion: String,
    /// The canonical encoding of the expected operand.
    pub expected: String,
    /// The canonical encoding of the actual operand.
    pub actual: String,
    /// The assertion call.
    pub origin: RowOrigin,
}

/// A trap, as the snapshot records it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowTrap {
    /// The trap code atom.
    pub code: String,
    /// The origin, when the trap has one.
    pub origin: Option<RowOrigin>,
}

/// One selected test and how it ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestRow {
    /// The canonical selector spelling.
    pub name: String,
    /// The outcome atom, such as `@test.passed`.
    pub status: String,
    /// The false assertion, for `@test.assertion-failed`.
    pub failure: Option<RowFailure>,
    /// The trap, for `@test.trap`.
    pub trap: Option<RowTrap>,
}

impl TestRow {
    /// The row of a test the reference interpreter ran, or did not run.
    pub fn from_item(item: &TestItem) -> Self {
        let origin = |origin: &vibra_ir::SourceOrigin| RowOrigin {
            source_id: origin.source_id().to_owned(),
            start: origin.span().start(),
            end: origin.span().end(),
        };
        Self {
            name: item.name().to_owned(),
            status: item.status().as_atom().to_owned(),
            failure: item.failure().map(|failure| RowFailure {
                assertion: failure.assertion().to_owned(),
                expected: failure.expected().to_owned(),
                actual: failure.actual().to_owned(),
                origin: RowOrigin {
                    source_id: failure.source_id().to_owned(),
                    start: failure.primary_span().start(),
                    end: failure.primary_span().end(),
                },
            }),
            trap: item.trap().map(|trap| RowTrap {
                code: trap.trap_code().to_owned(),
                origin: trap.origin().map(origin),
            }),
        }
    }
}

/// The suite's command result from its rows, by the precedence of the `test`
/// payload: a trap, then an unavailable test, then a failed assertion.
pub fn command_result(rows: &[TestRow]) -> &'static str {
    let any =
        |status: TestItemStatus| rows.iter().any(|row| row.status == status.as_atom());
    if any(TestItemStatus::Trap) {
        "@command.trap"
    } else if any(TestItemStatus::Unavailable) {
        "@command.unavailable"
    } else if any(TestItemStatus::AssertionFailed) {
        "@command.test-failed"
    } else {
        "@command.ok"
    }
}

/// Writes the `@test-run.v1` snapshot. Every test has an empty audit trace in
/// Stage 4A, since no host operation exists.
pub fn format_test_run(command_result: &str, rows: &[TestRow]) -> String {
    let mut output = format!(
        "(record\n  format: @test-run.v1\n  result: {command_result}\n  tests: (array"
    );
    if rows.is_empty() {
        output.push_str(")\n)\n");
        return output;
    }
    output.push('\n');
    for row in rows {
        let _ = write!(
            output,
            "    (record name: {} result: {}",
            vibon_string(&row.name),
            row.status
        );
        if let Some(failure) = &row.failure {
            let _ = write!(
                output,
                " failure: (record assertion: {} expected: {} actual: {} primary-span: (record source-id: {} start: {}u64 end: {}u64))",
                failure.assertion,
                vibon_string(&failure.expected),
                vibon_string(&failure.actual),
                vibon_string(&failure.origin.source_id),
                failure.origin.start,
                failure.origin.end
            );
        }
        if let Some(trap) = &row.trap {
            let _ = write!(
                output,
                " trap: (record trap-code: {}",
                vibon_string(&trap.code)
            );
            if let Some(origin) = &trap.origin {
                let _ = write!(
                    output,
                    " origin: (record source-id: {} start: {}u64 end: {}u64)",
                    vibon_string(&origin.source_id),
                    origin.start,
                    origin.end
                );
            }
            output.push(')');
        }
        output.push_str(
            " audit-trace: (record format: @audit-trace.v1 events: (array)))\n",
        );
    }
    output.push_str("  )\n)\n");
    output
}

fn vibon_string(value: &str) -> String {
    vibra_ir::Value::Str(value.to_owned()).canonical_vibon()
}

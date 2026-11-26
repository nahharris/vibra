//! The `workspaceRun` operation under `interpreterV2`.
//!
//! `docs/spec/07-diagnostics-and-conformance.md`, "Case operations and
//! inputs": a `workspaceRun` case has one confined tree with its
//! `project.vibon` and observes one `@targetRun.v2` document: startup, the
//! entry's result, and the ordered audit trace, in the case's `hostWorld`
//! or the default world. The manifest names no target, so the run selects
//! the tree's one binary target. The run is always under a fake world:
//! this handler never launches on the process.
//!
//! The verdict is the status of the run's `@command.*` result
//! (`docs/spec/05-tooling.md`, the result table): `ok` is accepted,
//! `diagnostics` is refuted, `unavailable` is unsupported, and every other
//! result is unknown. A run that exhausts its memory ends with the host
//! event `@runtime.memoryExhausted`, which the case expects with
//! `expect.hostEvent`.
//!
//! [`compare_target_run`] compares an expected `@targetRun.v2` document with
//! the observed one field by field and names the first field that differs.
//! A startup event's `failure` is compared by its `code`: its `message` is
//! text for a person, as a diagnostic's is.

use vibra_interp::MemoryBudget;
use vibra_syntax::{DataNode, DataValue};
use vibra_workspace::run::{CommandResult, Launch, run_target};
use vibra_workspace::{HostWorld, SourceSnapshot, discover_project_at};

use vibra_diagnostics::DiagnosticCode;

use crate::corpus::Case;
use crate::manifest::{Verdict, decode_host_world};
use crate::profile::{
    CaseObservation, ConformanceOperation, HandlerError, ProfileHandler,
};

/// The `workspaceRun` handler: captures the declared tree as a project root,
/// searching no ancestor, and runs its one binary target in a fresh copy of
/// the case's `hostWorld`, or of the default world when it declares none.
#[derive(Clone, Copy, Debug, Default)]
pub struct WorkspaceRunHandler;

impl ProfileHandler for WorkspaceRunHandler {
    fn can_run(&self, case: &Case) -> bool {
        case.manifest().operation == ConformanceOperation::WorkspaceRun
    }

    fn run(&self, case: &Case) -> Result<CaseObservation, HandlerError> {
        let tree = case
            .tree_directory()
            .map_err(|error| HandlerError::new(error.to_string()))?
            .ok_or_else(|| {
                HandlerError::new("a workspaceRun case declares one tree")
            })?;
        let snapshot =
            match discover_project_at(&tree).and_then(SourceSnapshot::capture) {
                Ok(snapshot) => snapshot,
                Err(diagnostics) => {
                    let mut observation = CaseObservation::new(Verdict::Refuted);
                    observation.diagnostics = diagnostics;
                    return Ok(observation);
                }
            };
        let world = match case.manifest().inputs.host_world.as_deref() {
            Some(source_id) => {
                let bytes = case
                    .read_bytes(source_id)
                    .map_err(|error| HandlerError::new(error.to_string()))?;
                decode_host_world(source_id, &bytes)
                    .map_err(|error| HandlerError::new(error.to_string()))?
            }
            None => HostWorld::default(),
        };
        let outcome = run_target(
            &snapshot,
            None,
            Launch::World(&world),
            MemoryBudget::DEFAULT,
        );
        let mut observation = CaseObservation::new(verdict(outcome.result));
        observation.target_run = Some(outcome.document());
        if outcome.limits_reached.iter().any(|limit| limit == "memory") {
            observation.host_event = Some(DiagnosticCode::RuntimeMemoryExhausted);
        }
        observation.diagnostics = outcome.diagnostics;
        Ok(observation)
    }
}

/// The status of a run's result.
const fn verdict(result: CommandResult) -> Verdict {
    match result {
        CommandResult::Ok => Verdict::Accepted,
        CommandResult::Diagnostics => Verdict::Refuted,
        CommandResult::Unavailable => Verdict::Unsupported,
        CommandResult::InvalidInput
        | CommandResult::OperationalFailure
        | CommandResult::Trap
        | CommandResult::ProgramFailed => Verdict::Unknown,
    }
}

/// The fields of a `@targetRun.v2` document, in the chapter's order.
const RUN_FIELDS: [&str; 8] = [
    "format",
    "result",
    "applicationCode",
    "stdout",
    "stderr",
    "startup",
    "trap",
    "auditTrace",
];

/// Compares the expected `@targetRun.v2` document `expected`, read from
/// `path`, with the observed one, `actual`, field by field.
///
/// # Errors
///
/// A document that does not decode, or the first field that differs, named
/// by its path, such as `stdout` or `auditTrace.events[2].grant`.
pub fn compare_target_run(
    path: &str,
    expected: &[u8],
    actual: &str,
) -> Result<(), String> {
    let expected_document = vibra_syntax::parse_data(Some(path), expected);
    let Some(expected) = expected_document.value() else {
        return Err(format!("snapshot `{path}` is not a valid VIBON document"));
    };
    let actual_document = vibra_syntax::parse_data(None, actual.as_bytes());
    let Some(actual) = actual_document.value() else {
        return Err(format!(
            "the targetRun observation compared with `{path}` is not a valid VIBON document"
        ));
    };
    let mismatch =
        |field: &str| format!("targetRun field `{field}` mismatch (`{path}`)");
    compare_run(expected, actual).map_err(|field| mismatch(&field))
}

/// The fields of a record, by name, in written order.
type Fields<'n> = Vec<(&'n str, &'n DataNode)>;

/// The fields of a record, in written order; `None` for another value.
fn fields(node: &DataNode) -> Option<Fields<'_>> {
    match node.value() {
        DataValue::Record(fields) => Some(
            fields
                .iter()
                .map(|field| (field.name(), field.value()))
                .collect(),
        ),
        _ => None,
    }
}

/// The value of the field `name` of `fields`.
fn field<'n>(fields: &[(&str, &'n DataNode)], name: &str) -> Option<&'n DataNode> {
    fields
        .iter()
        .find(|(field, _)| *field == name)
        .map(|(_, value)| *value)
}

/// The names of both records, `known` first in its order, then any other in
/// written order, each once.
fn names<'a>(
    known: &[&'a str],
    expected: &[(&'a str, &DataNode)],
    actual: &[(&'a str, &DataNode)],
) -> Vec<&'a str> {
    let mut names = known.to_vec();
    for (name, _) in expected.iter().chain(actual) {
        if !names.contains(name) {
            names.push(name);
        }
    }
    names
}

/// Compares two optional values as decoded values; `Err` names `path`.
fn plain(
    path: &str,
    expected: Option<&DataNode>,
    actual: Option<&DataNode>,
) -> Result<(), String> {
    match (expected, actual) {
        (None, None) => Ok(()),
        (Some(expected), Some(actual)) if expected.decoded_eq(actual) => Ok(()),
        _ => Err(path.to_owned()),
    }
}

/// Both values as records, or `Err` naming `path`.
fn records<'n>(
    path: &str,
    expected: Option<&'n DataNode>,
    actual: Option<&'n DataNode>,
) -> Result<Option<(Fields<'n>, Fields<'n>)>, String> {
    match (expected, actual) {
        (None, None) => Ok(None),
        (Some(expected), Some(actual)) => match (fields(expected), fields(actual)) {
            (Some(expected), Some(actual)) => Ok(Some((expected, actual))),
            _ => plain(path, Some(expected), Some(actual)).map(|()| None),
        },
        _ => Err(path.to_owned()),
    }
}

fn compare_run(expected: &DataNode, actual: &DataNode) -> Result<(), String> {
    let Some((expected, actual)) = records("", Some(expected), Some(actual))? else {
        return Ok(());
    };
    for name in names(&RUN_FIELDS, &expected, &actual) {
        let (left, right) = (field(&expected, name), field(&actual, name));
        if name == "auditTrace" {
            compare_trace(left, right)?;
        } else {
            plain(name, left, right)?;
        }
    }
    Ok(())
}

fn compare_trace(
    expected: Option<&DataNode>,
    actual: Option<&DataNode>,
) -> Result<(), String> {
    const TRACE_FIELDS: [&str; 7] = [
        "format",
        "revision",
        "target",
        "drivers",
        "policies",
        "redaction",
        "events",
    ];
    let Some((expected, actual)) = records("auditTrace", expected, actual)? else {
        return Ok(());
    };
    for name in names(&TRACE_FIELDS, &expected, &actual) {
        let path = format!("auditTrace.{name}");
        let (left, right) = (field(&expected, name), field(&actual, name));
        match (name, left.map(DataNode::value), right.map(DataNode::value)) {
            ("events", Some(DataValue::List(left)), Some(DataValue::List(right))) => {
                if left.len() != right.len() {
                    return Err(path);
                }
                for (index, (left, right)) in left.iter().zip(right).enumerate() {
                    compare_event(&format!("{path}[{index}]"), left, right)?;
                }
            }
            _ => plain(&path, left, right)?,
        }
    }
    Ok(())
}

fn compare_event(
    path: &str,
    expected: &DataNode,
    actual: &DataNode,
) -> Result<(), String> {
    let Some((expected, actual)) = records(path, Some(expected), Some(actual))? else {
        return Ok(());
    };
    for name in names(&[], &expected, &actual) {
        let inner = format!("{path}.{name}");
        let (left, right) = (field(&expected, name), field(&actual, name));
        if name == "failure" {
            // A failure's message is text for a person: its code is compared,
            // and both records have the same fields.
            if let Some((left, right)) = records(&inner, left, right)? {
                let mut left_names =
                    left.iter().map(|(name, _)| *name).collect::<Vec<_>>();
                let mut right_names =
                    right.iter().map(|(name, _)| *name).collect::<Vec<_>>();
                left_names.sort_unstable();
                right_names.sort_unstable();
                if left_names != right_names {
                    return Err(inner);
                }
                plain(
                    &format!("{inner}.code"),
                    field(&left, "code"),
                    field(&right, "code"),
                )?;
            }
        } else {
            plain(&inner, left, right)?;
        }
    }
    Ok(())
}

//! The `format` operation under `toolingV2`.
//!
//! `docs/spec/07-diagnostics-and-conformance.md`, "Case operations and
//! inputs": a `format` case has one confined tree with its `project.vibon`
//! and one document of that tree, and observes the formatted text and the
//! diagnostics. The handler runs the document through the workspace
//! engine's format plan, the one `vibra fmt` uses: a captured module is
//! checked with its candidate text, any other document is formatted by
//! syntax alone, and a recovered document is kept byte for byte. Whether the
//! text changed is the comparison of the observed text with the input.

use vibra_workspace::{Discovery, WorkspaceCapture, plan_format};

use crate::corpus::Case;
use crate::manifest::Verdict;
use crate::profile::{
    CaseObservation, ConformanceOperation, HandlerError, ProfileHandler,
};

/// The `format` handler: captures the declared tree as a project root,
/// searching no ancestor, and plans the format of the declared document.
#[derive(Clone, Copy, Debug, Default)]
pub struct FormatHandler;

impl ProfileHandler for FormatHandler {
    fn can_run(&self, case: &Case) -> bool {
        case.manifest().operation == ConformanceOperation::Format
    }

    fn run(&self, case: &Case) -> Result<CaseObservation, HandlerError> {
        let inputs = &case.manifest().inputs;
        let (Some(tree), Some(source)) =
            (inputs.tree.as_deref(), inputs.source.as_deref())
        else {
            return Err(HandlerError::new(
                "a format case declares one tree and one document",
            ));
        };
        let path = source
            .strip_prefix(tree)
            .and_then(|rest| rest.strip_prefix('/'))
            .ok_or_else(|| HandlerError::new("the document is beneath the tree"))?;
        let directory = case
            .tree_directory()
            .map_err(|error| HandlerError::new(error.to_string()))?
            .ok_or_else(|| HandlerError::new("a format case declares one tree"))?;
        let capture = WorkspaceCapture::capture(&directory, Discovery::Exact);
        let plan = plan_format(&directory, &capture, path)
            .map_err(|error| HandlerError::new(error.to_string()))?;
        let mut observation = CaseObservation::new(if plan.has_errors() {
            Verdict::Refuted
        } else {
            Verdict::Accepted
        });
        observation.diagnostics = plan.diagnostics().to_vec();
        observation.formatted = plan.text().map(str::to_owned);
        Ok(observation)
    }
}

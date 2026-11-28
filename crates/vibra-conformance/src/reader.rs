//! The `reader` operation under the `readerV2` profile.
//!
//! `docs/spec/07-diagnostics-and-conformance.md`, "Case operations and
//! inputs": a `reader` case has one `.vib` or `.vibon` document, and its
//! observation is the lossless tree, recovery, diagnostics, and optionally
//! the formatted text. The declared role of the input chooses the loader, and
//! the loader checks the extension before it reads anything.

use vibra_fmt::format_document;
use vibra_syntax::{DocumentMode, load, parse, tree_document};

use crate::corpus::Case;
use crate::manifest::Verdict;
use crate::profile::{
    CaseObservation, ConformanceOperation, HandlerError, ProfileHandler,
};

/// The reader handler: parses the declared input, collects its diagnostics,
/// formats it and checks that formatting is idempotent through the same
/// loader, and observes the `formatted` text and the `@tree.v2` tree.
#[derive(Clone, Copy, Debug, Default)]
pub struct ReaderV2Handler;

impl ProfileHandler for ReaderV2Handler {
    fn can_run(&self, case: &Case) -> bool {
        case.manifest().operation == ConformanceOperation::Reader
    }

    fn run(&self, case: &Case) -> Result<CaseObservation, HandlerError> {
        let inputs = &case.manifest().inputs;
        let (source_id, mode) = match (&inputs.source, inputs.data.as_slice()) {
            (Some(source), []) => (source, DocumentMode::Source),
            (None, [data]) => (data, DocumentMode::Data),
            _ => {
                return Err(HandlerError::new(
                    "a reader case declares exactly one document",
                ));
            }
        };
        let document = load(source_id, mode, || case.read_bytes(source_id))?;
        let formatted = format_document(&document);
        if let Some(output) = &formatted {
            let reparsed = parse(mode, Some(source_id), output.as_bytes());
            if format_document(&reparsed).as_deref() != Some(output.as_str()) {
                return Err(HandlerError::new(
                    "formatting is not idempotent through the same loader",
                ));
            }
            if let Some(value) = document.value()
                && !reparsed
                    .value()
                    .is_some_and(|reparsed| reparsed.decoded_eq(value))
            {
                return Err(HandlerError::new(
                    "the formatted document does not decode to the same value",
                ));
            }
        }
        let mut observation = CaseObservation::new(if document.has_errors() {
            Verdict::Refuted
        } else {
            Verdict::Accepted
        });
        observation.diagnostics = document.diagnostics().to_vec();
        observation.tree = tree_document(&document);
        observation.formatted = formatted;
        Ok(observation)
    }
}

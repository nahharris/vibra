//! Static and interpreter conformance adapters for the M2 primitive/binding path.

use crate::corpus::Case;
use crate::manifest::ConformanceOperation;
use crate::runner::{
    CaseObservation, ExecutionObservation, HandlerError, ProfileHandler,
};
use std::path::Path;

use vibra_types::{check_bootstrap_text_import, check_source, verify_bootstrap};

fn check_case_source(
    source_id: &str,
    source: &str,
) -> Result<vibra_types::CheckResult, HandlerError> {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    check_case_source_at_root(source_id, source, &repository)
}

fn check_case_source_at_root(
    source_id: &str,
    source: &str,
    repository: &Path,
) -> Result<vibra_types::CheckResult, HandlerError> {
    let document = vibra_syntax::parse_source(Path::new(source_id), source)
        .map_err(|error| HandlerError::new(format!("source dispatch parse failed: {error}")))?;
    let exact_import = document
        .ast()
        .is_some_and(|ast| {
            ast.declarations().iter().any(|declaration| {
                matches!(
                    declaration,
                    vibra_syntax::Declaration::Import(import)
                        if import.alias().kind() == vibra_syntax::NameKind::Symbol
                            && import.alias().value() == "text"
                            && import.target().kind() == vibra_syntax::NameKind::Atom
                            && import.target().value() == "std.text"
                )
            })
        });
    if exact_import {
        let verification = verify_bootstrap(repository).map_err(|error| {
            HandlerError::new(format!("bootstrap provenance verification failed: {error}"))
        })?;
        return Ok(check_bootstrap_text_import(
            &verification,
            source_id,
            source,
        ));
    }
    Ok(check_source(source_id, source))
}

/// Runs source type checking and returns the canonical checked-program
/// observation.
#[derive(Clone, Copy, Debug, Default)]
pub struct StaticV1TypeHandler;

impl ProfileHandler for StaticV1TypeHandler {
    fn can_run(&self, case: &Case) -> bool {
        case.manifest().operation == ConformanceOperation::TypeCheck
    }

    fn run(&self, case: &Case) -> Result<CaseObservation, HandlerError> {
        let source_id =
            case.manifest().inputs.source.as_deref().ok_or_else(|| {
                HandlerError::new("type-check case needs a source input")
            })?;
        let source = case
            .read_file(source_id)
            .map_err(|error| HandlerError::new(error.to_string()))?;
        let checked = check_case_source(source_id, &source)?;
        let accepted = checked.accepted();
        Ok(CaseObservation {
            accepted,
            diagnostics: checked.diagnostics().to_vec(),
            types: checked.program().map(|program| program.canonical_vibon()),
            ..CaseObservation::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::check_case_source_at_root;
    use std::path::Path;

    #[test]
    fn dispatch_parses_comments_and_multiline_imports() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let source = "; leading comment\n\n(import\n text\n @std.text)\n(defn answer () u64 (text.length \"😀\"))";
        let checked = check_case_source_at_root("app/main.vib", source, &repository)
            .expect("dispatch should verify bootstrap");
        assert!(checked.accepted(), "{:?}", checked.diagnostics());
    }

    #[test]
    fn dispatch_propagates_bootstrap_verification_failure() {
        let root = std::env::temp_dir().join(format!(
            "vibra-conformance-bootstrap-missing-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("temporary root");
        let source = "(import text @std.text)\n(defn answer () u64 (text.length \"x\"))";
        let error = check_case_source_at_root("app/main.vib", source, &root)
            .expect_err("bootstrap failure must cross the handler boundary");
        assert!(error.message().contains("bootstrap provenance verification failed"));
        let _ = std::fs::remove_dir_all(root);
    }
}

/// Checks and executes one source document through the M2 reference
/// interpreter.  The interpreter receives only the checked IR returned by
/// `vibra-types`.
#[derive(Clone, Copy, Debug, Default)]
pub struct InterpreterV1Handler;

impl ProfileHandler for InterpreterV1Handler {
    fn can_run(&self, case: &Case) -> bool {
        case.manifest().operation == ConformanceOperation::Interpret
    }

    fn run(&self, case: &Case) -> Result<CaseObservation, HandlerError> {
        let source_id =
            case.manifest().inputs.source.as_deref().ok_or_else(|| {
                HandlerError::new("interpret case needs a source input")
            })?;
        let source = case
            .read_file(source_id)
            .map_err(|error| HandlerError::new(error.to_string()))?;
        let checked = check_case_source(source_id, &source)?;
        let Some(program) = checked.program() else {
            return Ok(CaseObservation {
                accepted: false,
                diagnostics: checked.diagnostics().to_vec(),
                ..CaseObservation::default()
            });
        };
        let execution = vibra_interp::run(program)
            .map_err(|error| HandlerError::new(error.to_string()))?;
        Ok(CaseObservation {
            accepted: checked.accepted(),
            diagnostics: checked.diagnostics().to_vec(),
            interpreter: Some(ExecutionObservation {
                result: Some(execution.canonical_result()),
                audit_trace: execution.audit_trace().to_vec(),
            }),
            ..CaseObservation::default()
        })
    }
}

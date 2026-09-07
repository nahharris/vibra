//! Static and interpreter conformance adapters for the M2 primitive/binding path.

use crate::corpus::Case;
use crate::manifest::ConformanceOperation;
use crate::runner::{
    CaseObservation, ExecutionObservation, HandlerError, ProfileHandler,
};
use std::path::Path;

use vibra_types::{
    BootstrapVerification, check_bootstrap_text_import, check_source, verify_bootstrap,
};

const BOOTSTRAP_PROVENANCE_FAILURE: &str = "bootstrap provenance verification failed";

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
    let document =
        vibra_syntax::parse_source(Path::new(source_id), source).map_err(|error| {
            HandlerError::new(format!("source dispatch parse failed: {error}"))
        })?;
    let exact_import = document.ast().is_some_and(|ast| {
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
        let verification = verified_bootstrap(repository)?;
        return Ok(check_bootstrap_text_import(
            &verification,
            source_id,
            source,
        ));
    }
    Ok(check_source(source_id, source))
}

fn verified_bootstrap(
    repository: &Path,
) -> Result<BootstrapVerification, HandlerError> {
    verify_bootstrap(repository).map_err(|error| {
        HandlerError::new(format!("{BOOTSTRAP_PROVENANCE_FAILURE}: {error}"))
    })
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
#[allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
mod tests {
    use super::{BOOTSTRAP_PROVENANCE_FAILURE, check_case_source_at_root};
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
        let source =
            "(import text @std.text)\n(defn answer () u64 (text.length \"x\"))";
        let error = check_case_source_at_root("app/main.vib", source, &root)
            .expect_err("bootstrap failure must cross the handler boundary");
        assert!(error.message().starts_with(BOOTSTRAP_PROVENANCE_FAILURE));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn dispatch_propagates_tampered_bootstrap_artifact_failure() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        // macOS exposes the temporary directory through `/var`, a symlink to
        // `/private/var`.  The bootstrap verifier intentionally rejects
        // symlinked roots, so canonicalize the parent before constructing the
        // fixture to exercise the intended artifact-digest failure.
        let temporary_directory =
            std::fs::canonicalize(std::env::temp_dir()).expect("temporary directory");
        let root = temporary_directory.join(format!(
            "vibra-conformance-bootstrap-tamper-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        for relative in [
            "stdlib/m2/bootstrap-manifest.vibon",
            "stdlib/m2/bootstrap.vibon",
            "stdlib/m2/bootstrap.vibon.sig",
            "stdlib/m2/toolchain-ed25519.pub",
            "stdlib/m2/src/std/text.vib",
            "stdlib/m2/src/std/assert.vib",
        ] {
            let destination = root.join(relative);
            std::fs::create_dir_all(destination.parent().expect("bootstrap parent"))
                .expect("bootstrap directory");
            std::fs::copy(repository.join(relative), destination)
                .expect("bootstrap input copy");
        }
        std::fs::write(root.join("stdlib/m2/bootstrap.vibon"), b"tampered")
            .expect("tamper bootstrap artifact");
        let source =
            "(import text @std.text)\n(defn answer () u64 (text.length \"x\"))";
        let error = check_case_source_at_root("app/main.vib", source, &root)
            .expect_err("tampered bootstrap must cross the handler boundary");
        assert!(error.message().starts_with(BOOTSTRAP_PROVENANCE_FAILURE));
        assert!(error.message().contains("artifact digest mismatch"));
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

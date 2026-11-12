//! Capture and check a complete package before starting a store transaction.

use super::graph::{CheckedGraphInput, EncodedGraph, encode_checked_graph};
use crate::index::IndexDocument;
use crate::semantic::{CheckStatus, check_all_for_codebase};
use crate::test_runner::check_tests_from_resolved;
use crate::{WorkspaceError, WorkspaceSnapshot};
use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode, Level};

/// Immutable candidate staged before the store writer transaction.
pub struct PreparedImport {
    /// Canonical checked records, attachments, and revision identity.
    pub graph: EncodedGraph,
    /// Derived workspace facts from the same admitted source snapshot.
    pub index: IndexDocument,
    /// Warning diagnostics that did not reject the graph.
    pub diagnostics: Vec<Diagnostic>,
    /// Captured source used to render diagnostics against exact bytes.
    pub source_texts: BTreeMap<String, String>,
}

/// Failure to prepare an import; publication has not started.
pub struct ImportError {
    message: String,
    diagnostics: Vec<Diagnostic>,
    source_texts: BTreeMap<String, String>,
    source_rejected: bool,
}

impl ImportError {
    /// Stable diagnostics, preserving source locations.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
    /// Exact captured diagnostic source text.
    #[must_use]
    pub fn source_texts(&self) -> &BTreeMap<String, String> {
        &self.source_texts
    }
    /// Whether checking rejected source rather than an operation failing.
    #[must_use]
    pub const fn source_rejected(&self) -> bool {
        self.source_rejected
    }

    fn workspace(error: WorkspaceError) -> Self {
        let source_rejected = !error.diagnostics().is_empty()
            && !error.diagnostics().iter().any(|diagnostic| {
                matches!(
                    diagnostic.code(),
                    DiagnosticCode::ProjectIoError
                        | DiagnosticCode::ModuleIoError
                        | DiagnosticCode::StoreIoError
                )
            });
        let diagnostics = if !error.diagnostics().is_empty() {
            error.diagnostics().to_vec()
        } else {
            vec![Diagnostic::new(
                DiagnosticCode::StoreIoError,
                ByteSpan::empty_at(0),
                error.to_string(),
            )]
        };
        Self {
            message: error.to_string(),
            source_rejected,
            diagnostics,
            source_texts: error.source_texts().clone(),
        }
    }
    fn operational(message: String, source_texts: BTreeMap<String, String>) -> Self {
        Self {
            diagnostics: vec![Diagnostic::new(
                DiagnosticCode::CodebaseInvalidInput,
                ByteSpan::empty_at(0),
                &message,
            )],
            source_rejected: false,
            source_texts,
            message,
        }
    }
}

impl fmt::Display for ImportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}
impl std::error::Error for ImportError {}

impl fmt::Debug for ImportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImportError")
            .field("message", &self.message)
            .field("diagnostics", &self.diagnostics)
            .field("source_count", &self.source_texts.len())
            .field("source_rejected", &self.source_rejected)
            .finish()
    }
}

impl fmt::Debug for PreparedImport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedImport")
            .field("revision", &self.graph.revision.address)
            .field("object_count", &self.graph.objects.len())
            .field("source_count", &self.graph.sources.len())
            .field("diagnostics", &self.diagnostics)
            .finish()
    }
}

/// Captures and checks all local targets and tests without executing code.
/// Creates no store or persistent output and starts no publication transaction.
pub fn prepare_import(root: impl AsRef<Path>) -> Result<PreparedImport, ImportError> {
    let workspace =
        WorkspaceSnapshot::load_confined(root).map_err(ImportError::workspace)?;
    let mut source_texts = workspace
        .source()
        .documents()
        .filter_map(|document| {
            std::str::from_utf8(document.bytes())
                .ok()
                .map(|text| (document.source_id().to_owned(), text.to_owned()))
        })
        .collect::<BTreeMap<_, _>>();
    if let Ok(project) = std::str::from_utf8(workspace.project().project_bytes()) {
        source_texts.insert(
            workspace.project().project_source_id().to_owned(),
            project.to_owned(),
        );
    }
    let stdlib = vibra_types::load_stdlib().map_err(|error| {
        ImportError::operational(error.to_string(), source_texts.clone())
    })?;
    let resolved = workspace
        .resolve_with_bootstrap(&stdlib)
        .map_err(ImportError::workspace)?;
    for module in resolved.modules() {
        if let Ok(text) = std::str::from_utf8(module.bytes()) {
            source_texts.insert(module.source_id().to_owned(), text.to_owned());
        }
    }
    let checked = check_all_for_codebase(&workspace, &stdlib);
    let tests = check_tests_from_resolved(&resolved, Some(&stdlib));
    let mut diagnostics = checked
        .diagnostics()
        .iter()
        .chain(tests.diagnostics())
        .cloned()
        .collect::<Vec<_>>();
    diagnostics.sort_by(|left, right| {
        (
            left.source_id(),
            left.primary_span().start(),
            left.primary_span().end(),
            left.code(),
        )
            .cmp(&(
                right.source_id(),
                right.primary_span().start(),
                right.primary_span().end(),
                right.code(),
            ))
    });
    diagnostics.dedup();
    if checked.status() != CheckStatus::Accepted
        || tests.status() != CheckStatus::Accepted
        || diagnostics
            .iter()
            .any(|diagnostic| diagnostic.level() == Level::Error)
    {
        return Err(ImportError {
            message: "the complete project did not check".to_owned(),
            diagnostics,
            source_texts,
            source_rejected: true,
        });
    }
    let graph = encode_checked_graph(CheckedGraphInput {
        workspace: &workspace,
        resolved: &resolved,
        checked: &checked,
        stdlib: &stdlib,
        tests: tests.checked().map(|_| &tests),
    })
    .map_err(|error| {
        ImportError::operational(
            format!("checked graph encoding failed: {error:?}"),
            source_texts.clone(),
        )
    })?;
    let index = crate::index::index(&workspace, Some(&stdlib))
        .map_err(ImportError::workspace)?;
    Ok(PreparedImport {
        graph,
        index,
        diagnostics,
        source_texts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_io_diagnostics_are_operational_failures() {
        for code in [
            DiagnosticCode::ProjectIoError,
            DiagnosticCode::ModuleIoError,
        ] {
            let error = ImportError::workspace(WorkspaceError::new(
                "unreadable input",
                vec![Diagnostic::new(
                    code,
                    ByteSpan::empty_at(0),
                    "unreadable input",
                )],
            ));
            assert!(!error.source_rejected());
            assert_eq!(error.diagnostics()[0].code(), code);
        }
    }
}

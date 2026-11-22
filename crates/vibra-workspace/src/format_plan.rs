//! The format plan: one confined document, formatted by its extension.
//!
//! `docs/spec/05-tooling.md`, "Formatting" and the `PATH` operand of
//! "Command surface":
//!
//! 1. The extension selects the grammar: `.vib` is source, `.vibon` is
//!    VIBON, and any other path is invalid input before anything is read.
//! 2. `PATH` is workspace-relative and confined beneath the workspace root
//!    after links are resolved; an escape is `@module.pathEscape` and an
//!    unreadable file `@module.ioError`, each at offset 0 of the path.
//! 3. A VIBON document gets the source-preserving formatter. A source
//!    document that is not a captured module is formatted by syntax alone.
//!    A captured module is formatted, and the candidate text is checked in
//!    place of the captured bytes. When the check of the module is
//!    unavailable (`@tool.unavailable`), formatting proceeds from syntax
//!    alone and the incomplete check reports nothing.
//! 4. A document the reader recovered from is kept byte for byte. A
//!    document that is not UTF-8 plans no edit and has no text: it is never
//!    decoded lossily.
//!
//! The plan is a preview. [`FormatPlan::write`] replaces the file atomically
//! when the text changed and no error diagnostic is present.

use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode, Level};
use vibra_fmt::format_document;
use vibra_syntax::{DocumentMode, parse};

use crate::discovery::{DiscoveredProject, discover_project, discover_project_at};
use crate::revision::workspace_revision;
use crate::semantic::check_snapshot;
use crate::snapshot::SourceSnapshot;

/// A counter, not the clock, makes a temporary name unique in the process.
static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

/// How the project of a workspace is found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Discovery {
    /// The nearest `project.vibon` at or above the workspace root.
    Search,
    /// The `project.vibon` of exactly the workspace root: a confined tree.
    Exact,
}

/// The project found for a workspace, captured once.
#[derive(Clone, Debug)]
pub struct WorkspaceCapture {
    project: Option<DiscoveredProject>,
    snapshot: Option<SourceSnapshot>,
    diagnostics: Vec<Diagnostic>,
    revision: Option<String>,
}

impl WorkspaceCapture {
    /// Discovers the project of `workspace` and captures its snapshot.
    ///
    /// The revision is that of the snapshot. When the walk fails, it covers
    /// the project record, the only input admitted, and the toolchain record;
    /// when discovery fails there is none.
    #[must_use]
    pub fn capture(workspace: &Path, discovery: Discovery) -> Self {
        let discovered = match discovery {
            Discovery::Search => discover_project(workspace),
            Discovery::Exact => discover_project_at(workspace),
        };
        let project = match discovered {
            Ok(project) => project,
            Err(diagnostics) => {
                return Self {
                    project: None,
                    snapshot: None,
                    diagnostics,
                    revision: None,
                };
            }
        };
        match SourceSnapshot::capture(project.clone()) {
            Ok(snapshot) => {
                let revision = workspace_revision(&snapshot.revision_records()).ok();
                Self {
                    project: Some(project),
                    snapshot: Some(snapshot),
                    diagnostics: Vec::new(),
                    revision,
                }
            }
            Err(diagnostics) => {
                let mut records = vec![crate::revision::RevisionRecord::new(
                    crate::revision::RevisionKind::Project,
                    crate::project::PROJECT_FILE_NAME,
                    project.bytes(),
                )];
                records.extend(crate::revision::embedded_toolchain_record());
                Self {
                    revision: workspace_revision(&records).ok(),
                    project: Some(project),
                    snapshot: None,
                    diagnostics,
                }
            }
        }
    }

    /// The discovered project, when discovery succeeded.
    #[must_use]
    pub const fn project(&self) -> Option<&DiscoveredProject> {
        self.project.as_ref()
    }

    /// The snapshot, when discovery and the walk succeeded.
    #[must_use]
    pub const fn snapshot(&self) -> Option<&SourceSnapshot> {
        self.snapshot.as_ref()
    }

    /// The faults of discovery or of the walk.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// The workspace revision, `None` only when discovery failed.
    #[must_use]
    pub fn revision(&self) -> Option<&str> {
        self.revision.as_deref()
    }
}

/// Why a path is not a document `fmt` formats: invalid input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvalidPath {
    reason: String,
}

impl InvalidPath {
    /// What is wrong with the path.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

impl std::fmt::Display for InvalidPath {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.reason)
    }
}

impl std::error::Error for InvalidPath {}

/// One formatted document, before anything is written.
#[derive(Clone, Debug)]
pub struct FormatPlan {
    path: String,
    file: Option<PathBuf>,
    changed: bool,
    text: Option<String>,
    diagnostics: Vec<Diagnostic>,
    revision: Option<String>,
}

impl FormatPlan {
    /// The workspace-relative path, written with `/`.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Whether the input bytes differ from the formatted bytes.
    #[must_use]
    pub const fn changed(&self) -> bool {
        self.changed
    }

    /// The formatted text, or the planned text when diagnostics prevent
    /// writing; the file's bytes unchanged when the reader recovered.
    /// `None` when the file was never read or is not UTF-8. Every span of
    /// [`Self::diagnostics`] that names the path is an offset into it.
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }

    /// The diagnostics of the document, in order.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Whether any diagnostic is an error.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.level() == Level::Error)
    }

    /// The workspace revision of the captured snapshot, `None` when no
    /// project was found.
    #[must_use]
    pub fn revision(&self) -> Option<&str> {
        self.revision.as_deref()
    }

    /// Replaces the file with the formatted text, atomically, and returns
    /// whether it wrote: only a changed text with no error diagnostic is
    /// written.
    ///
    /// # Errors
    ///
    /// `@project.ioError` at offset 0 of the path when the file cannot be
    /// replaced; the file is then left as it was.
    pub fn write(&self) -> Result<bool, Box<Diagnostic>> {
        let (Some(file), Some(text)) = (&self.file, &self.text) else {
            return Ok(false);
        };
        if !self.changed || self.has_errors() {
            return Ok(false);
        }
        let fault = |message: String| {
            Box::new(
                Diagnostic::new(
                    DiagnosticCode::ProjectIoError,
                    ByteSpan::empty_at(0),
                    message,
                )
                .with_source_id(&self.path),
            )
        };
        let parent = file
            .parent()
            .ok_or_else(|| fault("the file has no directory".to_owned()))?;
        let name = file
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let temporary = parent.join(format!(
            ".{name}.{}-{}.vibra-fmt",
            std::process::id(),
            NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed)
        ));
        if let Err(error) = fs::write(&temporary, text.as_bytes()) {
            let _ = fs::remove_file(&temporary);
            return Err(fault(format!("cannot write the formatted text: {error}")));
        }
        if let Err(error) = fs::rename(&temporary, file) {
            let _ = fs::remove_file(&temporary);
            return Err(fault(format!("cannot replace the file: {error}")));
        }
        Ok(true)
    }
}

/// The grammar of `path` by its extension.
///
/// # Errors
///
/// Any other extension is invalid input.
pub fn format_mode(path: &str) -> Result<DocumentMode, InvalidPath> {
    DocumentMode::from_path(path).ok_or_else(|| InvalidPath {
        reason: format!("`{path}` is neither a `.vib` nor a `.vibon` file"),
    })
}

/// The path with `\` written as `/`, `.` parts removed, and `..` parts
/// applied, or `None` when it is absolute or leaves the workspace.
fn normalize(path: &str) -> Option<String> {
    let written = path.replace('\\', "/");
    if written.starts_with('/') || Path::new(path).is_absolute() {
        return None;
    }
    if Path::new(path)
        .components()
        .any(|component| matches!(component, Component::Prefix(_) | Component::RootDir))
    {
        return None;
    }
    let mut parts: Vec<&str> = Vec::new();
    for part in written.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

fn path_fault(code: DiagnosticCode, path: &str, message: String) -> Box<Diagnostic> {
    Box::new(Diagnostic::new(code, ByteSpan::empty_at(0), message).with_source_id(path))
}

/// Where a confined path leads: its canonical file, or the fault.
fn confine(root: &Path, relative: &str) -> Result<PathBuf, Box<Diagnostic>> {
    let escape = || {
        path_fault(
            DiagnosticCode::ModulePathEscape,
            relative,
            "the path leaves the workspace root".to_owned(),
        )
    };
    let lexical = root.join(relative);
    match fs::canonicalize(&lexical) {
        Ok(canonical) if canonical.starts_with(root) => Ok(canonical),
        Ok(_) => Err(escape()),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            // The file is missing: its nearest existing ancestor must still
            // be inside the root, so a link cannot hide an escape.
            let mut ancestor = lexical.parent();
            while let Some(directory) = ancestor {
                if let Ok(canonical) = fs::canonicalize(directory) {
                    if !canonical.starts_with(root) {
                        return Err(escape());
                    }
                    break;
                }
                ancestor = directory.parent();
            }
            Err(path_fault(
                DiagnosticCode::ModuleIoError,
                relative,
                "the file does not exist".to_owned(),
            ))
        }
        Err(error) => Err(path_fault(
            DiagnosticCode::ModuleIoError,
            relative,
            format!("cannot resolve the path: {error}"),
        )),
    }
}

/// The project-relative source identity of the canonical `file`.
fn source_id_in(project: &DiscoveredProject, file: &Path) -> Option<String> {
    let relative = file.strip_prefix(project.root()).ok()?;
    let parts = relative
        .components()
        .map(|component| match component {
            Component::Normal(part) => part.to_str(),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    Some(parts.join("/"))
}

/// Plans the format of `path`, a workspace-relative document, in the
/// workspace `capture` holds, whose root is `workspace`.
///
/// # Errors
///
/// [`InvalidPath`] when the extension selects no grammar or the path is
/// empty.
pub fn plan_format(
    workspace: &Path,
    capture: &WorkspaceCapture,
    path: &str,
) -> Result<FormatPlan, InvalidPath> {
    let mode = format_mode(path)?;
    let revision = capture.revision().map(str::to_owned);
    let refused = |relative: &str, diagnostic: Box<Diagnostic>| FormatPlan {
        path: relative.to_owned(),
        file: None,
        changed: false,
        text: None,
        diagnostics: vec![*diagnostic],
        revision: revision.clone(),
    };
    let written = path.replace('\\', "/");
    let Some(relative) = normalize(path) else {
        return Ok(refused(
            &written,
            path_fault(
                DiagnosticCode::ModulePathEscape,
                &written,
                "the path leaves the workspace root".to_owned(),
            ),
        ));
    };
    let root = match fs::canonicalize(workspace) {
        Ok(root) => root,
        Err(error) => {
            return Ok(refused(
                &relative,
                path_fault(
                    DiagnosticCode::ModuleIoError,
                    &relative,
                    format!("cannot resolve the workspace root: {error}"),
                ),
            ));
        }
    };
    let file = match confine(&root, &relative) {
        Ok(file) => file,
        Err(diagnostic) => return Ok(refused(&relative, diagnostic)),
    };
    let module = capture.snapshot().and_then(|snapshot| {
        let source_id = source_id_in(capture.project()?, &file)?;
        snapshot
            .modules()
            .find(|module| module.source_id() == source_id)
            .map(|module| (snapshot, source_id, module.bytes().to_vec()))
    });
    let bytes = match &module {
        Some((_, _, bytes)) if mode == DocumentMode::Source => bytes.clone(),
        _ => match fs::read(&file) {
            Ok(bytes) => bytes,
            Err(error) => {
                return Ok(refused(
                    &relative,
                    path_fault(
                        DiagnosticCode::ModuleIoError,
                        &relative,
                        format!("cannot read the file: {error}"),
                    ),
                ));
            }
        },
    };
    let document = parse(mode, Some(&relative), &bytes);
    let mut diagnostics = document.diagnostics().to_vec();
    let text = format_document(&document)
        .or_else(|| std::str::from_utf8(&bytes).ok().map(str::to_owned));
    if mode == DocumentMode::Source
        && diagnostics.is_empty()
        && let Some((snapshot, source_id, _)) = &module
        && let Some(text) = &text
    {
        diagnostics.extend(check_candidate(snapshot, source_id, &relative, text));
    }
    Ok(FormatPlan {
        changed: text.as_ref().is_some_and(|text| bytes != text.as_bytes()),
        path: relative,
        file: Some(file),
        text,
        diagnostics,
        revision,
    })
}

/// The diagnostics of the captured module `source_id` when `candidate`
/// replaces its bytes, named by `path`: none when the check of the module
/// is unavailable.
fn check_candidate(
    snapshot: &SourceSnapshot,
    source_id: &str,
    path: &str,
    candidate: &str,
) -> Vec<Diagnostic> {
    let Some(replaced) = snapshot.with_module_bytes(source_id, candidate.as_bytes())
    else {
        return Vec::new();
    };
    let check = check_snapshot(&replaced, None);
    let own = check
        .diagnostics()
        .iter()
        .filter(|diagnostic| {
            diagnostic
                .source_id()
                .is_none_or(|owner| owner == source_id)
        })
        .collect::<Vec<_>>();
    if own
        .iter()
        .any(|diagnostic| diagnostic.code() == DiagnosticCode::ToolUnavailable)
    {
        return Vec::new();
    }
    own.into_iter()
        .map(|diagnostic| {
            if diagnostic.source_id().is_some() {
                diagnostic.clone().with_source_id(path)
            } else {
                diagnostic.clone()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::normalize;

    #[test]
    fn a_path_is_normalized_or_refused() {
        assert_eq!(
            normalize("src/app/main.vib").as_deref(),
            Some("src/app/main.vib")
        );
        assert_eq!(
            normalize("src\\..\\src/./app/main.vib").as_deref(),
            Some("src/app/main.vib")
        );
        assert_eq!(normalize("../other/a.vib"), None);
        assert_eq!(normalize("src/../../a.vib"), None);
        assert_eq!(normalize("/a.vib"), None);
        assert_eq!(normalize("."), None);
    }
}

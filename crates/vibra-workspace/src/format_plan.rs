//! Confined, revision-checked canonical formatting plans.

use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest, Sha256};
use vibra_diagnostics::{Diagnostic, DocumentRevision};
use vibra_fmt::format_source_with_bindings;
use vibra_syntax::{DocumentMode, parse_data, parse_source};
use vibra_types::check_source;

use crate::{WorkspaceError, WorkspaceSnapshot};

static NEXT_TEMP_FILE: AtomicU64 = AtomicU64::new(0);

/// A failure while planning or applying one canonical format operation.
#[derive(Debug)]
pub enum FormatPlanError {
    /// The requested path is invalid or outside the workspace.
    InvalidPath(String),
    /// The extension does not select `.vib` or `.vibon`.
    UnsupportedExtension(String),
    /// The workspace cannot be loaded from its exact project marker.
    Workspace(WorkspaceError),
    /// A file could not be read or atomically replaced.
    Io(String),
    /// The document is not valid UTF-8.
    InvalidUtf8(String),
    /// The formatter rejected its supplied syntax or binding facts.
    Format(String),
    /// Reparse, semantic checking, or idempotence failed before writing.
    Postcondition(String),
    /// The workspace or target document changed after planning.
    StaleRevision {
        /// Revision used when the plan was created.
        expected: String,
        /// Revision observed when application was attempted.
        actual: String,
    },
}

impl fmt::Display for FormatPlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath(message)
            | Self::Io(message)
            | Self::InvalidUtf8(message)
            | Self::Format(message)
            | Self::Postcondition(message) => formatter.write_str(message),
            Self::UnsupportedExtension(path) => write!(
                formatter,
                "formatter accepts only `.vib` and `.vibon` paths: {path}"
            ),
            Self::Workspace(error) => error.fmt(formatter),
            Self::StaleRevision { expected, actual } => write!(
                formatter,
                "format plan is stale (expected {expected}, observed {actual})"
            ),
        }
    }
}

impl std::error::Error for FormatPlanError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Workspace(error) => Some(error),
            _ => None,
        }
    }
}

/// One previewable formatting plan tied to an immutable workspace snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormatPlan {
    workspace_root: PathBuf,
    path: PathBuf,
    relative_path: String,
    workspace_revision: DocumentRevision,
    document_revision: DocumentRevision,
    original_bytes: Vec<u8>,
    original_text: String,
    formatted_text: String,
    changed: bool,
    diagnostics: Vec<Diagnostic>,
    mode: DocumentMode,
    source_was_checked: bool,
}

impl FormatPlan {
    /// The canonical workspace-relative path selected by the plan.
    #[must_use]
    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }

    /// Workspace revision against which formatting was planned.
    #[must_use]
    pub const fn workspace_revision(&self) -> &DocumentRevision {
        &self.workspace_revision
    }

    /// Exact target document revision against which formatting was planned.
    #[must_use]
    pub const fn document_revision(&self) -> &DocumentRevision {
        &self.document_revision
    }

    /// Canonical preview text. Recovered input is retained byte-for-byte.
    #[must_use]
    pub fn formatted_text(&self) -> &str {
        &self.formatted_text
    }

    /// Exact UTF-8 input used to derive formatter diagnostics and output.
    #[must_use]
    pub fn original_text(&self) -> &str {
        &self.original_text
    }

    /// Whether the canonical text differs from the captured input bytes.
    #[must_use]
    pub const fn changed(&self) -> bool {
        self.changed
    }

    /// Parser and safe label-normalization diagnostics from the source snapshot.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

/// Builds a formatter preview from one confined workspace snapshot.
pub fn plan_format(
    workspace_root: impl AsRef<Path>,
    relative_path: impl AsRef<Path>,
) -> Result<FormatPlan, FormatPlanError> {
    let workspace_root = canonical_workspace_root(workspace_root.as_ref())?;
    let requested = relative_path.as_ref();
    let mode = document_mode(requested)?;
    let (path, relative_path) = resolve_target(&workspace_root, requested)?;
    let workspace = WorkspaceSnapshot::load_confined(&workspace_root)
        .map_err(FormatPlanError::Workspace)?;
    let original_bytes = fs::read(&path).map_err(|error| {
        FormatPlanError::Io(format!("cannot read {}: {error}", path.display()))
    })?;
    let text = std::str::from_utf8(&original_bytes).map_err(|error| {
        FormatPlanError::InvalidUtf8(format!(
            "{} is not valid UTF-8: {error}",
            path.display()
        ))
    })?;

    let snapshot_document = workspace
        .source()
        .documents()
        .find(|document| document.source_id() == relative_path);
    if let Some(document) = snapshot_document
        && document.bytes() != original_bytes
    {
        return Err(stale_document(workspace.revision(), &original_bytes));
    }

    let document = parse_by_mode(&relative_path, text, mode)?;
    let mut diagnostics = document
        .diagnostics()
        .iter()
        .cloned()
        .map(|diagnostic| diagnostic.with_source_id(&relative_path))
        .collect::<Vec<_>>();
    let source_check = (mode == DocumentMode::Source && snapshot_document.is_some())
        .then(|| check_source(&relative_path, text));
    let source_was_checked = source_check
        .as_ref()
        .is_some_and(|checked| checked.accepted());
    let bindings = source_check
        .as_ref()
        .filter(|checked| checked.accepted())
        .map_or(&[][..], |checked| checked.application_bindings());
    let formatted = format_source_with_bindings(&relative_path, text, bindings)
        .map_err(|error| FormatPlanError::Format(error.to_string()))?;
    diagnostics.extend(formatted.diagnostics().iter().cloned());
    let original_text = text.to_owned();

    validate_postcondition(
        &relative_path,
        text,
        formatted.text(),
        mode,
        source_was_checked,
    )?;

    Ok(FormatPlan {
        workspace_root,
        path,
        relative_path,
        workspace_revision: workspace.revision().clone(),
        document_revision: document_revision(&original_bytes),
        changed: original_bytes != formatted.text().as_bytes(),
        original_bytes,
        original_text,
        formatted_text: formatted.text().to_owned(),
        diagnostics,
        mode,
        source_was_checked,
    })
}

/// Applies a current format plan by atomically replacing its one target file.
///
/// The workspace and document revisions are checked immediately before the
/// replacement. A stale plan leaves all workspace bytes untouched.
pub fn apply_format(plan: &FormatPlan) -> Result<(), FormatPlanError> {
    let current_workspace = WorkspaceSnapshot::load_confined(&plan.workspace_root)
        .map_err(FormatPlanError::Workspace)?;
    if current_workspace.revision() != &plan.workspace_revision {
        return Err(FormatPlanError::StaleRevision {
            expected: plan.workspace_revision.as_str().to_owned(),
            actual: current_workspace.revision().as_str().to_owned(),
        });
    }
    ensure_current_target(plan)?;
    if !plan.changed {
        return Ok(());
    }
    validate_postcondition(
        &plan.relative_path,
        std::str::from_utf8(&plan.original_bytes)
            .map_err(|error| FormatPlanError::InvalidUtf8(error.to_string()))?,
        &plan.formatted_text,
        plan.mode,
        plan.source_was_checked,
    )?;

    let temporary_path = write_temporary(plan)?;
    let final_check = recheck_revisions(plan);
    if let Err(error) = final_check {
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }
    if let Err(error) = fs::rename(&temporary_path, &plan.path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(FormatPlanError::Io(format!(
            "cannot atomically replace {}: {error}",
            plan.path.display()
        )));
    }
    Ok(())
}

fn canonical_workspace_root(root: &Path) -> Result<PathBuf, FormatPlanError> {
    let canonical = fs::canonicalize(root).map_err(|error| {
        FormatPlanError::InvalidPath(format!(
            "cannot resolve workspace root {}: {error}",
            root.display()
        ))
    })?;
    if !canonical.is_dir() {
        return Err(FormatPlanError::InvalidPath(format!(
            "workspace root is not a directory: {}",
            canonical.display()
        )));
    }
    Ok(canonical)
}

fn document_mode(path: &Path) -> Result<DocumentMode, FormatPlanError> {
    match path.extension().and_then(std::ffi::OsStr::to_str) {
        Some("vib") => Ok(DocumentMode::Source),
        Some("vibon") => Ok(DocumentMode::Data),
        _ => Err(FormatPlanError::UnsupportedExtension(
            path.display().to_string(),
        )),
    }
}

fn resolve_target(
    root: &Path,
    requested: &Path,
) -> Result<(PathBuf, String), FormatPlanError> {
    if requested.as_os_str().is_empty() || requested.is_absolute() {
        return Err(FormatPlanError::InvalidPath(
            "formatter path must be a non-empty workspace-relative path".to_owned(),
        ));
    }
    let mut path = root.to_path_buf();
    let components = requested
        .components()
        .filter(|component| !matches!(component, Component::CurDir))
        .collect::<Vec<_>>();
    if components.is_empty() {
        return Err(FormatPlanError::InvalidPath(
            "formatter path must name a file".to_owned(),
        ));
    }
    for (index, component) in components.iter().enumerate() {
        match component {
            Component::CurDir => continue,
            Component::Normal(name) => path.push(name),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(FormatPlanError::InvalidPath(
                    "formatter path may not escape the workspace root".to_owned(),
                ));
            }
        }
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            FormatPlanError::Io(format!("cannot inspect {}: {error}", path.display()))
        })?;
        if metadata.file_type().is_symlink() {
            return Err(FormatPlanError::InvalidPath(format!(
                "formatter path contains a symbolic link: {}",
                path.display()
            )));
        }
        let final_component = index == components.len().saturating_sub(1);
        if (!final_component && !metadata.is_dir())
            || (final_component && !metadata.is_file())
        {
            return Err(FormatPlanError::InvalidPath(format!(
                "formatter path does not name a regular file: {}",
                path.display()
            )));
        }
    }
    let canonical = fs::canonicalize(&path).map_err(|error| {
        FormatPlanError::Io(format!("cannot resolve {}: {error}", path.display()))
    })?;
    if !canonical.starts_with(root) {
        return Err(FormatPlanError::InvalidPath(
            "formatter path resolves outside the workspace root".to_owned(),
        ));
    }
    let relative = canonical
        .strip_prefix(root)
        .map_err(|_| {
            FormatPlanError::InvalidPath(
                "formatter path resolves outside the workspace root".to_owned(),
            )
        })?
        .to_str()
        .ok_or_else(|| {
            FormatPlanError::InvalidPath(
                "formatter path is not a Unicode workspace path".to_owned(),
            )
        })?
        .replace('\\', "/");
    Ok((canonical, relative))
}

fn parse_by_mode(
    path: &str,
    text: &str,
    mode: DocumentMode,
) -> Result<vibra_syntax::Document, FormatPlanError> {
    match mode {
        DocumentMode::Source => parse_source(Path::new(path), text),
        DocumentMode::Data => parse_data(Path::new(path), text),
    }
    .map_err(|error| FormatPlanError::Format(error.to_string()))
}

fn validate_postcondition(
    path: &str,
    original: &str,
    formatted: &str,
    mode: DocumentMode,
    source_was_checked: bool,
) -> Result<(), FormatPlanError> {
    let original_document = parse_by_mode(path, original, mode)?;
    if original_document.recovered() {
        if original.as_bytes() != formatted.as_bytes() {
            return Err(FormatPlanError::Postcondition(
                "formatting changed a recovered document".to_owned(),
            ));
        }
        return Ok(());
    }
    let reparsed = parse_by_mode(path, formatted, mode)?;
    if !reparsed.accepted() || reparsed.recovered() {
        return Err(FormatPlanError::Postcondition(
            "formatted document did not reparse cleanly".to_owned(),
        ));
    }
    let checked = (mode == DocumentMode::Source).then(|| check_source(path, formatted));
    if source_was_checked && !checked.as_ref().is_some_and(|value| value.accepted()) {
        return Err(FormatPlanError::Postcondition(
            "formatted source failed its semantic recheck".to_owned(),
        ));
    }
    let bindings = checked
        .as_ref()
        .filter(|value| value.accepted())
        .map_or(&[][..], |value| value.application_bindings());
    let idempotent = format_source_with_bindings(path, formatted, bindings)
        .map_err(|error| FormatPlanError::Postcondition(error.to_string()))?;
    if idempotent.text() != formatted {
        return Err(FormatPlanError::Postcondition(
            "formatted document is not idempotent".to_owned(),
        ));
    }
    Ok(())
}

fn ensure_current_target(plan: &FormatPlan) -> Result<(), FormatPlanError> {
    let metadata = fs::symlink_metadata(&plan.path).map_err(|error| {
        FormatPlanError::StaleRevision {
            expected: plan.document_revision.as_str().to_owned(),
            actual: format!("target unavailable: {error}"),
        }
    })?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(FormatPlanError::InvalidPath(
            "format target is no longer a regular file".to_owned(),
        ));
    }
    let current = fs::read(&plan.path).map_err(|error| {
        FormatPlanError::Io(format!("cannot reread {}: {error}", plan.path.display()))
    })?;
    let actual_revision = document_revision(&current);
    if actual_revision != plan.document_revision || current != plan.original_bytes {
        return Err(FormatPlanError::StaleRevision {
            expected: plan.document_revision.as_str().to_owned(),
            actual: actual_revision.as_str().to_owned(),
        });
    }
    Ok(())
}

fn recheck_revisions(plan: &FormatPlan) -> Result<(), FormatPlanError> {
    let current_workspace = WorkspaceSnapshot::load_confined(&plan.workspace_root)
        .map_err(FormatPlanError::Workspace)?;
    if current_workspace.revision() != &plan.workspace_revision {
        return Err(FormatPlanError::StaleRevision {
            expected: plan.workspace_revision.as_str().to_owned(),
            actual: current_workspace.revision().as_str().to_owned(),
        });
    }
    ensure_current_target(plan)
}

fn write_temporary(plan: &FormatPlan) -> Result<PathBuf, FormatPlanError> {
    let parent = plan.path.parent().ok_or_else(|| {
        FormatPlanError::InvalidPath("format target has no parent directory".to_owned())
    })?;
    let file_name = plan
        .path
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .ok_or_else(|| {
            FormatPlanError::InvalidPath(
                "format target has no Unicode filename".to_owned(),
            )
        })?;
    for _ in 0..32 {
        let nonce = NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed);
        let temporary = parent.join(format!(
            ".{file_name}.vibra-{}-{nonce}.tmp",
            std::process::id()
        ));
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(FormatPlanError::Io(format!(
                    "cannot create temporary file {}: {error}",
                    temporary.display()
                )));
            }
        };
        if let Ok(metadata) = fs::metadata(&plan.path)
            && let Err(error) = file.set_permissions(metadata.permissions())
        {
            let _ = fs::remove_file(&temporary);
            return Err(FormatPlanError::Io(format!(
                "cannot preserve permissions for {}: {error}",
                plan.path.display()
            )));
        }
        if let Err(error) = file.write_all(plan.formatted_text.as_bytes()) {
            let _ = fs::remove_file(&temporary);
            return Err(FormatPlanError::Io(format!(
                "cannot write temporary file {}: {error}",
                temporary.display()
            )));
        }
        if let Err(error) = file.sync_all() {
            let _ = fs::remove_file(&temporary);
            return Err(FormatPlanError::Io(format!(
                "cannot flush temporary file {}: {error}",
                temporary.display()
            )));
        }
        return Ok(temporary);
    }
    Err(FormatPlanError::Io(
        "cannot allocate a unique format temporary file".to_owned(),
    ))
}

fn document_revision(bytes: &[u8]) -> DocumentRevision {
    let digest = Sha256::digest(bytes);
    let mut identifier = String::from("sha256:");
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(identifier, "{byte:02x}");
    }
    DocumentRevision::new(identifier)
}

fn stale_document(
    workspace_revision: &DocumentRevision,
    bytes: &[u8],
) -> FormatPlanError {
    FormatPlanError::StaleRevision {
        expected: workspace_revision.as_str().to_owned(),
        actual: document_revision(bytes).as_str().to_owned(),
    }
}

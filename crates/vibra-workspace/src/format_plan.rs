//! Confined, revision-checked canonical formatting plans.

use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest, Sha256};
use vibra_diagnostics::{Diagnostic, DiagnosticCode, DocumentRevision};
use vibra_fmt::format_source_with_bindings;
use vibra_syntax::{DocumentMode, parse_data, parse_source};
use vibra_types::check_source;

use crate::confined_fs::ConfinedDir;
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
    parent_relative: PathBuf,
    file_name: OsString,
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
    source_checker_unavailable: bool,
    snapshot_bindings_authorized: bool,
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
    let (path, relative_path, parent_relative, file_name) =
        resolve_target(&workspace_root, requested)?;
    let workspace = WorkspaceSnapshot::load_confined(&workspace_root)
        .map_err(FormatPlanError::Workspace)?;
    let root_dir = ConfinedDir::open(&workspace_root).map_err(|error| {
        FormatPlanError::InvalidPath(format!(
            "cannot open workspace without following links {}: {error}",
            workspace_root.display()
        ))
    })?;
    let parent_dir = root_dir.open_dir(&parent_relative).map_err(|error| {
        FormatPlanError::InvalidPath(format!(
            "formatter parent path is no longer confined: {error}"
        ))
    })?;
    let original_bytes = parent_dir.read_file(&file_name).map_err(|error| {
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
    let source_check =
        (mode == DocumentMode::Source).then(|| check_source(&relative_path, text));
    let source_checker_unavailable = source_check.as_ref().is_some_and(|checked| {
        checked
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code() == DiagnosticCode::ToolUnavailable)
    });
    // Incomplete standalone checking cannot authorize reordering or block a
    // syntax-only format; its cascaded semantic diagnostics are not facts.
    if !source_checker_unavailable {
        diagnostics.extend(source_check.iter().flat_map(|checked| {
            checked
                .diagnostics()
                .iter()
                .cloned()
                .map(|diagnostic| diagnostic.with_source_id(&relative_path))
        }));
    }
    let source_was_checked = source_check
        .as_ref()
        .is_some_and(|checked| checked.accepted());
    let snapshot_bindings_authorized = snapshot_document.is_some();
    let bindings = source_check
        .as_ref()
        .filter(|checked| checked.accepted() && snapshot_bindings_authorized)
        .map_or(&[][..], |checked| checked.application_bindings());
    let formatted = format_source_with_bindings(&relative_path, text, bindings)
        .map_err(|error| FormatPlanError::Format(error.to_string()))?;
    for diagnostic in formatted.diagnostics() {
        let checker_reported_order = diagnostic.code()
            == DiagnosticCode::StyleArgumentOrder
            && !source_checker_unavailable
            && source_check.as_ref().is_some_and(|checked| {
                checked.diagnostics().iter().any(|checked_diagnostic| {
                    checked_diagnostic.code() == diagnostic.code()
                        && checked_diagnostic.primary_span()
                            == diagnostic.primary_span()
                })
            });
        if !checker_reported_order {
            diagnostics.push(diagnostic.clone());
        }
    }
    deduplicate_diagnostics(&mut diagnostics);
    let original_text = text.to_owned();

    validate_postcondition(
        &relative_path,
        text,
        formatted.text(),
        mode,
        source_was_checked,
        snapshot_bindings_authorized,
        false,
    )?;

    Ok(FormatPlan {
        workspace_root,
        path,
        parent_relative,
        file_name,
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
        source_checker_unavailable,
        snapshot_bindings_authorized,
    })
}

/// Applies a current format plan by atomically replacing its one target file.
///
/// The workspace and document revisions are checked immediately before the
/// replacement. A stale plan leaves all workspace bytes untouched.
pub fn apply_format(plan: &FormatPlan) -> Result<(), FormatPlanError> {
    if is_vendor_path(Path::new(&plan.relative_path)) {
        return Err(FormatPlanError::InvalidPath(
            "formatter may not write vendored dependencies".to_owned(),
        ));
    }
    let root_dir = ConfinedDir::open(&plan.workspace_root).map_err(|error| {
        FormatPlanError::InvalidPath(format!(
            "cannot reopen workspace without following links: {error}"
        ))
    })?;
    let parent_dir = root_dir.open_dir(&plan.parent_relative).map_err(|error| {
        FormatPlanError::InvalidPath(format!(
            "format target parent is no longer confined: {error}"
        ))
    })?;
    let current_workspace = WorkspaceSnapshot::load_confined(&plan.workspace_root)
        .map_err(FormatPlanError::Workspace)?;
    if current_workspace.revision() != &plan.workspace_revision {
        return Err(FormatPlanError::StaleRevision {
            expected: plan.workspace_revision.as_str().to_owned(),
            actual: current_workspace.revision().as_str().to_owned(),
        });
    }
    ensure_current_target(plan, &parent_dir)?;
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
        plan.snapshot_bindings_authorized,
        !plan.source_checker_unavailable,
    )?;

    let target_permissions =
        parent_dir
            .file_permissions(&plan.file_name)
            .map_err(|error| {
                FormatPlanError::Io(format!(
                    "cannot inspect target permissions: {error}"
                ))
            })?;
    let temporary_name = write_temporary(
        &parent_dir,
        &plan.file_name,
        &plan.formatted_text,
        &target_permissions,
    )?;
    let final_check = recheck_revisions(plan, &parent_dir, &temporary_name);
    if let Err(error) = final_check {
        let _ = parent_dir.remove_file(&temporary_name);
        return Err(error);
    }
    let final_parent = match ConfinedDir::open(&plan.workspace_root)
        .and_then(|root| root.open_dir(&plan.parent_relative))
    {
        Ok(parent) => parent,
        Err(error) => {
            let _ = parent_dir.remove_file(&temporary_name);
            return Err(FormatPlanError::InvalidPath(format!(
                "format target parent changed before replacement: {error}"
            )));
        }
    };
    let same_parent = match final_parent.same_as(&parent_dir) {
        Ok(same) => same,
        Err(error) => {
            let _ = parent_dir.remove_file(&temporary_name);
            return Err(FormatPlanError::Io(format!(
                "cannot verify format target parent before replacement: {error}"
            )));
        }
    };
    if !same_parent {
        let _ = parent_dir.remove_file(&temporary_name);
        return Err(FormatPlanError::InvalidPath(
            "format target parent changed before replacement".to_owned(),
        ));
    }
    if let Err(error) =
        parent_dir.rename_to(&temporary_name, &parent_dir, &plan.file_name)
    {
        let _ = parent_dir.remove_file(&temporary_name);
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
) -> Result<(PathBuf, String, PathBuf, OsString), FormatPlanError> {
    if requested.as_os_str().is_empty() || requested.is_absolute() {
        return Err(FormatPlanError::InvalidPath(
            "formatter path must be a non-empty workspace-relative path".to_owned(),
        ));
    }
    let components = requested
        .components()
        .filter(|component| !matches!(component, Component::CurDir))
        .collect::<Vec<_>>();
    if components.is_empty() {
        return Err(FormatPlanError::InvalidPath(
            "formatter path must name a file".to_owned(),
        ));
    }
    let mut relative = PathBuf::new();
    for component in components.iter() {
        match component {
            Component::CurDir => {}
            Component::Normal(name) => relative.push(name),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(FormatPlanError::InvalidPath(
                    "formatter path may not escape the workspace root".to_owned(),
                ));
            }
        }
    }
    if is_vendor_path(&relative) {
        return Err(FormatPlanError::InvalidPath(
            "formatter may not target vendored dependencies".to_owned(),
        ));
    }
    let file_name = relative.file_name().map(OsString::from).ok_or_else(|| {
        FormatPlanError::InvalidPath("formatter path must name a file".to_owned())
    })?;
    let parent_relative = relative
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .to_path_buf();
    let relative_string = relative
        .to_str()
        .ok_or_else(|| {
            FormatPlanError::InvalidPath(
                "formatter path is not a Unicode workspace path".to_owned(),
            )
        })?
        .replace('\\', "/");
    let path = root.join(&relative);
    let root_dir = ConfinedDir::open(root).map_err(|error| {
        FormatPlanError::InvalidPath(format!(
            "cannot safely open workspace root: {error}"
        ))
    })?;
    let parent_dir = root_dir.open_dir(&parent_relative).map_err(|error| {
        FormatPlanError::InvalidPath(format!(
            "formatter path contains a link or invalid parent: {error}"
        ))
    })?;
    parent_dir.read_file(&file_name).map_err(|error| {
        FormatPlanError::Io(format!("cannot read {}: {error}", path.display()))
    })?;
    Ok((path, relative_string, parent_relative, file_name))
}

fn is_vendor_path(relative: &Path) -> bool {
    relative
        .components()
        .next()
        .and_then(|component| match component {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .is_some_and(|name| {
            name.eq_ignore_ascii_case("dep") || name.eq_ignore_ascii_case("vendor")
        })
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
    snapshot_bindings_authorized: bool,
    require_accepted_source: bool,
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
    let formatted_source_accepted =
        checked.as_ref().is_some_and(|value| value.accepted());
    if mode == DocumentMode::Source
        && ((require_accepted_source && !source_was_checked)
            || (source_was_checked && !formatted_source_accepted))
    {
        return Err(FormatPlanError::Postcondition(
            "source must pass semantic checks before and after formatting".to_owned(),
        ));
    }
    let bindings = checked
        .as_ref()
        .filter(|value| value.accepted() && snapshot_bindings_authorized)
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

fn deduplicate_diagnostics(diagnostics: &mut Vec<Diagnostic>) {
    let mut unique = Vec::<Diagnostic>::with_capacity(diagnostics.len());
    for diagnostic in diagnostics.drain(..) {
        let duplicate = unique.iter().position(|existing| {
            existing.code() == diagnostic.code()
                && existing.primary_span() == diagnostic.primary_span()
                && existing.message() == diagnostic.message()
        });
        if let Some(index) = duplicate {
            if let Some(existing) = unique.get_mut(index)
                && existing.source_id().is_none()
                && diagnostic.source_id().is_some()
            {
                *existing = diagnostic;
            }
        } else {
            unique.push(diagnostic);
        }
    }
    *diagnostics = unique;
}

fn ensure_current_target(
    plan: &FormatPlan,
    parent: &ConfinedDir,
) -> Result<(), FormatPlanError> {
    let current = parent.read_file(&plan.file_name).map_err(|error| {
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

fn recheck_revisions(
    plan: &FormatPlan,
    expected_parent: &ConfinedDir,
    temporary_name: &std::ffi::OsStr,
) -> Result<(), FormatPlanError> {
    if is_vendor_path(Path::new(&plan.relative_path)) {
        return Err(FormatPlanError::InvalidPath(
            "formatter may not write vendored dependencies".to_owned(),
        ));
    }
    let root_dir = ConfinedDir::open(&plan.workspace_root).map_err(|error| {
        FormatPlanError::InvalidPath(format!(
            "workspace path changed before formatter write: {error}"
        ))
    })?;
    let parent_dir = root_dir.open_dir(&plan.parent_relative).map_err(|error| {
        FormatPlanError::InvalidPath(format!(
            "format target parent changed before write: {error}"
        ))
    })?;
    if !parent_dir.same_as(expected_parent).map_err(|error| {
        FormatPlanError::Io(format!(
            "cannot verify format target parent identity: {error}"
        ))
    })? {
        return Err(FormatPlanError::InvalidPath(
            "format target parent changed before write".to_owned(),
        ));
    }
    let current_workspace = WorkspaceSnapshot::load_confined(&plan.workspace_root)
        .map_err(FormatPlanError::Workspace)?;
    if current_workspace.revision() != &plan.workspace_revision {
        return Err(FormatPlanError::StaleRevision {
            expected: plan.workspace_revision.as_str().to_owned(),
            actual: current_workspace.revision().as_str().to_owned(),
        });
    }
    ensure_current_target(plan, &parent_dir)?;
    let temporary = parent_dir.read_file(temporary_name).map_err(|error| {
        FormatPlanError::Io(format!("format temporary file moved or changed: {error}"))
    })?;
    if temporary != plan.formatted_text.as_bytes() {
        return Err(FormatPlanError::Io(
            "format temporary file content changed before replacement".to_owned(),
        ));
    }
    Ok(())
}

fn write_temporary(
    parent: &ConfinedDir,
    target_name: &std::ffi::OsStr,
    contents: &str,
    permissions: &fs::Permissions,
) -> Result<OsString, FormatPlanError> {
    let file_name = target_name.to_str().ok_or_else(|| {
        FormatPlanError::InvalidPath("format target has no Unicode filename".to_owned())
    })?;
    for _ in 0..32 {
        let nonce = NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed);
        let temporary = OsString::from(format!(
            ".{file_name}.vibra-{}-{nonce}.tmp",
            std::process::id()
        ));
        match parent.write_new_file(&temporary, contents.as_bytes(), Some(permissions))
        {
            Ok(()) => return Ok(temporary),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(FormatPlanError::Io(format!(
                    "cannot create or write temporary file for {}: {error}",
                    target_name.to_string_lossy()
                )));
            }
        }
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

#[cfg(test)]
mod tests {
    use super::{FormatPlanError, apply_format, plan_format};
    use std::fs;
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    struct TempWorkspace(std::path::PathBuf);

    impl TempWorkspace {
        fn new() -> Self {
            let nonce = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "vibra-format-plan-unit-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir_all(path.join("src/hello")).expect("create source root");
            fs::write(
                path.join("project.vibon"),
                "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/hello\" entry: @app.main.main effects: (array))) dependencies: (map))\n",
            )
            .expect("write project marker");
            fs::write(
                path.join("src/hello/main.vib"),
                "(defn main () void    (do))\n",
            )
            .expect("write source");
            Self(path)
        }
    }

    impl Drop for TempWorkspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn apply_rejects_a_plan_with_a_vendored_dependency_path() {
        let root = TempWorkspace::new();
        let mut plan = plan_format(&root.0, Path::new("src/hello/main.vib"))
            .expect("create a regular format plan");
        plan.relative_path = "dep/std/src/main.vib".to_owned();

        let error =
            apply_format(&plan).expect_err("apply must reject vendored paths too");

        assert!(matches!(error, FormatPlanError::InvalidPath(_)));
        assert_eq!(
            fs::read_to_string(root.0.join("src/hello/main.vib"))
                .expect("read source after refusal"),
            "(defn main () void    (do))\n"
        );
    }
}

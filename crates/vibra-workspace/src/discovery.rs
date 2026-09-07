//! Confined `project.vibon` discovery.
//!
//! Discovery is deliberately separate from source walking. It selects one
//! exact marker by canonical ancestor order, decodes that marker through the
//! existing data/project loaders, and returns no ambient filesystem handle.

use std::fs;
use std::path::{Path, PathBuf};

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_syntax::parse_data;

use crate::WorkspaceError;
use crate::project::{Project, ProjectDecoder, ProjectOrigin};

const PROJECT_FILE_NAME: &str = "project.vibon";

/// A successfully discovered and typed project marker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredProject {
    root: PathBuf,
    project_path: PathBuf,
    project: Project,
}

impl DiscoveredProject {
    /// Canonical project root containing `project.vibon`.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Canonical path to the exact project marker.
    #[must_use]
    pub fn project_path(&self) -> &Path {
        &self.project_path
    }

    /// The typed project data decoded from the marker.
    #[must_use]
    pub const fn project(&self) -> &Project {
        &self.project
    }

    /// The stable source identity of the project marker.
    #[must_use]
    pub const fn project_source_id(&self) -> &'static str {
        PROJECT_FILE_NAME
    }
}

/// Stateless project discovery entry point.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorkspaceDiscovery;

impl WorkspaceDiscovery {
    /// Finds the nearest exact `project.vibon` for `start`.
    pub fn discover(
        start: impl AsRef<Path>,
    ) -> Result<DiscoveredProject, WorkspaceError> {
        discover_project(start)
    }
}

/// Finds and decodes the nearest exact `project.vibon` marker.
///
/// Existing directories start at themselves; existing files start at their
/// parent. A missing path is a normal `@project.not-found` result. Once an
/// exact marker is found, all errors are terminal and no older ancestor is
/// considered.
pub fn discover_project(
    start: impl AsRef<Path>,
) -> Result<DiscoveredProject, WorkspaceError> {
    let start = start.as_ref();
    let canonical_start = fs::canonicalize(start).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            not_found(start)
        } else {
            project_io_error(
                PROJECT_FILE_NAME,
                ByteSpan::empty_at(0),
                format!(
                    "cannot canonicalize discovery start {}: {error}",
                    start.display()
                ),
            )
        }
    })?;
    let metadata = fs::metadata(&canonical_start).map_err(|error| {
        project_io_error(
            PROJECT_FILE_NAME,
            ByteSpan::empty_at(0),
            format!(
                "cannot inspect discovery start {}: {error}",
                canonical_start.display()
            ),
        )
    })?;
    let mut directory = if metadata.is_dir() {
        canonical_start
    } else if metadata.is_file() {
        canonical_start
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        return Err(not_found(start));
    };

    loop {
        let marker = directory.join(PROJECT_FILE_NAME);
        match fs::symlink_metadata(&marker) {
            Ok(marker_metadata) => {
                if marker_metadata.file_type().is_symlink()
                    || !marker_metadata.is_file()
                {
                    return Err(project_io_error(
                        PROJECT_FILE_NAME,
                        ByteSpan::empty_at(0),
                        format!(
                            "project marker is not a regular file: {}",
                            marker.display()
                        ),
                    ));
                }
                return load_project(&directory, &marker);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(project_io_error(
                    PROJECT_FILE_NAME,
                    ByteSpan::empty_at(0),
                    format!(
                        "cannot inspect project marker {}: {error}",
                        marker.display()
                    ),
                ));
            }
        }

        let Some(parent) = directory.parent() else {
            return Err(not_found(start));
        };
        if parent == directory {
            return Err(not_found(start));
        }
        directory = parent.to_path_buf();
    }
}

/// Loads the exact marker at `root` without searching any ancestor or sibling.
pub fn discover_project_at(
    root: impl AsRef<Path>,
) -> Result<DiscoveredProject, WorkspaceError> {
    let root = root.as_ref();
    let metadata = fs::symlink_metadata(root).map_err(|error| {
        project_io_error(
            PROJECT_FILE_NAME,
            ByteSpan::empty_at(0),
            format!(
                "cannot inspect confined project root {}: {error}",
                root.display()
            ),
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(project_io_error(
            PROJECT_FILE_NAME,
            ByteSpan::empty_at(0),
            format!(
                "confined project root is not a regular directory: {}",
                root.display()
            ),
        ));
    }
    let canonical_root = fs::canonicalize(root).map_err(|error| {
        project_io_error(
            PROJECT_FILE_NAME,
            ByteSpan::empty_at(0),
            format!(
                "cannot canonicalize confined project root {}: {error}",
                root.display()
            ),
        )
    })?;
    let marker = canonical_root.join(PROJECT_FILE_NAME);
    match fs::symlink_metadata(&marker) {
        Ok(marker_metadata)
            if !marker_metadata.file_type().is_symlink()
                && marker_metadata.is_file() =>
        {
            load_project(&canonical_root, &marker)
        }
        Ok(_) => Err(project_io_error(
            PROJECT_FILE_NAME,
            ByteSpan::empty_at(0),
            format!("project marker is not a regular file: {}", marker.display()),
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Err(not_found(&canonical_root))
        }
        Err(error) => Err(project_io_error(
            PROJECT_FILE_NAME,
            ByteSpan::empty_at(0),
            format!(
                "cannot inspect project marker {}: {error}",
                marker.display()
            ),
        )),
    }
}

fn load_project(
    root: &Path,
    marker: &Path,
) -> Result<DiscoveredProject, WorkspaceError> {
    let source = fs::read(marker).map_err(|error| {
        project_io_error(
            PROJECT_FILE_NAME,
            ByteSpan::empty_at(0),
            format!("cannot read project marker {}: {error}", marker.display()),
        )
    })?;
    let source = String::from_utf8(source).map_err(|error| {
        project_io_error(
            PROJECT_FILE_NAME,
            ByteSpan::empty_at(0),
            format!("project marker is not UTF-8: {error}"),
        )
    })?;
    let document = parse_data(marker, &source).map_err(|error| {
        project_io_error(
            PROJECT_FILE_NAME,
            ByteSpan::empty_at(0),
            format!("cannot select project data loader: {error}"),
        )
    })?;
    let source_id = PROJECT_FILE_NAME;
    let mut diagnostics = document
        .diagnostics()
        .iter()
        .cloned()
        .map(|diagnostic| diagnostic.with_source_id(source_id))
        .collect::<Vec<_>>();
    let Some(data) = document.data() else {
        if diagnostics.is_empty() {
            diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::DataInvalidShape,
                    ByteSpan::empty_at(0),
                    "project document has no valid VIBON root",
                )
                .with_source_id(source_id),
            );
        }
        return Err(WorkspaceError::new(
            "project document was not accepted",
            diagnostics,
        ));
    };
    let decoded = ProjectDecoder::decode(data, ProjectOrigin::new(source_id));
    diagnostics.extend(decoded.diagnostics().iter().cloned());
    let Some(project) = decoded.project().cloned() else {
        return Err(WorkspaceError::new(
            "project schema was not accepted",
            diagnostics,
        ));
    };
    if !diagnostics.is_empty() {
        return Err(WorkspaceError::new(
            "project document was not accepted",
            diagnostics,
        ));
    }
    Ok(DiscoveredProject {
        root: root.to_path_buf(),
        project_path: marker.to_path_buf(),
        project,
    })
}

fn not_found(start: &Path) -> WorkspaceError {
    WorkspaceError::new(
        format!("no project.vibon found from {}", start.display()),
        vec![Diagnostic::new(
            DiagnosticCode::ProjectNotFound,
            ByteSpan::empty_at(0),
            "no exact project.vibon was found in the discovery boundary",
        )],
    )
}

pub(crate) fn project_io_error(
    source_id: &str,
    span: ByteSpan,
    message: String,
) -> WorkspaceError {
    WorkspaceError::new(
        message.clone(),
        vec![
            Diagnostic::new(DiagnosticCode::ProjectIoError, span, message)
                .with_source_id(source_id),
        ],
    )
}

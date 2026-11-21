//! Project discovery.
//!
//! `docs/spec/04-programs-and-packages.md`, "Discovery and the source
//! snapshot": discovery starts at a directory, or at a file's parent, and
//! examines that directory and each ancestor. The first directory with a
//! regular file named exactly `project.vibon` is the project root. No such
//! file is `@project.notFound`. A nearest marker that cannot be read or
//! decoded is that failure, and no older ancestor is tried. A marker that is
//! a symbolic link or junction is `@project.ioError`.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};

use crate::project::{PROJECT_FILE_NAME, Project, decode_project};

/// A project root and its decoded marker.
#[derive(Clone, Debug, PartialEq)]
pub struct DiscoveredProject {
    root: PathBuf,
    bytes: Vec<u8>,
    project: Project,
}

impl DiscoveredProject {
    /// The canonical project root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The exact bytes of `project.vibon`.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The decoded project.
    #[must_use]
    pub const fn project(&self) -> &Project {
        &self.project
    }
}

fn not_found() -> Vec<Diagnostic> {
    vec![Diagnostic::new(
        DiagnosticCode::ProjectNotFound,
        ByteSpan::empty_at(0),
        "no `project.vibon` at or above the workspace",
    )]
}

fn io_error(message: String) -> Vec<Diagnostic> {
    vec![
        Diagnostic::new(
            DiagnosticCode::ProjectIoError,
            ByteSpan::empty_at(0),
            message,
        )
        .with_source_id(PROJECT_FILE_NAME),
    ]
}

/// What a directory holds under the marker's name.
enum Marker {
    Absent,
    Present(PathBuf),
}

fn marker_in(directory: &Path) -> Result<Marker, Vec<Diagnostic>> {
    let marker = directory.join(PROJECT_FILE_NAME);
    match fs::symlink_metadata(&marker) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(io_error(format!(
            "the project marker {} is a link",
            marker.display()
        ))),
        Ok(metadata) if metadata.is_file() => Ok(Marker::Present(marker)),
        // A directory or another entry named `project.vibon` is not a
        // regular file, so this directory has no marker.
        Ok(_) => Ok(Marker::Absent),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(Marker::Absent),
        Err(error) => Err(io_error(format!(
            "cannot inspect {}: {error}",
            marker.display()
        ))),
    }
}

fn load(root: &Path, marker: &Path) -> Result<DiscoveredProject, Vec<Diagnostic>> {
    let bytes = fs::read(marker).map_err(|error| {
        io_error(format!("cannot read {}: {error}", marker.display()))
    })?;
    let project = decode_project(PROJECT_FILE_NAME, &bytes)?;
    Ok(DiscoveredProject {
        root: root.to_path_buf(),
        bytes,
        project,
    })
}

/// Finds the nearest `project.vibon` at or above `start` (a directory, or a
/// file whose parent starts the search) and decodes it.
pub fn discover_project(
    start: impl AsRef<Path>,
) -> Result<DiscoveredProject, Vec<Diagnostic>> {
    let start = start.as_ref();
    let canonical = match fs::canonicalize(start) {
        Ok(canonical) => canonical,
        Err(error) if error.kind() == ErrorKind::NotFound => return Err(not_found()),
        Err(error) => {
            return Err(io_error(format!(
                "cannot resolve {}: {error}",
                start.display()
            )));
        }
    };
    let mut directory = if canonical.is_dir() {
        canonical
    } else {
        match canonical.parent() {
            Some(parent) => parent.to_path_buf(),
            None => return Err(not_found()),
        }
    };
    loop {
        if let Marker::Present(marker) = marker_in(&directory)? {
            return load(&directory, &marker);
        }
        match directory.parent() {
            Some(parent) if parent != directory => directory = parent.to_path_buf(),
            _ => return Err(not_found()),
        }
    }
}

/// Decodes the `project.vibon` of exactly the directory `root`, searching no
/// ancestor: the form a confined tree uses.
pub fn discover_project_at(
    root: impl AsRef<Path>,
) -> Result<DiscoveredProject, Vec<Diagnostic>> {
    let root = root.as_ref();
    let canonical = fs::canonicalize(root).map_err(|error| {
        io_error(format!("cannot resolve {}: {error}", root.display()))
    })?;
    match marker_in(&canonical)? {
        Marker::Present(marker) => load(&canonical, &marker),
        Marker::Absent => Err(not_found()),
    }
}

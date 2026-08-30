//! Discovery and safe loading of the conformance corpus.

use std::collections::BTreeSet;
use std::fmt;
use std::path::{Component, Path, PathBuf};

use crate::manifest::{CaseManifest, MANIFEST_FILE_NAME, ManifestError};

/// A loaded case and its manifest directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Case {
    directory: PathBuf,
    manifest_path: PathBuf,
    manifest: CaseManifest,
}

impl Case {
    /// The directory containing this case's inputs and snapshots.
    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// The path to this case's `case.toml`.
    #[must_use]
    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }

    /// The decoded case manifest.
    #[must_use]
    pub fn manifest(&self) -> &CaseManifest {
        &self.manifest
    }

    /// Resolves a case-relative path after validating that it is a regular
    /// file inside this case directory.
    pub fn file(&self, relative: &str) -> Result<PathBuf, CorpusError> {
        resolve_file(&self.directory, relative, &self.manifest.id)
    }

    /// Reads a case-relative UTF-8 file.
    pub fn read_file(&self, relative: &str) -> Result<String, CorpusError> {
        let path = self.file(relative)?;
        std::fs::read_to_string(&path)
            .map_err(|source| CorpusError::Io { path, source })
    }

    /// Reads the optional source input as UTF-8.
    pub fn read_source(&self) -> Result<Option<String>, CorpusError> {
        self.manifest
            .inputs
            .source
            .as_deref()
            .map(|path| self.read_file(path))
            .transpose()
    }
}

/// A deterministic collection of loaded corpus cases.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Corpus {
    root: PathBuf,
    cases: Vec<Case>,
}

impl Corpus {
    /// Discovers every `case.toml` below `root`.
    pub fn discover(root: impl AsRef<Path>) -> Result<Self, CorpusError> {
        let root = root.as_ref();
        let metadata = std::fs::metadata(root).map_err(|source| CorpusError::Io {
            path: root.to_path_buf(),
            source,
        })?;
        if !metadata.is_dir() {
            return Err(CorpusError::InvalidRoot(root.to_path_buf()));
        }
        let root = std::fs::canonicalize(root).map_err(|source| CorpusError::Io {
            path: root.to_path_buf(),
            source,
        })?;

        let mut manifest_paths = Vec::new();
        let mut visited_directories = BTreeSet::new();
        collect_manifests(&root, &root, &mut visited_directories, &mut manifest_paths)?;

        let mut cases = Vec::with_capacity(manifest_paths.len());
        for manifest_path in manifest_paths {
            let directory = manifest_path
                .parent()
                .ok_or_else(|| CorpusError::InvalidCase {
                    path: manifest_path.clone(),
                    message: "manifest has no parent directory".to_owned(),
                })?
                .to_path_buf();
            let manifest =
                CaseManifest::from_path(&manifest_path).map_err(|source| {
                    CorpusError::Manifest {
                        path: manifest_path.clone(),
                        source,
                    }
                })?;
            validate_case_directory(&directory, &manifest)?;
            validate_declared_files(&directory, &manifest)?;
            cases.push(Case {
                directory,
                manifest_path,
                manifest,
            });
        }

        cases.sort_by(|left, right| {
            left.manifest
                .id
                .cmp(&right.manifest.id)
                .then_with(|| left.manifest_path.cmp(&right.manifest_path))
        });

        for pair in cases.windows(2) {
            let Some(left) = pair.first() else {
                continue;
            };
            let Some(right) = pair.get(1) else {
                continue;
            };
            if left.manifest.id == right.manifest.id {
                return Err(CorpusError::DuplicateCaseId(left.manifest.id.clone()));
            }
        }

        Ok(Self { root, cases })
    }

    /// Alias for [`Self::discover`], useful to callers that already use
    /// "load" for corpus setup.
    pub fn load(root: impl AsRef<Path>) -> Result<Self, CorpusError> {
        Self::discover(root)
    }

    /// The canonicalized corpus root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Loaded cases in deterministic order.
    #[must_use]
    pub fn cases(&self) -> &[Case] {
        &self.cases
    }

    /// Number of loaded cases.
    #[must_use]
    pub fn len(&self) -> usize {
        self.cases.len()
    }

    /// Whether the corpus has no cases.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cases.is_empty()
    }
}

impl<'a> IntoIterator for &'a Corpus {
    type Item = &'a Case;
    type IntoIter = std::slice::Iter<'a, Case>;

    fn into_iter(self) -> Self::IntoIter {
        self.cases.iter()
    }
}

/// Corpus discovery or layout failure.
#[derive(Debug)]
pub enum CorpusError {
    /// The requested root exists but is not a directory.
    InvalidRoot(PathBuf),
    /// A filesystem operation failed.
    Io {
        /// The path involved in the operation.
        path: PathBuf,
        /// The underlying failure.
        source: std::io::Error,
    },
    /// The case manifest could not be decoded.
    Manifest {
        /// The manifest path.
        path: PathBuf,
        /// The decoding failure.
        source: ManifestError,
    },
    /// The case directory and manifest identifier disagree.
    InvalidCase {
        /// The case or manifest path.
        path: PathBuf,
        /// A human-readable explanation.
        message: String,
    },
    /// Two directories use the same stable case identifier.
    DuplicateCaseId(String),
}

impl fmt::Display for CorpusError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRoot(path) => {
                write!(
                    formatter,
                    "corpus root is not a directory: {}",
                    path.display()
                )
            }
            Self::Io { path, source } => {
                write!(formatter, "{}: {source}", path.display())
            }
            Self::Manifest { path, source } => {
                write!(formatter, "{}: {source}", path.display())
            }
            Self::InvalidCase { path, message } => {
                write!(
                    formatter,
                    "invalid conformance case {}: {message}",
                    path.display()
                )
            }
            Self::DuplicateCaseId(id) => {
                write!(formatter, "duplicate conformance case id `{id}`")
            }
        }
    }
}

impl std::error::Error for CorpusError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Manifest { source, .. } => Some(source),
            Self::InvalidRoot(_)
            | Self::InvalidCase { .. }
            | Self::DuplicateCaseId(_) => None,
        }
    }
}

fn collect_manifests(
    directory: &Path,
    root: &Path,
    visited_directories: &mut BTreeSet<PathBuf>,
    manifests: &mut Vec<PathBuf>,
) -> Result<(), CorpusError> {
    let canonical_directory =
        std::fs::canonicalize(directory).map_err(|source| CorpusError::Io {
            path: directory.to_path_buf(),
            source,
        })?;
    if !canonical_directory.starts_with(root) {
        return Err(CorpusError::InvalidCase {
            path: directory.to_path_buf(),
            message: "case directory resolves outside the corpus root".to_owned(),
        });
    }
    if !visited_directories.insert(canonical_directory) {
        return Ok(());
    }

    let mut entries = std::fs::read_dir(directory)
        .map_err(|source| CorpusError::Io {
            path: directory.to_path_buf(),
            source,
        })?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|source| CorpusError::Io {
                    path: directory.to_path_buf(),
                    source,
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort();

    for path in entries {
        let metadata = std::fs::metadata(&path).map_err(|source| CorpusError::Io {
            path: path.clone(),
            source,
        })?;
        if metadata.is_dir() {
            collect_manifests(&path, root, visited_directories, manifests)?;
        } else if path.file_name().and_then(|name| name.to_str())
            == Some(MANIFEST_FILE_NAME)
        {
            let canonical =
                std::fs::canonicalize(&path).map_err(|source| CorpusError::Io {
                    path: path.clone(),
                    source,
                })?;
            if !canonical.starts_with(root) {
                return Err(CorpusError::InvalidCase {
                    path,
                    message: "manifest resolves outside the corpus root".to_owned(),
                });
            }
            let declared_directory =
                path.parent().ok_or_else(|| CorpusError::InvalidCase {
                    path: path.clone(),
                    message: "manifest has no containing case directory".to_owned(),
                })?;
            let canonical_declared_directory =
                std::fs::canonicalize(declared_directory).map_err(|source| {
                    CorpusError::Io {
                        path: declared_directory.to_path_buf(),
                        source,
                    }
                })?;
            let canonical_manifest_directory =
                canonical.parent().ok_or_else(|| CorpusError::InvalidCase {
                    path: canonical.clone(),
                    message: "manifest has no canonical parent directory".to_owned(),
                })?;
            if canonical_manifest_directory != canonical_declared_directory {
                return Err(CorpusError::InvalidCase {
                    path,
                    message: "manifest resolves outside its case directory".to_owned(),
                });
            }
            manifests.push(canonical);
        }
    }
    Ok(())
}

fn validate_case_directory(
    directory: &Path,
    manifest: &CaseManifest,
) -> Result<(), CorpusError> {
    let directory_name = directory.file_name().and_then(|name| name.to_str());
    if directory_name != Some(manifest.id.as_str()) {
        return Err(CorpusError::InvalidCase {
            path: directory.to_path_buf(),
            message: format!("directory name must equal manifest id `{}`", manifest.id),
        });
    }
    Ok(())
}

fn validate_declared_files(
    directory: &Path,
    manifest: &CaseManifest,
) -> Result<(), CorpusError> {
    if let Some(source) = &manifest.inputs.source {
        let _ = resolve_file(directory, source, &manifest.id)?;
    }
    if let Some(project) = &manifest.inputs.project {
        let _ = resolve_file(directory, project, &manifest.id)?;
    }
    for data in &manifest.inputs.data {
        let _ = resolve_file(directory, data, &manifest.id)?;
    }

    let expectations = &manifest.expectations;
    for snapshot in [
        expectations.formatted.as_ref(),
        expectations.resolved.as_ref(),
        expectations.types.as_ref(),
        expectations.effects.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        let _ = resolve_file(directory, snapshot, &manifest.id)?;
    }
    for execution in [
        expectations.interpreter.as_ref(),
        expectations.wasm.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        for snapshot in [execution.result.as_ref(), execution.audit_trace.as_ref()]
            .into_iter()
            .flatten()
        {
            let _ = resolve_file(directory, snapshot, &manifest.id)?;
        }
    }
    Ok(())
}

fn resolve_file(
    directory: &Path,
    relative: &str,
    case_id: &str,
) -> Result<PathBuf, CorpusError> {
    let relative_path = Path::new(relative);
    if relative.is_empty()
        || relative_path.is_absolute()
        || relative_path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(CorpusError::InvalidCase {
            path: directory.to_path_buf(),
            message: format!(
                "case `{case_id}` uses a non-relative file path `{relative}`"
            ),
        });
    }

    let path = directory.join(relative_path);
    let metadata = std::fs::metadata(&path).map_err(|source| CorpusError::Io {
        path: path.clone(),
        source,
    })?;
    if !metadata.is_file() {
        return Err(CorpusError::InvalidCase {
            path,
            message: "declared input or snapshot is not a regular file".to_owned(),
        });
    }
    let canonical_directory =
        std::fs::canonicalize(directory).map_err(|source| CorpusError::Io {
            path: directory.to_path_buf(),
            source,
        })?;
    let canonical_file =
        std::fs::canonicalize(&path).map_err(|source| CorpusError::Io {
            path: path.clone(),
            source,
        })?;
    if !canonical_file.starts_with(&canonical_directory) {
        return Err(CorpusError::InvalidCase {
            path,
            message: "declared file resolves outside its case directory".to_owned(),
        });
    }
    Ok(canonical_file)
}

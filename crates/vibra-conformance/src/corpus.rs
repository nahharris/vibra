//! Discovery and safe loading of the conformance corpus.

use std::collections::BTreeSet;
use std::fmt;
use std::path::{Component, Path, PathBuf};

use crate::manifest::{CaseManifest, MANIFEST_FILE_NAME, ManifestError};
use vibra_syntax::DocumentMode;

/// One immutable document supplied to a profile handler.
///
/// The path is the neutral source identity used by diagnostics and snapshots;
/// handlers do not need to reconstruct it from a filesystem path or merge
/// documents into one synthetic source string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaseInputDocument {
    /// The case-relative document identity.
    pub source_id: String,
    /// The grammar selected by the manifest role and extension.
    pub mode: DocumentMode,
    /// The document bytes decoded as UTF-8.
    pub text: String,
}

/// One exact file acquired from a manifest-declared tree input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaseTreeFile {
    /// Case-relative slash-separated source identity.
    pub source_id: String,
    /// Exact bytes on disk.
    pub bytes: Vec<u8>,
}

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

    /// Loads every declared input as an explicit, source-identified document.
    ///
    /// Declaration order is source, project, then data paths, matching the
    /// manifest roles. The returned order is stable and each text remains
    /// independently addressable.
    pub fn input_documents(&self) -> Result<Vec<CaseInputDocument>, CorpusError> {
        let inputs = &self.manifest.inputs;
        let mut documents = Vec::new();
        if let Some(path) = &inputs.source {
            documents.push(self.load_input(path, DocumentMode::Source)?);
        }
        if let Some(path) = &inputs.project {
            documents.push(self.load_input(path, DocumentMode::Data)?);
        }
        for path in &inputs.data {
            documents.push(self.load_input(path, DocumentMode::Data)?);
        }
        Ok(documents)
    }

    /// Acquires every regular file under the optional confined tree input.
    ///
    /// The tree is walked in stable path order. Symlink and junction targets
    /// must stay inside both the tree and case directory; canonical directory
    /// identities suppress aliases and cycles.
    pub fn tree_files(&self) -> Result<Vec<CaseTreeFile>, CorpusError> {
        let Some(relative) = self.manifest.inputs.tree.as_deref() else {
            return Ok(Vec::new());
        };
        let tree = resolve_directory(&self.directory, relative, &self.manifest.id)?;
        let case_root = std::fs::canonicalize(&self.directory).map_err(|source| {
            CorpusError::Io {
                path: self.directory.clone(),
                source,
            }
        })?;
        let tree_root =
            std::fs::canonicalize(&tree).map_err(|source| CorpusError::Io {
                path: tree.clone(),
                source,
            })?;
        let mut files = Vec::new();
        let mut visited = BTreeSet::new();
        collect_tree_files(&tree, &case_root, &tree_root, &mut visited, &mut files)?;
        files.sort_by(|left, right| left.0.cmp(&right.0));
        files
            .into_iter()
            .map(|(source_id, path)| {
                let bytes = std::fs::read(&path).map_err(|source| CorpusError::Io {
                    path: path.clone(),
                    source,
                })?;
                Ok(CaseTreeFile { source_id, bytes })
            })
            .collect()
    }

    fn load_input(
        &self,
        source_id: &str,
        mode: DocumentMode,
    ) -> Result<CaseInputDocument, CorpusError> {
        Ok(CaseInputDocument {
            source_id: source_id.to_owned(),
            mode,
            text: self.read_file(source_id)?,
        })
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
    if let Some(tree) = &manifest.inputs.tree {
        let _ = resolve_directory(directory, tree, &manifest.id)?;
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
    for query in &expectations.queries {
        let _ = resolve_file(directory, &query.snapshot, &manifest.id)?;
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

fn resolve_directory(
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
                Component::CurDir
                    | Component::ParentDir
                    | Component::RootDir
                    | Component::Prefix(_)
            )
        })
    {
        return Err(CorpusError::InvalidCase {
            path: directory.to_path_buf(),
            message: format!(
                "case `{case_id}` uses a non-relative tree path `{relative}`"
            ),
        });
    }
    let path = directory.join(relative_path);
    let metadata =
        std::fs::symlink_metadata(&path).map_err(|source| CorpusError::Io {
            path: path.clone(),
            source,
        })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(CorpusError::InvalidCase {
            path,
            message: "declared tree input is not a regular directory or is a link"
                .to_owned(),
        });
    }
    let canonical_directory =
        std::fs::canonicalize(directory).map_err(|source| CorpusError::Io {
            path: directory.to_path_buf(),
            source,
        })?;
    let canonical_tree =
        std::fs::canonicalize(&path).map_err(|source| CorpusError::Io {
            path: path.clone(),
            source,
        })?;
    if !canonical_tree.starts_with(&canonical_directory) {
        return Err(CorpusError::InvalidCase {
            path,
            message: "declared tree resolves outside its case directory".to_owned(),
        });
    }
    Ok(canonical_tree)
}

fn collect_tree_files(
    directory: &Path,
    case_root: &Path,
    tree_root: &Path,
    visited: &mut BTreeSet<PathBuf>,
    files: &mut Vec<(String, PathBuf)>,
) -> Result<(), CorpusError> {
    let canonical_directory =
        std::fs::canonicalize(directory).map_err(|source| CorpusError::Io {
            path: directory.to_path_buf(),
            source,
        })?;
    if !canonical_directory.starts_with(case_root)
        || !canonical_directory.starts_with(tree_root)
    {
        return Err(CorpusError::InvalidCase {
            path: directory.to_path_buf(),
            message: "tree entry resolves outside the confined tree".to_owned(),
        });
    }
    if !visited.insert(canonical_directory) {
        return Ok(());
    }
    let mut entries = std::fs::read_dir(directory)
        .map_err(|source| CorpusError::Io {
            path: directory.to_path_buf(),
            source,
        })?
        .map(|entry| {
            entry
                .map_err(|source| CorpusError::Io {
                    path: directory.to_path_buf(),
                    source,
                })
                .map(|entry| entry.path())
        })
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort();
    for path in entries {
        let metadata =
            std::fs::symlink_metadata(&path).map_err(|source| CorpusError::Io {
                path: path.clone(),
                source,
            })?;
        if metadata.file_type().is_symlink() {
            let canonical =
                std::fs::canonicalize(&path).map_err(|source| CorpusError::Io {
                    path: path.clone(),
                    source,
                })?;
            if !canonical.starts_with(case_root) || !canonical.starts_with(tree_root) {
                return Err(CorpusError::InvalidCase {
                    path,
                    message: "tree link resolves outside its confined tree".to_owned(),
                });
            }
            let target_metadata =
                std::fs::metadata(&path).map_err(|source| CorpusError::Io {
                    path: path.clone(),
                    source,
                })?;
            if target_metadata.is_dir() {
                collect_tree_files(&path, case_root, tree_root, visited, files)?;
            } else if target_metadata.is_file() {
                files.push((relative_id(case_root, &path), canonical));
            }
        } else if metadata.is_dir() {
            collect_tree_files(&path, case_root, tree_root, visited, files)?;
        } else if metadata.is_file() {
            let canonical =
                std::fs::canonicalize(&path).map_err(|source| CorpusError::Io {
                    path: path.clone(),
                    source,
                })?;
            if !canonical.starts_with(case_root) || !canonical.starts_with(tree_root) {
                return Err(CorpusError::InvalidCase {
                    path,
                    message: "tree file resolves outside its confined tree".to_owned(),
                });
            }
            files.push((relative_id(case_root, &path), canonical));
        }
    }
    Ok(())
}

fn relative_id(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

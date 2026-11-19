//! Discovery and safe loading of the conformance corpus.
//!
//! The corpus lives at `conformance/cases`, one directory per section prefix
//! and one directory per case below it:
//! `conformance/cases/<PREFIX>/<case identity>/case.toml`. The rule index,
//! `conformance/rules.tsv`, lies beside the corpus root.

use std::collections::BTreeSet;
use std::fmt;
use std::path::{Component, Path, PathBuf};

use crate::manifest::{CaseManifest, MANIFEST_FILE_NAME, ManifestError};
use crate::rules::{RULES_FILE_NAME, RuleIndex, RuleStatus, RulesError};

/// The workspace root, found from this crate's manifest.
#[must_use]
pub fn workspace_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize().unwrap_or(root)
}

/// The conformance corpus of the workspace: `conformance/cases`.
#[must_use]
pub fn corpus_root() -> PathBuf {
    workspace_root().join("conformance/cases")
}

/// One input document of a case, as exact bytes.
///
/// The path is the case-relative source identity used by diagnostics and
/// snapshots. The bytes are not decoded: an encoding error is the reader's to
/// report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaseInputDocument {
    /// The case-relative document identity.
    pub source_id: String,
    /// The exact bytes on disk.
    pub bytes: Vec<u8>,
}

/// One file acquired from a declared tree input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaseTreeFile {
    /// The case-relative, slash-separated source identity.
    pub source_id: String,
    /// The exact bytes on disk.
    pub bytes: Vec<u8>,
}

/// A loaded case and its directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Case {
    directory: PathBuf,
    manifest_path: PathBuf,
    manifest: CaseManifest,
}

impl Case {
    /// The directory holding this case's inputs and snapshots.
    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// The path of this case's `case.toml`.
    #[must_use]
    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }

    /// The decoded manifest.
    #[must_use]
    pub fn manifest(&self) -> &CaseManifest {
        &self.manifest
    }

    /// Resolves a case-relative path after checking that it is a regular file
    /// inside this case directory.
    pub fn file(&self, relative: &str) -> Result<PathBuf, CorpusError> {
        resolve_file(&self.directory, relative, &self.manifest.id)
    }

    /// Reads a case-relative file as exact bytes.
    pub fn read_bytes(&self, relative: &str) -> Result<Vec<u8>, CorpusError> {
        let path = self.file(relative)?;
        std::fs::read(&path).map_err(|source| CorpusError::Io { path, source })
    }

    /// Reads a case-relative UTF-8 file.
    pub fn read_file(&self, relative: &str) -> Result<String, CorpusError> {
        let path = self.file(relative)?;
        std::fs::read_to_string(&path)
            .map_err(|source| CorpusError::Io { path, source })
    }

    /// Every declared document input, in the order source, project, data.
    pub fn input_documents(&self) -> Result<Vec<CaseInputDocument>, CorpusError> {
        let inputs = &self.manifest.inputs;
        inputs
            .source
            .iter()
            .chain(&inputs.project)
            .chain(&inputs.data)
            .map(|source_id| {
                Ok(CaseInputDocument {
                    source_id: source_id.clone(),
                    bytes: self.read_bytes(source_id)?,
                })
            })
            .collect()
    }

    /// The canonical directory of the declared tree input, after checking
    /// that it is a regular directory inside this case directory and not a
    /// link; `None` when the case declares no tree.
    pub fn tree_directory(&self) -> Result<Option<PathBuf>, CorpusError> {
        self.manifest
            .inputs
            .tree
            .as_deref()
            .map(|relative| {
                resolve_directory(&self.directory, relative, &self.manifest.id)
            })
            .transpose()
    }

    /// Acquires every regular file under the declared tree input, in stable
    /// path order. Link targets must stay inside both the tree and the case
    /// directory; canonical directory identities suppress aliases and cycles.
    pub fn tree_files(&self) -> Result<Vec<CaseTreeFile>, CorpusError> {
        let Some(relative) = self.manifest.inputs.tree.as_deref() else {
            return Ok(Vec::new());
        };
        let tree = resolve_directory(&self.directory, relative, &self.manifest.id)?;
        let case_root = canonical(&self.directory)?;
        let mut files = Vec::new();
        let mut visited = BTreeSet::new();
        collect_tree_files(&tree, &case_root, &tree, &mut visited, &mut files)?;
        files.sort_by(|left, right| left.0.cmp(&right.0));
        files
            .into_iter()
            .map(|(source_id, path)| {
                let bytes = std::fs::read(&path)
                    .map_err(|source| CorpusError::Io { path, source })?;
                Ok(CaseTreeFile { source_id, bytes })
            })
            .collect()
    }
}

/// A deterministic collection of loaded cases.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Corpus {
    root: PathBuf,
    cases: Vec<Case>,
}

impl Corpus {
    /// Discovers every case under `root`.
    ///
    /// Every entry of `root` is a prefix directory, every entry of a prefix
    /// directory is a case directory, and every case directory holds a
    /// `case.toml`. A case's directory is named by its identity and lies in
    /// the directory of its identity's prefix. When the corpus has a case,
    /// the rule index `rules.tsv` beside `root` must list the case's rule as
    /// `active`.
    pub fn discover(root: impl AsRef<Path>) -> Result<Self, CorpusError> {
        let root = root.as_ref();
        let metadata = std::fs::metadata(root).map_err(|source| CorpusError::Io {
            path: root.to_path_buf(),
            source,
        })?;
        if !metadata.is_dir() {
            return Err(CorpusError::InvalidRoot(root.to_path_buf()));
        }
        let root = canonical(root)?;

        let mut found = Vec::new();
        for prefix_directory in sorted_entries(&root)? {
            if !is_directory_inside(&prefix_directory, &root)? {
                return Err(CorpusError::InvalidCase {
                    path: prefix_directory,
                    message: "the corpus root holds only prefix directories".to_owned(),
                });
            }
            for case_directory in sorted_entries(&prefix_directory)? {
                if !is_directory_inside(&case_directory, &root)? {
                    return Err(CorpusError::InvalidCase {
                        path: case_directory,
                        message: "a prefix directory holds only case directories"
                            .to_owned(),
                    });
                }
                let manifest_path = case_directory.join(MANIFEST_FILE_NAME);
                if !manifest_path.is_file() {
                    return Err(CorpusError::InvalidCase {
                        path: case_directory,
                        message: format!(
                            "the case directory has no {MANIFEST_FILE_NAME}"
                        ),
                    });
                }
                found.push((prefix_directory.clone(), case_directory, manifest_path));
            }
        }
        if found.is_empty() {
            return Ok(Self {
                root,
                cases: Vec::new(),
            });
        }

        let rules_path = root.parent().map_or_else(
            || PathBuf::from(RULES_FILE_NAME),
            |parent| parent.join(RULES_FILE_NAME),
        );
        let rules =
            RuleIndex::from_path(&rules_path).map_err(|source| CorpusError::Rules {
                path: rules_path.clone(),
                source,
            })?;

        let mut cases = Vec::with_capacity(found.len());
        for (prefix_directory, directory, manifest_path) in found {
            let manifest =
                CaseManifest::from_path(&manifest_path).map_err(|source| {
                    CorpusError::Manifest {
                        path: manifest_path.clone(),
                        source,
                    }
                })?;
            validate_layout(&prefix_directory, &directory, &manifest)?;
            validate_rule(&rules, &directory, &manifest)?;
            validate_declared_files(&directory, &manifest)?;
            cases.push(Case {
                directory,
                manifest_path,
                manifest,
            });
        }
        cases.sort_by(|left, right| left.manifest.id.cmp(&right.manifest.id));
        for pair in cases.windows(2) {
            if let [left, right] = pair
                && left.manifest.id == right.manifest.id
            {
                return Err(CorpusError::DuplicateCaseId(left.manifest.id.clone()));
            }
        }
        Ok(Self { root, cases })
    }

    /// The canonical corpus root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The loaded cases, ordered by identity.
    #[must_use]
    pub fn cases(&self) -> &[Case] {
        &self.cases
    }

    /// The number of loaded cases.
    #[must_use]
    pub fn len(&self) -> usize {
        self.cases.len()
    }

    /// Whether the corpus has no case.
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

/// A corpus discovery or layout failure.
#[derive(Debug)]
pub enum CorpusError {
    /// The root exists but is not a directory.
    InvalidRoot(PathBuf),
    /// A filesystem operation failed.
    Io {
        /// The path involved.
        path: PathBuf,
        /// The underlying failure.
        source: std::io::Error,
    },
    /// A case manifest could not be decoded.
    Manifest {
        /// The manifest path.
        path: PathBuf,
        /// The decoding failure.
        source: ManifestError,
    },
    /// The rule index could not be decoded.
    Rules {
        /// The index path.
        path: PathBuf,
        /// The decoding failure.
        source: RulesError,
    },
    /// A case violates the corpus layout or addresses an inactive rule.
    InvalidCase {
        /// The case or directory path.
        path: PathBuf,
        /// A human-readable explanation.
        message: String,
    },
    /// Two directories use one case identity.
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
            Self::Rules { path, source } => {
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
            Self::Rules { source, .. } => Some(source),
            Self::InvalidRoot(_)
            | Self::InvalidCase { .. }
            | Self::DuplicateCaseId(_) => None,
        }
    }
}

fn canonical(path: &Path) -> Result<PathBuf, CorpusError> {
    std::fs::canonicalize(path).map_err(|source| CorpusError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn sorted_entries(directory: &Path) -> Result<Vec<PathBuf>, CorpusError> {
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
    Ok(entries)
}

/// Whether `path` is a directory, after checking that it resolves inside
/// `root`.
fn is_directory_inside(path: &Path, root: &Path) -> Result<bool, CorpusError> {
    let resolved = canonical(path)?;
    if !resolved.starts_with(root) {
        return Err(CorpusError::InvalidCase {
            path: path.to_path_buf(),
            message: "the entry resolves outside the corpus root".to_owned(),
        });
    }
    Ok(resolved.is_dir())
}

fn file_name(path: &Path) -> &str {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
}

/// A case directory is named by the case identity and lies in the directory
/// of its identity's prefix.
fn validate_layout(
    prefix_directory: &Path,
    directory: &Path,
    manifest: &CaseManifest,
) -> Result<(), CorpusError> {
    let prefix = file_name(prefix_directory);
    if prefix != manifest.prefix() {
        return Err(CorpusError::InvalidCase {
            path: directory.to_path_buf(),
            message: format!(
                "case `{}` lies in the prefix directory `{prefix}`, but its identity's \
                 prefix is `{}`",
                manifest.id,
                manifest.prefix()
            ),
        });
    }
    if file_name(directory) != manifest.id {
        return Err(CorpusError::InvalidCase {
            path: directory.to_path_buf(),
            message: format!(
                "the directory name must equal the case id `{}`",
                manifest.id
            ),
        });
    }
    Ok(())
}

/// A case may address only a rule the index lists as active.
fn validate_rule(
    rules: &RuleIndex,
    directory: &Path,
    manifest: &CaseManifest,
) -> Result<(), CorpusError> {
    let problem = match rules.status(&manifest.rule) {
        Some(RuleStatus::Active) => return Ok(()),
        Some(RuleStatus::Retired) => "which the rule index lists as retired",
        None => "which the rule index does not list",
    };
    Err(CorpusError::InvalidCase {
        path: directory.to_path_buf(),
        message: format!(
            "case `{}` addresses rule `{}`, {problem}",
            manifest.id, manifest.rule
        ),
    })
}

fn validate_declared_files(
    directory: &Path,
    manifest: &CaseManifest,
) -> Result<(), CorpusError> {
    let inputs = &manifest.inputs;
    for file in inputs
        .source
        .iter()
        .chain(&inputs.project)
        .chain(&inputs.data)
        .chain(&inputs.host_world)
    {
        resolve_file(directory, file, &manifest.id)?;
    }
    if let Some(tree) = &inputs.tree {
        resolve_directory(directory, tree, &manifest.id)?;
    }
    for (_, snapshot) in manifest.expectations.snapshots() {
        resolve_file(directory, snapshot, &manifest.id)?;
    }
    Ok(())
}

fn is_relative_path(relative: &str, current_directory_allowed: bool) -> bool {
    let path = Path::new(relative);
    !relative.is_empty()
        && !path.is_absolute()
        && !path.components().any(|component| match component {
            Component::CurDir => !current_directory_allowed,
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => true,
            Component::Normal(_) => false,
        })
}

fn resolve_file(
    directory: &Path,
    relative: &str,
    case_id: &str,
) -> Result<PathBuf, CorpusError> {
    if !is_relative_path(relative, true) {
        return Err(CorpusError::InvalidCase {
            path: directory.to_path_buf(),
            message: format!(
                "case `{case_id}` uses a non-relative file path `{relative}`"
            ),
        });
    }
    let path = directory.join(relative);
    let metadata = std::fs::metadata(&path).map_err(|source| CorpusError::Io {
        path: path.clone(),
        source,
    })?;
    if !metadata.is_file() {
        return Err(CorpusError::InvalidCase {
            path,
            message: "a declared input or snapshot is not a regular file".to_owned(),
        });
    }
    let file = canonical(&path)?;
    if !file.starts_with(canonical(directory)?) {
        return Err(CorpusError::InvalidCase {
            path,
            message: "a declared file resolves outside its case directory".to_owned(),
        });
    }
    Ok(file)
}

fn resolve_directory(
    directory: &Path,
    relative: &str,
    case_id: &str,
) -> Result<PathBuf, CorpusError> {
    if !is_relative_path(relative, false) {
        return Err(CorpusError::InvalidCase {
            path: directory.to_path_buf(),
            message: format!(
                "case `{case_id}` uses a non-relative tree path `{relative}`"
            ),
        });
    }
    let path = directory.join(relative);
    let metadata =
        std::fs::symlink_metadata(&path).map_err(|source| CorpusError::Io {
            path: path.clone(),
            source,
        })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(CorpusError::InvalidCase {
            path,
            message: "a declared tree is not a regular directory or is a link"
                .to_owned(),
        });
    }
    let tree = canonical(&path)?;
    if !tree.starts_with(canonical(directory)?) {
        return Err(CorpusError::InvalidCase {
            path,
            message: "a declared tree resolves outside its case directory".to_owned(),
        });
    }
    Ok(tree)
}

fn collect_tree_files(
    directory: &Path,
    case_root: &Path,
    tree_root: &Path,
    visited: &mut BTreeSet<PathBuf>,
    files: &mut Vec<(String, PathBuf)>,
) -> Result<(), CorpusError> {
    let resolved = canonical(directory)?;
    if !resolved.starts_with(case_root) || !resolved.starts_with(tree_root) {
        return Err(CorpusError::InvalidCase {
            path: directory.to_path_buf(),
            message: "a tree entry resolves outside the confined tree".to_owned(),
        });
    }
    if !visited.insert(resolved) {
        return Ok(());
    }
    for path in sorted_entries(directory)? {
        let target = canonical(&path)?;
        if !target.starts_with(case_root) || !target.starts_with(tree_root) {
            return Err(CorpusError::InvalidCase {
                path,
                message: "a tree entry resolves outside its confined tree".to_owned(),
            });
        }
        if target.is_dir() {
            collect_tree_files(&path, case_root, tree_root, visited, files)?;
        } else if target.is_file() {
            files.push((relative_id(case_root, &path), target));
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

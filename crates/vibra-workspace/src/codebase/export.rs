//! Revision-pinned source export into an absent destination tree.

use std::collections::BTreeSet;
use std::io;
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};
use vibra_ir::canonical_string;

use crate::codebase::store::{
    CodebaseStore, SourceFile, StoreError, map_path_error, published_sync_error,
    sync_directory_if_supported, temporary_file_name, validate_store_path,
};
use crate::confined_fs::ConfinedDir;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PublicationCheckpoint {
    BeforePublish,
    AfterPublish,
}

/// One source file in an export preview.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportFile {
    /// Project-relative output path using `/` separators.
    pub path: String,
    /// Raw SHA-256 address of the exact source bytes.
    pub source_hash: String,
    /// Exact number of source bytes.
    pub byte_length: u64,
}

/// A deterministic, absent-destination export plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportPlan {
    /// Selected immutable revision.
    pub revision: String,
    /// Normalized workspace-relative destination.
    pub destination: String,
    /// Address of the canonical plan record.
    pub plan: String,
    /// Sorted exact file list.
    pub files: Vec<ExportFile>,
}

/// Result of previewing or applying a source export.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportResult {
    /// Selected immutable revision.
    pub revision: String,
    /// Normalized workspace-relative destination.
    pub destination: String,
    /// Address of the canonical plan record.
    pub plan: String,
    /// Sorted exact file list.
    pub files: Vec<ExportFile>,
    /// Whether files were written and the tree published.
    pub written: bool,
}

impl CodebaseStore {
    /// Computes a source-export plan without writing to the store or destination.
    pub fn preview_export(
        &self,
        revision: &str,
        destination: impl AsRef<Path>,
    ) -> Result<ExportPlan, StoreError> {
        build_plan(self, revision, destination.as_ref(), false)
    }

    /// Writes a verified plan to a private sibling tree and publishes it once.
    pub fn apply_export(
        &self,
        revision: &str,
        destination: impl AsRef<Path>,
        expected_plan: &str,
    ) -> Result<ExportResult, StoreError> {
        self.apply_export_with_checkpoint(revision, destination, expected_plan, |_| {})
    }

    fn apply_export_with_checkpoint(
        &self,
        revision: &str,
        destination: impl AsRef<Path>,
        expected_plan: &str,
        mut checkpoint: impl FnMut(PublicationCheckpoint),
    ) -> Result<ExportResult, StoreError> {
        if !valid_address(expected_plan) {
            return Err(StoreError::InvalidInput(format!(
                "invalid export plan address {expected_plan:?}"
            )));
        }
        let plan = build_plan(self, revision, destination.as_ref(), true)?;
        if plan.plan != expected_plan {
            return Err(StoreError::StalePlan(
                "export plan no longer matches the selected revision and destination"
                    .to_owned(),
            ));
        }
        let (_, relative, parent_relative, file_name) =
            validate_store_path(&self.workspace_root, destination.as_ref())?;
        let root = ConfinedDir::open(&self.workspace_root).map_err(|error| {
            map_path_error(&self.workspace_root, error, "cannot open workspace safely")
        })?;
        let parent = root.open_dir(&parent_relative).map_err(|error| {
            map_path_error(
                &relative,
                error,
                "export destination parent must already exist",
            )
        })?;
        if parent.child_exists(&file_name).map_err(|error| {
            map_path_error(&relative, error, "cannot inspect export destination")
        })? {
            return Err(StoreError::StalePlan(format!(
                "export destination appeared after preview: {}",
                relative.display()
            )));
        }

        let sources = self.revision_sources(revision)?;
        let stage_name = temporary_file_name("export");
        parent.create_dir(&stage_name).map_err(|error| {
            map_path_error(&relative, error, "cannot create export staging tree")
        })?;
        let stage = parent.open_dir(Path::new(&stage_name)).map_err(|error| {
            map_path_error(&relative, error, "cannot open export staging tree")
        })?;
        write_export_tree(&stage, &sources)?;
        sync_directory_if_supported(&stage).map_err(|error| {
            map_path_error(&relative, error, "cannot flush export staging tree")
        })?;
        drop(stage);
        if parent.child_exists(&file_name).map_err(|error| {
            map_path_error(&relative, error, "cannot recheck export destination")
        })? {
            return Err(StoreError::StalePlan(format!(
                "export destination appeared after preview: {}",
                relative.display()
            )));
        }
        checkpoint(PublicationCheckpoint::BeforePublish);
        match parent.rename_to_new(&stage_name, &parent, &file_name) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                return Err(StoreError::StalePlan(format!(
                    "export destination appeared before publication: {}",
                    relative.display()
                )));
            }
            Err(error) => {
                return Err(map_path_error(
                    &relative,
                    error,
                    "cannot publish export tree",
                ));
            }
        }
        checkpoint(PublicationCheckpoint::AfterPublish);
        sync_directory_if_supported(&parent)
            .map_err(|error| published_sync_error(&relative, "export tree", error))?;
        Ok(ExportResult {
            revision: plan.revision,
            destination: plan.destination,
            plan: plan.plan,
            files: plan.files,
            written: true,
        })
    }
}

fn build_plan(
    store: &CodebaseStore,
    revision: &str,
    destination: &Path,
    applying: bool,
) -> Result<ExportPlan, StoreError> {
    if !valid_address(revision) {
        return Err(StoreError::InvalidInput(format!(
            "invalid revision address {revision:?}"
        )));
    }
    let (_, relative, parent_relative, file_name) =
        validate_store_path(&store.workspace_root, destination)?;
    let root = ConfinedDir::open(&store.workspace_root).map_err(|error| {
        map_path_error(&store.workspace_root, error, "cannot open workspace safely")
    })?;
    let parent = root.open_dir(&parent_relative).map_err(|error| {
        map_path_error(
            &relative,
            error,
            "export destination parent must already exist",
        )
    })?;
    if parent.child_exists(&file_name).map_err(|error| {
        map_path_error(&relative, error, "cannot inspect export destination")
    })? {
        return if applying {
            Err(StoreError::StalePlan(format!(
                "export destination appeared after preview: {}",
                relative.display()
            )))
        } else {
            Err(StoreError::UnsafePath(format!(
                "export destination already exists: {}",
                relative.display()
            )))
        };
    }
    let sources = store.revision_sources(revision)?;
    let mut seen_paths = BTreeSet::new();
    let mut files = Vec::with_capacity(sources.len());
    for source in &sources {
        let path = normalize_export_file(&source.origin)?;
        if !seen_paths.insert(path.clone()) {
            return Err(StoreError::Corrupt(format!(
                "revision contains duplicate export path {path}"
            )));
        }
        files.push(ExportFile {
            path,
            source_hash: source.raw_hash.clone(),
            byte_length: u64::try_from(source.text.len()).map_err(|_| {
                StoreError::Corrupt("source length exceeds u64".to_owned())
            })?,
        });
    }
    files.sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));
    let destination_text = path_text(&relative)?;
    let payload = canonical_plan(revision, &destination_text, &files);
    let digest = hash_plan(&payload);
    Ok(ExportPlan {
        revision: revision.to_owned(),
        destination: destination_text,
        plan: digest,
        files,
    })
}

fn canonical_plan(revision: &str, destination: &str, files: &[ExportFile]) -> Vec<u8> {
    let mut rendered_files = Vec::with_capacity(files.len());
    for file in files {
        rendered_files.push(format!(
            "(record path: {} source-hash: {} byte-length: {}u64)",
            canonical_string(&file.path),
            canonical_string(&file.source_hash),
            file.byte_length
        ));
    }
    let files = if rendered_files.is_empty() {
        "(array)".to_owned()
    } else {
        format!("(array {})", rendered_files.join(" "))
    };
    format!(
        "(record format: @codebase-export-plan.v1 revision: {} destination: {} expected: @absent files: {})\n",
        canonical_string(revision),
        canonical_string(destination),
        files
    )
    .into_bytes()
}

fn hash_plan(payload: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"vibra-codebase-export-plan-v1\0");
    hasher.update(payload);
    let digest = hasher.finalize();
    let mut address = String::from("sha256:");
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(address, "{byte:02x}");
    }
    address
}

fn valid_address(address: &str) -> bool {
    let Some(hex) = address.strip_prefix("sha256:") else {
        return false;
    };
    hex.len() == 64
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn path_text(path: &Path) -> Result<String, StoreError> {
    path.to_str()
        .map(|path| path.replace('\\', "/"))
        .ok_or_else(|| {
            StoreError::UnsafePath("export path is not valid Unicode".to_owned())
        })
}

fn normalize_export_file(origin: &str) -> Result<String, StoreError> {
    if origin.contains('\\') || origin.is_empty() {
        return Err(StoreError::Corrupt(format!(
            "unsafe source export origin {origin:?}"
        )));
    }
    let path = Path::new(origin);
    if path.is_absolute()
        || origin
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(StoreError::Corrupt(format!(
            "unsafe source export origin {origin:?}"
        )));
    }
    path.to_str()
        .map(|path| path.replace('\\', "/"))
        .ok_or_else(|| {
            StoreError::Corrupt("source export path is not valid Unicode".to_owned())
        })
}

fn write_export_tree(
    stage: &ConfinedDir,
    sources: &[SourceFile],
) -> Result<(), StoreError> {
    let mut directories = BTreeSet::<PathBuf>::new();
    directories.insert(PathBuf::new());
    for source in sources {
        let origin = normalize_export_file(&source.origin)?;
        let path = Path::new(&origin);
        let name = path.file_name().ok_or_else(|| {
            StoreError::Corrupt(format!("source path has no file name: {origin}"))
        })?;
        let parent_relative = path.parent().unwrap_or_else(|| Path::new(""));
        let parent = ensure_directories(stage, parent_relative, &mut directories)?;
        parent
            .write_new_file(name, &source.text, None)
            .map_err(|error| {
                map_path_error(path, error, "cannot write staged source file")
            })?;
    }
    let mut directories = directories.into_iter().collect::<Vec<_>>();
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for path in directories {
        let directory = stage.open_dir(&path).map_err(|error| {
            map_path_error(&path, error, "cannot open staged export directory")
        })?;
        sync_directory_if_supported(&directory).map_err(|error| {
            map_path_error(&path, error, "cannot flush staged export directory")
        })?;
    }
    Ok(())
}

fn ensure_directories(
    root: &ConfinedDir,
    relative: &Path,
    created: &mut BTreeSet<PathBuf>,
) -> Result<ConfinedDir, StoreError> {
    let mut current = root.open_dir("").map_err(|error| {
        map_path_error(root.path(), error, "cannot open export stage root")
    })?;
    let mut path = PathBuf::new();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err(StoreError::UnsafePath(format!(
                "invalid export path component in {}",
                relative.display()
            )));
        };
        path.push(name);
        if !current.child_exists(name).map_err(|error| {
            map_path_error(&path, error, "cannot inspect staged export path")
        })? {
            current.create_dir(name).map_err(|error| {
                map_path_error(&path, error, "cannot create staged export directory")
            })?;
        }
        current = current.open_dir(Path::new(name)).map_err(|error| {
            map_path_error(&path, error, "cannot open staged export directory")
        })?;
        created.insert(path.clone());
    }
    Ok(current)
}

#[cfg(test)]
mod tests;

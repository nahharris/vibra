//! Deterministic, confined source acquisition.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};

use crate::WorkspaceError;
use crate::discovery::DiscoveredProject;
use crate::project::{Target, TargetKind};

/// One validated local target root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetRoot {
    target_index: usize,
    name: String,
    kind: TargetKind,
    path: PathBuf,
    root_value: String,
    span: ByteSpan,
    source_id: String,
}

impl TargetRoot {
    /// Index of the target in project source order.
    #[must_use]
    pub const fn target_index(&self) -> usize {
        self.target_index
    }

    /// Target unit name without its `@` marker.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Target kind.
    #[must_use]
    pub const fn kind(&self) -> TargetKind {
        self.kind
    }

    /// Canonical target root on disk.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Original project root string.
    #[must_use]
    pub fn root_value(&self) -> &str {
        &self.root_value
    }

    /// Span of the target's root field value.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    /// Source identity of the project containing the root field.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }
}

/// One exact source file retained by an immutable snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceDocument {
    source_id: String,
    relative_path: String,
    module_segments: Vec<String>,
    bytes: Vec<u8>,
}

impl SourceDocument {
    /// Stable project-relative source identity using `/` separators.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// Alias for [`Self::source_id`].
    #[must_use]
    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }

    /// Target-relative dotted module components.
    #[must_use]
    pub fn module_segments(&self) -> &[String] {
        &self.module_segments
    }

    /// Exact bytes read from the source file.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// One local target and the source files found under its root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceUnitSnapshot {
    target_index: usize,
    name: String,
    kind: TargetKind,
    root: PathBuf,
    modules: Vec<SourceDocument>,
}

impl SourceUnitSnapshot {
    /// Index of the target in project source order.
    #[must_use]
    pub const fn target_index(&self) -> usize {
        self.target_index
    }

    /// Unit name without its `@` marker.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Target kind.
    #[must_use]
    pub const fn kind(&self) -> TargetKind {
        self.kind
    }

    /// Canonical target root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Source modules in deterministic project-relative order.
    #[must_use]
    pub fn modules(&self) -> &[SourceDocument] {
        &self.modules
    }
}

/// Immutable source acquisition for one discovered project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceSnapshot {
    project_root: PathBuf,
    project_path: PathBuf,
    units: Vec<SourceUnitSnapshot>,
}

impl SourceSnapshot {
    /// Validates roots and captures all local `.vib` modules.
    pub fn capture(project: &DiscoveredProject) -> Result<Self, WorkspaceError> {
        let roots = validate_target_roots(project)?;
        let mut units = Vec::with_capacity(roots.len());
        for root in roots {
            let modules = collect_modules(project.root(), &root)?;
            units.push(SourceUnitSnapshot {
                target_index: root.target_index,
                name: root.name,
                kind: root.kind,
                root: root.path,
                modules,
            });
        }
        Ok(Self {
            project_root: project.root().to_path_buf(),
            project_path: project.project_path().to_path_buf(),
            units,
        })
    }

    /// Canonical project root used for this snapshot.
    #[must_use]
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    /// Canonical project marker path.
    #[must_use]
    pub fn project_path(&self) -> &Path {
        &self.project_path
    }

    /// Local target units in project source order.
    #[must_use]
    pub fn units(&self) -> &[SourceUnitSnapshot] {
        &self.units
    }

    /// All source documents in deterministic unit and path order.
    pub fn documents(&self) -> impl Iterator<Item = &SourceDocument> {
        self.units.iter().flat_map(|unit| unit.modules.iter())
    }
}

/// Captures an already discovered project into an immutable source snapshot.
pub fn capture_snapshot(
    project: &DiscoveredProject,
) -> Result<SourceSnapshot, WorkspaceError> {
    SourceSnapshot::capture(project)
}

fn validate_target_roots(
    project: &DiscoveredProject,
) -> Result<Vec<TargetRoot>, WorkspaceError> {
    let mut roots = Vec::with_capacity(project.project().targets().len());
    let mut diagnostics = Vec::new();
    for (target_index, target) in project.project().targets().iter().enumerate() {
        match validate_target_root(project, target_index, target) {
            Ok(root) => roots.push(root),
            Err(error) => diagnostics.extend(error.diagnostics().iter().cloned()),
        }
    }
    if !diagnostics.is_empty() {
        return Err(WorkspaceError::new(
            "one or more target roots are invalid",
            diagnostics,
        ));
    }

    for later_index in 0..roots.len() {
        let Some(later) = roots.get(later_index) else {
            continue;
        };
        for earlier in roots.iter().take(later_index) {
            if later.path.starts_with(&earlier.path)
                || earlier.path.starts_with(&later.path)
            {
                diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::ProjectOverlappingTargetRoots,
                        later.span,
                        "target roots must be pairwise disjoint",
                    )
                    .with_source_id(later.source_id.clone())
                    .with_related_source(
                        earlier.source_id.clone(),
                        earlier.span,
                        "an earlier target root overlaps this root",
                    ),
                );
                break;
            }
        }
    }
    if !diagnostics.is_empty() {
        return Err(WorkspaceError::new("target roots overlap", diagnostics));
    }
    Ok(roots)
}

fn validate_target_root(
    project: &DiscoveredProject,
    target_index: usize,
    target: &Target,
) -> Result<TargetRoot, WorkspaceError> {
    let value = target.root().value();
    let path = Path::new(value);
    if value.is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::CurDir
                    | Component::ParentDir
                    | Component::RootDir
                    | Component::Prefix(_)
            )
        })
    {
        return Err(invalid_root(
            project,
            target,
            "target root must be a relative path",
        ));
    }
    let lexical = project.root().join(path);
    let metadata = fs::symlink_metadata(&lexical).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            invalid_root(project, target, "target root does not exist")
        } else {
            root_io_error(
                project,
                target,
                format!("cannot inspect target root: {error}"),
            )
        }
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(invalid_root(
            project,
            target,
            "target root must be a regular directory and cannot be a link",
        ));
    }
    let canonical = fs::canonicalize(&lexical).map_err(|error| {
        root_io_error(
            project,
            target,
            format!("cannot canonicalize target root: {error}"),
        )
    })?;
    if !canonical.starts_with(project.root()) {
        return Err(invalid_root(
            project,
            target,
            "target root resolves outside the project",
        ));
    }
    Ok(TargetRoot {
        target_index,
        name: target.name().atom().value().to_owned(),
        kind: target.kind(),
        path: canonical,
        root_value: value.to_owned(),
        span: target.root().span(),
        source_id: target.origin().source_id().to_owned(),
    })
}

fn invalid_root(
    project: &DiscoveredProject,
    target: &Target,
    message: &str,
) -> WorkspaceError {
    WorkspaceError::new(
        message,
        vec![
            Diagnostic::new(
                DiagnosticCode::ProjectInvalidTargetRoot,
                target.root().span(),
                message,
            )
            .with_source_id(project.project().origin().source_id()),
        ],
    )
}

fn root_io_error(
    project: &DiscoveredProject,
    target: &Target,
    message: String,
) -> WorkspaceError {
    WorkspaceError::new(
        message.clone(),
        vec![
            Diagnostic::new(
                DiagnosticCode::ProjectIoError,
                target.root().span(),
                message.clone(),
            )
            .with_source_id(project.project().origin().source_id())
            .with_note(format!("target root: {}", target.root().value())),
        ],
    )
}

#[derive(Clone, Debug)]
struct CandidateFile {
    canonical: PathBuf,
    source_id: String,
    module_segments: Vec<String>,
}

#[derive(Debug)]
struct ModuleCollector<'a> {
    project_root: &'a Path,
    target_root: &'a TargetRoot,
    files: BTreeMap<PathBuf, CandidateFile>,
    directories: BTreeMap<Vec<String>, String>,
    visited_directories: BTreeSet<PathBuf>,
    active_directories: Vec<PathBuf>,
}

fn collect_modules(
    project_root: &Path,
    target_root: &TargetRoot,
) -> Result<Vec<SourceDocument>, WorkspaceError> {
    let mut collector = ModuleCollector {
        project_root,
        target_root,
        files: BTreeMap::new(),
        directories: BTreeMap::new(),
        visited_directories: BTreeSet::new(),
        active_directories: Vec::new(),
    };
    collector.walk(&target_root.path, Vec::new())?;

    let mut diagnostics = Vec::new();
    for candidate in collector.files.values() {
        if let Some(directory_source_id) =
            collector.directories.get(&candidate.module_segments)
        {
            diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::ModuleFileDirectoryCollision,
                    ByteSpan::empty_at(0),
                    "a module path is claimed by both a file and a directory",
                )
                .with_source_id(candidate.source_id.clone())
                .with_related_source(
                    directory_source_id.clone(),
                    ByteSpan::empty_at(0),
                    "the same module path is also a directory",
                ),
            );
        }
    }
    if !diagnostics.is_empty() {
        return Err(WorkspaceError::new(
            "source layout has a file/directory module collision",
            diagnostics,
        ));
    }

    let mut candidates = collector.files.into_values().collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.source_id.cmp(&right.source_id));
    let mut modules = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let bytes = fs::read(&candidate.canonical).map_err(|error| {
            source_io_error(
                &candidate.source_id,
                format!("cannot read source module: {error}"),
            )
        })?;
        modules.push(SourceDocument {
            source_id: candidate.source_id.clone(),
            relative_path: candidate.source_id,
            module_segments: candidate.module_segments,
            bytes,
        });
    }
    Ok(modules)
}

impl ModuleCollector<'_> {
    fn walk(
        &mut self,
        directory: &Path,
        module_segments: Vec<String>,
    ) -> Result<(), WorkspaceError> {
        let canonical_directory = fs::canonicalize(directory).map_err(|error| {
            source_io_error(
                &relative_id(self.project_root, directory),
                format!("cannot canonicalize source directory: {error}"),
            )
        })?;
        if !canonical_directory.starts_with(self.project_root)
            || !canonical_directory.starts_with(&self.target_root.path)
        {
            return Err(source_escape_error(
                &relative_id(self.project_root, directory),
                "source directory resolves outside its confined target root",
            ));
        }
        if self.active_directories.contains(&canonical_directory) {
            return Err(source_escape_error(
                &relative_id(self.project_root, directory),
                "source directory link forms a cycle",
            ));
        }
        if !self.visited_directories.insert(canonical_directory.clone()) {
            return Ok(());
        }
        self.active_directories.push(canonical_directory);
        let result = self.walk_entries(directory, module_segments);
        self.active_directories.pop();
        result
    }

    fn walk_entries(
        &mut self,
        directory: &Path,
        module_segments: Vec<String>,
    ) -> Result<(), WorkspaceError> {
        let mut entries = fs::read_dir(directory)
            .map_err(|error| {
                source_io_error(
                    &relative_id(self.project_root, directory),
                    format!("cannot enumerate source directory: {error}"),
                )
            })?
            .map(|entry| {
                entry.map_err(|error| {
                    source_io_error(
                        &relative_id(self.project_root, directory),
                        format!("cannot read source directory entry: {error}"),
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|entry| entry.file_name());

        for entry in entries {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_str().ok_or_else(|| {
                source_io_error(
                    &relative_id(self.project_root, &path),
                    "source path is not valid UTF-8".to_owned(),
                )
            })?;
            if name.starts_with('.') {
                continue;
            }
            let metadata = fs::symlink_metadata(&path).map_err(|error| {
                source_io_error(
                    &relative_id(self.project_root, &path),
                    format!("cannot inspect source path: {error}"),
                )
            })?;
            if metadata.file_type().is_symlink() {
                self.walk_link(&path, name, &module_segments)?;
            } else if metadata.is_dir() {
                self.walk_directory(&path, name, &module_segments)?;
            } else if metadata.is_file() {
                self.collect_file(&path, name, &module_segments)?;
            }
        }
        Ok(())
    }

    fn walk_directory(
        &mut self,
        path: &Path,
        name: &str,
        parent_segments: &[String],
    ) -> Result<(), WorkspaceError> {
        validate_segment(self.project_root, path, name)?;
        let mut segments = parent_segments.to_vec();
        segments.push(name.to_owned());
        self.directories
            .entry(segments.clone())
            .or_insert_with(|| relative_id(self.project_root, path));
        self.walk(path, segments)
    }

    fn walk_link(
        &mut self,
        path: &Path,
        name: &str,
        parent_segments: &[String],
    ) -> Result<(), WorkspaceError> {
        let canonical = fs::canonicalize(path).map_err(|error| {
            source_io_error(
                &relative_id(self.project_root, path),
                format!("cannot resolve source link: {error}"),
            )
        })?;
        if !canonical.starts_with(self.project_root)
            || !canonical.starts_with(&self.target_root.path)
        {
            return Err(source_escape_error(
                &relative_id(self.project_root, path),
                "source link resolves outside its confined target root",
            ));
        }
        let metadata = fs::metadata(path).map_err(|error| {
            source_io_error(
                &relative_id(self.project_root, path),
                format!("cannot inspect source link target: {error}"),
            )
        })?;
        if metadata.is_dir() {
            self.walk_directory(path, name, parent_segments)
        } else if metadata.is_file() {
            self.collect_file_with_canonical(path, name, parent_segments, canonical)
        } else {
            Err(source_io_error(
                &relative_id(self.project_root, path),
                "source link target is not a regular file or directory".to_owned(),
            ))
        }
    }

    fn collect_file(
        &mut self,
        path: &Path,
        name: &str,
        parent_segments: &[String],
    ) -> Result<(), WorkspaceError> {
        let canonical = fs::canonicalize(path).map_err(|error| {
            source_io_error(
                &relative_id(self.project_root, path),
                format!("cannot canonicalize source file: {error}"),
            )
        })?;
        if !canonical.starts_with(self.project_root)
            || !canonical.starts_with(&self.target_root.path)
        {
            return Err(source_escape_error(
                &relative_id(self.project_root, path),
                "source file resolves outside its confined target root",
            ));
        }
        self.collect_file_with_canonical(path, name, parent_segments, canonical)
    }

    fn collect_file_with_canonical(
        &mut self,
        path: &Path,
        name: &str,
        parent_segments: &[String],
        canonical: PathBuf,
    ) -> Result<(), WorkspaceError> {
        if Path::new(name)
            .extension()
            .and_then(|extension| extension.to_str())
            != Some("vib")
        {
            return Ok(());
        }
        let Some(stem) = Path::new(name).file_stem().and_then(|stem| stem.to_str())
        else {
            return Err(source_io_error(
                &relative_id(self.project_root, path),
                "source module name is not valid UTF-8".to_owned(),
            ));
        };
        validate_segment(self.project_root, path, stem)?;
        let mut segments = parent_segments.to_vec();
        segments.push(stem.to_owned());
        let source_id = relative_id(self.project_root, path);
        let candidate = CandidateFile {
            canonical,
            source_id: source_id.clone(),
            module_segments: segments,
        };
        match self.files.get(&candidate.canonical) {
            Some(existing) if existing.source_id <= source_id => {}
            _ => {
                self.files.insert(candidate.canonical.clone(), candidate);
            }
        }
        Ok(())
    }
}

fn validate_segment(
    project_root: &Path,
    path: &Path,
    segment: &str,
) -> Result<(), WorkspaceError> {
    if is_kebab_component(segment) {
        return Ok(());
    }
    let source_id = relative_id(project_root, path);
    Err(WorkspaceError::new(
        "source path segment is not one kebab-name component",
        vec![
            Diagnostic::new(
                DiagnosticCode::ModuleInvalidSegment,
                ByteSpan::empty_at(0),
                "source path segment is not one kebab-name component",
            )
            .with_source_id(source_id),
        ],
    ))
}

fn is_kebab_component(segment: &str) -> bool {
    let mut characters = segment.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    first.is_ascii_lowercase()
        && characters.all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '-'
        })
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

fn source_io_error(source_id: &str, message: String) -> WorkspaceError {
    WorkspaceError::new(
        message.clone(),
        vec![
            Diagnostic::new(
                DiagnosticCode::ModuleIoError,
                ByteSpan::empty_at(0),
                message,
            )
            .with_source_id(source_id),
        ],
    )
}

fn source_escape_error(source_id: &str, message: &str) -> WorkspaceError {
    WorkspaceError::new(
        message,
        vec![
            Diagnostic::new(
                DiagnosticCode::ModulePathEscape,
                ByteSpan::empty_at(0),
                message,
            )
            .with_source_id(source_id),
        ],
    )
}

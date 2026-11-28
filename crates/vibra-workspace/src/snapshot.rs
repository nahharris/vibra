//! The confined source snapshot.
//!
//! `docs/spec/04-programs-and-packages.md`, "Discovery and the source
//! snapshot", in the order this module applies it:
//!
//! 1. Unit names and target roots. A target or dependency named `tests` or
//!    `std` is `@project.reservedUnitName`. A root is a non-empty relative
//!    path without `.` or `..` parts whose canonical directory exists inside
//!    the canonical project root, and is not itself a link
//!    (`@project.invalidTargetRoot`). The optional `tests/` root follows the
//!    same rules.
//! 2. Overlap. Canonical roots are pairwise disjoint, the later one is
//!    `@project.overlappingTargetRoots`, and every target root is disjoint
//!    from `tests/`. These checks finish before any source directory is
//!    listed.
//! 3. The walk. A link below a root resolves inside the project root and its
//!    own root (`@module.pathEscape` otherwise, and for a directory cycle); a
//!    dangling or unreadable entry is `@module.ioError`. A canonical
//!    directory is walked once and a linked file is admitted once, at its
//!    first path in sorted order. A regular file is a module only with the
//!    exact extension `.vib`; every name on its path is an identifier that is
//!    not a reserved word (`@module.invalidSegment`). Paths that differ only
//!    by case are `@module.caseCollision`, and `text.vib` beside `text/` is
//!    `@module.fileDirectoryCollision`.
//! 4. Dependencies. After the root's own checks pass, [`crate::dependencies`]
//!    reads the dependency graph; each dependency's unit is walked under the
//!    same rules, with its package's directory as its project root.
//! 5. The bytes. No source byte is read before the walk's checks pass; the
//!    bytes are then copied exactly, sorted by project-relative path. One
//!    source identity in two units is `@module.sourceIdCollision`.
//! 6. Nesting. Each module is measured against the source nesting limit
//!    ([`crate::nesting_fault`]); a module nested past it is kept, with its fault
//!    in [`SourceSnapshot::limit_faults`], so that its text can be shown.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_syntax::{is_reserved_word, parse_source};

use crate::dependencies::{Dependencies, name_faults};
use crate::discovery::DiscoveredProject;
use crate::nesting::{SOURCE_NESTING, nesting_fault};
use crate::project::{PROJECT_FILE_NAME, Package, TargetKind};
use crate::revision::{RevisionKind, RevisionRecord, embedded_toolchain_record};

/// The name of the test unit's root directory, and its source identity.
pub const TESTS_ROOT: &str = "tests";

/// The kind of a unit of the snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UnitKind {
    /// A binary target.
    Bin,
    /// A library target.
    Lib,
    /// The reserved test unit.
    Tests,
    /// The library target of another package, bound by a dependency.
    Dependency,
}

impl UnitKind {
    /// The atom, `@` included.
    #[must_use]
    pub const fn as_atom(self) -> &'static str {
        match self {
            Self::Bin => "@bin",
            Self::Lib => "@lib",
            Self::Tests => "@tests",
            Self::Dependency => "@dependency",
        }
    }
}

/// One captured module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceFile {
    source_id: String,
    path: String,
    bytes: Vec<u8>,
}

impl SourceFile {
    /// The project-relative path with `/` separators.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// The module identity: the unit, then the root-relative path without the
    /// extension, separated by dots.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The exact bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// One unit and its modules, sorted by project-relative path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceUnit {
    name: String,
    kind: UnitKind,
    modules: Vec<SourceFile>,
    package: Option<Package>,
    dependencies: Vec<String>,
}

impl SourceUnit {
    /// The unit name, an atom with its `@`.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The kind.
    #[must_use]
    pub const fn kind(&self) -> UnitKind {
        self.kind
    }

    /// The modules, sorted by project-relative path.
    #[must_use]
    pub fn modules(&self) -> &[SourceFile] {
        &self.modules
    }

    /// The package of a dependency's unit; `None` for a local unit.
    #[must_use]
    pub const fn package(&self) -> Option<&Package> {
        self.package.as_ref()
    }

    /// The dependency units its modules may name: the dependencies its
    /// project declares, `@` included.
    #[must_use]
    pub fn dependencies(&self) -> &[String] {
        &self.dependencies
    }
}

/// The immutable capture of one project: its marker and every module.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceSnapshot {
    project: DiscoveredProject,
    units: Vec<SourceUnit>,
    /// Each dependency's project file, by its path from the project root.
    dependency_files: Vec<(String, Vec<u8>)>,
    unavailable: Vec<Diagnostic>,
    /// `@tool.limitExceeded` for each module nested past the source nesting
    /// limit, in module order.
    limit_faults: Vec<Diagnostic>,
}

impl SourceSnapshot {
    /// Checks the unit names and roots, walks every root and `tests/`, and
    /// copies the modules' bytes.
    pub fn capture(project: DiscoveredProject) -> Result<Self, Vec<Diagnostic>> {
        let mut roots = validate_roots(&project)?;
        let dependencies = crate::dependencies::load(&project)?;
        add_dependency_roots(&mut roots, &dependencies);
        let mut diagnostics = Vec::new();
        let mut walks = Vec::with_capacity(roots.len());
        for root in &roots {
            let mut walk =
                Walk::new(root.project_root.as_deref().unwrap_or(project.root()), root);
            walk.run();
            walk.collisions();
            diagnostics.append(&mut walk.diagnostics);
            walks.push(walk.candidates());
        }
        if !diagnostics.is_empty() {
            return Err(sorted(diagnostics));
        }
        let mut units = Vec::with_capacity(roots.len());
        for (root, candidates) in roots.iter().zip(walks) {
            let mut modules = Vec::with_capacity(candidates.len());
            for candidate in candidates {
                match fs::read(&candidate.canonical) {
                    Ok(bytes) => modules.push(SourceFile {
                        source_id: candidate.source_id,
                        path: module_path(&root.name, &candidate.segments),
                        bytes,
                    }),
                    Err(error) => diagnostics.push(module_fault(
                        DiagnosticCode::ModuleIoError,
                        &candidate.source_id,
                        format!("cannot read the module: {error}"),
                    )),
                }
            }
            units.push(SourceUnit {
                name: root.name.clone(),
                kind: root.kind,
                modules,
                package: root.package.clone(),
                dependencies: root.dependencies.clone(),
            });
        }
        if !diagnostics.is_empty() {
            return Err(sorted(diagnostics));
        }
        if !units.iter().any(|unit| unit.kind == UnitKind::Tests) {
            units.push(SourceUnit {
                name: "@tests".to_owned(),
                kind: UnitKind::Tests,
                modules: Vec::new(),
                package: None,
                dependencies: dependencies.root().to_vec(),
            });
        }
        let collisions = source_id_collisions(&units);
        if !collisions.is_empty() {
            return Err(collisions);
        }
        let limit_faults = limit_faults(&units);
        Ok(Self {
            project,
            units,
            dependency_files: dependencies.files().to_vec(),
            unavailable: dependencies.unavailable().to_vec(),
            limit_faults,
        })
    }

    /// `@tool.unavailable` at each `@git` dependency entry of every project
    /// file read: a valid construct outside this release.
    #[must_use]
    pub fn unavailable(&self) -> &[Diagnostic] {
        &self.unavailable
    }

    /// `@tool.limitExceeded` at the first token past the source nesting limit
    /// of each module nested deeper ([`crate::nesting_fault`]), in module order.
    /// When there is one, no later stage reads the snapshot: the check, the
    /// tests, and the run report these alone.
    #[must_use]
    pub fn limit_faults(&self) -> &[Diagnostic] {
        &self.limit_faults
    }

    /// The limits the capture reached, by the name `limits.reached` gives
    /// them: `sourceNesting` when a module is nested past the source nesting
    /// limit, the only limit the snapshot measures.
    #[must_use]
    pub fn limits_reached(&self) -> Vec<String> {
        if self.limit_faults.is_empty() {
            Vec::new()
        } else {
            vec![SOURCE_NESTING.to_owned()]
        }
    }

    /// The discovered project.
    #[must_use]
    pub const fn project(&self) -> &DiscoveredProject {
        &self.project
    }

    /// The target units in project order, then the dependency units in
    /// discovery order, then `@tests`, which is always present.
    #[must_use]
    pub fn units(&self) -> &[SourceUnit] {
        &self.units
    }

    /// Every module, unit by unit.
    pub fn modules(&self) -> impl Iterator<Item = &SourceFile> {
        self.units.iter().flat_map(|unit| unit.modules.iter())
    }

    /// This snapshot with the bytes of the module `source_id` replaced by
    /// `bytes`, or `None` when no module has that source identity.
    #[must_use]
    pub fn with_module_bytes(&self, source_id: &str, bytes: &[u8]) -> Option<Self> {
        let mut replaced = self.clone();
        let module = replaced
            .units
            .iter_mut()
            .flat_map(|unit| unit.modules.iter_mut())
            .find(|module| module.source_id == source_id)?;
        module.bytes = bytes.to_vec();
        replaced.limit_faults = limit_faults(&replaced.units);
        Some(replaced)
    }

    /// The records of the workspace revision: the `project` and `source`
    /// records over the exact captured bytes, and the one `toolchain`
    /// record of this toolchain.
    #[must_use]
    pub fn revision_records(&self) -> Vec<RevisionRecord> {
        let mut records = self.input_records();
        records.extend(embedded_toolchain_record());
        records
    }

    /// The records of the workspace revision with `toolchain` as the one
    /// `toolchain` record, as a toolchain embedding another library has.
    #[must_use]
    pub fn revision_records_with(
        &self,
        toolchain: RevisionRecord,
    ) -> Vec<RevisionRecord> {
        let mut records = self.input_records();
        records.push(toolchain);
        records
    }

    /// The `project`, `dependency`, and `source` records, over the exact
    /// captured bytes: one `dependency` record for each dependency's project
    /// file, by its path from the project root.
    fn input_records(&self) -> Vec<RevisionRecord> {
        let mut records = vec![RevisionRecord::new(
            RevisionKind::Project,
            PROJECT_FILE_NAME,
            self.project.bytes(),
        )];
        records.extend(self.dependency_files.iter().map(|(source_id, bytes)| {
            RevisionRecord::new(
                RevisionKind::Dependency,
                source_id.clone(),
                bytes.clone(),
            )
        }));
        records.extend(self.modules().map(|module| {
            RevisionRecord::new(
                RevisionKind::Source,
                module.source_id.clone(),
                module.bytes.clone(),
            )
        }));
        records
    }
}

/// The nesting fault of each module of `units` read past the limit.
fn limit_faults(units: &[SourceUnit]) -> Vec<Diagnostic> {
    units
        .iter()
        .flat_map(|unit| unit.modules.iter())
        .filter_map(|module| {
            nesting_fault(&parse_source(Some(&module.source_id), &module.bytes))
        })
        .collect()
}

fn module_path(unit: &str, segments: &[String]) -> String {
    let mut path = unit.to_owned();
    for segment in segments {
        path.push('.');
        path.push_str(segment);
    }
    path
}

fn sorted(mut diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    diagnostics.sort_by(|left, right| {
        (
            left.source_id(),
            left.primary_span().start(),
            left.primary_span().end(),
            left.code(),
        )
            .cmp(&(
                right.source_id(),
                right.primary_span().start(),
                right.primary_span().end(),
                right.code(),
            ))
    });
    diagnostics
}

fn module_fault(
    code: DiagnosticCode,
    source_id: &str,
    message: impl Into<String>,
) -> Diagnostic {
    Diagnostic::new(code, ByteSpan::empty_at(0), message).with_source_id(source_id)
}

fn project_fault(
    code: DiagnosticCode,
    span: ByteSpan,
    message: impl Into<String>,
) -> Diagnostic {
    Diagnostic::new(code, span, message).with_source_id(PROJECT_FILE_NAME)
}

/// One checked root.
#[derive(Clone, Debug)]
struct Root {
    name: String,
    kind: UnitKind,
    /// The project-relative path as written, `tests` for the test root.
    relative: String,
    canonical: PathBuf,
    /// The span of the target's `root` value; `None` for `tests/`.
    span: Option<ByteSpan>,
    /// The canonical directory of a dependency's package, its project root
    /// for the walk; `None` for the root project's units.
    project_root: Option<PathBuf>,
    /// The package of a dependency's unit.
    package: Option<Package>,
    /// The dependency units its modules may name.
    dependencies: Vec<String>,
}

/// Gives the root project's units its dependencies, and adds the units of
/// `dependencies` before `@tests`, which `roots` ends with when present.
fn add_dependency_roots(roots: &mut Vec<Root>, dependencies: &Dependencies) {
    for root in roots.iter_mut() {
        root.dependencies = dependencies.root().to_vec();
    }
    let at = roots
        .iter()
        .position(|root| root.kind == UnitKind::Tests)
        .unwrap_or(roots.len());
    let units = dependencies.units().iter().map(|unit| Root {
        name: unit.name().to_owned(),
        kind: UnitKind::Dependency,
        relative: unit.root().to_owned(),
        canonical: unit.canonical_root().to_path_buf(),
        span: Some(unit.root_span().1),
        project_root: Some(unit.package_root().to_path_buf()),
        package: Some(unit.package().clone()),
        dependencies: unit.dependencies().to_vec(),
    });
    let tail = roots.split_off(at);
    roots.extend(units);
    roots.extend(tail);
}

/// `@module.sourceIdCollision` at offset 0 of each source identity that two
/// units of the snapshot capture: a file reached through a dependency that
/// another unit also holds.
fn source_id_collisions(units: &[SourceUnit]) -> Vec<Diagnostic> {
    let mut seen = BTreeSet::new();
    let mut reported = BTreeSet::new();
    let mut diagnostics = Vec::new();
    for module in units.iter().flat_map(|unit| unit.modules.iter()) {
        if !seen.insert(module.source_id.as_str())
            && reported.insert(module.source_id.as_str())
        {
            diagnostics.push(module_fault(
                DiagnosticCode::ModuleSourceIdCollision,
                &module.source_id,
                "two units capture one source identity",
            ));
        }
    }
    sorted(diagnostics)
}

/// Whether `name` is a reserved unit name.
pub(crate) fn is_reserved_unit(name: &str) -> bool {
    matches!(name, "tests" | "std")
}

/// Whether `value` is a non-empty relative path without `.` or `..` parts.
pub(crate) fn lexically_relative(value: &str) -> bool {
    let path = Path::new(value);
    !value.is_empty()
        && !value.starts_with(['/', '\\'])
        && !path.is_absolute()
        && !value
            .split(['/', '\\'])
            .any(|part| part == "." || part == "..")
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

/// The canonical directory of `relative` below `project_root`, or the code of
/// the fault and its reason.
pub(crate) fn canonical_root(
    project_root: &Path,
    relative: &str,
) -> Result<Option<PathBuf>, (DiagnosticCode, String)> {
    let lexical = project_root.join(relative);
    let metadata = match fs::symlink_metadata(&lexical) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err((
                DiagnosticCode::ProjectIoError,
                format!("cannot inspect the root: {error}"),
            ));
        }
    };
    if metadata.file_type().is_symlink() {
        return Err((
            DiagnosticCode::ProjectInvalidTargetRoot,
            "a source root is not a link".to_owned(),
        ));
    }
    let canonical = fs::canonicalize(&lexical).map_err(|error| {
        (
            DiagnosticCode::ProjectIoError,
            format!("cannot resolve the root: {error}"),
        )
    })?;
    if !canonical.is_dir() {
        return Err((
            DiagnosticCode::ProjectInvalidTargetRoot,
            "a source root is a directory".to_owned(),
        ));
    }
    if !canonical.starts_with(project_root) {
        return Err((
            DiagnosticCode::ProjectInvalidTargetRoot,
            "a source root is inside the project root".to_owned(),
        ));
    }
    Ok(Some(canonical))
}

fn validate_roots(project: &DiscoveredProject) -> Result<Vec<Root>, Vec<Diagnostic>> {
    let project_root = project.root();
    let mut diagnostics = Vec::new();
    let mut roots = Vec::new();
    diagnostics.extend(name_faults(project.project(), PROJECT_FILE_NAME));
    for target in project.project().targets() {
        if !lexically_relative(target.root()) {
            diagnostics.push(project_fault(
                DiagnosticCode::ProjectInvalidTargetRoot,
                target.root_span(),
                "a target root is a non-empty relative path without `.` or `..` parts",
            ));
            continue;
        }
        match canonical_root(project_root, target.root()) {
            Ok(Some(canonical)) => roots.push(Root {
                name: target.name().to_owned(),
                kind: match target.kind() {
                    TargetKind::Bin => UnitKind::Bin,
                    TargetKind::Lib => UnitKind::Lib,
                },
                relative: target.root().replace('\\', "/"),
                canonical,
                span: Some(target.root_span()),
                project_root: None,
                package: None,
                dependencies: Vec::new(),
            }),
            Ok(None) => diagnostics.push(project_fault(
                DiagnosticCode::ProjectInvalidTargetRoot,
                target.root_span(),
                "the target root does not exist",
            )),
            Err((code, message)) => {
                diagnostics.push(project_fault(code, target.root_span(), message));
            }
        }
    }

    let tests = match canonical_root(project_root, TESTS_ROOT) {
        Ok(canonical) => canonical.map(|canonical| Root {
            name: "@tests".to_owned(),
            kind: UnitKind::Tests,
            relative: TESTS_ROOT.to_owned(),
            canonical,
            span: None,
            project_root: None,
            package: None,
            dependencies: Vec::new(),
        }),
        Err((code, message)) => {
            diagnostics.push(module_fault(code, TESTS_ROOT, message));
            None
        }
    };
    if !diagnostics.is_empty() {
        return Err(sorted(diagnostics));
    }
    for (index, later) in roots.iter().enumerate() {
        let overlaps = |other: &Path| {
            later.canonical.starts_with(other) || other.starts_with(&later.canonical)
        };
        let earlier = roots
            .iter()
            .take(index)
            .any(|earlier| overlaps(&earlier.canonical));
        let with_tests = tests
            .as_ref()
            .is_some_and(|tests| overlaps(&tests.canonical));
        if earlier || with_tests {
            diagnostics.push(project_fault(
                DiagnosticCode::ProjectOverlappingTargetRoots,
                later.span.unwrap_or(ByteSpan::empty_at(0)),
                if earlier {
                    "target roots are pairwise disjoint"
                } else {
                    "a target root is disjoint from `tests/`"
                },
            ));
        }
    }
    if !diagnostics.is_empty() {
        return Err(sorted(diagnostics));
    }
    roots.extend(tests);
    Ok(roots)
}

fn is_segment(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && characters.all(|character| character.is_ascii_alphanumeric())
        && !is_reserved_word(name)
}

/// A module found by the walk, before its bytes are read.
#[derive(Clone, Debug)]
struct Candidate {
    source_id: String,
    segments: Vec<String>,
    canonical: PathBuf,
}

/// The walk of one root.
#[derive(Debug)]
struct Walk<'a> {
    project_root: &'a Path,
    root: &'a Root,
    candidates: Vec<Candidate>,
    /// Every directory below the root, by its root-relative segments, with
    /// its project-relative path.
    directories: BTreeMap<Vec<String>, String>,
    visited: BTreeSet<PathBuf>,
    active: Vec<PathBuf>,
    diagnostics: Vec<Diagnostic>,
}

/// One entry of a directory, as the walk sees it.
enum Entry {
    Directory(PathBuf),
    File(PathBuf),
    Other,
}

impl<'a> Walk<'a> {
    fn new(project_root: &'a Path, root: &'a Root) -> Self {
        Self {
            project_root,
            root,
            candidates: Vec::new(),
            directories: BTreeMap::new(),
            visited: BTreeSet::new(),
            active: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    fn fault(
        &mut self,
        code: DiagnosticCode,
        source_id: &str,
        message: impl Into<String>,
    ) {
        self.diagnostics
            .push(module_fault(code, source_id, message));
    }

    fn inside(&self, canonical: &Path) -> bool {
        canonical.starts_with(self.project_root)
            && canonical.starts_with(&self.root.canonical)
    }

    fn run(&mut self) {
        let root = self.root.canonical.clone();
        let relative = self.root.relative.clone();
        self.directory(&root, root.clone(), &relative, &[]);
    }

    /// Walks the directory `lexical`, whose canonical path is `canonical`.
    fn directory(
        &mut self,
        lexical: &Path,
        canonical: PathBuf,
        relative: &str,
        segments: &[String],
    ) {
        if self.active.contains(&canonical) {
            self.fault(
                DiagnosticCode::ModulePathEscape,
                relative,
                "a directory link forms a cycle",
            );
            return;
        }
        if !self.visited.insert(canonical.clone()) {
            return;
        }
        let mut names = match fs::read_dir(lexical) {
            Ok(entries) => {
                let mut names = Vec::new();
                for entry in entries {
                    match entry {
                        Ok(entry) => names.push(entry.file_name()),
                        Err(error) => {
                            self.fault(
                                DiagnosticCode::ModuleIoError,
                                relative,
                                format!("cannot list the directory: {error}"),
                            );
                            return;
                        }
                    }
                }
                names
            }
            Err(error) => {
                self.fault(
                    DiagnosticCode::ModuleIoError,
                    relative,
                    format!("cannot list the directory: {error}"),
                );
                return;
            }
        };
        names.sort();
        self.active.push(canonical);
        for name in names {
            self.entry(lexical, relative, segments, &name);
        }
        self.active.pop();
    }

    fn classify(&mut self, path: &Path, relative: &str) -> Entry {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) => {
                self.fault(
                    DiagnosticCode::ModuleIoError,
                    relative,
                    format!("cannot inspect the entry: {error}"),
                );
                return Entry::Other;
            }
        };
        let canonical = match fs::canonicalize(path) {
            Ok(canonical) => canonical,
            Err(error) => {
                self.fault(
                    DiagnosticCode::ModuleIoError,
                    relative,
                    format!("the entry cannot be resolved: {error}"),
                );
                return Entry::Other;
            }
        };
        if !self.inside(&canonical) {
            self.fault(
                DiagnosticCode::ModulePathEscape,
                relative,
                "the entry resolves outside its source root",
            );
            return Entry::Other;
        }
        let followed = if metadata.file_type().is_symlink() {
            match fs::metadata(path) {
                Ok(followed) => followed,
                Err(error) => {
                    self.fault(
                        DiagnosticCode::ModuleIoError,
                        relative,
                        format!("the link cannot be followed: {error}"),
                    );
                    return Entry::Other;
                }
            }
        } else {
            metadata
        };
        if followed.is_dir() {
            Entry::Directory(canonical)
        } else if followed.is_file() {
            Entry::File(canonical)
        } else {
            Entry::Other
        }
    }

    fn entry(
        &mut self,
        directory: &Path,
        relative: &str,
        segments: &[String],
        name: &OsString,
    ) {
        let path = directory.join(name);
        let name = name.to_string_lossy().into_owned();
        let entry_relative = format!("{relative}/{name}");
        match self.classify(&path, &entry_relative) {
            Entry::Directory(canonical) => {
                let mut inner = segments.to_vec();
                inner.push(name);
                self.directories
                    .entry(inner.clone())
                    .or_insert_with(|| entry_relative.clone());
                self.directory(&path, canonical, &entry_relative, &inner);
            }
            Entry::File(canonical) => {
                let Some(stem) = name.strip_suffix(".vib") else {
                    return;
                };
                if stem.is_empty() {
                    return;
                }
                let mut module = segments.to_vec();
                module.push(stem.to_owned());
                if !module.iter().all(|segment| is_segment(segment)) {
                    self.fault(
                        DiagnosticCode::ModuleInvalidSegment,
                        &entry_relative,
                        "every name on a module's path is an identifier that is not a reserved word",
                    );
                    return;
                }
                self.candidates.push(Candidate {
                    source_id: entry_relative,
                    segments: module,
                    canonical,
                });
            }
            Entry::Other => {}
        }
    }

    /// The case and file-directory collisions of the modules found.
    fn collisions(&mut self) {
        self.candidates.sort_by(|left, right| {
            left.source_id.as_bytes().cmp(right.source_id.as_bytes())
        });
        for (code, source_id, message) in
            collision_faults(&self.candidates, &self.directories)
        {
            self.fault(code, &source_id, message);
        }
    }

    /// The modules, each linked file once at its first path in sorted order.
    fn candidates(self) -> Vec<Candidate> {
        let mut admitted = BTreeSet::new();
        self.candidates
            .into_iter()
            .filter(|candidate| admitted.insert(candidate.canonical.clone()))
            .collect()
    }
}

/// The collisions among `candidates` (sorted by source identity) and
/// `directories`: a module beside a directory of its name, and two paths that
/// differ only by ASCII case, each at the later path.
fn collision_faults(
    candidates: &[Candidate],
    directories: &BTreeMap<Vec<String>, String>,
) -> Vec<(DiagnosticCode, String, &'static str)> {
    let mut faults = Vec::new();
    let lower = |segments: &[String]| {
        segments
            .iter()
            .map(|segment| segment.to_ascii_lowercase())
            .collect::<Vec<_>>()
    };
    let mut seen: BTreeMap<Vec<String>, Vec<String>> = BTreeMap::new();
    for candidate in candidates {
        if directories.contains_key(&candidate.segments) {
            faults.push((
                DiagnosticCode::ModuleFileDirectoryCollision,
                candidate.source_id.clone(),
                "a module file is beside a directory of the same name",
            ));
        }
        let folded = lower(&candidate.segments);
        let case_differs = directories.keys().any(|directory| {
            *directory != candidate.segments && lower(directory) == folded
        }) || seen
            .get(&folded)
            .is_some_and(|earlier| *earlier != candidate.segments);
        if case_differs {
            faults.push((
                DiagnosticCode::ModuleCaseCollision,
                candidate.source_id.clone(),
                "two paths under one root differ only by case",
            ));
        }
        seen.entry(folded)
            .or_insert_with(|| candidate.segments.clone());
    }
    let mut folded_directories: BTreeMap<Vec<String>, &Vec<String>> = BTreeMap::new();
    for (segments, relative) in directories {
        let folded = lower(segments);
        if folded_directories
            .get(&folded)
            .is_some_and(|earlier| *earlier != segments)
        {
            faults.push((
                DiagnosticCode::ModuleCaseCollision,
                relative.clone(),
                "two paths under one root differ only by case",
            ));
        }
        folded_directories.entry(folded).or_insert(segments);
    }
    faults
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use vibra_diagnostics::DiagnosticCode;

    use super::{Candidate, collision_faults};

    fn candidate(source_id: &str, segments: &[&str]) -> Candidate {
        Candidate {
            source_id: source_id.to_owned(),
            segments: segments
                .iter()
                .map(|segment| (*segment).to_owned())
                .collect(),
            canonical: PathBuf::from(source_id),
        }
    }

    #[test]
    fn paths_that_differ_only_by_case_collide_at_the_later_path() {
        let candidates = [
            candidate("src/app/A.vib", &["A"]),
            candidate("src/app/a.vib", &["a"]),
            candidate("src/app/b.vib", &["b"]),
        ];
        let faults = collision_faults(&candidates, &BTreeMap::new());
        assert_eq!(
            faults
                .iter()
                .map(|(code, source, _)| (*code, source.as_str()))
                .collect::<Vec<_>>(),
            [(DiagnosticCode::ModuleCaseCollision, "src/app/a.vib")]
        );
        let directories = BTreeMap::from([
            (vec!["Dir".to_owned()], "src/app/Dir".to_owned()),
            (vec!["dir".to_owned()], "src/app/dir".to_owned()),
        ]);
        let faults = collision_faults(&[], &directories);
        assert_eq!(
            faults
                .iter()
                .map(|(code, source, _)| (*code, source.as_str()))
                .collect::<Vec<_>>(),
            [(DiagnosticCode::ModuleCaseCollision, "src/app/dir")]
        );
    }
}

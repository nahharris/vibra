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
//! 4. The bytes. No source byte is read before the walk's checks pass; the
//!    bytes are then copied exactly, sorted by project-relative path.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_syntax::is_reserved_word;

use crate::discovery::DiscoveredProject;
use crate::project::{PROJECT_FILE_NAME, TargetKind};
use crate::revision::{RevisionKind, RevisionRecord};

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
}

impl UnitKind {
    /// The atom, `@` included.
    #[must_use]
    pub const fn as_atom(self) -> &'static str {
        match self {
            Self::Bin => "@bin",
            Self::Lib => "@lib",
            Self::Tests => "@tests",
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
}

/// The immutable capture of one project: its marker and every module.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceSnapshot {
    project: DiscoveredProject,
    units: Vec<SourceUnit>,
}

impl SourceSnapshot {
    /// Checks the unit names and roots, walks every root and `tests/`, and
    /// copies the modules' bytes.
    pub fn capture(project: DiscoveredProject) -> Result<Self, Vec<Diagnostic>> {
        let roots = validate_roots(&project)?;
        let mut diagnostics = Vec::new();
        let mut walks = Vec::with_capacity(roots.len());
        for root in &roots {
            let mut walk = Walk::new(project.root(), root);
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
            });
        }
        Ok(Self { project, units })
    }

    /// The discovered project.
    #[must_use]
    pub const fn project(&self) -> &DiscoveredProject {
        &self.project
    }

    /// The target units in project order, then `@tests`, which is always
    /// present.
    #[must_use]
    pub fn units(&self) -> &[SourceUnit] {
        &self.units
    }

    /// Every module, unit by unit.
    pub fn modules(&self) -> impl Iterator<Item = &SourceFile> {
        self.units.iter().flat_map(|unit| unit.modules.iter())
    }

    /// The `project` and `source` records of the workspace revision, over the
    /// exact captured bytes.
    #[must_use]
    pub fn revision_records(&self) -> Vec<RevisionRecord> {
        let mut records = vec![RevisionRecord::new(
            RevisionKind::Project,
            PROJECT_FILE_NAME,
            self.project.bytes(),
        )];
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
}

fn is_reserved_unit(name: &str) -> bool {
    matches!(name, "tests" | "std")
}

fn lexically_relative(value: &str) -> bool {
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
fn canonical_root(
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

/// `@project.duplicateName` for two targets with one name, or a target and
/// a dependency with one name: at the later name in the project file, a
/// target's `name` value or a dependency's key, related to the earlier one.
fn duplicate_names(project: &DiscoveredProject) -> Vec<Diagnostic> {
    let manifest = project.project();
    let mut names: Vec<(&str, ByteSpan)> = manifest
        .targets()
        .iter()
        .map(|target| {
            let part = target.name().strip_prefix('@').unwrap_or(target.name());
            (part, target.name_span())
        })
        .chain(
            manifest
                .dependencies()
                .iter()
                .map(|dependency| (dependency.name(), dependency.name_span())),
        )
        .collect();
    names.sort_by_key(|(_, span)| (span.start(), span.end()));
    let mut diagnostics = Vec::new();
    for (index, (name, span)) in names.iter().enumerate() {
        if let Some((_, earlier)) =
            names.iter().take(index).find(|(other, _)| other == name)
        {
            diagnostics.push(
                project_fault(
                    DiagnosticCode::ProjectDuplicateName,
                    *span,
                    format!("two units are named `{name}`"),
                )
                .with_related(*earlier, "the unit of the same name"),
            );
        }
    }
    diagnostics
}

fn validate_roots(project: &DiscoveredProject) -> Result<Vec<Root>, Vec<Diagnostic>> {
    let project_root = project.root();
    let mut diagnostics = Vec::new();
    let mut roots = Vec::new();
    for target in project.project().targets() {
        let part = target.name().strip_prefix('@').unwrap_or(target.name());
        if is_reserved_unit(part) {
            diagnostics.push(project_fault(
                DiagnosticCode::ProjectReservedUnitName,
                target.name_span(),
                format!("`{}` is a reserved unit name", target.name()),
            ));
        }
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
    for dependency in project.project().dependencies() {
        if is_reserved_unit(dependency.name()) {
            diagnostics.push(project_fault(
                DiagnosticCode::ProjectReservedUnitName,
                dependency.name_span(),
                format!("`{}` is a reserved unit name", dependency.name()),
            ));
        }
    }
    diagnostics.extend(duplicate_names(project));
    let tests = match canonical_root(project_root, TESTS_ROOT) {
        Ok(canonical) => canonical.map(|canonical| Root {
            name: "@tests".to_owned(),
            kind: UnitKind::Tests,
            relative: TESTS_ROOT.to_owned(),
            canonical,
            span: None,
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

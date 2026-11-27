//! The local dependencies of a project.
//!
//! `docs/spec/04-programs-and-packages.md`, "Dependencies and lock" and
//! "Packages, units, and targets": a `@path` dependency binds the `@lib`
//! target of the package in its directory whose name is the dependency's
//! key. A dependency never binds a binary target. Unit names are global: a
//! unit has one name in the whole snapshot. Package dependencies form a
//! directed acyclic graph. No lock file is read while every dependency is
//! `@path`; a `@git` dependency is `@tool.unavailable` at its entry.
//!
//! [`load`] reads the dependency graph with an explicit work list, in
//! discovery order: the root project's entries in dict order, then each
//! package found, in the order it was found, with its entries in dict order.
//!
//! 1. A `path` is relative to the directory of the project file that writes
//!    it and stays beneath the workspace root, the root itself included:
//!    first lexically, then by its canonical directory (`@module.pathEscape`
//!    at `path` otherwise). The directory holds a regular `project.vibon`
//!    (`@project.notFound` at `path` otherwise), decoded under its path from
//!    the workspace root.
//! 2. An entry that closes a cycle, in discovery order, is
//!    `@project.dependencyCycle` at that entry, related along the shortest
//!    chain of accepted entries from the package it names back to the one
//!    that writes it; a package that names itself has no related span.
//! 3. The key selects the target: a key that names no target of the
//!    package is `@name.unknownSymbol`, and one that names a `@bin` target is
//!    `@name.wrongEntityKind`, both at the key.
//! 4. A unit is one (package directory, target) pair; the same unit reached
//!    twice is one unit. Names are unique in the whole snapshot: a later key
//!    that names another unit with the name of an earlier unit, a root
//!    target included, is `@project.duplicateName` at that key, related to
//!    the earlier name.
//!
//! The selected target's `root` is checked as a target root of its own
//! package (`@project.invalidTargetRoot` at that `root`). The source snapshot
//! walks it; its modules are `source` records of the workspace revision, and
//! each dependency's `project.vibon` is one `dependency` record.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};

use crate::discovery::DiscoveredProject;
use crate::project::{
    Dependency, DependencyKind, PROJECT_FILE_NAME, Package, Project, TargetKind,
    decode_project,
};
use crate::snapshot::{canonical_root, is_reserved_unit, lexically_relative};

/// One dependency's unit: the selected library target of one package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyUnit {
    name: String,
    package: Package,
    package_root: PathBuf,
    root: String,
    canonical_root: PathBuf,
    root_span: (String, ByteSpan),
    entry: (String, ByteSpan),
    dependencies: Vec<String>,
}

impl DependencyUnit {
    /// The unit name: the key that binds it, which is the target's name,
    /// `@` included.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The package the unit's declarations belong to.
    #[must_use]
    pub const fn package(&self) -> &Package {
        &self.package
    }

    /// The canonical directory of the package.
    #[must_use]
    pub fn package_root(&self) -> &Path {
        &self.package_root
    }

    /// The target's root, as a path from the workspace root with `/`
    /// separators.
    #[must_use]
    pub fn root(&self) -> &str {
        &self.root
    }

    /// The canonical directory of the target's root.
    #[must_use]
    pub fn canonical_root(&self) -> &Path {
        &self.canonical_root
    }

    /// The source identity of the package's project file and where the
    /// target's `root` is written in it.
    #[must_use]
    pub fn root_span(&self) -> (&str, ByteSpan) {
        (&self.root_span.0, self.root_span.1)
    }

    /// The source identity of the project file and the span of the first
    /// entry, in discovery order, that binds this unit.
    #[must_use]
    pub fn entry(&self) -> (&str, ByteSpan) {
        (&self.entry.0, self.entry.1)
    }

    /// The dependency units its modules may name: the accepted `@path` keys
    /// of its package's project file, as atoms.
    #[must_use]
    pub fn dependencies(&self) -> &[String] {
        &self.dependencies
    }
}

/// The dependency graph of one project, as [`load`] read it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dependencies {
    units: Vec<DependencyUnit>,
    root: Vec<String>,
    files: Vec<(String, Vec<u8>)>,
    unavailable: Vec<Diagnostic>,
}

impl Dependencies {
    /// The dependency units, in discovery order.
    #[must_use]
    pub fn units(&self) -> &[DependencyUnit] {
        &self.units
    }

    /// The dependency units the root project's modules may name: its
    /// accepted `@path` keys, as atoms.
    #[must_use]
    pub fn root(&self) -> &[String] {
        &self.root
    }

    /// Each dependency's project file: its path from the workspace root and
    /// its exact bytes.
    #[must_use]
    pub fn files(&self) -> &[(String, Vec<u8>)] {
        &self.files
    }

    /// `@tool.unavailable` at each `@git` entry of every project file read.
    #[must_use]
    pub fn unavailable(&self) -> &[Diagnostic] {
        &self.unavailable
    }
}

/// One package of the graph: node 0 is the root project.
struct Node {
    dir: PathBuf,
    relative: String,
    source_id: String,
    project: Project,
}

/// One `@path` entry, from the package that writes it to the package it
/// names.
#[derive(Clone, Copy)]
struct Edge {
    from: usize,
    entry: usize,
    to: usize,
}

fn fault(
    code: DiagnosticCode,
    source_id: &str,
    span: ByteSpan,
    message: impl Into<String>,
) -> Diagnostic {
    Diagnostic::new(code, span, message).with_source_id(source_id)
}

/// The path from the workspace root of `path`, written in the package whose
/// path from the workspace root is `declaring`; `None` when it leaves the
/// workspace root lexically or is absolute.
fn lexical(declaring: &str, path: &str) -> Option<Vec<String>> {
    if path.starts_with(['/', '\\']) || path.contains(':') {
        return None;
    }
    let mut parts = declaring
        .split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            name => parts.push(name.to_owned()),
        }
    }
    Some(parts)
}

/// The path of `canonical` from `root`, with `/` separators.
fn relative_to(root: &Path, canonical: &Path) -> Option<String> {
    let rest = canonical.strip_prefix(root).ok()?;
    Some(
        rest.components()
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

fn project_id(relative: &str) -> String {
    if relative.is_empty() {
        PROJECT_FILE_NAME.to_owned()
    } else {
        format!("{relative}/{PROJECT_FILE_NAME}")
    }
}

/// `@project.reservedUnitName` and `@project.duplicateName` among the names
/// of `project`, read as `source_id`: a target or dependency named `tests`
/// or `std`, and a later name equal to an earlier one, a target's `name`
/// value or a dependency's key, related to the earlier one.
pub(crate) fn name_faults(project: &Project, source_id: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut names: Vec<(&str, ByteSpan)> = Vec::new();
    for target in project.targets() {
        let part = target.name().strip_prefix('@').unwrap_or(target.name());
        names.push((part, target.name_span()));
    }
    for dependency in project.dependencies() {
        names.push((dependency.name(), dependency.name_span()));
    }
    for (name, span) in &names {
        if is_reserved_unit(name) {
            diagnostics.push(fault(
                DiagnosticCode::ProjectReservedUnitName,
                source_id,
                *span,
                format!("`{name}` is a reserved unit name"),
            ));
        }
    }
    names.sort_by_key(|(_, span)| (span.start(), span.end()));
    for (index, (name, span)) in names.iter().enumerate() {
        if let Some((_, earlier)) =
            names.iter().take(index).find(|(other, _)| other == name)
        {
            diagnostics.push(
                fault(
                    DiagnosticCode::ProjectDuplicateName,
                    source_id,
                    *span,
                    format!("two units are named `{name}`"),
                )
                .with_related(*earlier, "the unit of the same name"),
            );
        }
    }
    diagnostics
}

/// The graph being read.
struct Loader {
    root: PathBuf,
    nodes: Vec<Node>,
    by_dir: BTreeMap<PathBuf, usize>,
    edges: Vec<Edge>,
    files: Vec<(String, Vec<u8>)>,
    faults: Vec<Diagnostic>,
    unavailable: Vec<Diagnostic>,
}

impl Loader {
    fn dependency(&self, edge: Edge) -> Option<(&Node, &Dependency)> {
        let node = self.nodes.get(edge.from)?;
        Some((node, node.project.dependencies().get(edge.entry)?))
    }

    /// Reads the package `dependency` of node `from` names, and returns its
    /// node.
    fn package(&mut self, from: usize, dependency: &Dependency) -> Option<usize> {
        let (declaring, source_id) = {
            let node = self.nodes.get(from)?;
            (node.relative.clone(), node.source_id.clone())
        };
        let at = |code, message: &str| {
            fault(code, &source_id, dependency.source_span(), message)
        };
        let Some(parts) = lexical(&declaring, dependency.source()) else {
            self.faults.push(at(
                DiagnosticCode::ModulePathEscape,
                "a path dependency stays beneath the workspace root",
            ));
            return None;
        };
        let lexical_dir = parts
            .iter()
            .fold(self.root.clone(), |dir, part| dir.join(part));
        let canonical = match fs::canonicalize(&lexical_dir) {
            Ok(canonical) => canonical,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                self.faults.push(at(
                    DiagnosticCode::ProjectNotFound,
                    "no `project.vibon` in the dependency's directory",
                ));
                return None;
            }
            Err(error) => {
                self.faults.push(at(
                    DiagnosticCode::ProjectIoError,
                    &format!("cannot resolve the dependency's directory: {error}"),
                ));
                return None;
            }
        };
        let Some(relative) = relative_to(&self.root, &canonical) else {
            self.faults.push(at(
                DiagnosticCode::ModulePathEscape,
                "a path dependency resolves beneath the workspace root",
            ));
            return None;
        };
        if let Some(&node) = self.by_dir.get(&canonical) {
            return Some(node);
        }
        let marker = canonical.join(PROJECT_FILE_NAME);
        match fs::symlink_metadata(&marker) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                self.faults.push(at(
                    DiagnosticCode::ProjectIoError,
                    "the dependency's project marker is a link",
                ));
                return None;
            }
            Ok(metadata) if metadata.is_file() => {}
            Ok(_) => {
                self.faults.push(at(
                    DiagnosticCode::ProjectNotFound,
                    "no `project.vibon` in the dependency's directory",
                ));
                return None;
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                self.faults.push(at(
                    DiagnosticCode::ProjectNotFound,
                    "no `project.vibon` in the dependency's directory",
                ));
                return None;
            }
            Err(error) => {
                self.faults.push(at(
                    DiagnosticCode::ProjectIoError,
                    &format!("cannot inspect the dependency's project marker: {error}"),
                ));
                return None;
            }
        }
        let bytes = match fs::read(&marker) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.faults.push(at(
                    DiagnosticCode::ProjectIoError,
                    &format!("cannot read the dependency's project marker: {error}"),
                ));
                return None;
            }
        };
        let id = project_id(&relative);
        let project = match decode_project(&id, &bytes) {
            Ok(project) => project,
            Err(mut diagnostics) => {
                self.faults.append(&mut diagnostics);
                return None;
            }
        };
        let mut names = name_faults(&project, &id);
        if !names.is_empty() {
            self.faults.append(&mut names);
            return None;
        }
        self.files.push((id.clone(), bytes));
        self.nodes.push(Node {
            dir: canonical.clone(),
            relative,
            source_id: id,
            project,
        });
        let node = self.nodes.len() - 1;
        self.by_dir.insert(canonical, node);
        Some(node)
    }

    /// Reads every package reachable from the root, with a work list.
    fn read(&mut self) {
        let mut next = 0;
        while next < self.nodes.len() {
            let dependencies = self
                .nodes
                .get(next)
                .map(|node| node.project.dependencies().to_vec())
                .unwrap_or_default();
            let source_id = self
                .nodes
                .get(next)
                .map(|node| node.source_id.clone())
                .unwrap_or_default();
            for (entry, dependency) in dependencies.iter().enumerate() {
                if dependency.kind() == DependencyKind::Git {
                    self.unavailable.push(fault(
                        DiagnosticCode::ToolUnavailable,
                        &source_id,
                        dependency.span(),
                        "a Git dependency is unavailable in this release",
                    ));
                    continue;
                }
                if let Some(to) = self.package(next, dependency) {
                    self.edges.push(Edge {
                        from: next,
                        entry,
                        to,
                    });
                }
            }
            next += 1;
        }
    }

    /// The accepted edges in discovery order, each edge that closes a cycle
    /// reported and left out.
    fn acyclic(&mut self) -> Vec<Edge> {
        let mut outgoing: Vec<Vec<Edge>> = vec![Vec::new(); self.nodes.len()];
        let mut accepted = Vec::new();
        for edge in self.edges.clone() {
            let chain = if edge.from == edge.to {
                Some(Vec::new())
            } else {
                shortest_chain(&outgoing, edge.to, edge.from)
            };
            let Some(chain) = chain else {
                if let Some(list) = outgoing.get_mut(edge.from) {
                    list.push(edge);
                }
                accepted.push(edge);
                continue;
            };
            let Some((node, dependency)) = self.dependency(edge) else {
                continue;
            };
            let mut diagnostic = fault(
                DiagnosticCode::ProjectDependencyCycle,
                &node.source_id,
                dependency.span(),
                "this dependency closes a cycle of package dependencies",
            );
            for earlier in chain {
                if let Some((node, dependency)) = self.dependency(earlier) {
                    diagnostic = diagnostic.with_related_source(
                        node.source_id.clone(),
                        dependency.span(),
                        "a dependency of the cycle",
                    );
                }
            }
            self.faults.push(diagnostic);
        }
        accepted
    }

    /// The index of the library target of node `edge.to` that the entry's
    /// key names, or the fault.
    fn select(&self, edge: Edge) -> Result<usize, Box<Diagnostic>> {
        let Some((node, dependency)) = self.dependency(edge) else {
            return Err(Box::new(Diagnostic::new(
                DiagnosticCode::ProjectIoError,
                ByteSpan::empty_at(0),
                "the dependency graph is inconsistent",
            )));
        };
        let targets = self
            .nodes
            .get(edge.to)
            .map(|package| package.project.targets())
            .unwrap_or_default();
        let name = format!("@{}", dependency.name());
        let at = |code, message: String| {
            Box::new(fault(
                code,
                &node.source_id,
                dependency.name_span(),
                message,
            ))
        };
        match targets.iter().position(|target| target.name() == name) {
            None => Err(at(
                DiagnosticCode::NameUnknownSymbol,
                format!("the package has no target `{name}`"),
            )),
            Some(index)
                if targets.get(index).map(|target| target.kind())
                    == Some(TargetKind::Bin) =>
            {
                Err(at(
                    DiagnosticCode::NameWrongEntityKind,
                    format!(
                        "`{name}` is a binary target; a dependency binds a library"
                    ),
                ))
            }
            Some(index) => Ok(index),
        }
    }

    /// The unit `name` of library target `target` of node `package`, bound
    /// by the entry at `entry`, once its root is checked.
    fn unit(
        &self,
        package: usize,
        target: usize,
        name: String,
        entry: (String, ByteSpan),
    ) -> Result<DependencyUnit, Box<Diagnostic>> {
        let inconsistent = || {
            Box::new(Diagnostic::new(
                DiagnosticCode::ProjectIoError,
                ByteSpan::empty_at(0),
                "the dependency graph is inconsistent",
            ))
        };
        let package = self.nodes.get(package).ok_or_else(inconsistent)?;
        let selected = package
            .project
            .targets()
            .get(target)
            .ok_or_else(inconsistent)?;
        let at = |code, message: &str| {
            Box::new(fault(
                code,
                &package.source_id,
                selected.root_span(),
                message,
            ))
        };
        if !lexically_relative(selected.root()) {
            return Err(at(
                DiagnosticCode::ProjectInvalidTargetRoot,
                "a target root is a non-empty relative path without `.` or `..` parts",
            ));
        }
        let canonical = match canonical_root(&package.dir, selected.root()) {
            Ok(Some(canonical)) => canonical,
            Ok(None) => {
                return Err(at(
                    DiagnosticCode::ProjectInvalidTargetRoot,
                    "the target root does not exist",
                ));
            }
            Err((code, message)) => return Err(at(code, &message)),
        };
        Ok(DependencyUnit {
            name,
            package: package.project.package().clone(),
            package_root: package.dir.clone(),
            root: relative_to(&self.root, &canonical).unwrap_or_default(),
            canonical_root: canonical,
            root_span: (package.source_id.clone(), selected.root_span()),
            entry,
            dependencies: Vec::new(),
        })
    }
}
/// The accepted edges of the shortest chain from `from` to `to`, in order;
/// `None` when there is none.
fn shortest_chain(outgoing: &[Vec<Edge>], from: usize, to: usize) -> Option<Vec<Edge>> {
    let mut reached: Vec<Option<Edge>> = vec![None; outgoing.len()];
    let mut queue = VecDeque::from([from]);
    while let Some(node) = queue.pop_front() {
        for edge in outgoing.get(node).into_iter().flatten() {
            if edge.to == from || reached.get(edge.to).is_some_and(Option::is_some) {
                continue;
            }
            if let Some(slot) = reached.get_mut(edge.to) {
                *slot = Some(*edge);
            }
            if edge.to == to {
                let mut chain = Vec::new();
                let mut current = to;
                while current != from {
                    let step = reached.get(current).copied().flatten()?;
                    chain.push(step);
                    current = step.from;
                }
                chain.reverse();
                return Some(chain);
            }
            queue.push_back(edge.to);
        }
    }
    None
}

/// Reads the dependency graph of `project`, whose own unit names have
/// already been checked.
///
/// # Errors
///
/// Every fault of the graph: of a path, a project file, a cycle, a target
/// selection, a unit name, or a selected target's root.
pub fn load(project: &DiscoveredProject) -> Result<Dependencies, Vec<Diagnostic>> {
    let root = project.root().to_path_buf();
    let mut loader = Loader {
        root: root.clone(),
        nodes: vec![Node {
            dir: root.clone(),
            relative: String::new(),
            source_id: PROJECT_FILE_NAME.to_owned(),
            project: project.project().clone(),
        }],
        by_dir: BTreeMap::from([(root, 0)]),
        edges: Vec::new(),
        files: Vec::new(),
        faults: Vec::new(),
        unavailable: Vec::new(),
    };
    loader.read();
    if !loader.faults.is_empty() {
        return Err(loader.faults);
    }
    let accepted = loader.acyclic();
    if !loader.faults.is_empty() {
        return Err(loader.faults);
    }
    let mut names: BTreeMap<String, (String, ByteSpan)> = project
        .project()
        .targets()
        .iter()
        .map(|target| {
            (
                target.name().to_owned(),
                (PROJECT_FILE_NAME.to_owned(), target.name_span()),
            )
        })
        .collect();
    let mut units: Vec<DependencyUnit> = Vec::new();
    let mut unit_of: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    let mut visible: Vec<Vec<String>> = vec![Vec::new(); loader.nodes.len()];
    let mut faults = Vec::new();
    for edge in accepted {
        let target = match loader.select(edge) {
            Ok(target) => target,
            Err(diagnostic) => {
                faults.push(*diagnostic);
                continue;
            }
        };
        let Some((node, dependency)) = loader.dependency(edge) else {
            continue;
        };
        let name = format!("@{}", dependency.name());
        let key = (node.source_id.clone(), dependency.name_span());
        if let Entry::Vacant(slot) = unit_of.entry((edge.to, target)) {
            if let Some((source_id, span)) = names.get(&name) {
                faults.push(
                    fault(
                        DiagnosticCode::ProjectDuplicateName,
                        &key.0,
                        key.1,
                        format!("two units of the snapshot are named `{name}`"),
                    )
                    .with_related_source(
                        source_id.clone(),
                        *span,
                        "the unit of the same name",
                    ),
                );
                continue;
            }
            names.insert(name.clone(), key);
            let entry = (node.source_id.clone(), dependency.span());
            match loader.unit(edge.to, target, name.clone(), entry) {
                Ok(unit) => {
                    slot.insert(units.len());
                    units.push(unit);
                }
                Err(diagnostic) => {
                    faults.push(*diagnostic);
                    continue;
                }
            }
        }
        if let Some(list) = visible.get_mut(edge.from)
            && !list.contains(&name)
        {
            list.push(name);
        }
    }
    if !faults.is_empty() {
        return Err(faults);
    }
    for (&(node, _), &unit) in &unit_of {
        if let (Some(unit), Some(list)) = (units.get_mut(unit), visible.get(node)) {
            unit.dependencies.clone_from(list);
        }
    }
    Ok(Dependencies {
        units,
        root: visible.first().cloned().unwrap_or_default(),
        files: loader.files,
        unavailable: loader.unavailable,
    })
}

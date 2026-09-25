//! Pure decoding for the M2 `@project.v1` VIBON record.
//!
//! This module accepts generic data that has already passed the `.vibon`
//! reader. The decoder owns only the project schema: it validates record
//! shape and schema-local values, preserves value spans and source identity,
//! and leaves every entity reference unresolved. No method in this module
//! reads a path or consults a source graph.

use std::collections::BTreeMap;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_fmt::format_document;
use vibra_syntax::{
    CstNode, DataField, DataNode, DataValue, Literal, Name, SyntaxKind, parse_data,
};

const PROJECT_FIELDS: &[&str] = &["format", "package", "targets", "dependencies"];
const PACKAGE_FIELDS: &[&str] = &["name", "version"];
const TARGET_FIELDS: &[&str] = &["name", "kind", "root", "entry", "effects"];
const PATH_DEPENDENCY_FIELDS: &[&str] = &["kind", "path", "target"];
const GIT_DEPENDENCY_FIELDS: &[&str] = &["kind", "git", "rev", "target"];
const PROJECT_ORDER: &[&str] = &["format", "package", "targets", "dependencies"];
const PACKAGE_ORDER: &[&str] = &["name", "version"];
const TARGET_ORDER: &[&str] = &["name", "kind", "root", "entry", "effects"];
const PATH_ORDER: &[&str] = &["kind", "path", "target"];
const GIT_ORDER: &[&str] = &["kind", "git", "rev", "target"];

/// The source identity attached to every decoded project value and
/// diagnostic.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectOrigin {
    source_id: String,
}

impl ProjectOrigin {
    /// Creates an origin from the caller's document identity.
    #[must_use]
    pub fn new(source_id: impl Into<String>) -> Self {
        Self {
            source_id: source_id.into(),
        }
    }

    /// Returns the stable source identity.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }
}

/// A field occurrence retained by a typed record decoder.
///
/// The generic data tree owns the value span and the label span. The project
/// adapter carries both forward with the document identity so later schema,
/// diagnostic, and edit phases never have to reconstruct provenance from a
/// field name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectField {
    label: Name,
    label_span: ByteSpan,
    value_span: ByteSpan,
    span: ByteSpan,
    origin: ProjectOrigin,
}

impl ProjectField {
    /// The field label without its trailing colon.
    #[must_use]
    pub const fn label(&self) -> &Name {
        &self.label
    }

    /// The source span occupied by the label.
    #[must_use]
    pub const fn label_span(&self) -> ByteSpan {
        self.label_span
    }

    /// The source span occupied by the decoded value.
    #[must_use]
    pub const fn value_span(&self) -> ByteSpan {
        self.value_span
    }

    /// The complete field occurrence span.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    /// The source identity owning this field.
    #[must_use]
    pub const fn origin(&self) -> &ProjectOrigin {
        &self.origin
    }
}

/// The entity kind required by a schema-selected reference slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EntityKind {
    /// A module-level declaration used by a binary entry.
    Declaration,
    /// A nominal effect root used by a binary effect ceiling.
    Effect,
    /// A library target selected by a dependency alias.
    LibraryTarget,
}

/// The schema role retained for an atom.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProjectAtomRole {
    /// The atom is a value and must never be resolved.
    Value,
    /// The atom is a deferred entity reference of the given kind.
    Reference(EntityKind),
}

/// An atom retained with its schema role, span, and source identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectAtom {
    atom: Name,
    role: ProjectAtomRole,
    span: ByteSpan,
    origin: ProjectOrigin,
    raw: String,
}

impl ProjectAtom {
    /// The lexical atom, including its `@` spelling.
    #[must_use]
    pub const fn atom(&self) -> &Name {
        &self.atom
    }

    /// The role selected by the project schema.
    #[must_use]
    pub const fn role(&self) -> ProjectAtomRole {
        self.role
    }

    /// The value span in the source document.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    /// The source identity owning this atom.
    #[must_use]
    pub const fn origin(&self) -> &ProjectOrigin {
        &self.origin
    }

    /// The exact atom spelling from the input document.
    #[must_use]
    pub fn raw(&self) -> &str {
        &self.raw
    }
}

/// A string retained with its value spelling, span, and source identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectString {
    value: String,
    span: ByteSpan,
    origin: ProjectOrigin,
    raw: String,
}

impl ProjectString {
    /// The decoded string value.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    /// The value span in the source document.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    /// The source identity owning this string.
    #[must_use]
    pub const fn origin(&self) -> &ProjectOrigin {
        &self.origin
    }

    /// The exact quoted spelling from the input document.
    #[must_use]
    pub fn raw(&self) -> &str {
        &self.raw
    }
}

/// The package provenance in a project record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package {
    name: ProjectString,
    version: ProjectString,
    fields: Vec<ProjectField>,
    span: ByteSpan,
    origin: ProjectOrigin,
    raw: String,
}

impl Package {
    /// The package's kebab-case name.
    #[must_use]
    pub const fn name(&self) -> &ProjectString {
        &self.name
    }

    /// The package's exact semantic version.
    #[must_use]
    pub const fn version(&self) -> &ProjectString {
        &self.version
    }

    /// Every package field in source order, with label and value provenance.
    #[must_use]
    pub fn fields(&self) -> &[ProjectField] {
        &self.fields
    }

    /// The complete package record span.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    /// The source identity owning the package record.
    #[must_use]
    pub const fn origin(&self) -> &ProjectOrigin {
        &self.origin
    }

    /// The exact source bytes covered by the package record, including any
    /// retained comments inside it.
    #[must_use]
    pub fn raw(&self) -> &str {
        &self.raw
    }
}

/// The target kind admitted by `@project.v1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TargetKind {
    /// An executable target with an entry declaration and effect ceiling.
    Bin,
    /// A library target without executable-only fields.
    Lib,
}

/// One local package target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    name: ProjectAtom,
    kind_atom: ProjectAtom,
    kind: TargetKind,
    root: ProjectString,
    entry: Option<ProjectAtom>,
    effects: Option<Vec<ProjectAtom>>,
    fields: Vec<ProjectField>,
    span: ByteSpan,
    origin: ProjectOrigin,
    raw: String,
}

impl Target {
    /// The target's atom name, retained as a value.
    #[must_use]
    pub const fn name(&self) -> &ProjectAtom {
        &self.name
    }

    /// The target kind selected by its `kind` atom.
    #[must_use]
    pub const fn kind(&self) -> TargetKind {
        self.kind
    }

    /// The target kind atom, retained as a schema-selected value.
    #[must_use]
    pub const fn kind_atom(&self) -> &ProjectAtom {
        &self.kind_atom
    }

    /// The target's source-root string.
    #[must_use]
    pub const fn root(&self) -> &ProjectString {
        &self.root
    }

    /// The unresolved entry declaration reference for a binary target.
    #[must_use]
    pub const fn entry(&self) -> Option<&ProjectAtom> {
        self.entry.as_ref()
    }

    /// The unresolved effect-root references for a binary target.
    #[must_use]
    pub fn effects(&self) -> Option<&[ProjectAtom]> {
        self.effects.as_deref()
    }

    /// Every target field in source order, with label and value provenance.
    #[must_use]
    pub fn fields(&self) -> &[ProjectField] {
        &self.fields
    }

    /// The complete target record span.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    /// The source identity owning the target record.
    #[must_use]
    pub const fn origin(&self) -> &ProjectOrigin {
        &self.origin
    }

    /// The exact source bytes covered by the target record.
    #[must_use]
    pub fn raw(&self) -> &str {
        &self.raw
    }
}

/// The dependency variant selected by its `kind` field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DependencyKind {
    /// A dependency reached through a local path.
    Path,
    /// A dependency pinned to an HTTPS Git revision.
    Git,
}

/// One dependency entry keyed by its project alias.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dependency {
    /// A local path dependency.
    Path(PathDependency),
    /// An HTTPS Git dependency pinned to a full revision.
    Git(GitDependency),
}

impl Dependency {
    /// The dependency alias map key, retained as an atom value.
    #[must_use]
    pub fn alias(&self) -> &ProjectAtom {
        match self {
            Self::Path(dependency) => dependency.alias(),
            Self::Git(dependency) => dependency.alias(),
        }
    }

    /// The selected dependency variant.
    #[must_use]
    pub const fn kind(&self) -> DependencyKind {
        match self {
            Self::Path(_) => DependencyKind::Path,
            Self::Git(_) => DependencyKind::Git,
        }
    }

    /// The optional library-target reference selected by `target:`.
    #[must_use]
    pub fn target(&self) -> Option<&ProjectAtom> {
        match self {
            Self::Path(dependency) => dependency.target(),
            Self::Git(dependency) => dependency.target(),
        }
    }

    /// The dependency's schema-selected `kind` atom.
    #[must_use]
    pub fn kind_atom(&self) -> &ProjectAtom {
        match self {
            Self::Path(dependency) => dependency.kind_atom(),
            Self::Git(dependency) => dependency.kind_atom(),
        }
    }

    /// The complete dependency record span.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        match self {
            Self::Path(dependency) => dependency.span(),
            Self::Git(dependency) => dependency.span(),
        }
    }

    /// The source identity owning the dependency record.
    #[must_use]
    pub fn origin(&self) -> &ProjectOrigin {
        match self {
            Self::Path(dependency) => dependency.origin(),
            Self::Git(dependency) => dependency.origin(),
        }
    }

    /// The exact source bytes covered by the dependency record.
    #[must_use]
    pub fn raw(&self) -> &str {
        match self {
            Self::Path(dependency) => dependency.raw(),
            Self::Git(dependency) => dependency.raw(),
        }
    }

    /// The path value for a path dependency, if this is one.
    #[must_use]
    pub fn path(&self) -> Option<&ProjectString> {
        match self {
            Self::Path(dependency) => Some(dependency.path()),
            Self::Git(_) => None,
        }
    }

    /// The Git URL value for a Git dependency, if this is one.
    #[must_use]
    pub fn git(&self) -> Option<&ProjectString> {
        match self {
            Self::Path(_) => None,
            Self::Git(dependency) => Some(dependency.git()),
        }
    }

    /// The revision value for a Git dependency, if this is one.
    #[must_use]
    pub fn rev(&self) -> Option<&ProjectString> {
        match self {
            Self::Path(_) => None,
            Self::Git(dependency) => Some(dependency.rev()),
        }
    }

    fn canonical(&self) -> String {
        match self {
            Self::Path(dependency) => dependency.canonical(),
            Self::Git(dependency) => dependency.canonical(),
        }
    }
}

/// A local path dependency.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathDependency {
    alias: ProjectAtom,
    kind: ProjectAtom,
    path: ProjectString,
    target: Option<ProjectAtom>,
    fields: Vec<ProjectField>,
    span: ByteSpan,
    origin: ProjectOrigin,
    raw: String,
}

impl PathDependency {
    /// The dependency alias.
    #[must_use]
    pub const fn alias(&self) -> &ProjectAtom {
        &self.alias
    }

    /// The `@path` kind atom.
    #[must_use]
    pub const fn kind_atom(&self) -> &ProjectAtom {
        &self.kind
    }

    /// The local path string.
    #[must_use]
    pub const fn path(&self) -> &ProjectString {
        &self.path
    }

    /// The optional library-target reference.
    #[must_use]
    pub const fn target(&self) -> Option<&ProjectAtom> {
        self.target.as_ref()
    }

    /// Every dependency field in source order, with label and value provenance.
    #[must_use]
    pub fn fields(&self) -> &[ProjectField] {
        &self.fields
    }

    /// The complete dependency record span.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    /// The source identity owning the dependency record.
    #[must_use]
    pub const fn origin(&self) -> &ProjectOrigin {
        &self.origin
    }

    /// The exact source bytes covered by the dependency record.
    #[must_use]
    pub fn raw(&self) -> &str {
        &self.raw
    }

    fn canonical(&self) -> String {
        let mut output = format!(
            "(record kind: {} path: {}",
            self.kind.raw(),
            self.path.raw()
        );
        if let Some(target) = &self.target {
            output.push_str(" target: ");
            output.push_str(target.raw());
        }
        output.push(')');
        output
    }
}

/// An HTTPS Git dependency.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitDependency {
    alias: ProjectAtom,
    kind: ProjectAtom,
    git: ProjectString,
    rev: ProjectString,
    target: Option<ProjectAtom>,
    fields: Vec<ProjectField>,
    span: ByteSpan,
    origin: ProjectOrigin,
    raw: String,
}

impl GitDependency {
    /// The dependency alias.
    #[must_use]
    pub const fn alias(&self) -> &ProjectAtom {
        &self.alias
    }

    /// The `@git` kind atom.
    #[must_use]
    pub const fn kind_atom(&self) -> &ProjectAtom {
        &self.kind
    }

    /// The HTTPS Git URL.
    #[must_use]
    pub const fn git(&self) -> &ProjectString {
        &self.git
    }

    /// The full lowercase hexadecimal revision.
    #[must_use]
    pub const fn rev(&self) -> &ProjectString {
        &self.rev
    }

    /// The optional library-target reference.
    #[must_use]
    pub const fn target(&self) -> Option<&ProjectAtom> {
        self.target.as_ref()
    }

    /// Every dependency field in source order, with label and value provenance.
    #[must_use]
    pub fn fields(&self) -> &[ProjectField] {
        &self.fields
    }

    /// The complete dependency record span.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    /// The source identity owning the dependency record.
    #[must_use]
    pub const fn origin(&self) -> &ProjectOrigin {
        &self.origin
    }

    /// The exact source bytes covered by the dependency record.
    #[must_use]
    pub fn raw(&self) -> &str {
        &self.raw
    }

    fn canonical(&self) -> String {
        let mut output = format!(
            "(record kind: {} git: {} rev: {}",
            self.kind.raw(),
            self.git.raw(),
            self.rev.raw()
        );
        if let Some(target) = &self.target {
            output.push_str(" target: ");
            output.push_str(target.raw());
        }
        output.push(')');
        output
    }
}

/// A fully decoded `@project.v1` record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    format: ProjectAtom,
    package: Package,
    targets: Vec<Target>,
    dependencies: Vec<Dependency>,
    fields: Vec<ProjectField>,
    span: ByteSpan,
    origin: ProjectOrigin,
    raw: String,
}

impl Project {
    /// The validated `@project.v1` format atom, retained as a value.
    #[must_use]
    pub const fn format(&self) -> &ProjectAtom {
        &self.format
    }

    /// The package provenance record.
    #[must_use]
    pub const fn package(&self) -> &Package {
        &self.package
    }

    /// Local targets in source array order.
    #[must_use]
    pub fn targets(&self) -> &[Target] {
        &self.targets
    }

    /// Dependency entries in source map order.
    #[must_use]
    pub fn dependencies(&self) -> &[Dependency] {
        &self.dependencies
    }

    /// Every project field in source order, with label and value provenance.
    #[must_use]
    pub fn fields(&self) -> &[ProjectField] {
        &self.fields
    }

    /// The complete project record span.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    /// The source identity owning the project record.
    #[must_use]
    pub const fn origin(&self) -> &ProjectOrigin {
        &self.origin
    }

    /// The exact source bytes covered by the project record, including
    /// comments retained by the generic data node.
    #[must_use]
    pub fn raw(&self) -> &str {
        &self.raw
    }

    /// Returns canonical VIBON text in project schema order with one trailing
    /// newline.
    #[must_use]
    pub fn canonical_vibon(&self) -> String {
        if self.raw.contains(';')
            && let Some(formatted) = canonical_commented_project(&self.raw)
        {
            return formatted;
        }
        let mut output = format!(
            "(record format: {} package: (record name: {} version: {}) targets: (array",
            self.format.raw(),
            self.package.name.raw(),
            self.package.version.raw()
        );
        for target in &self.targets {
            output.push(' ');
            output.push_str(&canonical_target(target));
        }
        output.push_str(") dependencies: (map");

        let mut dependencies = self.dependencies.iter().collect::<Vec<_>>();
        dependencies.sort_by(|left, right| {
            left.alias()
                .raw()
                .as_bytes()
                .cmp(right.alias().raw().as_bytes())
        });
        for dependency in dependencies {
            output.push(' ');
            output.push_str(dependency.alias().raw());
            output.push(' ');
            output.push_str(&dependency.canonical());
        }
        output.push_str("))\n");
        output
    }
}

fn canonical_commented_project(raw: &str) -> Option<String> {
    let document = parse_data("project.vibon", raw).ok()?;
    if !document.accepted() {
        return None;
    }
    let reordered = rewrite_schema_data(document.root(), raw);
    let reordered_document = parse_data("project.vibon", &reordered).ok()?;
    if !reordered_document.accepted() {
        return None;
    }
    Some(format_document(&reordered_document))
}

/// Reorders only the closed project records and dependency map while retaining
/// every trivia node. The ordinary formatter then owns whitespace and comment
/// attachment; this pass supplies the schema order that generic data lacks.
fn rewrite_schema_data(node: &CstNode, source: &str) -> String {
    if node.children().is_empty() {
        return node.to_source();
    }
    let children = node.children();
    let rewritten = children
        .iter()
        .map(|child| rewrite_schema_data(child, source))
        .collect::<Vec<_>>();
    let significant = children
        .iter()
        .enumerate()
        .filter(|(_, child)| {
            matches!(child.kind(), SyntaxKind::Atom | SyntaxKind::List)
        })
        .collect::<Vec<_>>();
    if node.kind() != SyntaxKind::List {
        return rewritten.concat();
    }
    let Some(head) = significant.first().and_then(|(_, child)| child.leaf_text())
    else {
        return rewritten.concat();
    };
    match head {
        "record" => reorder_record(children, &rewritten, &significant, source),
        "map" => reorder_map(children, &rewritten, &significant, source),
        _ => rewritten.concat(),
    }
}

// The preceding shape checks establish these indexes from the lossless CST.
#[allow(clippy::indexing_slicing)]
fn reorder_record(
    children: &[CstNode],
    rewritten: &[String],
    significant: &[(usize, &CstNode)],
    source: &str,
) -> String {
    let Some(order) = record_schema_order(significant) else {
        return rewritten.concat();
    };
    if significant.len() < 3 || !(significant.len() - 1).is_multiple_of(2) {
        return rewritten.concat();
    }
    let head_index = significant[0].0;
    let mut fields = Vec::new();
    for pair in significant[1..].chunks_exact(2) {
        let value_index = pair[1].0;
        fields.push(RecordChunk {
            label: pair[0]
                .1
                .leaf_text()
                .unwrap_or_default()
                .trim_end_matches(':')
                .to_owned(),
            label_index: pair[0].0,
            value_index,
            body: rewritten[pair[0].0..=value_index].concat(),
            leading: String::new(),
            trailing: String::new(),
        });
    }
    attach_gaps(
        children,
        rewritten,
        significant,
        head_index,
        &mut fields,
        source,
    );
    let (final_attachment, final_suffix) = split_final_tail(
        children,
        rewritten,
        significant.last().map_or(head_index, |(index, _)| *index),
    );
    if let Some(last) = fields.last_mut() {
        last.trailing.push_str(&final_attachment);
    }
    fields.sort_by_key(|field| {
        order
            .iter()
            .position(|candidate| *candidate == field.label)
            .unwrap_or(usize::MAX)
    });
    let mut output = rewritten[..=head_index].concat();
    for field in fields {
        if field.leading.is_empty() {
            output.push(' ');
        } else {
            output.push_str(&field.leading);
        }
        output.push_str(&field.body);
        output.push_str(&field.trailing);
    }
    output.push_str(&final_suffix);
    output
}

// The preceding shape checks establish these indexes from the lossless CST.
#[allow(clippy::indexing_slicing)]
fn reorder_map(
    children: &[CstNode],
    rewritten: &[String],
    significant: &[(usize, &CstNode)],
    source: &str,
) -> String {
    if significant.len() < 3 || !(significant.len() - 1).is_multiple_of(2) {
        return rewritten.concat();
    }
    let head_index = significant[0].0;
    let mut entries = Vec::new();
    for pair in significant[1..].chunks_exact(2) {
        entries.push(MapChunk {
            key: pair[0].1.leaf_text().unwrap_or_default().to_owned(),
            key_index: pair[0].0,
            value_index: pair[1].0,
            body: rewritten[pair[0].0..=pair[1].0].concat(),
            leading: String::new(),
            trailing: String::new(),
        });
    }
    attach_gaps(
        children,
        rewritten,
        significant,
        head_index,
        &mut entries,
        source,
    );
    let (final_attachment, final_suffix) = split_final_tail(
        children,
        rewritten,
        significant.last().map_or(head_index, |(index, _)| *index),
    );
    if let Some(last) = entries.last_mut() {
        last.trailing.push_str(&final_attachment);
    }
    entries.sort_by(|left, right| left.key.as_bytes().cmp(right.key.as_bytes()));
    let mut output = rewritten[..=head_index].concat();
    for entry in entries {
        if entry.leading.is_empty() {
            output.push(' ');
        } else {
            output.push_str(&entry.leading);
        }
        output.push_str(&entry.body);
        output.push_str(&entry.trailing);
    }
    output.push_str(&final_suffix);
    output
}

struct RecordChunk {
    label: String,
    label_index: usize,
    value_index: usize,
    body: String,
    leading: String,
    trailing: String,
}

struct MapChunk {
    key: String,
    key_index: usize,
    value_index: usize,
    body: String,
    leading: String,
    trailing: String,
}

trait ReorderChunk {
    fn first_index(&self) -> usize;
    fn value_index(&self) -> usize;
    fn leading_mut(&mut self) -> &mut String;
    fn trailing_mut(&mut self) -> &mut String;
}

impl ReorderChunk for RecordChunk {
    fn first_index(&self) -> usize {
        self.label_index
    }

    fn value_index(&self) -> usize {
        self.value_index
    }

    fn leading_mut(&mut self) -> &mut String {
        &mut self.leading
    }

    fn trailing_mut(&mut self) -> &mut String {
        &mut self.trailing
    }
}

impl ReorderChunk for MapChunk {
    fn first_index(&self) -> usize {
        self.key_index
    }

    fn value_index(&self) -> usize {
        self.value_index
    }

    fn leading_mut(&mut self) -> &mut String {
        &mut self.leading
    }

    fn trailing_mut(&mut self) -> &mut String {
        &mut self.trailing
    }
}

#[allow(clippy::indexing_slicing)]
fn attach_gaps<T: ReorderChunk>(
    children: &[CstNode],
    rewritten: &[String],
    significant: &[(usize, &CstNode)],
    head_index: usize,
    chunks: &mut [T],
    source: &str,
) {
    for index in 0..chunks.len() {
        let previous_index = if index == 0 {
            head_index
        } else {
            chunks[index - 1].value_index()
        };
        let gap_start = previous_index.saturating_add(1);
        let gap_end = chunks[index].first_index();
        let previous_node = significant
            .get(if index == 0 { 0 } else { index * 2 })
            .map(|(_, node)| *node);
        let (previous, current) = if index == 0 {
            (None, &mut chunks[index])
        } else {
            let (before, current) = chunks.split_at_mut(index);
            (before.get_mut(index - 1), &mut current[0])
        };
        attach_gap(
            children,
            rewritten,
            gap_start,
            gap_end,
            previous_node,
            previous,
            current,
            source,
        );
    }
}

#[allow(clippy::indexing_slicing)]
fn split_final_tail(
    children: &[CstNode],
    rewritten: &[String],
    final_value: usize,
) -> (String, String) {
    let tail_start = final_value.saturating_add(1);
    let Some(tail) = children.get(tail_start..) else {
        return (String::new(), String::new());
    };
    let Some(last_comment) = tail
        .iter()
        .enumerate()
        .filter(|(_, child)| child.kind() == SyntaxKind::LineComment)
        .map(|(index, _)| tail_start + index)
        .next_back()
    else {
        return (String::new(), rewritten[tail_start..].concat());
    };
    let mut attachment_end = last_comment;
    for (index, child) in children
        .get(last_comment.saturating_add(1)..)
        .unwrap_or_default()
        .iter()
        .enumerate()
    {
        let absolute = last_comment.saturating_add(1).saturating_add(index);
        if child.kind() == SyntaxKind::Whitespace
            && child
                .leaf_text()
                .is_some_and(|text| text.contains(['\r', '\n']))
        {
            attachment_end = absolute;
            break;
        }
    }
    (
        rewritten[tail_start..=attachment_end].concat(),
        rewritten[attachment_end.saturating_add(1)..].concat(),
    )
}

#[allow(clippy::indexing_slicing, clippy::too_many_arguments)]
fn attach_gap<T: ReorderChunk>(
    children: &[CstNode],
    rewritten: &[String],
    gap_start: usize,
    gap_end: usize,
    previous_node: Option<&CstNode>,
    mut previous: Option<&mut T>,
    current: &mut T,
    source: &str,
) {
    if gap_start >= gap_end {
        return;
    }
    let comments = children
        .get(gap_start..gap_end)
        .unwrap_or_default()
        .iter()
        .enumerate()
        .filter(|(_, child)| child.kind() == SyntaxKind::LineComment)
        .map(|(offset, child)| (gap_start + offset, child))
        .collect::<Vec<_>>();
    if comments.is_empty() {
        current
            .leading_mut()
            .push_str(&rewritten[gap_start..gap_end].concat());
        return;
    }
    let mut segment_start = gap_start;
    let mut remainder_to_previous = false;
    for (comment_index, comment) in comments {
        let attach_to_previous = previous_node.is_some_and(|previous| {
            !has_source_line_break(
                source,
                previous.span().end(),
                comment.span().start(),
            )
        });
        let mut segment_end = comment_index.saturating_add(1);
        let mut trailing_line_break = false;
        if attach_to_previous && previous.is_some() {
            while let Some(child) = children.get(segment_end) {
                if child.kind() != SyntaxKind::Whitespace {
                    break;
                }
                segment_end = segment_end.saturating_add(1);
                if child
                    .leaf_text()
                    .is_some_and(|text| text.contains(['\r', '\n']))
                {
                    trailing_line_break = true;
                    break;
                }
            }
        }
        let segment = rewritten[segment_start..segment_end].concat();
        if attach_to_previous && let Some(previous) = previous.as_deref_mut() {
            previous.trailing_mut().push_str(&segment);
        } else {
            current.leading_mut().push_str(&segment);
        }
        remainder_to_previous =
            attach_to_previous && previous.is_some() && !trailing_line_break;
        segment_start = segment_end;
    }
    if segment_start < gap_end {
        let segment = rewritten[segment_start..gap_end].concat();
        if remainder_to_previous && let Some(previous) = previous {
            previous.trailing_mut().push_str(&segment);
        } else {
            current.leading_mut().push_str(&segment);
        }
    }
}

fn has_source_line_break(source: &str, start: usize, end: usize) -> bool {
    source
        .get(start..end)
        .is_some_and(|text| text.contains(['\r', '\n']))
}

fn record_schema_order(
    significant: &[(usize, &CstNode)],
) -> Option<&'static [&'static str]> {
    let labels = significant
        .get(1..)?
        .iter()
        .step_by(2)
        .map(|(_, node)| node.leaf_text().map(|label| label.trim_end_matches(':')))
        .collect::<Option<Vec<_>>>()?;
    let order = match labels.as_slice() {
        labels
            if labels.len() == 4
                && labels.contains(&"format")
                && labels.contains(&"package")
                && labels.contains(&"targets")
                && labels.contains(&"dependencies") =>
        {
            PROJECT_ORDER
        }
        labels
            if labels.len() == 2
                && labels.contains(&"name")
                && labels.contains(&"version") =>
        {
            PACKAGE_ORDER
        }
        labels if labels.contains(&"root") && labels.contains(&"kind") => TARGET_ORDER,
        labels if labels.contains(&"path") => PATH_ORDER,
        labels if labels.contains(&"git") => GIT_ORDER,
        _ => return None,
    };
    Some(order)
}

fn canonical_target(target: &Target) -> String {
    let mut output = format!(
        "(record name: {} kind: {} root: {}",
        target.name.raw(),
        match target.kind {
            TargetKind::Bin => "@bin",
            TargetKind::Lib => "@lib",
        },
        target.root.raw()
    );
    if let Some(entry) = &target.entry {
        output.push_str(" entry: ");
        output.push_str(entry.raw());
    }
    if let Some(effects) = &target.effects {
        output.push_str(" effects: (array");
        for effect in effects {
            output.push(' ');
            output.push_str(effect.raw());
        }
        output.push(')');
    }
    output.push(')');
    output
}

/// The result of typed project decoding.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectDecode {
    project: Option<Project>,
    diagnostics: Vec<Diagnostic>,
}

impl ProjectDecode {
    /// The decoded project if every schema check succeeded.
    #[must_use]
    pub const fn project(&self) -> Option<&Project> {
        self.project.as_ref()
    }

    /// Diagnostics emitted by the typed decoder.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Whether the project schema was accepted.
    #[must_use]
    pub const fn accepted(&self) -> bool {
        self.project.is_some()
    }
}

/// Decoder for the closed M2 `@project.v1` schema.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProjectDecoder;

impl ProjectDecoder {
    /// Decodes a generic data root without resolving any atom or reading any
    /// external input.
    #[must_use]
    pub fn decode(root: &DataNode, origin: ProjectOrigin) -> ProjectDecode {
        let mut diagnostics = Vec::new();
        let project = decode_project(root, &origin, &mut diagnostics);
        ProjectDecode {
            project: project.filter(|_| diagnostics.is_empty()),
            diagnostics,
        }
    }
}

fn decode_project(
    node: &DataNode,
    origin: &ProjectOrigin,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Project> {
    let fields = record_fields(node, PROJECT_FIELDS, origin, diagnostics)?;
    let field_provenance = project_fields(node, origin);
    let format = required_atom_field(
        &fields,
        "format",
        ProjectAtomRole::Value,
        node,
        origin,
        diagnostics,
    )?;
    if format.atom.value() != "project.v1" {
        invalid_value(
            &format,
            origin,
            diagnostics,
            "project format must be @project.v1",
        );
        return None;
    }
    let package_node = required_field(&fields, "package", node, origin, diagnostics)?;
    let package = decode_package(package_node, origin, diagnostics)?;
    let targets_node = required_field(&fields, "targets", node, origin, diagnostics)?;
    let targets = decode_targets(targets_node, origin, diagnostics)?;
    if targets.is_empty() {
        invalid_shape(
            targets_node,
            origin,
            diagnostics,
            "a project needs at least one target",
        );
        return None;
    }
    let dependencies_node =
        required_field(&fields, "dependencies", node, origin, diagnostics)?;
    let dependencies = decode_dependencies(dependencies_node, origin, diagnostics)?;
    let target_names = targets
        .iter()
        .map(|target| (target.name().raw().to_owned(), target.name().span()))
        .collect::<BTreeMap<_, _>>();
    for dependency in &dependencies {
        if let Some(target_span) = target_names.get(dependency.alias().raw()) {
            diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::NameMemberCollision,
                    dependency.alias().span(),
                    "target and dependency aliases share one project namespace",
                )
                .with_source_id(origin.source_id())
                .with_related(*target_span, "a target already uses this project name"),
            );
        }
    }
    Some(Project {
        format,
        package,
        targets,
        dependencies,
        fields: field_provenance,
        span: node.span(),
        origin: origin.clone(),
        raw: node.raw().to_owned(),
    })
}

fn decode_package(
    node: &DataNode,
    origin: &ProjectOrigin,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Package> {
    let fields = record_fields(node, PACKAGE_FIELDS, origin, diagnostics)?;
    let field_provenance = project_fields(node, origin);
    let name = string_field(&fields, "name", node, origin, diagnostics)?;
    if !is_kebab_name(name.value()) {
        invalid_value(
            &name,
            origin,
            diagnostics,
            "package name must be kebab-case",
        );
        return None;
    }
    let version = string_field(&fields, "version", node, origin, diagnostics)?;
    if !is_semver(version.value()) {
        invalid_value(
            &version,
            origin,
            diagnostics,
            "package version must be one semantic version",
        );
        return None;
    }
    Some(Package {
        name,
        version,
        fields: field_provenance,
        span: node.span(),
        origin: origin.clone(),
        raw: node.raw().to_owned(),
    })
}

fn decode_targets(
    node: &DataNode,
    origin: &ProjectOrigin,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Vec<Target>> {
    let DataValue::Array(values) = node.value() else {
        invalid_shape(
            node,
            origin,
            diagnostics,
            "project targets must be an array",
        );
        return None;
    };
    let mut targets = Vec::with_capacity(values.len());
    let mut seen = BTreeMap::new();
    for value in values {
        let target = decode_target(value, origin, diagnostics)?;
        if let Some(previous) =
            seen.insert(target.name().raw().to_owned(), target.name().span())
        {
            diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::NameMemberCollision,
                    target.name().span(),
                    "target names must be unique in the project namespace",
                )
                .with_source_id(origin.source_id())
                .with_related(previous, "the first target uses this name"),
            );
            return None;
        }
        targets.push(target);
    }
    Some(targets)
}

fn decode_target(
    node: &DataNode,
    origin: &ProjectOrigin,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Target> {
    let fields = record_fields(node, TARGET_FIELDS, origin, diagnostics)?;
    let field_provenance = project_fields(node, origin);
    let name = required_atom_field(
        &fields,
        "name",
        ProjectAtomRole::Value,
        node,
        origin,
        diagnostics,
    )?;
    if !is_unit_name(name.atom.value()) {
        invalid_value(
            &name,
            origin,
            diagnostics,
            "target name must be one kebab-name atom component",
        );
        return None;
    }
    let kind_atom = required_atom_field(
        &fields,
        "kind",
        ProjectAtomRole::Value,
        node,
        origin,
        diagnostics,
    )?;
    let kind = match kind_atom.atom.value() {
        "bin" => TargetKind::Bin,
        "lib" => TargetKind::Lib,
        _ => {
            invalid_value(
                &kind_atom,
                origin,
                diagnostics,
                "target kind must be @bin or @lib",
            );
            return None;
        }
    };
    let root = string_field(&fields, "root", node, origin, diagnostics)?;
    let entry = optional_atom_field(
        &fields,
        "entry",
        ProjectAtomRole::Reference(EntityKind::Declaration),
        origin,
        diagnostics,
    )?;
    let effects = optional_atom_array_field(
        &fields,
        "effects",
        ProjectAtomRole::Reference(EntityKind::Effect),
        origin,
        diagnostics,
    )?;
    match kind {
        TargetKind::Bin if entry.is_none() || effects.is_none() => {
            invalid_shape(
                node,
                origin,
                diagnostics,
                "a binary target requires entry and effects",
            );
            return None;
        }
        TargetKind::Lib if entry.is_some() || effects.is_some() => {
            invalid_shape(
                node,
                origin,
                diagnostics,
                "a library target omits entry and effects",
            );
            return None;
        }
        TargetKind::Bin | TargetKind::Lib => {}
    }
    Some(Target {
        name,
        kind_atom: kind_atom.clone(),
        kind,
        root,
        entry,
        effects,
        fields: field_provenance,
        span: node.span(),
        origin: origin.clone(),
        raw: node.raw().to_owned(),
    })
}

fn decode_dependencies(
    node: &DataNode,
    origin: &ProjectOrigin,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Vec<Dependency>> {
    let DataValue::Map(entries) = node.value() else {
        invalid_shape(
            node,
            origin,
            diagnostics,
            "project dependencies must be a map",
        );
        return None;
    };
    let mut dependencies = Vec::with_capacity(entries.len());
    for (alias_node, value_node) in entries {
        let alias = atom(alias_node, ProjectAtomRole::Value, origin, diagnostics)?;
        if !is_unit_name(alias.atom.value()) {
            invalid_value(
                &alias,
                origin,
                diagnostics,
                "dependency alias must be one kebab-name atom component",
            );
            return None;
        }
        let fields = record_fields(value_node, &[], origin, diagnostics)?;
        let kind = required_atom_field(
            &fields,
            "kind",
            ProjectAtomRole::Value,
            value_node,
            origin,
            diagnostics,
        )?;
        match kind.atom.value() {
            "path" => {
                let fields = record_fields(
                    value_node,
                    PATH_DEPENDENCY_FIELDS,
                    origin,
                    diagnostics,
                )?;
                let kind = required_atom_field(
                    &fields,
                    "kind",
                    ProjectAtomRole::Value,
                    value_node,
                    origin,
                    diagnostics,
                )?;
                let path =
                    string_field(&fields, "path", value_node, origin, diagnostics)?;
                let target = optional_atom_field(
                    &fields,
                    "target",
                    ProjectAtomRole::Reference(EntityKind::LibraryTarget),
                    origin,
                    diagnostics,
                )?;
                dependencies.push(Dependency::Path(PathDependency {
                    alias,
                    kind,
                    path,
                    target,
                    fields: project_fields(value_node, origin),
                    span: value_node.span(),
                    origin: origin.clone(),
                    raw: value_node.raw().to_owned(),
                }));
            }
            "git" => {
                let fields = record_fields(
                    value_node,
                    GIT_DEPENDENCY_FIELDS,
                    origin,
                    diagnostics,
                )?;
                let kind = required_atom_field(
                    &fields,
                    "kind",
                    ProjectAtomRole::Value,
                    value_node,
                    origin,
                    diagnostics,
                )?;
                let git =
                    string_field(&fields, "git", value_node, origin, diagnostics)?;
                if !is_https_url(git.value()) {
                    invalid_value(
                        &git,
                        origin,
                        diagnostics,
                        "Git dependency URL must use HTTPS",
                    );
                    return None;
                }
                let rev =
                    string_field(&fields, "rev", value_node, origin, diagnostics)?;
                if !is_revision(rev.value()) {
                    invalid_value(
                        &rev,
                        origin,
                        diagnostics,
                        "Git dependency revision must be 40 lowercase hexadecimal characters",
                    );
                    return None;
                }
                let target = optional_atom_field(
                    &fields,
                    "target",
                    ProjectAtomRole::Reference(EntityKind::LibraryTarget),
                    origin,
                    diagnostics,
                )?;
                dependencies.push(Dependency::Git(GitDependency {
                    alias,
                    kind,
                    git,
                    rev,
                    target,
                    fields: project_fields(value_node, origin),
                    span: value_node.span(),
                    origin: origin.clone(),
                    raw: value_node.raw().to_owned(),
                }));
            }
            _ => {
                invalid_value(
                    &kind,
                    origin,
                    diagnostics,
                    "dependency kind must be @path or @git",
                );
                return None;
            }
        }
    }
    Some(dependencies)
}

fn record_fields<'a>(
    node: &'a DataNode,
    allowed: &[&str],
    origin: &ProjectOrigin,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<BTreeMap<String, &'a DataField>> {
    let DataValue::Record(fields) = node.value() else {
        invalid_shape(
            node,
            origin,
            diagnostics,
            "project schema value must be a record",
        );
        return None;
    };
    let mut result = BTreeMap::new();
    for field in fields {
        let label = field.label().value();
        if !allowed.is_empty() && !allowed.contains(&label) {
            invalid_shape(
                field.value(),
                origin,
                diagnostics,
                "unknown project record field",
            );
            return None;
        }
        if result.insert(label.to_owned(), field).is_some() {
            invalid_shape(
                field.value(),
                origin,
                diagnostics,
                "duplicate project record field",
            );
            return None;
        }
    }
    Some(result)
}

fn project_fields(node: &DataNode, origin: &ProjectOrigin) -> Vec<ProjectField> {
    let DataValue::Record(fields) = node.value() else {
        return Vec::new();
    };
    fields
        .iter()
        .map(|field| ProjectField {
            label: field.label().clone(),
            label_span: field.label_span(),
            value_span: field.value().span(),
            span: field.span(),
            origin: origin.clone(),
        })
        .collect()
}

fn required_field<'a>(
    fields: &BTreeMap<String, &'a DataField>,
    name: &str,
    owner: &DataNode,
    origin: &ProjectOrigin,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<&'a DataNode> {
    fields.get(name).map(|field| field.value()).or_else(|| {
        invalid_shape(
            owner,
            origin,
            diagnostics,
            "required project field is missing",
        );
        None
    })
}

fn required_atom_field(
    fields: &BTreeMap<String, &DataField>,
    name: &str,
    role: ProjectAtomRole,
    owner: &DataNode,
    origin: &ProjectOrigin,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<ProjectAtom> {
    fields
        .get(name)
        .map(|field| atom(field.value(), role, origin, diagnostics))
        .unwrap_or_else(|| {
            invalid_shape(
                owner,
                origin,
                diagnostics,
                "required project field is missing",
            );
            None
        })
}

fn optional_atom_field(
    fields: &BTreeMap<String, &DataField>,
    name: &str,
    role: ProjectAtomRole,
    origin: &ProjectOrigin,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Option<ProjectAtom>> {
    fields
        .get(name)
        .map(|field| atom(field.value(), role, origin, diagnostics).map(Some))
        .unwrap_or(Some(None))
}

fn string_field(
    fields: &BTreeMap<String, &DataField>,
    name: &str,
    owner: &DataNode,
    origin: &ProjectOrigin,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<ProjectString> {
    fields
        .get(name)
        .map(|field| string(field.value(), origin, diagnostics))
        .unwrap_or_else(|| {
            invalid_shape(
                owner,
                origin,
                diagnostics,
                "required project field is missing",
            );
            None
        })
}

fn optional_atom_array_field(
    fields: &BTreeMap<String, &DataField>,
    name: &str,
    role: ProjectAtomRole,
    origin: &ProjectOrigin,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Option<Vec<ProjectAtom>>> {
    let Some(field) = fields.get(name) else {
        return Some(None);
    };
    let DataValue::Array(values) = field.value().value() else {
        invalid_shape(
            field.value(),
            origin,
            diagnostics,
            "project effects must be an array",
        );
        return None;
    };
    let mut atoms = Vec::with_capacity(values.len());
    for value in values {
        atoms.push(atom(value, role, origin, diagnostics)?);
    }
    Some(Some(atoms))
}

fn atom(
    node: &DataNode,
    role: ProjectAtomRole,
    origin: &ProjectOrigin,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<ProjectAtom> {
    let DataValue::Atom(atom) = node.value() else {
        invalid_shape(
            node,
            origin,
            diagnostics,
            "project atom field must contain an atom",
        );
        return None;
    };
    Some(ProjectAtom {
        atom: atom.clone(),
        role,
        span: node.span(),
        origin: origin.clone(),
        raw: node.raw().to_owned(),
    })
}

fn string(
    node: &DataNode,
    origin: &ProjectOrigin,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<ProjectString> {
    let DataValue::Literal(Literal::String(value)) = node.value() else {
        invalid_shape(
            node,
            origin,
            diagnostics,
            "project string field must contain a string",
        );
        return None;
    };
    Some(ProjectString {
        value: value.value().to_owned(),
        span: node.span(),
        origin: origin.clone(),
        raw: node.raw().to_owned(),
    })
}

fn invalid_shape(
    node: &DataNode,
    origin: &ProjectOrigin,
    diagnostics: &mut Vec<Diagnostic>,
    message: &str,
) {
    diagnostics.push(
        Diagnostic::new(DiagnosticCode::DataInvalidShape, node.span(), message)
            .with_source_id(origin.source_id()),
    );
}

fn invalid_value<T>(
    value: &T,
    origin: &ProjectOrigin,
    diagnostics: &mut Vec<Diagnostic>,
    message: &str,
) where
    T: ProjectValueSpan,
{
    diagnostics.push(
        Diagnostic::new(DiagnosticCode::DataInvalidValue, value.span(), message)
            .with_source_id(origin.source_id()),
    );
}

trait ProjectValueSpan {
    fn span(&self) -> ByteSpan;
}

impl ProjectValueSpan for ProjectAtom {
    fn span(&self) -> ByteSpan {
        self.span
    }
}

impl ProjectValueSpan for ProjectString {
    fn span(&self) -> ByteSpan {
        self.span
    }
}

fn is_kebab_name(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.first().is_none_or(|byte| !byte.is_ascii_lowercase())
        || bytes.last() == Some(&b'-')
    {
        return false;
    }
    let mut previous_hyphen = false;
    for byte in bytes.iter().copied() {
        if byte == b'-' {
            if previous_hyphen {
                return false;
            }
            previous_hyphen = true;
        } else if byte.is_ascii_lowercase() || byte.is_ascii_digit() {
            previous_hyphen = false;
        } else {
            return false;
        }
    }
    true
}

fn is_unit_name(value: &str) -> bool {
    !value.contains('.') && is_kebab_name(value)
}

fn is_semver(value: &str) -> bool {
    let (core, build) = value
        .split_once('+')
        .map_or((value, None), |(core, build)| (core, Some(build)));
    if let Some(build) = build
        && !valid_identifiers(build, false)
    {
        return false;
    }
    let (core, prerelease) = core
        .split_once('-')
        .map_or((core, None), |(core, prerelease)| (core, Some(prerelease)));
    let numbers = core.split('.').collect::<Vec<_>>();
    if numbers.len() != 3 || numbers.iter().any(|part| !valid_numeric_identifier(part))
    {
        return false;
    }
    prerelease.is_none_or(|value| valid_identifiers(value, true))
}

fn valid_identifiers(value: &str, numeric_leading_zero_rule: bool) -> bool {
    !value.is_empty()
        && value.split('.').all(|identifier| {
            !identifier.is_empty()
                && identifier
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                && (!numeric_leading_zero_rule
                    || !identifier.bytes().all(|byte| byte.is_ascii_digit())
                    || valid_numeric_identifier(identifier))
        })
}

fn valid_numeric_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && (value == "0" || !value.starts_with('0'))
}

fn is_https_url(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("https://") else {
        return false;
    };
    if rest.is_empty()
        || value
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
    {
        return false;
    }
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    if authority.is_empty() || authority.contains('@') {
        return false;
    }

    if let Some(bracketed) = authority.strip_prefix('[') {
        let Some(close) = bracketed.find(']') else {
            return false;
        };
        let host = &bracketed[..close];
        if host.parse::<std::net::Ipv6Addr>().is_err() {
            return false;
        }
        let suffix = &bracketed[close + 1..];
        let port = suffix.strip_prefix(':');
        return suffix.is_empty() && port.is_none() || port.is_some_and(valid_port);
    }

    let mut pieces = authority.split(':');
    let host = pieces.next().unwrap_or_default();
    let port = pieces.next();
    if pieces.next().is_some() {
        return false;
    }

    if !is_https_host(host) {
        return false;
    }
    port.is_none_or(valid_port)
}

fn valid_port(port: &str) -> bool {
    !port.is_empty()
        && port.bytes().all(|byte| byte.is_ascii_digit())
        && port.parse::<u16>().is_ok_and(|number| number != 0)
}

fn is_https_host(host: &str) -> bool {
    !host.is_empty()
        && host.split('.').all(|label| {
            !label.is_empty()
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                && label
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_alphanumeric)
                && label
                    .as_bytes()
                    .last()
                    .is_some_and(u8::is_ascii_alphanumeric)
        })
}

fn is_revision(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::{is_https_url, is_kebab_name, is_revision, is_semver};

    #[test]
    fn package_name_validation_is_kebab_case() {
        assert!(is_kebab_name("hello-world2"));
        assert!(!is_kebab_name("Hello"));
        assert!(!is_kebab_name("hello-"));
        assert!(!is_kebab_name("hello--world"));
    }

    #[test]
    fn semantic_versions_are_single_versions() {
        assert!(is_semver("0.1.0"));
        assert!(is_semver("1.2.3-alpha.1+build.7"));
        assert!(!is_semver("1.2"));
        assert!(!is_semver("1.2.3 || 2.0.0"));
        assert!(!is_semver("01.2.3"));
    }

    #[test]
    fn git_inputs_are_strictly_https_and_full_lowercase_revisions() {
        assert!(is_https_url("https://example.com/repo.git"));
        assert!(is_https_url(
            "https://example.com:443/repo.git?view=full#src"
        ));
        assert!(is_https_url("https://[::1]:8443/repo.git"));
        assert!(!is_https_url("http://example.com/repo.git"));
        assert!(!is_https_url("https://"));
        assert!(!is_https_url("https://?repo"));
        assert!(!is_https_url("https://example.com:abc/repo.git"));
        assert!(!is_https_url("https://user@example.com/repo.git"));
        assert!(!is_https_url("https://[not-an-ipv6]/repo.git"));
        assert!(!is_https_url("https://example.com/repo\u{0}.git"));
        assert!(!is_https_url("https://example.com/repo\u{7f}.git"));
        assert!(is_revision("0123456789abcdef0123456789abcdef01234567"));
        assert!(!is_revision("0123456789ABCDEF0123456789abcdef01234567"));
    }
}

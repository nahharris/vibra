//! Declaration identity and import resolution over an explicit source graph.
//!
//! This crate deliberately owns a small, filesystem-free graph input. The
//! workspace adapter turns its immutable Step 3 snapshot into this input;
//! resolution never discovers files, reads dependencies, consults a lock, or
//! performs type, effect, runtime, or network work.

#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used
    )
)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode, Level};
use vibra_syntax::{
    Attribute, Declaration, DeftypeBody, Expression, ExpressionKind,
    FunctionDeclaration, Literal, Name, Pattern, PatternKind, TypeMember, parse_source,
};

/// Package provenance carried by every declaration identity.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackageId {
    name: String,
    version: String,
}

impl PackageId {
    /// Creates package provenance from the project record.
    #[must_use]
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
        }
    }

    /// Package name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Exact package version.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }
}

/// The target kind used by the neutral resolver input.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TargetKind {
    /// An executable target with an optional resolver entry reference.
    Bin,
    /// A library target without an executable entry.
    Lib,
}

/// A project entry or other explicit atom path supplied to the resolver.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferencePath {
    segments: Vec<String>,
    source_id: String,
    span: ByteSpan,
}

impl ReferencePath {
    /// Creates a path from its already validated atom segments.
    #[must_use]
    pub fn new(
        segments: impl IntoIterator<Item = impl Into<String>>,
        source_id: impl Into<String>,
        span: ByteSpan,
    ) -> Self {
        Self {
            segments: segments.into_iter().map(Into::into).collect(),
            source_id: source_id.into(),
            span,
        }
    }

    /// Creates a path from the dotted spelling used by a test or adapter.
    #[must_use]
    pub fn from_dotted(
        value: &str,
        source_id: impl Into<String>,
        span: ByteSpan,
    ) -> Self {
        let value = value.strip_prefix('@').unwrap_or(value);
        Self::new(value.split('.'), source_id, span)
    }

    /// Path components without the atom marker.
    #[must_use]
    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    /// Source identity owning the path.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// Half-open path span.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

/// One immutable source module supplied to resolution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceModule {
    unit: String,
    segments: Vec<String>,
    source_id: String,
    bytes: Vec<u8>,
}

impl SourceModule {
    /// Creates a module with exact immutable source bytes.
    #[must_use]
    pub fn new<S, I>(
        unit: impl Into<String>,
        segments: I,
        source_id: impl Into<String>,
        bytes: impl AsRef<[u8]>,
    ) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            unit: unit.into(),
            segments: segments.into_iter().map(Into::into).collect(),
            source_id: source_id.into(),
            bytes: bytes.as_ref().to_vec(),
        }
    }

    /// Unit name without its atom marker.
    #[must_use]
    pub fn unit(&self) -> &str {
        &self.unit
    }

    /// Target-relative module segments.
    #[must_use]
    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    /// Project-relative source identity.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// Exact source bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// One local target unit supplied to resolution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceUnit {
    name: String,
    kind: TargetKind,
    entry: Option<ReferencePath>,
    modules: Vec<SourceModule>,
}

impl SourceUnit {
    /// Creates a target unit.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        kind: TargetKind,
        entry: Option<ReferencePath>,
        modules: Vec<SourceModule>,
    ) -> Self {
        Self {
            name: name.into(),
            kind,
            entry,
            modules,
        }
    }

    /// Creates a binary unit from a dotted entry spelling.
    #[must_use]
    pub fn bin(
        name: impl Into<String>,
        entry: Option<&str>,
        modules: Vec<SourceModule>,
    ) -> Self {
        let entry = entry.map(|value| {
            ReferencePath::from_dotted(value, "project.vibon", ByteSpan::empty_at(0))
        });
        Self::new(name, TargetKind::Bin, entry, modules)
    }

    /// Creates a library unit.
    #[must_use]
    pub fn lib(name: impl Into<String>, modules: Vec<SourceModule>) -> Self {
        Self::new(name, TargetKind::Lib, None, modules)
    }

    /// Unit name without its atom marker.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Target kind.
    #[must_use]
    pub const fn kind(&self) -> TargetKind {
        self.kind
    }

    /// Optional project entry.
    #[must_use]
    pub const fn entry(&self) -> Option<&ReferencePath> {
        self.entry.as_ref()
    }

    /// Modules in this unit.
    #[must_use]
    pub fn modules(&self) -> &[SourceModule] {
        &self.modules
    }
}

/// Explicit immutable input to the resolver.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolveInput {
    package: PackageId,
    units: Vec<SourceUnit>,
    overlay: Option<PackageOverlay>,
    reserved_import_paths: Vec<(String, Vec<String>)>,
}

/// A verified package whose source modules are overlaid on the local graph.
///
/// The resolver does not establish trust for this input. Callers must only
/// provide an overlay after verifying its provenance.
#[derive(Clone, Debug, PartialEq, Eq)]
struct PackageOverlay {
    package: PackageId,
    modules: Vec<SourceModule>,
}

impl ResolveInput {
    /// Creates an input graph from package provenance and target units.
    #[must_use]
    pub fn new(
        package_name: impl Into<String>,
        package_version: impl Into<String>,
        units: Vec<SourceUnit>,
    ) -> Self {
        Self {
            package: PackageId::new(package_name, package_version),
            units,
            overlay: None,
            reserved_import_paths: Vec::new(),
        }
    }

    /// Reserves exact module paths for a separately verified package overlay.
    ///
    /// A reserved path never falls back to the local package when the overlay
    /// is absent. This lets callers report unavailable verified provenance
    /// without accidentally binding an untrusted local module with the same
    /// spelling. The resolver itself performs no trust or filesystem checks.
    #[must_use]
    pub fn with_reserved_import_paths(
        mut self,
        paths: impl IntoIterator<Item = (String, Vec<String>)>,
    ) -> Self {
        self.reserved_import_paths.extend(paths);
        self.reserved_import_paths.sort();
        self.reserved_import_paths.dedup();
        self
    }

    /// Adds modules from a separately verified package to the source graph.
    ///
    /// The modules retain this package identity in their declaration IDs.
    /// The caller is responsible for verifying the package before creating
    /// this input; ordinary dependency delivery remains outside the resolver.
    #[must_use]
    pub fn with_verified_overlay(
        mut self,
        package_name: impl Into<String>,
        package_version: impl Into<String>,
        modules: Vec<SourceModule>,
    ) -> Self {
        self.overlay = Some(PackageOverlay {
            package: PackageId::new(package_name, package_version),
            modules,
        });
        self
    }

    /// Creates a one-unit, one-module input useful for focused host tests.
    #[must_use]
    pub fn single_module(
        package_name: impl Into<String>,
        package_version: impl Into<String>,
        unit: impl Into<String>,
        module: impl Into<String>,
        bytes: impl AsRef<[u8]>,
    ) -> Self {
        let unit = unit.into();
        let module = module.into();
        let source_id = format!("src/{module}.vib");
        Self::new(
            package_name,
            package_version,
            vec![SourceUnit::bin(
                unit.clone(),
                None,
                vec![SourceModule::new(unit, [module], source_id, bytes)],
            )],
        )
    }

    /// Package provenance.
    #[must_use]
    pub const fn package(&self) -> &PackageId {
        &self.package
    }

    /// Target units.
    #[must_use]
    pub fn units(&self) -> &[SourceUnit] {
        &self.units
    }
}

/// Entity category in the declaration identity tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EntityKind {
    /// A source module.
    Module,
    /// A nominal type.
    Type,
    /// An interface.
    Interface,
    /// A nominal effect root.
    Effect,
    /// An immutable module value.
    Value,
    /// A function or method.
    Function,
    /// A test declaration.
    Test,
    /// A record field.
    Field,
    /// An enum variant.
    Variant,
    /// An effect operation.
    Operation,
}

impl EntityKind {
    /// Stable canonical artifact spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Module => "module",
            Self::Type => "type",
            Self::Interface => "interface",
            Self::Effect => "effect",
            Self::Value => "value",
            Self::Function => "function",
            Self::Test => "test",
            Self::Field => "field",
            Self::Variant => "variant",
            Self::Operation => "operation",
        }
    }
}

/// Canonical identity for one declaration.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeclarationId {
    package: PackageId,
    unit: String,
    module: Vec<String>,
    path: Vec<String>,
    kind: EntityKind,
}

impl DeclarationId {
    /// Creates an identity from the complete ownership path.
    #[must_use]
    pub fn new<S, I, O>(
        package_name: impl Into<String>,
        package_version: impl Into<String>,
        unit: impl Into<String>,
        module: I,
        path: O,
        kind: EntityKind,
    ) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
        O: IntoIterator<Item = S>,
    {
        Self {
            package: PackageId::new(package_name, package_version),
            unit: unit.into(),
            module: module.into_iter().map(Into::into).collect(),
            path: path.into_iter().map(Into::into).collect(),
            kind,
        }
    }

    fn with_package(
        package: &PackageId,
        unit: impl Into<String>,
        module: impl IntoIterator<Item = impl Into<String>>,
        path: impl IntoIterator<Item = impl Into<String>>,
        kind: EntityKind,
    ) -> Self {
        Self {
            package: package.clone(),
            unit: unit.into(),
            module: module.into_iter().map(Into::into).collect(),
            path: path.into_iter().map(Into::into).collect(),
            kind,
        }
    }

    /// Package provenance.
    #[must_use]
    pub const fn package(&self) -> &PackageId {
        &self.package
    }

    /// Owning unit.
    #[must_use]
    pub fn unit(&self) -> &str {
        &self.unit
    }

    /// Owning module path.
    #[must_use]
    pub fn module(&self) -> &[String] {
        &self.module
    }

    /// Full declaration ownership path.
    #[must_use]
    pub fn path(&self) -> &[String] {
        &self.path
    }

    /// Final declaration spelling.
    #[must_use]
    pub fn name(&self) -> &str {
        self.path.last().map(String::as_str).unwrap_or("")
    }

    /// Entity kind.
    #[must_use]
    pub const fn kind(&self) -> EntityKind {
        self.kind
    }

    /// Stable machine identity including package and ownership.
    #[must_use]
    pub fn canonical(&self) -> String {
        let mut output = format!(
            "@{}@{}/{}",
            self.package.name(),
            self.package.version(),
            self.unit
        );
        if !self.module.is_empty() {
            output.push('.');
            output.push_str(&self.module.join("."));
        }
        if !self.path.is_empty() {
            output.push('.');
            output.push_str(&self.path.join("."));
        }
        output
    }
}

/// Canonical module identity used by imports.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModuleId {
    package: PackageId,
    unit: String,
    segments: Vec<String>,
}

impl ModuleId {
    fn new(package: &PackageId, unit: impl Into<String>, segments: &[String]) -> Self {
        Self {
            package: package.clone(),
            unit: unit.into(),
            segments: segments.to_vec(),
        }
    }

    /// Package provenance.
    #[must_use]
    pub const fn package(&self) -> &PackageId {
        &self.package
    }

    /// Owning unit.
    #[must_use]
    pub fn unit(&self) -> &str {
        &self.unit
    }

    /// Target-relative module segments.
    #[must_use]
    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    /// Stable source-path atom spelling.
    #[must_use]
    pub fn as_atom(&self) -> String {
        if self.segments.is_empty() {
            format!("@{}", self.unit)
        } else {
            format!("@{}.{}", self.unit, self.segments.join("."))
        }
    }
}

/// Visibility retained on a declaration record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Visibility {
    /// Only same-module and privileged entry references may use it.
    Private,
    /// Explicitly exposed through an imported module alias.
    Public,
}

impl Visibility {
    /// Stable artifact spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Public => "public",
        }
    }
}

/// One resolved declaration header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedDeclaration {
    id: DeclarationId,
    visibility: Visibility,
    source_id: String,
    span: ByteSpan,
}

impl ResolvedDeclaration {
    /// Canonical identity.
    #[must_use]
    pub const fn id(&self) -> &DeclarationId {
        &self.id
    }

    /// Visibility marker.
    #[must_use]
    pub const fn visibility(&self) -> Visibility {
        self.visibility
    }

    /// Source identity owning the header.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// Declaration span.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

/// One resolved import alias.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedImport {
    alias: String,
    written: String,
    module: Option<ModuleId>,
    source_id: String,
    span: ByteSpan,
}

impl ResolvedImport {
    /// Lexical alias without its marker.
    #[must_use]
    pub fn alias(&self) -> &str {
        &self.alias
    }

    /// Original dotted atom spelling without its marker.
    #[must_use]
    pub fn written(&self) -> &str {
        &self.written
    }

    /// Resolved module, when module lookup succeeded.
    #[must_use]
    pub const fn module(&self) -> Option<&ModuleId> {
        self.module.as_ref()
    }

    /// Import source identity.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// Import declaration span.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

/// One body reference and its canonical target, if resolution succeeded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedReference {
    from: DeclarationId,
    written: String,
    target: Option<DeclarationId>,
    source_id: String,
    span: ByteSpan,
}

impl ResolvedReference {
    /// Declaration containing the reference.
    #[must_use]
    pub const fn from(&self) -> &DeclarationId {
        &self.from
    }

    /// Original symbol spelling.
    #[must_use]
    pub fn written(&self) -> &str {
        &self.written
    }

    /// Resolved target, if no diagnostic was emitted for this edge.
    #[must_use]
    pub const fn target(&self) -> Option<&DeclarationId> {
        self.target.as_ref()
    }

    /// Reference source identity.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// Reference span.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

/// Immutable result of declaration and import resolution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedSnapshot {
    package: PackageId,
    modules: Vec<ModuleRecord>,
    declarations: Vec<ResolvedDeclaration>,
    entries: Vec<ResolvedEntry>,
    imports: Vec<ResolvedImport>,
    references: Vec<ResolvedReference>,
    diagnostics: Vec<Diagnostic>,
}

impl ResolvedSnapshot {
    /// Whether resolution emitted no error-level diagnostics.
    #[must_use]
    pub fn accepted(&self) -> bool {
        self.diagnostics
            .iter()
            .all(|diagnostic| diagnostic.level() != Level::Error)
    }

    /// Package provenance.
    #[must_use]
    pub const fn package(&self) -> &PackageId {
        &self.package
    }

    /// Modules in deterministic unit/path order.
    #[must_use]
    pub fn modules(&self) -> &[ModuleRecord] {
        &self.modules
    }

    /// Declaration headers in canonical identity order.
    #[must_use]
    pub fn declarations(&self) -> &[ResolvedDeclaration] {
        &self.declarations
    }

    /// Project entries after the shared unit-rooted atom-path walk.
    #[must_use]
    pub fn entries(&self) -> &[ResolvedEntry] {
        &self.entries
    }

    /// Import edges in deterministic source order.
    #[must_use]
    pub fn imports(&self) -> &[ResolvedImport] {
        &self.imports
    }

    /// Body reference edges in deterministic source order.
    #[must_use]
    pub fn references(&self) -> &[ResolvedReference] {
        &self.references
    }

    /// Diagnostics in canonical order.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Canonical structured VIBON artifact for conformance snapshots.
    #[must_use]
    pub fn canonical_vibon(&self) -> String {
        let mut output = String::new();
        output.push_str("(record\n  format: @resolved.v1\n");
        let _ = writeln!(
            output,
            "  package: (record name: {} version: {})",
            quoted(&self.package.name),
            quoted(&self.package.version)
        );
        output.push_str("  modules: (array\n");
        for module in &self.modules {
            let _ = writeln!(
                output,
                "    (record unit: @{} path: {} source: {})",
                module.unit,
                if module.segments.is_empty() {
                    format!("@{}", module.unit)
                } else {
                    format!("@{}.{}", module.unit, module.segments.join("."))
                },
                quoted(&module.source_id)
            );
        }
        output.push_str("  )\n  declarations: (array\n");
        for declaration in &self.declarations {
            let _ = writeln!(
                output,
                "    (record id: {} kind: @{} visibility: @{} source: {} span: (array {}u64 {}u64))",
                quoted(&declaration.id.canonical()),
                declaration.id.kind().as_str(),
                declaration.visibility.as_str(),
                quoted(&declaration.source_id),
                declaration.span.start(),
                declaration.span.end()
            );
        }
        output.push_str("  )\n  imports: (array\n");
        for import in &self.imports {
            let module = import
                .module
                .as_ref()
                .map_or_else(|| "void".to_owned(), ModuleId::as_atom);
            let _ = writeln!(
                output,
                "    (record alias: @{} target: {} source: {} span: (array {}u64 {}u64))",
                import.alias,
                module,
                quoted(&import.source_id),
                import.span.start(),
                import.span.end()
            );
        }
        output.push_str("  )\n  references: (array\n");
        for reference in &self.references {
            let target = reference.target.as_ref().map_or_else(
                || "void".to_owned(),
                |target| quoted(&target.canonical()),
            );
            let _ = writeln!(
                output,
                "    (record from: {} written: {} target: {} source: {} span: (array {}u64 {}u64))",
                quoted(&reference.from.canonical()),
                quoted(&reference.written),
                target,
                quoted(&reference.source_id),
                reference.span.start(),
                reference.span.end()
            );
        }
        output.push_str("  )\n)\n");
        output
    }
}

/// One source module identity and provenance record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleRecord {
    package: PackageId,
    unit: String,
    segments: Vec<String>,
    source_id: String,
    bytes: Vec<u8>,
    ast: Option<vibra_syntax::SourceAst>,
}

impl ModuleRecord {
    /// Package provenance carried by declarations in this module.
    #[must_use]
    pub const fn package(&self) -> &PackageId {
        &self.package
    }

    /// Unit name.
    #[must_use]
    pub fn unit(&self) -> &str {
        &self.unit
    }

    /// Module segments.
    #[must_use]
    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    /// Source identity.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// Exact immutable source bytes supplied to the resolver.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Parsed source AST retained from this resolution snapshot, if valid UTF-8.
    #[must_use]
    pub const fn ast(&self) -> Option<&vibra_syntax::SourceAst> {
        self.ast.as_ref()
    }
}

/// One project entry after atom-path resolution and entity-kind validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedEntry {
    unit: String,
    source_id: String,
    span: ByteSpan,
    declaration: Option<DeclarationId>,
}

impl ResolvedEntry {
    /// Binary target unit that owns this entry.
    #[must_use]
    pub fn unit(&self) -> &str {
        &self.unit
    }

    /// Project document that contains the entry reference.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// Half-open span of the entry atom.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    /// Resolved declaration identity when the path names one.
    #[must_use]
    pub const fn declaration(&self) -> Option<&DeclarationId> {
        self.declaration.as_ref()
    }
}

/// Stateless resolver over one explicit input snapshot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Resolver;

impl Resolver {
    /// Resolves headers, imports, cycles, entries, and supported body names.
    #[must_use]
    pub fn resolve(input: ResolveInput) -> ResolvedSnapshot {
        Resolution::new(input).run()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct ModuleKey {
    package: PackageId,
    unit: String,
    segments: Vec<String>,
}

#[derive(Clone, Debug)]
struct ParsedModule {
    package: PackageId,
    module: SourceModule,
    ast: Option<vibra_syntax::SourceAst>,
}

#[derive(Clone, Debug)]
struct DeclarationWork {
    declaration: ResolvedDeclaration,
    body: Option<BodyWork>,
}

#[derive(Clone, Debug)]
enum BodyWork {
    Def(vibra_syntax::DefDeclaration),
    Function(FunctionDeclaration),
    Test(vibra_syntax::TestDeclaration),
}

#[derive(Clone, Debug)]
struct ImportWork {
    module: Option<ModuleKey>,
    alias: String,
    written: String,
    source_id: String,
    span: ByteSpan,
}

struct Resolution {
    input: ResolveInput,
    modules: Vec<ParsedModule>,
    module_indexes: BTreeMap<ModuleKey, usize>,
    module_bindings: BTreeMap<ModuleKey, BTreeMap<String, (ByteSpan, String)>>,
    declarations: Vec<DeclarationWork>,
    declaration_indexes: BTreeMap<(ModuleKey, Vec<String>), usize>,
    entries: Vec<ResolvedEntry>,
    imports: Vec<(ModuleKey, ImportWork)>,
    imports_by_module: BTreeMap<ModuleKey, Vec<usize>>,
    references: Vec<ResolvedReference>,
    diagnostics: Vec<Diagnostic>,
}

impl Resolution {
    fn new(input: ResolveInput) -> Self {
        Self {
            input,
            modules: Vec::new(),
            module_indexes: BTreeMap::new(),
            module_bindings: BTreeMap::new(),
            declarations: Vec::new(),
            declaration_indexes: BTreeMap::new(),
            entries: Vec::new(),
            imports: Vec::new(),
            imports_by_module: BTreeMap::new(),
            references: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    fn run(mut self) -> ResolvedSnapshot {
        self.acquire_modules();
        self.collect_headers();
        self.resolve_imports();
        self.check_import_cycles();
        self.resolve_entries();
        self.resolve_bodies();

        self.diagnostics.sort_by_key(diagnostic_key);
        self.entries.sort_by(|left, right| {
            (
                left.unit.as_str(),
                left.source_id.as_str(),
                left.span.start(),
            )
                .cmp(&(
                    right.unit.as_str(),
                    right.source_id.as_str(),
                    right.span.start(),
                ))
        });
        self.references.sort_by(|left, right| {
            (
                left.source_id.as_str(),
                left.span.start(),
                left.span.end(),
                left.written.as_str(),
            )
                .cmp(&(
                    right.source_id.as_str(),
                    right.span.start(),
                    right.span.end(),
                    right.written.as_str(),
                ))
        });
        self.declarations
            .sort_by(|left, right| left.declaration.id.cmp(&right.declaration.id));

        let imports = self
            .imports
            .iter()
            .map(|(_, import)| ResolvedImport {
                alias: import.alias.clone(),
                written: import.written.clone(),
                module: import.module.as_ref().and_then(|module| {
                    self.module_indexes.contains_key(module).then(|| {
                        ModuleId::new(&module.package, &module.unit, &module.segments)
                    })
                }),
                source_id: import.source_id.clone(),
                span: import.span,
            })
            .collect::<Vec<_>>();
        let declarations = self
            .declarations
            .into_iter()
            .map(|work| work.declaration)
            .collect::<Vec<_>>();
        let modules = self
            .modules
            .iter()
            .map(|module| ModuleRecord {
                package: module.package.clone(),
                unit: module.module.unit.clone(),
                segments: module.module.segments.clone(),
                source_id: module.module.source_id.clone(),
                bytes: module.module.bytes.clone(),
                ast: module.ast.clone(),
            })
            .collect::<Vec<_>>();
        let entries = self.entries;
        ResolvedSnapshot {
            package: self.input.package,
            modules,
            declarations,
            entries,
            imports,
            references: self.references,
            diagnostics: self.diagnostics,
        }
    }

    fn acquire_modules(&mut self) {
        let mut modules = self
            .input
            .units
            .iter()
            .flat_map(|unit| unit.modules.iter().cloned())
            .map(|module| (self.input.package.clone(), module))
            .collect::<Vec<_>>();
        if let Some(overlay) = &self.input.overlay {
            modules.extend(
                overlay
                    .modules
                    .iter()
                    .cloned()
                    .map(|module| (overlay.package.clone(), module)),
            );
        }
        modules.sort_by(|left, right| {
            (
                &left.0,
                left.1.unit.as_str(),
                left.1.segments.as_slice(),
                left.1.source_id.as_str(),
            )
                .cmp(&(
                    &right.0,
                    right.1.unit.as_str(),
                    right.1.segments.as_slice(),
                    right.1.source_id.as_str(),
                ))
        });
        let mut source_owners = BTreeMap::<String, ModuleKey>::new();
        for (package, module) in modules {
            let key = ModuleKey {
                package: package.clone(),
                unit: module.unit.clone(),
                segments: module.segments.clone(),
            };
            if self.module_indexes.contains_key(&key) {
                self.diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::ModuleFileDirectoryCollision,
                        ByteSpan::empty_at(0),
                        "two source modules share one canonical module path",
                    )
                    .with_source_id(module.source_id.clone()),
                );
                continue;
            }
            if let Some(earlier) = source_owners.get(&module.source_id) {
                self.diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::ModuleSourceIdCollision,
                        ByteSpan::empty_at(0),
                        format!(
                            "source identity `{}` belongs to more than one module",
                            module.source_id
                        ),
                    )
                    .with_source_id(module.source_id.clone())
                    .with_note(format!(
                        "conflicting package identities: `{}@{}` and `{}@{}`",
                        earlier.package.name(),
                        earlier.package.version(),
                        key.package.name(),
                        key.package.version()
                    )),
                );
            } else {
                source_owners.insert(module.source_id.clone(), key.clone());
            }
            let index = self.modules.len();
            self.module_indexes.insert(key, index);
            let ast = match std::str::from_utf8(&module.bytes) {
                Ok(source) => {
                    match parse_source(Path::new(&module.source_id), source) {
                        Ok(document) => {
                            self.diagnostics.extend(
                                document.diagnostics().iter().cloned().map(
                                    |diagnostic| {
                                        diagnostic
                                            .with_source_id(module.source_id.clone())
                                    },
                                ),
                            );
                            document.ast().cloned()
                        }
                        Err(error) => {
                            self.diagnostics.push(
                                Diagnostic::new(
                                    DiagnosticCode::ModuleIoError,
                                    ByteSpan::empty_at(0),
                                    error.to_string(),
                                )
                                .with_source_id(module.source_id.clone()),
                            );
                            None
                        }
                    }
                }
                Err(error) => {
                    self.diagnostics.push(
                        Diagnostic::new(
                            DiagnosticCode::ModuleIoError,
                            ByteSpan::empty_at(0),
                            format!("source module is not UTF-8: {error}"),
                        )
                        .with_source_id(module.source_id.clone()),
                    );
                    None
                }
            };
            self.modules.push(ParsedModule {
                package,
                module,
                ast,
            });
        }
    }

    fn collect_headers(&mut self) {
        for module_index in 0..self.modules.len() {
            let Some(module_key) = self.module_key(module_index) else {
                continue;
            };
            let Some(parsed) = self.modules.get(module_index).cloned() else {
                continue;
            };
            let Some(ast) = parsed.ast else {
                continue;
            };
            let source_id = parsed.module.source_id;
            let mut names = BTreeMap::<String, (ByteSpan, String)>::new();
            for declaration in ast.declarations() {
                if let Declaration::Import(import) = declaration {
                    self.record_import_name(
                        &mut names,
                        import.alias().value(),
                        import.span(),
                        &source_id,
                    );
                    continue;
                }
                self.collect_declaration(
                    &module_key,
                    declaration,
                    &mut names,
                    Vec::new(),
                    &source_id,
                );
            }
            self.module_bindings.insert(module_key, names);
        }
        self.collect_verified_assertion_headers();
    }

    fn collect_verified_assertion_headers(&mut self) {
        let Some(overlay) = self.input.overlay.as_ref() else {
            return;
        };
        let Some(assert_module) = overlay.modules.iter().find(|module| {
            module.unit == "std"
                && module.segments == ["assert"]
                && module.source_id == "stdlib/m2/src/std/assert.vib"
        }) else {
            return;
        };
        let module = ModuleKey {
            package: overlay.package.clone(),
            unit: "std".to_owned(),
            segments: vec!["assert".to_owned()],
        };
        let names = [
            "true",
            "false",
            "equal-bool",
            "equal-char",
            "equal-str",
            "equal-i32",
            "equal-u64",
        ];
        for name in names {
            let path = vec![name.to_owned()];
            let id = DeclarationId::with_package(
                &overlay.package,
                "std",
                ["assert"],
                path.clone(),
                EntityKind::Function,
            );
            let span = ByteSpan::empty_at(0);
            let declaration = ResolvedDeclaration {
                id,
                visibility: Visibility::Public,
                source_id: assert_module.source_id.clone(),
                span,
            };
            let index = self.declarations.len();
            self.declaration_indexes
                .insert((module.clone(), path), index);
            self.declarations.push(DeclarationWork {
                declaration,
                body: None,
            });
        }
    }

    fn record_import_name(
        &mut self,
        names: &mut BTreeMap<String, (ByteSpan, String)>,
        name: &str,
        span: ByteSpan,
        source_id: &str,
    ) {
        if matches!(name, "map" | "array" | "tuple") {
            self.diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::NameReservedValueSpelling,
                    span,
                    "a module-level import alias uses a reserved value spelling",
                )
                .with_source_id(source_id),
            );
        }
        if let Some((earlier_span, earlier_source)) = names.get(name).cloned() {
            self.diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::NameRedeclaration,
                    span,
                    "a module-level name is introduced more than once",
                )
                .with_source_id(source_id)
                .with_related_source(
                    earlier_source,
                    earlier_span,
                    "the earlier module-level name is here",
                ),
            );
        } else {
            names.insert(name.to_owned(), (span, source_id.to_owned()));
        }
    }

    fn unavailable(&mut self, source_id: &str, span: ByteSpan, message: &'static str) {
        self.diagnostics.push(
            Diagnostic::new(DiagnosticCode::ToolUnavailable, span, message)
                .with_source_id(source_id),
        );
    }

    fn collect_declaration(
        &mut self,
        module: &ModuleKey,
        declaration: &Declaration,
        names: &mut BTreeMap<String, (ByteSpan, String)>,
        owner: Vec<String>,
        source_id: &str,
    ) {
        if let Declaration::Test(test) = declaration {
            let Literal::String(name) = test.name() else {
                return;
            };
            let mut path = owner;
            path.extend(["$test".to_owned(), name.value().to_owned()]);
            let id = DeclarationId::with_package(
                &module.package,
                &module.unit,
                &module.segments,
                &path,
                EntityKind::Test,
            );
            let work_index = self.declarations.len();
            self.declaration_indexes
                .insert((module.clone(), path), work_index);
            self.declarations.push(DeclarationWork {
                declaration: ResolvedDeclaration {
                    id,
                    visibility: Visibility::Private,
                    source_id: source_id.to_owned(),
                    span: test.span(),
                },
                body: Some(BodyWork::Test(test.clone())),
            });
            return;
        }
        let (name, kind, visibility, span, body) = match declaration {
            Declaration::Deftype(value) => (
                value.name().value(),
                EntityKind::Type,
                visibility(value.attributes().items()),
                value.span(),
                None,
            ),
            Declaration::Defint(value) => (
                value.name().value(),
                EntityKind::Interface,
                visibility(value.attributes().items()),
                value.span(),
                None,
            ),
            Declaration::Deffect(value) => (
                value.name().value(),
                EntityKind::Effect,
                visibility(value.attributes().items()),
                value.span(),
                None,
            ),
            Declaration::Def(value) => (
                value.name().value(),
                EntityKind::Value,
                visibility(value.attributes().items()),
                value.span(),
                Some(BodyWork::Def(value.clone())),
            ),
            Declaration::Defn(value) => (
                value.name().value(),
                EntityKind::Function,
                visibility(value.attributes().items()),
                value.span(),
                Some(BodyWork::Function(value.clone())),
            ),
            Declaration::Import(_) => return,
            Declaration::Test(_) => return,
        };
        let unavailable_message = match declaration {
            Declaration::Deftype(_) => {
                Some("nominal type resolution is unavailable in Step 4")
            }
            Declaration::Defint(_) => {
                Some("interface resolution is unavailable in Step 4")
            }
            Declaration::Deffect(_) => {
                Some("effect resolution is unavailable in Step 4")
            }
            Declaration::Import(_)
            | Declaration::Def(_)
            | Declaration::Defn(_)
            | Declaration::Test(_) => None,
        };
        if let Some(message) = unavailable_message {
            self.unavailable(source_id, span, message);
        }
        let mut path = owner;
        path.push(name.to_owned());
        if path.len() == 1 {
            if let Some((earlier_span, earlier_source)) = names.get(name).cloned() {
                self.diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::NameRedeclaration,
                        span,
                        "a module-level name is introduced more than once",
                    )
                    .with_source_id(source_id)
                    .with_related_source(
                        earlier_source,
                        earlier_span,
                        "the earlier module-level name is here",
                    ),
                );
            } else {
                names.insert(name.to_owned(), (span, source_id.to_owned()));
            }
            if matches!(kind, EntityKind::Value | EntityKind::Function)
                && matches!(name, "map" | "array" | "tuple")
            {
                self.diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::NameReservedValueSpelling,
                        span,
                        "a module-level value uses a reserved value spelling",
                    )
                    .with_source_id(source_id),
                );
            }
        }
        let id = DeclarationId::with_package(
            &module.package,
            &module.unit,
            &module.segments,
            &path,
            kind,
        );
        let work_index = self.declarations.len();
        self.declaration_indexes
            .insert((module.clone(), path.clone()), work_index);
        self.declarations.push(DeclarationWork {
            declaration: ResolvedDeclaration {
                id,
                visibility,
                source_id: source_id.to_owned(),
                span,
            },
            body,
        });

        match declaration {
            Declaration::Deftype(value) => {
                self.collect_type_members(
                    module,
                    value.members(),
                    path.clone(),
                    source_id,
                );
                self.collect_deftype_fields(module, value.body(), path, source_id);
            }
            Declaration::Defint(value) => {
                self.collect_type_members(module, value.members(), path, source_id);
            }
            Declaration::Deffect(value) => {
                for member in value.members() {
                    self.unavailable(
                        source_id,
                        member.span(),
                        "effect operations are unavailable in Step 4",
                    );
                    self.collect_member(
                        module,
                        member,
                        &path,
                        EntityKind::Operation,
                        source_id,
                    );
                }
            }
            Declaration::Def(_) | Declaration::Defn(_) | Declaration::Test(_) => {}
            Declaration::Import(_) => {}
        }
    }

    fn collect_type_members(
        &mut self,
        module: &ModuleKey,
        members: &[TypeMember],
        owner: Vec<String>,
        source_id: &str,
    ) {
        let mut names = BTreeMap::<String, (ByteSpan, String)>::new();
        for member in members {
            match member {
                TypeMember::Method(function) => {
                    self.unavailable(
                        source_id,
                        function.span(),
                        "type and interface members are unavailable in Step 4",
                    );
                    self.collect_member(
                        module,
                        function,
                        &owner,
                        EntityKind::Function,
                        source_id,
                    );
                    self.check_member_name(
                        &mut names,
                        function.name().value(),
                        function.span(),
                        source_id,
                    );
                }
                TypeMember::Implementation(implementation) => {
                    self.unavailable(
                        source_id,
                        implementation.span(),
                        "implementation resolution is unavailable in Step 4",
                    );
                    for member in implementation.members() {
                        self.unavailable(
                            source_id,
                            member.span(),
                            "implementation members are unavailable in Step 4",
                        );
                    }
                }
            }
        }
    }

    fn collect_deftype_fields(
        &mut self,
        module: &ModuleKey,
        body: &DeftypeBody,
        owner: Vec<String>,
        source_id: &str,
    ) {
        let fields = match body {
            DeftypeBody::Record(fields) => fields,
            DeftypeBody::Enum(fields) => fields,
            DeftypeBody::Type(_) | DeftypeBody::Union(_) | DeftypeBody::Newtype(_) => {
                return;
            }
        };
        let kind = match body {
            DeftypeBody::Record(_) => EntityKind::Field,
            DeftypeBody::Enum(_) => EntityKind::Variant,
            DeftypeBody::Type(_) | DeftypeBody::Union(_) | DeftypeBody::Newtype(_) => {
                return;
            }
        };
        let mut names = BTreeMap::<String, (ByteSpan, String)>::new();
        for field in fields {
            let field_name = field.name().value();
            let field_span = field.span();
            self.unavailable(
                source_id,
                field_span,
                "nominal type members are unavailable in Step 4",
            );
            self.check_member_name(&mut names, field_name, field_span, source_id);
            let mut path = owner.clone();
            path.push(field_name.to_owned());
            let id = DeclarationId::with_package(
                &module.package,
                &module.unit,
                &module.segments,
                path,
                kind,
            );
            self.declaration_indexes.insert(
                (module.clone(), id.path().to_vec()),
                self.declarations.len(),
            );
            self.declarations.push(DeclarationWork {
                declaration: ResolvedDeclaration {
                    id,
                    visibility: Visibility::Private,
                    source_id: source_id.to_owned(),
                    span: field_span,
                },
                body: None,
            });
        }
    }

    fn check_member_name(
        &mut self,
        names: &mut BTreeMap<String, (ByteSpan, String)>,
        name: &str,
        span: ByteSpan,
        source_id: &str,
    ) {
        if let Some((earlier_span, earlier_source)) =
            names.insert(name.to_owned(), (span, source_id.to_owned()))
        {
            self.diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::NameMemberCollision,
                    span,
                    "members of one owner share a name",
                )
                .with_source_id(source_id)
                .with_related_source(
                    earlier_source,
                    earlier_span,
                    "the earlier member is here",
                ),
            );
        }
    }

    fn collect_member(
        &mut self,
        module: &ModuleKey,
        function: &FunctionDeclaration,
        owner: &[String],
        kind: EntityKind,
        source_id: &str,
    ) {
        let mut path = owner.to_vec();
        path.push(function.name().value().to_owned());
        let id = DeclarationId::with_package(
            &module.package,
            &module.unit,
            &module.segments,
            path,
            kind,
        );
        let index = self.declarations.len();
        self.declaration_indexes
            .insert((module.clone(), id.path().to_vec()), index);
        self.declarations.push(DeclarationWork {
            declaration: ResolvedDeclaration {
                id,
                visibility: visibility(function.attributes().items()),
                source_id: source_id.to_owned(),
                span: function.span(),
            },
            body: Some(BodyWork::Function(function.clone())),
        });
    }

    fn resolve_imports(&mut self) {
        for module_index in 0..self.modules.len() {
            let Some(module_key) = self.module_key(module_index) else {
                continue;
            };
            let Some(parsed) = self.modules.get(module_index).cloned() else {
                continue;
            };
            let Some(ast) = parsed.ast else {
                continue;
            };
            for declaration in ast.declarations() {
                let Declaration::Import(import) = declaration else {
                    continue;
                };
                let target = import.target();
                let target_segments = target.segments();
                let Some(unit) = target_segments.first() else {
                    continue;
                };
                let target_path =
                    target_segments.iter().skip(1).cloned().collect::<Vec<_>>();
                let target_span = import.target().span_or(import.span());
                let test_unit_import_forbidden =
                    unit == "tests" && module_key.unit != "tests";
                if test_unit_import_forbidden {
                    self.diagnostics.push(
                        Diagnostic::new(
                            DiagnosticCode::ModuleUnknownPath,
                            target_span,
                            "the reserved `@tests` unit is importable only from test modules",
                        )
                        .with_source_id(parsed.module.source_id.clone()),
                    );
                    let work = ImportWork {
                        module: None,
                        alias: import.alias().value().to_owned(),
                        written: target.value().to_owned(),
                        source_id: parsed.module.source_id.clone(),
                        span: import.span(),
                    };
                    let index = self.imports.len();
                    self.imports.push((module_key.clone(), work));
                    self.imports_by_module
                        .entry(module_key.clone())
                        .or_default()
                        .push(index);
                    continue;
                }
                let mut resolved_module = None;
                let overlay_package = self
                    .input
                    .overlay
                    .as_ref()
                    .filter(|overlay| {
                        overlay.modules.iter().any(|module| {
                            module.unit == *unit
                                && target_path.starts_with(&module.segments)
                        })
                    })
                    .map(|overlay| overlay.package.clone());
                let reserved_import = self.input.reserved_import_paths.iter().any(
                    |(reserved_unit, reserved_segments)| {
                        reserved_unit == unit
                            && target_path.starts_with(reserved_segments)
                    },
                );
                let package = match overlay_package {
                    Some(package) => package,
                    None if reserved_import => {
                        self.diagnostics.push(
                            Diagnostic::new(
                                DiagnosticCode::ModuleUnknownPath,
                                target_span,
                                "reserved import requires its verified package overlay",
                            )
                            .with_source_id(parsed.module.source_id.clone()),
                        );
                        let work = ImportWork {
                            module: None,
                            alias: import.alias().value().to_owned(),
                            written: target.value().to_owned(),
                            source_id: parsed.module.source_id.clone(),
                            span: import.span(),
                        };
                        let index = self.imports.len();
                        self.imports.push((module_key.clone(), work));
                        self.imports_by_module
                            .entry(module_key.clone())
                            .or_default()
                            .push(index);
                        continue;
                    }
                    None => module_key.package.clone(),
                };
                let Some(module_len) =
                    self.longest_module_prefix(&package, unit, &target_path)
                else {
                    self.diagnostics.push(
                        Diagnostic::new(
                            DiagnosticCode::ModuleUnknownPath,
                            target_span,
                            "import target does not resolve to a source module",
                        )
                        .with_source_id(parsed.module.source_id.clone()),
                    );
                    let work = ImportWork {
                        module: None,
                        alias: import.alias().value().to_owned(),
                        written: target.value().to_owned(),
                        source_id: parsed.module.source_id.clone(),
                        span: import.span(),
                    };
                    let index = self.imports.len();
                    self.imports.push((module_key.clone(), work));
                    self.imports_by_module
                        .entry(module_key.clone())
                        .or_default()
                        .push(index);
                    continue;
                };
                let Some(module_segments) = target_path.get(..module_len) else {
                    continue;
                };
                let Some(declaration_segments) = target_path.get(module_len..) else {
                    continue;
                };
                let module = ModuleKey {
                    package,
                    unit: unit.clone(),
                    segments: module_segments.to_vec(),
                };
                if declaration_segments.is_empty() {
                    resolved_module = Some(module);
                } else {
                    let declaration_path = declaration_segments.to_vec();
                    let target = self
                        .declaration_indexes
                        .get(&(module.clone(), declaration_path))
                        .and_then(|index| self.declarations.get(*index))
                        .map(|work| {
                            (work.declaration.source_id.clone(), work.declaration.span)
                        });
                    if let Some((source_id, span)) = target {
                        self.diagnostics.push(
                            Diagnostic::new(
                                DiagnosticCode::NameWrongEntityKind,
                                target_span,
                                "import target names a declaration; imports require modules",
                            )
                            .with_source_id(parsed.module.source_id.clone())
                            .with_related_source(
                                source_id,
                                span,
                                "the imported entity is declared here",
                            ),
                        );
                    } else {
                        self.diagnostics.push(
                            Diagnostic::new(
                                DiagnosticCode::NameUnknownSymbol,
                                target_span,
                                "import member does not resolve to a declaration",
                            )
                            .with_source_id(parsed.module.source_id.clone()),
                        );
                    }
                }
                let work = ImportWork {
                    module: resolved_module,
                    alias: import.alias().value().to_owned(),
                    written: target.value().to_owned(),
                    source_id: parsed.module.source_id.clone(),
                    span: import.span(),
                };
                let index = self.imports.len();
                self.imports.push((module_key.clone(), work));
                self.imports_by_module
                    .entry(module_key.clone())
                    .or_default()
                    .push(index);
            }
        }
    }

    fn check_import_cycles(&mut self) {
        let mut state = BTreeMap::<ModuleKey, u8>::new();
        let mut stack = Vec::<ModuleKey>::new();
        let keys = self.module_indexes.keys().cloned().collect::<Vec<_>>();
        for key in keys {
            self.visit_imports(&key, &mut state, &mut stack);
        }
    }

    fn visit_imports(
        &mut self,
        module: &ModuleKey,
        state: &mut BTreeMap<ModuleKey, u8>,
        stack: &mut Vec<ModuleKey>,
    ) {
        match state.get(module).copied() {
            Some(2) => return,
            Some(1) => return,
            _ => {}
        }
        state.insert(module.clone(), 1);
        stack.push(module.clone());
        let import_indexes = self
            .imports_by_module
            .get(module)
            .cloned()
            .unwrap_or_default();
        for import_index in import_indexes {
            let Some((_, edge)) = self.imports.get(import_index) else {
                continue;
            };
            let Some(target) = edge.module.clone() else {
                continue;
            };
            if state.get(&target) == Some(&1) {
                let Some((_, current)) = self.imports.get(import_index) else {
                    continue;
                };
                let earlier = stack
                    .iter()
                    .position(|candidate| candidate == &target)
                    .and_then(|position| stack.get(position.saturating_add(1)))
                    .and_then(|next| {
                        self.imports.iter().find(|(owner, edge)| {
                            owner == &target && edge.module.as_ref() == Some(next)
                        })
                    })
                    .map(|(_, edge)| (edge.source_id.clone(), edge.span));
                let mut diagnostic = Diagnostic::new(
                    DiagnosticCode::ModuleImportCycle,
                    current.span,
                    "module imports form a cycle",
                )
                .with_source_id(current.source_id.clone());
                if let Some((source_id, span)) = earlier {
                    diagnostic = diagnostic.with_related_source(
                        source_id,
                        span,
                        "the earlier import edge closes this cycle",
                    );
                }
                self.diagnostics.push(diagnostic);
                continue;
            }
            self.visit_imports(&target, state, stack);
        }
        stack.pop();
        state.insert(module.clone(), 2);
    }

    fn resolve_entries(&mut self) {
        for unit in &self.input.units {
            let Some(entry) = unit.entry.as_ref() else {
                if unit.kind == TargetKind::Lib {
                    continue;
                }
                continue;
            };
            if unit.kind == TargetKind::Lib {
                self.diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::ProjectEntryOnLibrary,
                        entry.span,
                        "a library target cannot declare an entry",
                    )
                    .with_source_id(entry.source_id.clone()),
                );
                self.entries.push(ResolvedEntry {
                    unit: unit.name.clone(),
                    source_id: entry.source_id.clone(),
                    span: entry.span,
                    declaration: None,
                });
                continue;
            }
            if entry.segments.first().map(String::as_str) != Some(unit.name.as_str()) {
                self.diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::ProjectEntryOutsideTarget,
                        entry.span,
                        "entry reference names a different target",
                    )
                    .with_source_id(entry.source_id.clone()),
                );
                self.entries.push(ResolvedEntry {
                    unit: unit.name.clone(),
                    source_id: entry.source_id.clone(),
                    span: entry.span,
                    declaration: None,
                });
                continue;
            }
            let target_path =
                entry.segments.iter().skip(1).cloned().collect::<Vec<_>>();
            let module_len = self.longest_module_prefix(
                &self.input.package,
                &unit.name,
                &target_path,
            );
            let Some(module_len) = module_len else {
                self.diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::ModuleUnknownPath,
                        entry.span,
                        "entry reference does not resolve to a source module",
                    )
                    .with_source_id(entry.source_id.clone()),
                );
                self.entries.push(ResolvedEntry {
                    unit: unit.name.clone(),
                    source_id: entry.source_id.clone(),
                    span: entry.span,
                    declaration: None,
                });
                continue;
            };
            let Some(module_segments) = target_path.get(..module_len) else {
                continue;
            };
            let Some(declaration_segments) = target_path.get(module_len..) else {
                continue;
            };
            let module = ModuleKey {
                package: self.input.package.clone(),
                unit: unit.name.clone(),
                segments: module_segments.to_vec(),
            };
            if declaration_segments.is_empty() {
                let mut diagnostic = Diagnostic::new(
                    DiagnosticCode::NameWrongEntityKind,
                    entry.span,
                    "entry reference names a module, not a module-level function",
                )
                .with_source_id(entry.source_id.clone());
                if let Some(module_index) = self.module_indexes.get(&module).copied()
                    && let Some(parsed) = self.modules.get(module_index)
                {
                    diagnostic = diagnostic.with_related_source(
                        parsed.module.source_id.clone(),
                        ByteSpan::empty_at(0),
                        "the referenced module is here",
                    );
                }
                self.diagnostics.push(diagnostic);
                self.entries.push(ResolvedEntry {
                    unit: unit.name.clone(),
                    source_id: entry.source_id.clone(),
                    span: entry.span,
                    declaration: None,
                });
                continue;
            }
            let declaration_path = declaration_segments.to_vec();
            let Some(index) = self
                .declaration_indexes
                .get(&(module.clone(), declaration_path))
                .copied()
            else {
                self.diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::NameUnknownSymbol,
                        entry.span,
                        "entry declaration does not resolve",
                    )
                    .with_source_id(entry.source_id.clone()),
                );
                self.entries.push(ResolvedEntry {
                    unit: unit.name.clone(),
                    source_id: entry.source_id.clone(),
                    span: entry.span,
                    declaration: None,
                });
                continue;
            };
            let Some(target) = self.declarations.get(index) else {
                continue;
            };
            self.entries.push(ResolvedEntry {
                unit: unit.name.clone(),
                source_id: entry.source_id.clone(),
                span: entry.span,
                declaration: Some(target.declaration.id.clone()),
            });
            if target.declaration.id.kind() != EntityKind::Function
                || target.declaration.id.path().len() != 1
            {
                self.diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::NameWrongEntityKind,
                        entry.span,
                        "entry reference must name a module-level function",
                    )
                    .with_source_id(entry.source_id.clone())
                    .with_related_source(
                        target.declaration.source_id.clone(),
                        target.declaration.span,
                        "the referenced entity is declared here",
                    ),
                );
            }
        }
    }

    fn resolve_bodies(&mut self) {
        let works = self
            .declarations
            .iter()
            .map(|work| {
                (
                    work.declaration.id.clone(),
                    work.declaration.source_id.clone(),
                    work.body.clone(),
                )
            })
            .collect::<Vec<_>>();
        for (from, source_id, body) in works {
            let Some(body) = body else {
                continue;
            };
            let module = ModuleKey {
                package: from.package().clone(),
                unit: from.unit.clone(),
                segments: from.module.clone(),
            };
            match body {
                BodyWork::Def(value) => {
                    self.resolve_expression(
                        &module,
                        &from,
                        value.expression(),
                        &[],
                        &source_id,
                    );
                }
                BodyWork::Function(function) => {
                    let mut scope = Vec::new();
                    self.bind_parameters(&module, &function, &mut scope, &source_id);
                    for expression in function.expressions() {
                        self.resolve_expression(
                            &module, &from, expression, &scope, &source_id,
                        );
                    }
                }
                BodyWork::Test(test) => {
                    for expression in test.expressions() {
                        self.resolve_expression(
                            &module,
                            &from,
                            expression,
                            &[],
                            &source_id,
                        );
                    }
                }
            }
        }
    }

    fn bind_parameters(
        &mut self,
        module: &ModuleKey,
        function: &FunctionDeclaration,
        scope: &mut Vec<(String, ByteSpan)>,
        source_id: &str,
    ) {
        for parameter in function.parameters() {
            let mut names = Vec::new();
            collect_pattern_names(parameter.parsed_pattern(), &mut names);
            self.bind_names(module, scope, names, source_id);
        }
        self.bind_function_attributes(
            module,
            function.attributes().items(),
            scope,
            source_id,
        );
    }

    fn bind_function_attributes(
        &mut self,
        module: &ModuleKey,
        attributes: &[Attribute],
        scope: &mut Vec<(String, ByteSpan)>,
        source_id: &str,
    ) {
        for attribute in attributes {
            match attribute {
                Attribute::Labelled(parameters) => {
                    for parameter in parameters {
                        self.bind_names(
                            module,
                            scope,
                            [(parameter.name().value().to_owned(), parameter.span())],
                            source_id,
                        );
                    }
                }
                Attribute::Variadic(parameter) => {
                    if !parameter.name().is_discard() {
                        self.bind_names(
                            module,
                            scope,
                            [(parameter.name().value().to_owned(), parameter.span())],
                            source_id,
                        );
                    }
                }
                Attribute::Where(_)
                | Attribute::Visibility(_)
                | Attribute::Effects(_)
                | Attribute::External(_)
                | Attribute::Symbol(_)
                | Attribute::Doc(_) => {}
            }
        }
    }

    fn bind_names<I>(
        &mut self,
        module: &ModuleKey,
        scope: &mut Vec<(String, ByteSpan)>,
        names: I,
        source_id: &str,
    ) where
        I: IntoIterator<Item = (String, ByteSpan)>,
    {
        for (name, span) in names {
            if name == "-" || name == "@-" || name == "-:" {
                continue;
            }
            // One diagnostic per introduction, relating the nearest earlier
            // one: the innermost lexical binding, else the module binding.
            if let Some((_, earlier_span)) =
                scope.iter().rev().find(|(bound, _)| bound == &name)
            {
                self.diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::NameRedeclaration,
                        span,
                        "a lexical name is introduced more than once",
                    )
                    .with_source_id(source_id)
                    .with_related_source(
                        source_id,
                        *earlier_span,
                        "the earlier lexical binding is here",
                    ),
                );
            } else if let Some((earlier_span, earlier_source)) = self
                .module_bindings
                .get(module)
                .and_then(|bindings| bindings.get(&name))
                .cloned()
            {
                self.diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::NameRedeclaration,
                        span,
                        "a lexical name shadows a visible module binding",
                    )
                    .with_source_id(source_id)
                    .with_related_source(
                        earlier_source,
                        earlier_span,
                        "the visible module binding is here",
                    ),
                );
            }
            scope.push((name, span));
        }
    }

    fn resolve_expression(
        &mut self,
        module: &ModuleKey,
        from: &DeclarationId,
        expression: &Expression,
        scope: &[(String, ByteSpan)],
        source_id: &str,
    ) {
        match expression.kind() {
            ExpressionKind::Literal(_) => {}
            ExpressionKind::Name(name) => {
                if name.kind() != vibra_syntax::NameKind::Symbol
                    || name.segments().first().is_some_and(|segment| {
                        scope.iter().any(|(bound, _)| bound == segment)
                    })
                {
                    return;
                }
                self.resolve_reference(
                    module,
                    from,
                    name,
                    expression.span(),
                    source_id,
                );
            }
            ExpressionKind::Application(application) => {
                self.resolve_expression(
                    module,
                    from,
                    application.callee(),
                    scope,
                    source_id,
                );
                for argument in application.arguments() {
                    self.resolve_expression(
                        module,
                        from,
                        argument.value(),
                        scope,
                        source_id,
                    );
                }
            }
            ExpressionKind::Lambda(lambda) => {
                let mut nested_scope = scope.to_vec();
                for parameter in lambda.parameters() {
                    let mut names = Vec::new();
                    collect_pattern_names(parameter.parsed_pattern(), &mut names);
                    self.bind_names(module, &mut nested_scope, names, source_id);
                }
                self.bind_function_attributes(
                    module,
                    lambda.attributes().items(),
                    &mut nested_scope,
                    source_id,
                );
                for expression in lambda.body() {
                    self.resolve_expression(
                        module,
                        from,
                        expression,
                        &nested_scope,
                        source_id,
                    );
                }
            }
            ExpressionKind::Do(expressions) => {
                for expression in expressions {
                    self.resolve_expression(module, from, expression, scope, source_id);
                }
            }
            ExpressionKind::Let {
                pattern,
                value,
                body,
            } => {
                self.resolve_expression(module, from, value, scope, source_id);
                let mut nested_scope = scope.to_vec();
                let mut names = Vec::new();
                collect_pattern_names(pattern, &mut names);
                self.bind_names(module, &mut nested_scope, names, source_id);
                for expression in body {
                    self.resolve_expression(
                        module,
                        from,
                        expression,
                        &nested_scope,
                        source_id,
                    );
                }
            }
            ExpressionKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.resolve_expression(module, from, condition, scope, source_id);
                self.resolve_expression(module, from, then_branch, scope, source_id);
                self.resolve_expression(module, from, else_branch, scope, source_id);
            }
            ExpressionKind::Match { scrutinee, arms } => {
                self.resolve_expression(module, from, scrutinee, scope, source_id);
                for arm in arms {
                    let mut nested_scope = scope.to_vec();
                    let mut names = Vec::new();
                    collect_pattern_names(arm.pattern(), &mut names);
                    self.bind_names(module, &mut nested_scope, names, source_id);
                    self.resolve_expression(
                        module,
                        from,
                        arm.result(),
                        &nested_scope,
                        source_id,
                    );
                }
            }
            ExpressionKind::As { operand, .. } | ExpressionKind::Try(operand) => {
                self.resolve_expression(module, from, operand, scope, source_id);
            }
        }
    }

    fn resolve_reference(
        &mut self,
        module: &ModuleKey,
        from: &DeclarationId,
        name: &Name,
        span: ByteSpan,
        source_id: &str,
    ) {
        let path = name.segments();
        let (target_module, declaration_path, imported) =
            if let Some(first) = path.first() {
                let import = self
                    .imports
                    .iter()
                    .find(|(owner, import)| owner == module && import.alias == *first);
                if let Some((_, import)) = import {
                    (
                        import.module.clone(),
                        path.iter().skip(1).cloned().collect::<Vec<_>>(),
                        true,
                    )
                } else {
                    (Some(module.clone()), path.to_vec(), false)
                }
            } else {
                (Some(module.clone()), Vec::new(), false)
            };
        if imported
            && declaration_path.is_empty()
            && let Some(target_module) = target_module.as_ref()
            && let Some(module_index) = self.module_indexes.get(target_module).copied()
        {
            let target = DeclarationId::with_package(
                &target_module.package,
                target_module.unit.clone(),
                target_module.segments.iter().cloned(),
                std::iter::empty::<String>(),
                EntityKind::Module,
            );
            let mut diagnostic = Diagnostic::new(
                DiagnosticCode::NameWrongEntityKind,
                span,
                "a module is not a body value",
            )
            .with_source_id(source_id);
            if let Some(parsed) = self.modules.get(module_index) {
                diagnostic = diagnostic.with_related_source(
                    parsed.module.source_id.clone(),
                    ByteSpan::empty_at(0),
                    "the referenced module is here",
                );
            }
            self.diagnostics.push(diagnostic);
            self.references.push(ResolvedReference {
                from: from.clone(),
                written: name.value().to_owned(),
                target: Some(target),
                source_id: source_id.to_owned(),
                span,
            });
            return;
        }
        let target = target_module
            .as_ref()
            .and_then(|target_module| {
                self.declaration_indexes
                    .get(&(target_module.clone(), declaration_path))
            })
            .and_then(|index| self.declarations.get(*index))
            .map(|work| &work.declaration);
        let Some(target) = target else {
            let unavailable_assertion = target_module.as_ref().is_some_and(|target| {
                target.package.name() == "vibra-stdlib"
                    && target.unit == "std"
                    && target.segments == ["assert"]
                    && imported
            });
            let code = if unavailable_assertion {
                DiagnosticCode::ToolUnavailable
            } else {
                DiagnosticCode::NameUnknownSymbol
            };
            self.diagnostics.push(
                Diagnostic::new(
                    code,
                    span,
                    if unavailable_assertion {
                        "assertion members outside the closed M2 table are unavailable"
                    } else {
                        "symbol does not resolve to a declaration"
                    },
                )
                .with_source_id(source_id),
            );
            self.references.push(ResolvedReference {
                from: from.clone(),
                written: name.value().to_owned(),
                target: None,
                source_id: source_id.to_owned(),
                span,
            });
            return;
        };
        if imported && target.visibility == Visibility::Private {
            self.diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::NamePrivateAccess,
                    span,
                    "imported declaration is private",
                )
                .with_source_id(source_id)
                .with_related_source(
                    target.source_id.clone(),
                    target.span,
                    "the private declaration is here",
                ),
            );
        }
        if target.id.kind() == EntityKind::Module {
            self.diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::NameWrongEntityKind,
                    span,
                    "a module is not a body value",
                )
                .with_source_id(source_id),
            );
        }
        self.references.push(ResolvedReference {
            from: from.clone(),
            written: name.value().to_owned(),
            target: Some(target.id.clone()),
            source_id: source_id.to_owned(),
            span,
        });
    }

    fn resolve_module_path(
        &self,
        package: &PackageId,
        unit: &str,
        segments: &[String],
    ) -> Option<ModuleKey> {
        let key = ModuleKey {
            package: package.clone(),
            unit: unit.to_owned(),
            segments: segments.to_vec(),
        };
        self.module_indexes.contains_key(&key).then_some(key)
    }

    fn longest_module_prefix(
        &self,
        package: &PackageId,
        unit: &str,
        path: &[String],
    ) -> Option<usize> {
        (1..=path.len()).rev().find(|length| {
            path.get(..*length).is_some_and(|prefix| {
                self.resolve_module_path(package, unit, prefix).is_some()
            })
        })
    }

    fn module_key(&self, index: usize) -> Option<ModuleKey> {
        self.modules.get(index).map(|work| ModuleKey {
            package: work.package.clone(),
            unit: work.module.unit.clone(),
            segments: work.module.segments.clone(),
        })
    }
}

fn collect_pattern_names(pattern: &Pattern, names: &mut Vec<(String, ByteSpan)>) {
    match pattern.kind() {
        PatternKind::Binding(name) if !name.is_discard() => {
            names.push((name.value().to_owned(), pattern.span()));
        }
        PatternKind::Constructor { arguments, .. } => {
            for argument in arguments {
                collect_pattern_names(argument.pattern(), names);
            }
        }
        PatternKind::Tuple(patterns) | PatternKind::Array(patterns) => {
            for pattern in patterns {
                collect_pattern_names(pattern, names);
            }
        }
        PatternKind::As { pattern, .. } => collect_pattern_names(pattern, names),
        PatternKind::Binding(_) | PatternKind::Literal(_) => {}
    }
}

fn visibility(attributes: &[Attribute]) -> Visibility {
    attributes
        .iter()
        .find_map(|attribute| {
            let Attribute::Visibility(name) = attribute else {
                return None;
            };
            (name.value() == "public").then_some(Visibility::Public)
        })
        .unwrap_or(Visibility::Private)
}

fn diagnostic_key(diagnostic: &Diagnostic) -> (String, usize, usize, String) {
    let span = diagnostic.primary_span();
    (
        diagnostic.source_id().unwrap_or("").to_owned(),
        span.start(),
        span.end(),
        diagnostic.code().as_atom().to_owned(),
    )
}

fn quoted(value: &str) -> String {
    let mut output = String::with_capacity(value.len() + 2);
    output.push('"');
    for character in value.chars() {
        match character {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

trait SpanOr {
    fn span_or(&self, fallback: ByteSpan) -> ByteSpan;
}

impl SpanOr for Name {
    fn span_or(&self, fallback: ByteSpan) -> ByteSpan {
        let _ = self;
        fallback
    }
}

//! The neutral, filesystem-free input of resolution.
//!
//! `docs/spec/07-diagnostics-and-conformance.md`, "Availability, discovery
//! and resolution": resolution reads only the captured source graph. The
//! caller hands every module's exact bytes, its module path, and its source
//! identity; this crate never reads a file.

/// The local package: its name and version, the provenance every
/// declaration identity carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageId {
    name: String,
    version: String,
}

impl PackageId {
    /// The package `name` at `version`.
    #[must_use]
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
        }
    }

    /// The package name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The package version.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }
}

/// The kind of a local unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UnitKind {
    /// A binary target.
    Bin,
    /// A library target.
    Lib,
    /// The reserved unit `@tests`.
    Tests,
    /// The library target of another package, bound by a dependency name.
    Dependency,
}

/// One module: its path, its source identity, and its exact bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceModule {
    path: String,
    source_id: String,
    bytes: Vec<u8>,
}

impl SourceModule {
    /// The module at `path`, an atom with its `@` whose first part is the
    /// unit (`@app.model`), read from `source_id`.
    #[must_use]
    pub fn new(
        path: impl Into<String>,
        source_id: impl Into<String>,
        bytes: impl Into<Vec<u8>>,
    ) -> Self {
        Self {
            path: path.into(),
            source_id: source_id.into(),
            bytes: bytes.into(),
        }
    }

    /// The module path, `@` included.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The project-relative source identity.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// The exact bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// One unit and its modules: a local unit, or a dependency's unit with the
/// package it comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceUnit {
    name: String,
    kind: UnitKind,
    modules: Vec<SourceModule>,
    package: Option<PackageId>,
    dependencies: Vec<String>,
}

impl SourceUnit {
    /// The unit `name`, an atom with its `@`.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        kind: UnitKind,
        modules: Vec<SourceModule>,
    ) -> Self {
        Self {
            name: name.into(),
            kind,
            modules,
            package: None,
            dependencies: Vec::new(),
        }
    }

    /// This unit, whose declarations belong to `package` rather than to the
    /// input's local package: a dependency's unit.
    #[must_use]
    pub fn with_package(mut self, package: PackageId) -> Self {
        self.package = Some(package);
        self
    }

    /// This unit, whose modules may also name the dependency units
    /// `dependencies` (atoms with their `@`): the dependencies its project
    /// declares.
    #[must_use]
    pub fn with_dependencies(mut self, dependencies: Vec<String>) -> Self {
        self.dependencies = dependencies;
        self
    }

    /// The package of a dependency's unit; `None` for a local unit.
    #[must_use]
    pub const fn package(&self) -> Option<&PackageId> {
        self.package.as_ref()
    }

    /// The dependency units its modules may name, `@` included.
    #[must_use]
    pub fn dependencies(&self) -> &[String] {
        &self.dependencies
    }

    /// The unit name, `@` included.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The kind.
    #[must_use]
    pub const fn kind(&self) -> UnitKind {
        self.kind
    }

    /// The modules.
    #[must_use]
    pub fn modules(&self) -> &[SourceModule] {
        &self.modules
    }
}

/// One prelude name and the identity of the declaration it resolves to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreludeBinding {
    name: String,
    identity: String,
}

impl PreludeBinding {
    /// The prelude name `name`, bound to the declaration whose identity is
    /// `identity` (`sha256:` and 64 hexadecimal digits).
    #[must_use]
    pub fn new(name: impl Into<String>, identity: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            identity: identity.into(),
        }
    }

    /// The prelude name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The identity of the declaration the name resolves to.
    #[must_use]
    pub fn identity(&self) -> &str {
        &self.identity
    }
}

/// The embedded standard library as resolution reads it: its package, its
/// unit with every module's bytes, and the prelude table.
///
/// `docs/spec/04-programs-and-packages.md`, "The standard library": the
/// library is the unit `@std` of the embedded package, which every module
/// names. `docs/spec/02-type-system.md`, "The prelude": a prelude name
/// resolves to its real declaration in every module. `Any` and `Self` are
/// predeclared and have no declaration, so the table holds every other
/// prelude name. The modules are read in standard-library mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmbeddedUnit {
    package: PackageId,
    name: String,
    modules: Vec<SourceModule>,
    prelude: Vec<PreludeBinding>,
}

impl EmbeddedUnit {
    /// The unit `name` (an atom with its `@`) of the embedded `package`, with
    /// its `modules` and its `prelude` table.
    #[must_use]
    pub fn new(
        package: PackageId,
        name: impl Into<String>,
        modules: Vec<SourceModule>,
        prelude: Vec<PreludeBinding>,
    ) -> Self {
        Self {
            package,
            name: name.into(),
            modules,
            prelude,
        }
    }

    /// The embedded package, the provenance of its declarations' identities.
    #[must_use]
    pub const fn package(&self) -> &PackageId {
        &self.package
    }

    /// The unit name, `@` included.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The modules.
    #[must_use]
    pub fn modules(&self) -> &[SourceModule] {
        &self.modules
    }

    /// The prelude table.
    #[must_use]
    pub fn prelude(&self) -> &[PreludeBinding] {
        &self.prelude
    }
}

/// Everything resolution reads: the package, its local units, and the
/// embedded standard library.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolveInput {
    package: PackageId,
    units: Vec<SourceUnit>,
    embedded: EmbeddedUnit,
}

impl ResolveInput {
    /// The input of `package` with its `units` and the `embedded` standard
    /// library.
    #[must_use]
    pub const fn new(
        package: PackageId,
        units: Vec<SourceUnit>,
        embedded: EmbeddedUnit,
    ) -> Self {
        Self {
            package,
            units,
            embedded,
        }
    }

    /// The package.
    #[must_use]
    pub const fn package(&self) -> &PackageId {
        &self.package
    }

    /// The units.
    #[must_use]
    pub fn units(&self) -> &[SourceUnit] {
        &self.units
    }

    /// The embedded standard library.
    #[must_use]
    pub const fn embedded(&self) -> &EmbeddedUnit {
        &self.embedded
    }
}

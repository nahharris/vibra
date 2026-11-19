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

/// One local unit and its modules.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceUnit {
    name: String,
    kind: UnitKind,
    modules: Vec<SourceModule>,
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
        }
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

/// Everything resolution reads: the package and its local units.
///
/// The embedded standard library enters as the unit `@std`. Until its
/// overlay exists, `@std` is the fixed module list and prelude table of
/// [`crate::prelude`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolveInput {
    package: PackageId,
    units: Vec<SourceUnit>,
}

impl ResolveInput {
    /// The input of `package` with its `units`.
    #[must_use]
    pub const fn new(package: PackageId, units: Vec<SourceUnit>) -> Self {
        Self { package, units }
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
}

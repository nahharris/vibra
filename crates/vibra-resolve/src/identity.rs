//! Declaration identities.
//!
//! `docs/spec/02-type-system.md`, "Declared types": a declaration's identity
//! is the hash of its logical anchor, the package name and version, the
//! module path, the owner path, the declaration kind, and the name. It is
//! `sha256:` and 64 lower-case hexadecimal digits of the SHA-256 of the
//! ASCII text `vibraDeclarationIdentityV2`, one zero byte, and the six
//! strings, each preceded by its byte length as a big-endian 64-bit integer.
//! The anchor holds no body, so a body change never changes an identity.

use std::fmt::{self, Write as _};

use sha2::{Digest, Sha256};

/// The domain text that starts every identity's hashed bytes.
const DOMAIN: &[u8] = b"vibraDeclarationIdentityV2";

/// The closed set of declaration kinds an anchor names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DeclarationKind {
    /// A module.
    Module,
    /// A `type`.
    Type,
    /// An `interface`.
    Interface,
    /// A `case` of a variant type.
    Case,
    /// A `fn`, top-level or a member.
    Fn,
    /// A `proc`, top-level or a member.
    Proc,
    /// A module value.
    Value,
    /// A `test`.
    Test,
    /// An associated type of an interface.
    AssociatedType,
}

impl DeclarationKind {
    /// The kind atom, `@` included.
    #[must_use]
    pub const fn as_atom(self) -> &'static str {
        match self {
            Self::Module => "@module",
            Self::Type => "@type",
            Self::Interface => "@interface",
            Self::Case => "@case",
            Self::Fn => "@fn",
            Self::Proc => "@proc",
            Self::Value => "@value",
            Self::Test => "@test",
            Self::AssociatedType => "@associatedType",
        }
    }
}

/// The logical anchor of a declaration: the six hashed fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Anchor {
    package: String,
    version: String,
    module: String,
    owner: String,
    kind: DeclarationKind,
    name: String,
}

impl Anchor {
    /// The anchor of `name`, of `kind`, owned by `owner` (the dotted member
    /// path within the module, empty for a top-level declaration), in the
    /// module `module` (an atom with its `@`) of `package` at `version`.
    #[must_use]
    pub fn new(
        package: impl Into<String>,
        version: impl Into<String>,
        module: impl Into<String>,
        owner: impl Into<String>,
        kind: DeclarationKind,
        name: impl Into<String>,
    ) -> Self {
        Self {
            package: package.into(),
            version: version.into(),
            module: module.into(),
            owner: owner.into(),
            kind,
            name: name.into(),
        }
    }

    /// The package name.
    #[must_use]
    pub fn package(&self) -> &str {
        &self.package
    }

    /// The package version.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }

    /// The module path, `@` included.
    #[must_use]
    pub fn module(&self) -> &str {
        &self.module
    }

    /// The owner path; empty for a top-level declaration.
    #[must_use]
    pub fn owner(&self) -> &str {
        &self.owner
    }

    /// The kind.
    #[must_use]
    pub const fn kind(&self) -> DeclarationKind {
        self.kind
    }

    /// The name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The identity this anchor hashes to.
    #[must_use]
    pub fn identity(&self) -> DeclarationId {
        DeclarationId(declaration_identity(
            &self.package,
            &self.version,
            &self.module,
            &self.owner,
            self.kind.as_atom(),
            &self.name,
        ))
    }
}

/// The identity of the declaration whose logical anchor is the package
/// `package` at `version`, the module path atom `module` (with its `@`), the
/// dotted `owner` path within the module (empty for a top-level declaration),
/// the kind atom `kind` (with its `@`), and the name `name`.
#[must_use]
pub fn declaration_identity(
    package: &str,
    version: &str,
    module: &str,
    owner: &str,
    kind: &str,
    name: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(DOMAIN);
    hasher.update([0u8]);
    for field in [package, version, module, owner, kind, name] {
        let length = u64::try_from(field.len()).unwrap_or(u64::MAX);
        hasher.update(length.to_be_bytes());
        hasher.update(field.as_bytes());
    }
    let mut text = String::from("sha256:");
    for byte in hasher.finalize() {
        let _ = write!(text, "{byte:02x}");
    }
    text
}

/// The identity of an `impl` block: the hash of the anchor whose package,
/// version, and module are its module's, whose owner is the type or
/// interface it is nested in, whose kind is `@impl`, and whose name is
/// [`implementation_name`] of its target.
#[must_use]
pub fn implementation_identity(
    package: &str,
    version: &str,
    module: &str,
    owner: &str,
    target: &str,
) -> DeclarationId {
    DeclarationId(declaration_identity(
        package,
        version,
        module,
        owner,
        "@impl",
        &implementation_name(target),
    ))
}

/// A declaration identity: `sha256:` and 64 lower-case hexadecimal digits.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeclarationId(String);

/// The name an `impl` block's anchor gives it: its target's source text
/// `text` with every run of white space dropped next to `(`, `)`, `{`, `}`,
/// and `:`, and shortened to one space elsewhere. `Iter( U8 )` and `Iter(U8)`
/// name one implementation.
#[must_use]
pub fn implementation_name(text: &str) -> String {
    let tight = |c: char| matches!(c, '(' | ')' | '{' | '}' | ':');
    let mut name = String::with_capacity(text.len());
    let mut space = false;
    for c in text.trim().chars() {
        if c.is_whitespace() {
            space = true;
            continue;
        }
        if space && !tight(c) && !name.ends_with(tight) && !name.is_empty() {
            name.push(' ');
        }
        space = false;
        name.push(c);
    }
    name
}

impl DeclarationId {
    /// The identity of the member `name` of the implementation this
    /// identity identifies: this identity, `.`, and the name.
    #[must_use]
    pub fn member(&self, name: &str) -> Self {
        Self(format!("{}.{name}", self.0))
    }

    /// The identity's text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DeclarationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

//! The `@project.v2` document.
//!
//! `docs/spec/04-programs-and-packages.md`, "Project file", gives the record
//! table: `project`, `package`, `target`, `grant binding`, `limits`,
//! `path dependency`, and `Git dependency`. The decoder reads one document's
//! bytes and no path. Every target is available for checking: a `@bin`
//! target keeps its `entry`, its `effects` list, and its `grants` dict with
//! where each is written, for the checks of [`crate::entry`]. Every
//! dependency is decoded and marked unavailable (path dependencies until M2,
//! Git dependencies throughout this release).
//!
//! The grant binding's `resource` and `policy` are opaque here: their schemas
//! depend on the provider and are the effects chapter's, checked with the
//! binary target's recipes.

use vibra_diagnostics::{ByteSpan, Diagnostic};
use vibra_syntax::{
    FieldSchema, IntegerSuffix, RecordSchema, Schema, Slot, TypedDocument, TypedNode,
    VariantSchema, decode, is_reserved_word,
};

/// The name of the project marker.
pub const PROJECT_FILE_NAME: &str = "project.vibon";

/// The format atom of the project file.
pub const PROJECT_FORMAT: &str = "@project.v2";

/// The kind of a local target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TargetKind {
    /// `@bin`.
    Bin,
    /// `@lib`.
    Lib,
}

impl TargetKind {
    /// The atom, `@` included.
    #[must_use]
    pub const fn as_atom(self) -> &'static str {
        match self {
            Self::Bin => "@bin",
            Self::Lib => "@lib",
        }
    }
}

/// The kind of a dependency.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DependencyKind {
    /// `@path`.
    Path,
    /// `@git`.
    Git,
}

impl DependencyKind {
    /// The atom, `@` included.
    #[must_use]
    pub const fn as_atom(self) -> &'static str {
        match self {
            Self::Path => "@path",
            Self::Git => "@git",
        }
    }
}

/// Whether this release can deliver a dependency.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Availability {
    /// Delivered.
    Available,
    /// Decoded, and unavailable for delivery.
    Unavailable,
}

/// The package record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package {
    name: String,
    version: String,
}

impl Package {
    /// The package name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The semantic version.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }
}

/// One local target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    span: ByteSpan,
    name: String,
    name_span: ByteSpan,
    kind: TargetKind,
    root: String,
    root_span: ByteSpan,
    binary: Option<Binary>,
}

/// What a `@bin` target writes beside its name, kind, and root.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Binary {
    /// The `entry` atom and its value's span.
    entry: (String, ByteSpan),
    /// The `effects` list's value span, and each entry with its span.
    effects: (ByteSpan, Vec<(String, ByteSpan)>),
    /// The `grants` dict's value span, and each key with its span.
    grants: (ByteSpan, Vec<(String, ByteSpan)>),
}

impl Target {
    /// Where the target's record is written in the project file.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    /// The unit name, an atom with its `@`.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Where the name is written in the project file.
    #[must_use]
    pub const fn name_span(&self) -> ByteSpan {
        self.name_span
    }

    /// The target kind.
    #[must_use]
    pub const fn kind(&self) -> TargetKind {
        self.kind
    }

    /// The root as written.
    #[must_use]
    pub fn root(&self) -> &str {
        &self.root
    }

    /// Where the root is written in the project file.
    #[must_use]
    pub const fn root_span(&self) -> ByteSpan {
        self.root_span
    }

    /// The `entry` atom of a binary, `@` included, and where its value is
    /// written; `None` for a library.
    #[must_use]
    pub fn entry(&self) -> Option<(&str, ByteSpan)> {
        self.binary
            .as_ref()
            .map(|binary| (binary.entry.0.as_str(), binary.entry.1))
    }

    /// Where the `effects` list of a binary is written; `None` for a
    /// library.
    #[must_use]
    pub fn effects_span(&self) -> Option<ByteSpan> {
        self.binary.as_ref().map(|binary| binary.effects.0)
    }

    /// The entries of a binary's `effects` list, in written order, each
    /// with where it is written; empty for a library.
    #[must_use]
    pub fn effects(&self) -> &[(String, ByteSpan)] {
        self.binary
            .as_ref()
            .map_or(&[], |binary| binary.effects.1.as_slice())
    }

    /// Where the `grants` dict of a binary is written; `None` for a
    /// library.
    #[must_use]
    pub fn grants_span(&self) -> Option<ByteSpan> {
        self.binary.as_ref().map(|binary| binary.grants.0)
    }

    /// The leaf keys of a binary's `grants` dict, in dict order, each with
    /// where it is written; empty for a library.
    #[must_use]
    pub fn grant_keys(&self) -> &[(String, ByteSpan)] {
        self.binary
            .as_ref()
            .map_or(&[], |binary| binary.grants.1.as_slice())
    }
}

/// One declared dependency.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dependency {
    name: String,
    name_span: ByteSpan,
    span: ByteSpan,
    kind: DependencyKind,
    source: String,
    rev: Option<String>,
    target: Option<String>,
}

impl Dependency {
    /// The dependency name, the dict key.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Where the name is written.
    #[must_use]
    pub const fn name_span(&self) -> ByteSpan {
        self.name_span
    }

    /// The span of the whole entry, key to value.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    /// The kind.
    #[must_use]
    pub const fn kind(&self) -> DependencyKind {
        self.kind
    }

    /// The path of a path dependency or the URL of a Git dependency.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The revision of a Git dependency.
    #[must_use]
    pub fn rev(&self) -> Option<&str> {
        self.rev.as_deref()
    }

    /// The selected library target, an atom with its `@`.
    #[must_use]
    pub fn target(&self) -> Option<&str> {
        self.target.as_deref()
    }

    /// Dependencies are unavailable in this release.
    #[must_use]
    pub const fn availability(&self) -> Availability {
        Availability::Unavailable
    }
}

/// A decoded `@project.v2` document.
#[derive(Clone, Debug, PartialEq)]
pub struct Project {
    source_id: String,
    package: Package,
    targets: Vec<Target>,
    dependencies: Vec<Dependency>,
    document: TypedDocument,
}

impl Project {
    /// The source identity the project was decoded under.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// The package.
    #[must_use]
    pub const fn package(&self) -> &Package {
        &self.package
    }

    /// The targets, in written order.
    #[must_use]
    pub fn targets(&self) -> &[Target] {
        &self.targets
    }

    /// The dependencies, in dict order (the UTF-8 bytes of their names).
    #[must_use]
    pub fn dependencies(&self) -> &[Dependency] {
        &self.dependencies
    }

    /// The validated document.
    #[must_use]
    pub const fn document(&self) -> &TypedDocument {
        &self.document
    }
}

fn is_identifier(text: &str) -> bool {
    let mut characters = text.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && characters.all(|character| character.is_ascii_alphanumeric())
}

fn package_name(text: &str) -> Option<&'static str> {
    if !is_identifier(text) {
        return Some("a package name satisfies the source identifier grammar");
    }
    if is_reserved_word(text) {
        return Some("a package name is not a reserved word");
    }
    if !text.starts_with(|first: char| first.is_ascii_lowercase()) {
        return Some("a package name is camelCase");
    }
    None
}

fn numeric_identifier(text: &str) -> bool {
    !text.is_empty()
        && text.bytes().all(|byte| byte.is_ascii_digit())
        && (text == "0" || !text.starts_with('0'))
}

fn dotted_identifiers(text: &str, numeric_rule: bool) -> bool {
    text.split('.').all(|part| {
        !part.is_empty()
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            && (!numeric_rule
                || !part.bytes().all(|byte| byte.is_ascii_digit())
                || numeric_identifier(part))
    })
}

/// One semantic version (Semantic Versioning 2.0.0): `MAJOR.MINOR.PATCH`,
/// an optional pre-release, and optional build metadata.
fn semantic_version(text: &str) -> Option<&'static str> {
    const REASON: &str = "a version is one semantic version, never a range";
    let (rest, build) = match text.split_once('+') {
        Some((rest, build)) => (rest, Some(build)),
        None => (text, None),
    };
    let (core, pre) = match rest.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (rest, None),
    };
    let parts = core.split('.').collect::<Vec<_>>();
    let valid = parts.len() == 3
        && parts.iter().all(|part| numeric_identifier(part))
        && pre.is_none_or(|pre| dotted_identifiers(pre, true))
        && build.is_none_or(|build| dotted_identifiers(build, false));
    (!valid).then_some(REASON)
}

fn target_name(atom: &str) -> Option<&'static str> {
    let part = atom.strip_prefix('@').unwrap_or(atom);
    if part.contains('.') {
        return Some("a target name is a one-part atom");
    }
    if is_reserved_word(part) {
        return Some("a target name's part is not a reserved word");
    }
    None
}

fn git_url(url: &str) -> Option<&'static str> {
    let scheme_end = url.find("://");
    let https = scheme_end
        .and_then(|end| url.get(..end))
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("https"));
    if !https {
        return Some("a Git dependency is an HTTPS URL");
    }
    let rest = scheme_end
        .and_then(|end| url.get(end + 3..))
        .unwrap_or_default();
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.is_empty() {
        return Some("a Git dependency is an HTTPS URL");
    }
    if authority.contains('@') {
        return Some("a Git dependency URL has no userinfo");
    }
    None
}

fn git_revision(rev: &str) -> Option<&'static str> {
    let valid = rev.len() == 40
        && rev
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'));
    (!valid).then_some("a revision is 40 lower-case hexadecimal digits")
}

fn grant_binding() -> Slot {
    let bound = || Slot::Integer(IntegerSuffix::U64);
    Slot::Record(RecordSchema::new(vec![
        FieldSchema::required("provider", Slot::Atom(None)),
        FieldSchema::required("resource", Slot::Opaque),
        FieldSchema::optional("policy", Slot::Opaque),
        FieldSchema::optional(
            "limits",
            Slot::Record(RecordSchema::at_least_one(vec![
                FieldSchema::optional("invocations", bound()),
                FieldSchema::optional("inputBytes", bound()),
                FieldSchema::optional("outputBytes", bound()),
            ])),
        ),
        FieldSchema::required("after", Slot::list(Slot::Atom(None))),
    ]))
}

fn target() -> Slot {
    let common = |kind: &'static str| {
        vec![
            FieldSchema::required("name", Slot::Atom(Some(target_name))),
            FieldSchema::required("kind", Slot::AtomOf(vec![kind])),
            FieldSchema::required("root", Slot::Str(None)),
        ]
    };
    let mut binary = common("@bin");
    binary.extend([
        FieldSchema::required("entry", Slot::Atom(None)),
        FieldSchema::required("effects", Slot::list(Slot::Atom(None))),
        FieldSchema::required("grants", Slot::dict(Slot::Atom(None), grant_binding())),
    ]);
    Slot::Variant(VariantSchema::new(
        "kind",
        vec![
            ("@bin", RecordSchema::new(binary)),
            ("@lib", RecordSchema::new(common("@lib"))),
        ],
    ))
}

fn dependency() -> Slot {
    Slot::Variant(VariantSchema::new(
        "kind",
        vec![
            (
                "@path",
                RecordSchema::new(vec![
                    FieldSchema::required("kind", Slot::AtomOf(vec!["@path"])),
                    FieldSchema::required("path", Slot::Str(None)),
                    FieldSchema::optional("target", Slot::Atom(None)),
                ]),
            ),
            (
                "@git",
                RecordSchema::new(vec![
                    FieldSchema::required("kind", Slot::AtomOf(vec!["@git"])),
                    FieldSchema::required("git", Slot::Str(Some(git_url))),
                    FieldSchema::required("rev", Slot::Str(Some(git_revision))),
                    FieldSchema::optional("target", Slot::Atom(None)),
                ]),
            ),
        ],
    ))
}

/// The closed `@project.v2` schema.
#[must_use]
pub fn project_schema() -> Schema {
    Schema::new(
        PROJECT_FORMAT,
        vec![
            FieldSchema::required(
                "package",
                Slot::Record(RecordSchema::new(vec![
                    FieldSchema::required("name", Slot::Str(Some(package_name))),
                    FieldSchema::required("version", Slot::Str(Some(semantic_version))),
                ])),
            ),
            FieldSchema::required("targets", Slot::non_empty_list(target())),
            FieldSchema::required(
                "dependencies",
                Slot::dict(Slot::Str(None), dependency()),
            ),
        ],
    )
}

fn text_of(node: Option<&TypedNode>) -> String {
    node.and_then(|node| node.as_str().or_else(|| node.as_atom()))
        .unwrap_or_default()
        .to_owned()
}

/// The atom of `node`, when it is one, with where it is written.
fn atom_of(node: &TypedNode) -> (String, ByteSpan) {
    (text_of(Some(node)), node.span())
}

/// The fields a validated `@bin` target record writes beside its name,
/// kind, and root. A field the schema requires is present in a validated
/// document; the target's own span stands in where one is not.
fn binary(target: &TypedNode) -> Binary {
    let entry = target
        .get("entry")
        .map_or_else(|| (String::new(), target.span()), atom_of);
    let effects = target.get("effects").map_or_else(
        || (target.span(), Vec::new()),
        |list| {
            (
                list.span(),
                list.as_list()
                    .unwrap_or_default()
                    .iter()
                    .map(atom_of)
                    .collect(),
            )
        },
    );
    let grants = target.get("grants").map_or_else(
        || (target.span(), Vec::new()),
        |dict| {
            (
                dict.span(),
                dict.as_entries()
                    .unwrap_or_default()
                    .iter()
                    .map(|entry| atom_of(entry.key()))
                    .collect(),
            )
        },
    );
    Binary {
        entry,
        effects,
        grants,
    }
}

/// Decodes one `project.vibon`, read as `source_id`, with no discovery and
/// no path read.
pub fn decode_project(
    source_id: &str,
    bytes: &[u8],
) -> Result<Project, Vec<Diagnostic>> {
    let (document, diagnostics) =
        decode(&project_schema(), Some(source_id), bytes).into_parts();
    let Some(document) = document else {
        return Err(diagnostics);
    };
    let root = document.root();
    let package = root.get("package");
    let package = Package {
        name: text_of(package.and_then(|package| package.get("name"))),
        version: text_of(package.and_then(|package| package.get("version"))),
    };
    let targets = root
        .get("targets")
        .and_then(TypedNode::as_list)
        .unwrap_or_default()
        .iter()
        .map(|target| {
            let kind =
                if target.get("kind").and_then(TypedNode::as_atom) == Some("@bin") {
                    TargetKind::Bin
                } else {
                    TargetKind::Lib
                };
            let name = target.get("name");
            let root = target.get("root");
            Target {
                span: target.span(),
                name: text_of(name),
                name_span: name.map_or(target.span(), TypedNode::span),
                kind,
                root: text_of(root),
                root_span: root.map_or(target.span(), TypedNode::span),
                binary: (kind == TargetKind::Bin).then(|| binary(target)),
            }
        })
        .collect();
    let dependencies = root
        .get("dependencies")
        .and_then(TypedNode::as_entries)
        .unwrap_or_default()
        .iter()
        .map(|entry| {
            let value = entry.value();
            let kind = if value.get("kind").and_then(TypedNode::as_atom) == Some("@git")
            {
                DependencyKind::Git
            } else {
                DependencyKind::Path
            };
            let source = match kind {
                DependencyKind::Path => value.get("path"),
                DependencyKind::Git => value.get("git"),
            };
            Dependency {
                name: text_of(Some(entry.key())),
                name_span: entry.key().span(),
                span: entry.span(),
                kind,
                source: text_of(source),
                rev: value.get("rev").map(|rev| text_of(Some(rev))),
                target: value.get("target").map(|target| text_of(Some(target))),
            }
        })
        .collect();
    Ok(Project {
        source_id: source_id.to_owned(),
        package,
        targets,
        dependencies,
        document,
    })
}

#[cfg(test)]
mod tests {
    use super::{git_revision, git_url, package_name, semantic_version, target_name};

    #[test]
    fn the_value_constraints_of_the_record_table() {
        assert_eq!(package_name("hello"), None);
        assert_eq!(package_name("myApp2"), None);
        assert!(package_name("My-App").is_some());
        assert!(package_name("MyApp").is_some());
        assert!(package_name("type").is_some());
        assert!(package_name("").is_some());
        for good in ["0.1.0", "1.2.3-alpha.1", "1.0.0+build.5", "10.20.30-rc-1+x"] {
            assert_eq!(semantic_version(good), None, "{good}");
        }
        for bad in [
            "^1.0", "1.0", "01.0.0", "1.0.0-", "1.0.0-01", ">=1.0.0", "1.0.0 ",
        ] {
            assert!(semantic_version(bad).is_some(), "{bad}");
        }
        assert_eq!(target_name("@hello"), None);
        assert!(target_name("@hello.main").is_some());
        assert!(target_name("@fn").is_some());
        assert_eq!(git_url("https://example.com/money.git"), None);
        assert_eq!(git_url("HTTPS://example.com/x"), None);
        assert!(git_url("http://example.com/x").is_some());
        assert!(git_url("https://user@example.com/x").is_some());
        assert!(git_url("https:///x").is_some());
        assert!(git_url("example.com/x").is_some());
        assert_eq!(
            git_revision("0123456789abcdef0123456789abcdef01234567"),
            None
        );
        assert!(git_revision("abc").is_some());
        assert!(git_revision("0123456789ABCDEF0123456789abcdef01234567").is_some());
    }
}

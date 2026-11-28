//! The `@stdlibManifest.v2` document (`docs/spec/04-programs-and-packages.md`,
//! "The standard library"): its closed schema and its decoded form.
//!
//! The `symbols` record names its third list `nativeSymbol`, because `native`
//! is a reserved word, which the data grammar does not admit as a field name.

use vibra_diagnostics::ByteSpan;
use vibra_syntax::{FieldSchema, RecordSchema, Schema, Slot, TypedNode, decode};

use super::Fault;
use super::roles::ROLE_ATOMS;

/// The format atom of the manifest.
pub(crate) const FORMAT: &str = "@stdlibManifest.v2";

/// The schema of the standard-library manifest: the closed record of the
/// programs chapter.
#[must_use]
pub fn manifest_schema() -> Schema {
    let record = |fields| Slot::Record(RecordSchema::new(fields));
    let strings = || Slot::list(Slot::Str(None));
    Schema::new(
        FORMAT,
        vec![
            FieldSchema::required(
                "package",
                record(vec![
                    FieldSchema::required("name", Slot::Str(None)),
                    FieldSchema::required("version", Slot::Str(None)),
                ]),
            ),
            FieldSchema::required(
                "modules",
                Slot::list(record(vec![
                    FieldSchema::required("path", Slot::Atom(Some(unit_module_path))),
                    FieldSchema::required("digest", Slot::Str(Some(digest_spelling))),
                    FieldSchema::required(
                        "languageRoles",
                        Slot::list(Slot::AtomOf(ROLE_ATOMS.to_vec())),
                    ),
                ])),
            ),
            FieldSchema::required(
                "symbols",
                record(vec![
                    FieldSchema::required("compiler", strings()),
                    FieldSchema::required("host", strings()),
                    FieldSchema::required("nativeSymbol", strings()),
                ]),
            ),
        ],
    )
}

/// A module path of the unit `@std`: `@std.` and at least one more part.
fn unit_module_path(spelling: &str) -> Option<&'static str> {
    match spelling.strip_prefix("@std.") {
        Some(rest) if !rest.is_empty() => None,
        _ => Some("a module of the standard library is a path of the unit `@std`"),
    }
}

/// `sha256:` and 64 lower-case hexadecimal digits.
fn digest_spelling(spelling: &str) -> Option<&'static str> {
    (!vibra_ir::is_identity(spelling))
        .then_some("a digest is `sha256:` and 64 lower-case hexadecimal digits")
}

/// A string or an atom of the manifest and where it was written.
#[derive(Clone, Debug)]
pub(crate) struct Spelled {
    pub(crate) text: String,
    pub(crate) span: ByteSpan,
}

/// One entry of `modules`.
#[derive(Clone, Debug)]
pub(crate) struct ModuleEntry {
    pub(crate) path: Spelled,
    pub(crate) digest: Spelled,
    pub(crate) roles: Vec<Spelled>,
}

/// The decoded manifest.
#[derive(Clone, Debug)]
pub(crate) struct Manifest {
    pub(crate) package_name: String,
    pub(crate) package_version: String,
    pub(crate) modules: Vec<ModuleEntry>,
    pub(crate) compiler: Vec<Spelled>,
    pub(crate) host: Vec<Spelled>,
    pub(crate) native: Vec<Spelled>,
}

/// The source identity of the embedded manifest.
pub(crate) const MANIFEST_SOURCE_ID: &str = "stdlib/manifest.vibon";

/// Reads `bytes` through the manifest schema, or gives the faults of the
/// reading.
pub(crate) fn read(bytes: &[u8]) -> Result<Manifest, Vec<Fault>> {
    let (document, diagnostics) =
        decode(&manifest_schema(), Some(MANIFEST_SOURCE_ID), bytes).into_parts();
    let Some(document) = document else {
        return Err(diagnostics
            .iter()
            .map(|diagnostic| {
                Fault::at(
                    MANIFEST_SOURCE_ID,
                    diagnostic.primary_span(),
                    format!(
                        "{}: {}",
                        diagnostic.code().as_atom(),
                        diagnostic.message()
                    ),
                )
            })
            .collect());
    };
    let root = document.root();
    assemble(root).ok_or_else(|| {
        vec![Fault::at(
            MANIFEST_SOURCE_ID,
            root.span(),
            "the manifest does not have the shape of its schema",
        )]
    })
}

/// The manifest a schema-valid root record holds.
fn assemble(root: &TypedNode) -> Option<Manifest> {
    let package = root.get("package")?;
    let symbols = root.get("symbols")?;
    let modules = root
        .get("modules")?
        .as_list()?
        .iter()
        .map(|entry| {
            Some(ModuleEntry {
                path: spelled(entry.get("path")?)?,
                digest: spelled(entry.get("digest")?)?,
                roles: spelled_list(entry.get("languageRoles")?)?,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(Manifest {
        package_name: spelled(package.get("name")?)?.text,
        package_version: spelled(package.get("version")?)?.text,
        modules,
        compiler: spelled_list(symbols.get("compiler")?)?,
        host: spelled_list(symbols.get("host")?)?,
        native: spelled_list(symbols.get("nativeSymbol")?)?,
    })
}

/// A string or an atom and its span.
fn spelled(node: &TypedNode) -> Option<Spelled> {
    node.as_str()
        .or_else(|| node.as_atom())
        .map(|text| Spelled {
            text: text.to_owned(),
            span: node.span(),
        })
}

/// A list of strings or atoms and their spans.
fn spelled_list(node: &TypedNode) -> Option<Vec<Spelled>> {
    node.as_list()?.iter().map(spelled).collect()
}

/// The faults of a list that is not strictly ascending by the UTF-8 bytes of
/// its items: one at each item that does not follow its predecessor.
pub(crate) fn order_faults(what: &str, items: &[Spelled]) -> Vec<Fault> {
    items
        .windows(2)
        .filter_map(|pair| match pair {
            [earlier, later] if earlier.text.as_bytes() >= later.text.as_bytes() => {
                Some(Fault::at(
                    MANIFEST_SOURCE_ID,
                    later.span,
                    format!(
                        "{what} are not sorted by their UTF-8 bytes with each once: `{}` follows `{}`",
                        later.text, earlier.text
                    ),
                ))
            }
            _ => None,
        })
        .collect()
}

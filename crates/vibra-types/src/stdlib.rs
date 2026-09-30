//! The toolchain standard-library input (`docs/spec/04-programs-and-packages.md`,
//! "Toolchain standard-library input").
//!
//! Authority comes only from embedding: the manifest and every module are
//! compiled into the toolchain, and nothing a project supplies can add to
//! them. This module performs no I/O. [`load_stdlib_bytes`] is a pure function
//! over explicit byte inputs: it decodes `stdlib/manifest.vibon` through its
//! closed record, hashes each module against its entry, and checks every
//! `external: @compiler` symbol against the manifest and the compiler
//! registry. A mismatch is an internal toolchain defect, never a fallback.

use std::fmt;
use std::path::Path;

use sha2::{Digest, Sha256};
use vibra_ir::external::CompilerIntrinsic;
use vibra_syntax::{
    Attribute, DataField, DataNode, DataValue, Declaration, Literal, NameKind,
    TypeMember,
};

/// The source identity of the `@std.text` module.
pub const STDLIB_TEXT_SOURCE_ID: &str = "stdlib/src/std/text.vib";
/// The source identity of the `@std.assert` test-registry module.
pub const STDLIB_ASSERT_SOURCE_ID: &str = "stdlib/src/std/assert.vib";
/// The source identity of the `@std.option` module.
pub const STDLIB_OPTION_SOURCE_ID: &str = "stdlib/src/std/option.vib";
/// The source identity of the embedded `@std.result` module.
pub const STDLIB_RESULT_SOURCE_ID: &str = "stdlib/src/std/result.vib";
/// The source identity of the embedded `@std.core` module.
pub const STDLIB_CORE_SOURCE_ID: &str = "stdlib/src/std/core.vib";
/// The source identity of the `@std.builtin` module.
pub const STDLIB_BUILTIN_SOURCE_ID: &str = "stdlib/src/std/builtin.vib";

const MANIFEST_PATH: &str = "stdlib/manifest.vibon";
const SOURCE_ROOT: &str = "stdlib/src/";

/// The closed table of language roles (`docs/spec/02-type-system.md`,
/// "Language core and standard library"). Each is claimed by at most one
/// declaration; a role nothing claims yet is still implemented by the
/// toolchain until its migration step.
const LANGUAGE_ROLES: &[&str] =
    &["bool", "str", "bytes", "option", "result", "map", "iter"];

const PACKAGE_NAME: &str = "vibra-stdlib";
const PACKAGE_VERSION: &str = "0.2.0";

const EMBEDDED_MANIFEST: &[u8] = include_bytes!("../../../stdlib/manifest.vibon");

/// Every module compiled into the toolchain, by `stdlib/src/`-relative path.
const EMBEDDED_MODULES: &[(&str, &[u8])] = &[
    (
        "std/core.vib",
        include_bytes!("../../../stdlib/src/std/core.vib"),
    ),
    (
        "std/text.vib",
        include_bytes!("../../../stdlib/src/std/text.vib"),
    ),
    (
        "std/option.vib",
        include_bytes!("../../../stdlib/src/std/option.vib"),
    ),
    (
        "std/result.vib",
        include_bytes!("../../../stdlib/src/std/result.vib"),
    ),
    (
        "std/builtin.vib",
        include_bytes!("../../../stdlib/src/std/builtin.vib"),
    ),
    (
        "std/assert.vib",
        include_bytes!("../../../stdlib/src/std/assert.vib"),
    ),
];

/// The identity of the declared type `name` in the standard-library module
/// `@std.<module…>`: the identity the resolver gives every declaration of the
/// embedded package.
pub(crate) fn stdlib_type_id(module: &[&str], name: &str) -> vibra_ir::TypeId {
    let path = std::iter::once("std")
        .chain(module.iter().copied())
        .chain(std::iter::once(name))
        .collect::<Vec<_>>()
        .join(".");
    vibra_ir::TypeId::new(format!("@{PACKAGE_NAME}@{PACKAGE_VERSION}/{path}"), path)
}

/// Every embedded module's bytes, by `stdlib/src/`-relative path.
pub(crate) fn embedded_modules() -> &'static [(&'static str, &'static [u8])] {
    EMBEDDED_MODULES
}

/// The embedded bytes of the module at `path`, relative to `stdlib/src/`.
pub(crate) fn embedded_module(path: &str) -> Option<&'static [u8]> {
    EMBEDDED_MODULES
        .iter()
        .find(|(entry, _)| *entry == path)
        .map(|(_, bytes)| *bytes)
}

/// The exact byte inputs of one standard-library load.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StdlibInputs<'bytes> {
    /// `stdlib/manifest.vibon`.
    pub manifest: &'bytes [u8],
    /// Module bytes keyed by their `stdlib/src/`-relative path.
    pub modules: Vec<(&'bytes str, &'bytes [u8])>,
}

impl StdlibInputs<'static> {
    /// The manifest and modules embedded into the toolchain at build time.
    #[must_use]
    pub fn embedded() -> Self {
        Self {
            manifest: EMBEDDED_MANIFEST,
            modules: EMBEDDED_MODULES.to_vec(),
        }
    }
}

impl<'bytes> StdlibInputs<'bytes> {
    /// Replaces the bytes of the module at `path`, for host tests.
    #[must_use]
    pub fn with_module(mut self, path: &'bytes str, bytes: &'bytes [u8]) -> Self {
        if let Some(entry) = self.modules.iter_mut().find(|(entry, _)| *entry == path) {
            entry.1 = bytes;
        } else {
            self.modules.push((path, bytes));
        }
        self
    }
}

/// One admitted standard-library module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StdlibModule {
    atom: String,
    source_id: String,
    role: String,
    bytes: Vec<u8>,
}

impl StdlibModule {
    /// The module atom without its `@`, such as `std.text`.
    #[must_use]
    pub fn atom(&self) -> &str {
        &self.atom
    }

    /// The module's canonical source identity under `stdlib/src/`.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// The exact admitted bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Whether the module is the test-only assertion registry.
    #[must_use]
    pub fn is_test_registry(&self) -> bool {
        self.role == "test-registry"
    }
}

/// The loaded standard library: the `vibra-stdlib@0.2.0` package and its
/// admitted modules.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stdlib {
    package: vibra_resolve::PackageId,
    modules: Vec<StdlibModule>,
    compiler: Vec<String>,
    native: Vec<String>,
    assertions: Vec<String>,
}

impl Stdlib {
    /// The exact package identity the manifest fixes.
    #[must_use]
    pub const fn package(&self) -> &vibra_resolve::PackageId {
        &self.package
    }

    /// The admitted modules in manifest order.
    #[must_use]
    pub fn modules(&self) -> &[StdlibModule] {
        &self.modules
    }

    /// The admitted module with atom `atom`, such as `std.option`.
    #[must_use]
    pub fn module(&self, atom: &str) -> Option<&StdlibModule> {
        self.modules.iter().find(|module| module.atom == atom)
    }

    /// The `@compiler` symbols the modules may bind.
    #[must_use]
    pub fn compiler_symbols(&self) -> &[String] {
        &self.compiler
    }

    /// The native implementation symbols the modules may name.
    #[must_use]
    pub fn native_symbols(&self) -> &[String] {
        &self.native
    }

    /// The test-only assertion members.
    #[must_use]
    pub fn assertion_members(&self) -> &[String] {
        &self.assertions
    }

    /// The admitted modules as the resolver's standard-library overlay.
    #[must_use]
    pub fn resolver_overlay(
        &self,
    ) -> (vibra_resolve::PackageId, Vec<vibra_resolve::SourceModule>) {
        (
            self.package.clone(),
            self.modules
                .iter()
                .map(|module| {
                    let segments = module
                        .atom
                        .strip_prefix("std.")
                        .unwrap_or(&module.atom)
                        .split('.')
                        .collect::<Vec<_>>();
                    vibra_resolve::SourceModule::new(
                        "std",
                        segments,
                        &module.source_id,
                        &module.bytes,
                    )
                })
                .collect(),
        )
    }

    /// Whether the manifest maps `atom` to the module at `source_id`.
    pub(crate) fn maps(&self, atom: &str, source_id: &str) -> bool {
        self.modules
            .iter()
            .any(|module| module.atom == atom && module.source_id == source_id)
    }

    /// Whether `module` is exactly an admitted standard-library module.
    pub(crate) fn trusts_module(&self, module: &vibra_resolve::ModuleRecord) -> bool {
        module.package() == &self.package
            && self.modules.iter().any(|admitted| {
                module.source_id() == admitted.source_id
                    && module.bytes() == admitted.bytes
            })
    }
}

/// A standard-library input that fails its embedded contract: an internal
/// toolchain defect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StdlibError(String);

impl StdlibError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for StdlibError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for StdlibError {}

/// Loads the standard library embedded into this toolchain.
pub fn load_stdlib() -> Result<Stdlib, StdlibError> {
    load_stdlib_bytes(&StdlibInputs::embedded())
}

/// Loads one set of standard-library bytes without touching the filesystem.
pub fn load_stdlib_bytes(inputs: &StdlibInputs<'_>) -> Result<Stdlib, StdlibError> {
    let manifest = decode_manifest(inputs.manifest)?;
    if manifest.package_name != PACKAGE_NAME
        || manifest.package_version != PACKAGE_VERSION
    {
        return Err(StdlibError::new(format!(
            "standard-library manifest names `{}@{}`, expected `{PACKAGE_NAME}@{PACKAGE_VERSION}`",
            manifest.package_name, manifest.package_version
        )));
    }
    // The two tiers are disjoint: a primitive operation has no body, and a
    // native implementation accelerates a Vibra body.
    for (symbol, native) in manifest
        .compiler
        .iter()
        .map(|symbol| (symbol, false))
        .chain(manifest.native.iter().map(|symbol| (symbol, true)))
    {
        match CompilerIntrinsic::from_symbol(symbol) {
            None => {
                return Err(StdlibError::new(format!(
                    "standard-library manifest lists `{symbol}`, which is not in the compiler registry"
                )));
            }
            Some(intrinsic) if intrinsic.is_native() != native => {
                return Err(StdlibError::new(format!(
                    "standard-library manifest lists `{symbol}` in the wrong tier"
                )));
            }
            Some(_) => {}
        }
    }
    for (path, _) in &inputs.modules {
        if !manifest.modules.iter().any(|entry| entry.path == *path) {
            return Err(StdlibError::new(format!(
                "embedded standard-library module `{path}` has no manifest entry"
            )));
        }
    }
    let mut modules = Vec::with_capacity(manifest.modules.len());
    let mut claimed_roles = Vec::new();
    for entry in &manifest.modules {
        if !matches!(entry.role.as_str(), "source" | "test-registry") {
            return Err(StdlibError::new(format!(
                "standard-library module `@{}` has unknown role `@{}`",
                entry.atom, entry.role
            )));
        }
        let Some((_, bytes)) =
            inputs.modules.iter().find(|(path, _)| *path == entry.path)
        else {
            return Err(StdlibError::new(format!(
                "standard-library module `@{}` at `{}` is not embedded",
                entry.atom, entry.path
            )));
        };
        let actual = format!("{:x}", Sha256::digest(bytes));
        if actual != entry.sha256 {
            return Err(StdlibError::new(format!(
                "standard-library module `@{}` digest mismatch: expected sha256:{}, got sha256:{actual}",
                entry.atom, entry.sha256
            )));
        }
        let source_id = format!("{SOURCE_ROOT}{}", entry.path);
        for role in check_compiler_symbols(
            &source_id,
            bytes,
            &manifest.compiler,
            &manifest.native,
        )? {
            // A role is claimed once, from the closed table of the type chapter.
            if !LANGUAGE_ROLES.contains(&role.as_str()) {
                return Err(StdlibError::new(format!(
                    "`{source_id}` claims `@{role}`, which is not a language role"
                )));
            }
            if claimed_roles.contains(&role) {
                return Err(StdlibError::new(format!(
                    "`@{role}` is claimed by more than one declaration"
                )));
            }
            claimed_roles.push(role);
        }
        modules.push(StdlibModule {
            atom: entry.atom.clone(),
            source_id,
            role: entry.role.clone(),
            bytes: bytes.to_vec(),
        });
    }
    Ok(Stdlib {
        package: vibra_resolve::PackageId::new(
            manifest.package_name,
            manifest.package_version,
        ),
        modules,
        compiler: manifest.compiler,
        native: manifest.native,
        assertions: manifest.assertions,
    })
}

/// Requires every `external: @compiler` symbol and every `native:` symbol a
/// module declares to be listed in the manifest's matching tier. The checker
/// binds each against the registry signature. Returns the roles the module
/// claims with `role:`.
fn check_compiler_symbols(
    source_id: &str,
    bytes: &[u8],
    compiler: &[String],
    native: &[String],
) -> Result<Vec<String>, StdlibError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| StdlibError::new(format!("`{source_id}` is not UTF-8")))?;
    let document = vibra_syntax::parse_source(Path::new(source_id), text)
        .map_err(|error| StdlibError::new(format!("`{source_id}`: {error}")))?;
    if !document.accepted() || document.recovered() {
        return Err(StdlibError::new(format!(
            "`{source_id}` does not read cleanly"
        )));
    }
    let Some(ast) = document.ast() else {
        return Ok(Vec::new());
    };
    let mut roles = Vec::new();
    let mut attribute_lists = Vec::new();
    for declaration in ast.declarations() {
        match declaration {
            Declaration::Defn(function) => {
                attribute_lists.push(function.attributes().items())
            }
            Declaration::Deftype(deftype) => {
                for attribute in deftype.attributes().items() {
                    if let Attribute::Role(role) = attribute {
                        roles.push(role.value().to_owned());
                    }
                }
                for member in deftype.members() {
                    if let TypeMember::Method(method) = member {
                        attribute_lists.push(method.attributes().items());
                    }
                }
            }
            _ => {}
        }
    }
    for attributes in attribute_lists {
        for attribute in attributes {
            let (symbol, listed) = match attribute {
                Attribute::Symbol(Literal::String(symbol)) => (symbol, compiler),
                Attribute::Native(Literal::String(symbol)) => (symbol, native),
                _ => continue,
            };
            if !listed.iter().any(|listed| listed == symbol.value()) {
                return Err(StdlibError::new(format!(
                    "`{source_id}` binds `{}`, which the manifest does not list",
                    symbol.value()
                )));
            }
        }
    }
    Ok(roles)
}

/// One entry of the manifest's module map.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ModuleEntry {
    atom: String,
    path: String,
    sha256: String,
    role: String,
}

/// The decoded `@stdlib-manifest.v1` record.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Manifest {
    package_name: String,
    package_version: String,
    modules: Vec<ModuleEntry>,
    compiler: Vec<String>,
    native: Vec<String>,
    assertions: Vec<String>,
}

fn decode_manifest(bytes: &[u8]) -> Result<Manifest, StdlibError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| StdlibError::new("standard-library manifest is not UTF-8"))?;
    let document =
        vibra_syntax::parse_data(Path::new(MANIFEST_PATH), text).map_err(|error| {
            StdlibError::new(format!("standard-library manifest: {error}"))
        })?;
    if !document.accepted() || document.recovered() {
        return Err(StdlibError::new(
            "standard-library manifest is not valid VIBON data",
        ));
    }
    let root = document.data().ok_or_else(|| {
        StdlibError::new("standard-library manifest has no data value")
    })?;
    let mut record = RecordReader::new("manifest", root)?;
    record.expect_atom("format", "stdlib-manifest.v1")?;
    let manifest = Manifest {
        package_name: record.string("package-name")?,
        package_version: record.string("package-version")?,
        modules: record.modules("modules")?,
        compiler: record.strings("compiler")?,
        native: record.strings("native")?,
        assertions: record.strings("assertions")?,
    };
    record.finish()?;
    Ok(manifest)
}

/// Reads a record's fields in their exact, closed order.
struct RecordReader<'node> {
    label: &'static str,
    fields: std::slice::Iter<'node, DataField>,
}

impl<'node> RecordReader<'node> {
    fn new(label: &'static str, node: &'node DataNode) -> Result<Self, StdlibError> {
        match node.value() {
            DataValue::Record(fields) => Ok(Self {
                label,
                fields: fields.iter(),
            }),
            _ => Err(shape(label, "the root", "a record")),
        }
    }

    fn field(&mut self, name: &str) -> Result<&'node DataNode, StdlibError> {
        match self.fields.next() {
            Some(field) if field.label().value() == name => Ok(field.value()),
            Some(field) => Err(StdlibError::new(format!(
                "standard-library {} field `{}` appears where `{name}` is required",
                self.label,
                field.label().value()
            ))),
            None => Err(StdlibError::new(format!(
                "standard-library {} is missing field `{name}`",
                self.label
            ))),
        }
    }

    fn string(&mut self, name: &str) -> Result<String, StdlibError> {
        let node = self.field(name)?;
        string_value(node).ok_or_else(|| shape(self.label, name, "a string"))
    }

    fn digest(&mut self, name: &str) -> Result<String, StdlibError> {
        let value = self.string(name)?;
        digest_value(&value)
            .ok_or_else(|| shape(self.label, name, "a `sha256:` digest"))
    }

    fn atom(&mut self, name: &str) -> Result<String, StdlibError> {
        let node = self.field(name)?;
        atom_value(node).ok_or_else(|| shape(self.label, name, "an atom"))
    }

    fn expect_atom(&mut self, name: &str, expected: &str) -> Result<(), StdlibError> {
        let actual = self.atom(name)?;
        if actual == expected {
            Ok(())
        } else {
            Err(StdlibError::new(format!(
                "standard-library {} `{name}` is `@{actual}`, expected `@{expected}`",
                self.label
            )))
        }
    }

    fn strings(&mut self, name: &str) -> Result<Vec<String>, StdlibError> {
        let label = self.label;
        match self.field(name)?.value() {
            DataValue::Array(items) => items
                .iter()
                .map(|item| {
                    string_value(item).ok_or_else(|| shape(label, name, "strings"))
                })
                .collect(),
            _ => Err(shape(label, name, "an array")),
        }
    }

    fn modules(&mut self, name: &str) -> Result<Vec<ModuleEntry>, StdlibError> {
        let label = self.label;
        let DataValue::Map(entries) = self.field(name)?.value() else {
            return Err(shape(label, name, "a map"));
        };
        entries
            .iter()
            .map(|(key, value)| {
                let atom =
                    atom_value(key).ok_or_else(|| shape(label, name, "atom keys"))?;
                let mut entry = RecordReader::new(label, value)
                    .map_err(|_| shape(label, name, "record values"))?;
                let module = ModuleEntry {
                    atom,
                    path: entry.string("path")?,
                    sha256: entry.digest("sha256")?,
                    role: entry.atom("role")?,
                };
                entry.finish()?;
                Ok(module)
            })
            .collect()
    }

    fn finish(mut self) -> Result<(), StdlibError> {
        match self.fields.next() {
            None => Ok(()),
            Some(field) => Err(StdlibError::new(format!(
                "standard-library {} has unexpected field `{}`",
                self.label,
                field.label().value()
            ))),
        }
    }
}

fn shape(label: &str, field: &str, expected: &str) -> StdlibError {
    StdlibError::new(format!(
        "standard-library {label} `{field}` must be {expected}"
    ))
}

fn string_value(node: &DataNode) -> Option<String> {
    match node.value() {
        DataValue::Literal(Literal::String(literal)) => {
            Some(literal.value().to_owned())
        }
        _ => None,
    }
}

fn atom_value(node: &DataNode) -> Option<String> {
    match node.value() {
        DataValue::Atom(name) if name.kind() == NameKind::Atom => {
            Some(name.value().to_owned())
        }
        _ => None,
    }
}

/// Strips the `sha256:` prefix from a lowercase 64-digit hex digest.
fn digest_value(value: &str) -> Option<String> {
    let hex = value.strip_prefix("sha256:")?;
    (hex.len() == 64
        && hex
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f')))
    .then(|| hex.to_owned())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
mod tests {
    use super::{StdlibInputs, load_stdlib, load_stdlib_bytes};

    fn rejects(inputs: &StdlibInputs<'_>, fragment: &str) {
        let error = load_stdlib_bytes(inputs).expect_err("input must be rejected");
        assert!(
            error.to_string().contains(fragment),
            "`{error}` does not mention `{fragment}`"
        );
    }

    fn manifest() -> &'static str {
        std::str::from_utf8(StdlibInputs::embedded().manifest).expect("manifest text")
    }

    #[test]
    fn the_embedded_standard_library_loads() {
        let stdlib = load_stdlib().expect("embedded standard library");
        assert_eq!(
            stdlib.package(),
            &vibra_resolve::PackageId::new("vibra-stdlib", "0.2.0")
        );
        assert!(stdlib.maps("std.text", super::STDLIB_TEXT_SOURCE_ID));
        assert!(stdlib.maps("std.option", super::STDLIB_OPTION_SOURCE_ID));
        assert!(
            stdlib
                .module("std.assert")
                .is_some_and(|module| module.is_test_registry())
        );
        assert!(!stdlib.maps("std.text", super::STDLIB_ASSERT_SOURCE_ID));
    }

    #[test]
    fn a_module_digest_mismatch_is_rejected() {
        let inputs =
            StdlibInputs::embedded().with_module("std/text.vib", b"; tampered\n");
        rejects(&inputs, "`@std.text` digest mismatch");
    }

    #[test]
    fn an_unlisted_embedded_module_is_rejected() {
        let inputs = StdlibInputs::embedded().with_module("std/extra.vib", b"");
        rejects(&inputs, "`std/extra.vib` has no manifest entry");
    }

    #[test]
    fn a_manifest_symbol_absent_from_the_registry_is_rejected() {
        let manifest = manifest().replace(
            "compiler: (array\n    \"text.concat\"",
            "compiler: (array\n    \"text.reverse\"\n    \"text.concat\"",
        );
        let mut inputs = StdlibInputs::embedded();
        inputs.manifest = manifest.as_bytes();
        rejects(
            &inputs,
            "`text.reverse`, which is not in the compiler registry",
        );
    }

    #[test]
    fn a_module_binding_an_unlisted_symbol_is_rejected() {
        let manifest = manifest().replace("    \"text.length\"\n", "");
        let mut inputs = StdlibInputs::embedded();
        inputs.manifest = manifest.as_bytes();
        rejects(
            &inputs,
            "binds `text.length`, which the manifest does not list",
        );
    }

    #[test]
    fn a_native_symbol_in_the_wrong_tier_or_unlisted_is_rejected() {
        let listed = "native: (array \"array.of\" \"map.of\")";
        let moved = manifest().replace(
            listed,
            "native: (array \"array.of\" \"map.of\" \"array.length\")",
        );
        let mut inputs = StdlibInputs::embedded();
        inputs.manifest = moved.as_bytes();
        rejects(&inputs, "`array.length` in the wrong tier");
        let unlisted = manifest().replace(listed, "native: (array \"array.of\")");
        let mut inputs = StdlibInputs::embedded();
        inputs.manifest = unlisted.as_bytes();
        rejects(&inputs, "binds `map.of`, which the manifest does not list");
    }

    #[test]
    fn an_unknown_or_repeated_role_is_rejected() {
        use sha2::Digest;
        let embedded = StdlibInputs::embedded();
        let option = embedded
            .modules
            .iter()
            .find(|(path, _)| *path == "std/option.vib")
            .map(|(_, bytes)| std::str::from_utf8(bytes).expect("text"))
            .expect("option module");
        let digest = |bytes: &[u8]| format!("{:x}", sha2::Sha256::digest(bytes));
        let with_option = |text: &str| {
            manifest().replace(&digest(option.as_bytes()), &digest(text.as_bytes()))
        };

        let unknown = option.replace("role: @option", "role: @maybe");
        let manifest = with_option(&unknown);
        let mut inputs =
            StdlibInputs::embedded().with_module("std/option.vib", unknown.as_bytes());
        inputs.manifest = manifest.as_bytes();
        rejects(&inputs, "claims `@maybe`, which is not a language role");

        let repeated = format!(
            "{option}\n(deftype other (enum a void)\n  role: @option\n  visibility: @public)\n"
        );
        let manifest = with_option(&repeated);
        let mut inputs =
            StdlibInputs::embedded().with_module("std/option.vib", repeated.as_bytes());
        inputs.manifest = manifest.as_bytes();
        rejects(&inputs, "`@option` is claimed by more than one declaration");
    }

    #[test]
    fn malformed_manifests_are_rejected_by_shape() {
        let embedded = manifest();
        let cases = [
            ("not data at all".to_owned(), "not valid VIBON data"),
            ("(array 1)".to_owned(), "must be a record"),
            (
                embedded.replace("format: @stdlib-manifest.v1", "format: @other"),
                "expected `@stdlib-manifest.v1`",
            ),
            (
                embedded.replace(
                    "package-version: \"0.2.0\"",
                    "package-version: \"0.1.0\"",
                ),
                "expected `vibra-stdlib@0.2.0`",
            ),
            (
                embedded.replace("sha256: \"sha256:", "sha256: \"md5:"),
                "`sha256` must be a `sha256:` digest",
            ),
            (
                embedded.replace("  compiler:", "  extra: 1\n  compiler:"),
                "`extra` appears where `compiler` is required",
            ),
            (
                embedded.replace("role: @source", "role: @script"),
                "unknown role `@script`",
            ),
        ];
        for (text, fragment) in &cases {
            let mut inputs = StdlibInputs::embedded();
            inputs.manifest = text.as_bytes();
            rejects(&inputs, fragment);
        }
    }
}

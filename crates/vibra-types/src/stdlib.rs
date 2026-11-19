//! The embedded standard library and its loader
//! (`docs/spec/04-programs-and-packages.md`, "The standard library").
//!
//! The standard library's authority is its embedding: the manifest and every
//! module are compiled into the toolchain, and nothing a project supplies can
//! add to them or replace them. [`load_stdlib_from`] is a pure function over
//! explicit bytes. It reads the manifest through its closed schema, checks
//! every module against its digest, parses every module in standard-library
//! mode, checks every external declaration against its registry, checks that
//! each of the nine language roles is claimed exactly once on a declaration of
//! the shape the type chapter requires, and binds the prelude. Any fault is a
//! toolchain defect, reported as `@runtime.invalidCheckedProgram` with the
//! fault in its notes, never a fallback.

mod external;
mod identity;
mod manifest;
mod names;
mod roles;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::OnceLock;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::external::CompilerIntrinsic;
use vibra_ir::external::host::host_row;
use vibra_ir::{IrError, Library, LibraryTypes, TypeId};
use vibra_resolve::{EmbeddedUnit, PackageId, PreludeBinding, SourceModule};
use vibra_syntax::source::{Declaration, Module, SourceOptions, TypeBody, module};
use vibra_syntax::{Document, parse_source_with};

pub use external::check_externals;
pub use identity::declaration_identity;
pub use manifest::manifest_schema;
pub use names::LibraryNames;
pub use roles::Role;

use manifest::{MANIFEST_SOURCE_ID, Manifest, order_faults};
use names::{BUILTIN_MODULE, intrinsic};
use roles::{Claim, Parsed, check_core_types, check_roles};

/// The unit the standard library enters resolution as.
pub const STDLIB_UNIT: &str = "@std";

/// The prelude of the type chapter: the only names a program names without
/// an import. `Any` and `Self` are predeclared and have no declaration; every
/// other name is a declaration of the standard library.
pub const PRELUDE: [&str; 31] = [
    "Void",
    "Never",
    "Atom",
    "Char",
    "I8",
    "I16",
    "I32",
    "I64",
    "U8",
    "U16",
    "U32",
    "U64",
    "F32",
    "F64",
    "List",
    "Bool",
    "Str",
    "Bytes",
    "Option",
    "Result",
    "Dict",
    "Iter",
    "Equatable",
    "Ordered",
    "From",
    "TryFrom",
    "Any",
    "Self",
    "Ordering",
    "true",
    "false",
];

/// The prelude names that are predeclared rather than declared.
const PREDECLARED: [&str; 2] = ["Any", "Self"];

/// The prelude names that are builtin types: each is a type of
/// `@std.builtin` whose body is `Intrinsic` with its wire tag.
const BUILTIN_PRELUDE: [&str; 15] = [
    "Void", "Never", "Atom", "Char", "I8", "I16", "I32", "I64", "U8", "U16", "U32",
    "U64", "F32", "F64", "List",
];

const EMBEDDED_MANIFEST: &[u8] = include_bytes!("../../../stdlib/manifest.vibon");

/// Every module compiled into the toolchain, by module path atom, in
/// manifest order.
const EMBEDDED_MODULES: [(&str, &[u8]); 6] = [
    (
        "@std.bool",
        include_bytes!("../../../stdlib/src/std/bool.vib"),
    ),
    (
        "@std.builtin",
        include_bytes!("../../../stdlib/src/std/builtin.vib"),
    ),
    (
        "@std.core",
        include_bytes!("../../../stdlib/src/std/core.vib"),
    ),
    (
        "@std.iter",
        include_bytes!("../../../stdlib/src/std/iter.vib"),
    ),
    (
        "@std.option",
        include_bytes!("../../../stdlib/src/std/option.vib"),
    ),
    (
        "@std.result",
        include_bytes!("../../../stdlib/src/std/result.vib"),
    ),
];

/// The source identity of the module at the path atom `path`: its file
/// under `stdlib/src/`, such as `stdlib/src/std/bool.vib`.
fn source_id_of(path: &str) -> String {
    let parts = path.strip_prefix('@').unwrap_or(path).replace('.', "/");
    format!("stdlib/src/{parts}.vib")
}

/// The exact bytes of one standard-library load.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StdlibInputs<'bytes> {
    /// The bytes of `stdlib/manifest.vibon`.
    pub manifest: &'bytes [u8],
    /// The bytes of each module, keyed by its module path atom, such as
    /// `@std.bool`.
    pub modules: Vec<(&'bytes str, &'bytes [u8])>,
}

impl StdlibInputs<'static> {
    /// The manifest and the modules embedded in the toolchain at build time.
    #[must_use]
    pub fn embedded() -> Self {
        Self {
            manifest: EMBEDDED_MANIFEST,
            modules: EMBEDDED_MODULES.to_vec(),
        }
    }
}

impl<'bytes> StdlibInputs<'bytes> {
    /// The same inputs with the manifest `bytes`.
    #[must_use]
    pub fn with_manifest(mut self, bytes: &'bytes [u8]) -> Self {
        self.manifest = bytes;
        self
    }

    /// The same inputs with the module `path` holding `bytes`, added when it
    /// is not one of them.
    #[must_use]
    pub fn with_module(mut self, path: &'bytes str, bytes: &'bytes [u8]) -> Self {
        match self.modules.iter_mut().find(|(entry, _)| *entry == path) {
            Some(entry) => entry.1 = bytes,
            None => self.modules.push((path, bytes)),
        }
        self
    }

    /// The same inputs without the module `path`.
    #[must_use]
    pub fn without_module(mut self, path: &str) -> Self {
        self.modules.retain(|(entry, _)| *entry != path);
        self
    }
}

/// One module of the loaded standard library.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StdlibModule {
    path: String,
    source_id: String,
    digest: String,
    bytes: Vec<u8>,
    roles: Vec<Role>,
}

impl StdlibModule {
    /// The module path atom, such as `@std.bool`.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The module's parts after the unit, such as `["bool"]`.
    #[must_use]
    pub fn segments(&self) -> Vec<&str> {
        self.path
            .strip_prefix("@std.")
            .map(|rest| rest.split('.').collect())
            .unwrap_or_default()
    }

    /// The source identity, such as `stdlib/src/std/bool.vib`.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// The digest the manifest records and the bytes have.
    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// The module's exact bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The roles the module's declarations claim, in the order of the type
    /// chapter's table.
    #[must_use]
    pub fn roles(&self) -> &[Role] {
        &self.roles
    }

    /// The module parsed in standard-library mode.
    #[must_use]
    pub fn parse(&self) -> Document {
        parse_source_with(Some(&self.source_id), &self.bytes, standard_library())
    }
}

fn standard_library() -> SourceOptions {
    SourceOptions {
        standard_library: true,
    }
}

/// A top-level declaration of the standard library, with its identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StdlibDeclaration {
    module: String,
    name: String,
    kind: &'static str,
    identity: String,
}

impl StdlibDeclaration {
    fn new(
        package: (&str, &str),
        module: &str,
        name: &str,
        kind: &'static str,
    ) -> Self {
        Self {
            module: module.to_owned(),
            name: name.to_owned(),
            kind,
            identity: declaration_identity(
                package.0, package.1, module, "", kind, name,
            ),
        }
    }

    /// The module path atom, such as `@std.bool`.
    #[must_use]
    pub fn module(&self) -> &str {
        &self.module
    }

    /// The declaration's name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The declaration kind atom: `@type`, `@interface`, or `@value`.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        self.kind
    }

    /// The declaration identity, `sha256:` and 64 hexadecimal digits.
    #[must_use]
    pub fn identity(&self) -> &str {
        &self.identity
    }

    /// The canonical atom path without its `@`, such as `std.bool.Bool`.
    #[must_use]
    pub fn path(&self) -> String {
        format!(
            "{}.{}",
            self.module.strip_prefix('@').unwrap_or(&self.module),
            self.name
        )
    }

    /// The identity of a type or an interface in the checked IR.
    ///
    /// # Errors
    ///
    /// The path or the identity is not the IR's spelling.
    pub fn type_id(&self) -> Result<TypeId, IrError> {
        TypeId::new(self.path(), self.identity.clone())
    }
}

/// One module of the standard library as the resolver reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OverlayModule {
    /// The module path atom, such as `@std.bool`.
    pub path: String,
    /// The module's parts after the unit, such as `["bool"]`.
    pub segments: Vec<String>,
    /// The source identity.
    pub source_id: String,
    /// The module's exact bytes.
    pub bytes: Vec<u8>,
}

/// The standard library as the resolver reads it: the unit `@std` of the
/// embedded package, its modules, and the declaration each prelude name
/// resolves to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StdlibOverlay {
    /// The unit, `@std`.
    pub unit: &'static str,
    /// The package name.
    pub package_name: String,
    /// The package version.
    pub package_version: String,
    /// The modules, in manifest order.
    pub modules: Vec<OverlayModule>,
    /// Each prelude name that is a declaration, with that declaration.
    pub prelude: BTreeMap<String, StdlibDeclaration>,
}

impl StdlibOverlay {
    /// The overlay as resolution's input: the unit, its modules as sources,
    /// and the prelude table with each declaration's identity.
    #[must_use]
    pub fn embedded_unit(&self) -> EmbeddedUnit {
        EmbeddedUnit::new(
            PackageId::new(&self.package_name, &self.package_version),
            self.unit,
            self.modules
                .iter()
                .map(|module| {
                    SourceModule::new(
                        &module.path,
                        &module.source_id,
                        module.bytes.clone(),
                    )
                })
                .collect(),
            self.prelude
                .iter()
                .map(|(name, declaration)| {
                    PreludeBinding::new(name, declaration.identity())
                })
                .collect(),
        )
    }
}

/// The loaded standard library.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stdlib {
    package_name: String,
    package_version: String,
    modules: Vec<StdlibModule>,
    roles: BTreeMap<Role, StdlibDeclaration>,
    library: BTreeMap<Library, StdlibDeclaration>,
    library_types: LibraryTypes,
    prelude: BTreeMap<String, StdlibDeclaration>,
    compiler: Vec<String>,
    host: Vec<String>,
    native: Vec<String>,
    names: LibraryNames,
}

impl Stdlib {
    /// The package name the manifest records.
    #[must_use]
    pub fn package_name(&self) -> &str {
        &self.package_name
    }

    /// The package version the manifest records.
    #[must_use]
    pub fn package_version(&self) -> &str {
        &self.package_version
    }

    /// The modules, in manifest order.
    #[must_use]
    pub fn modules(&self) -> &[StdlibModule] {
        &self.modules
    }

    /// The module at the path atom `path`.
    #[must_use]
    pub fn module(&self, path: &str) -> Option<&StdlibModule> {
        self.modules.iter().find(|module| module.path == path)
    }

    /// The declaration that claims each role.
    #[must_use]
    pub const fn roles(&self) -> &BTreeMap<Role, StdlibDeclaration> {
        &self.roles
    }

    /// The declaration that claims `role`.
    #[must_use]
    pub fn role(&self, role: Role) -> Option<&StdlibDeclaration> {
        self.roles.get(&role)
    }

    /// The declaration of each library declaration the checked IR names:
    /// the nine roles and the three core types.
    #[must_use]
    pub const fn library(&self) -> &BTreeMap<Library, StdlibDeclaration> {
        &self.library
    }

    /// The identities the checked IR binds to its library declarations.
    #[must_use]
    pub const fn library_types(&self) -> &LibraryTypes {
        &self.library_types
    }

    /// The declaration each prelude name resolves to; `Any` and `Self` have
    /// none.
    #[must_use]
    pub const fn prelude(&self) -> &BTreeMap<String, StdlibDeclaration> {
        &self.prelude
    }

    /// The `@compiler` symbols the manifest lets the modules bind.
    #[must_use]
    pub fn compiler_symbols(&self) -> &[String] {
        &self.compiler
    }

    /// The `@host` symbols the manifest lets the modules bind.
    #[must_use]
    pub fn host_symbols(&self) -> &[String] {
        &self.host
    }

    /// The native symbols the manifest declares.
    #[must_use]
    pub fn native_symbols(&self) -> &[String] {
        &self.native
    }

    /// What the standard library's declarations are, for
    /// [`check_externals`].
    #[must_use]
    pub const fn names(&self) -> &LibraryNames {
        &self.names
    }

    /// The standard library as the resolver reads it: the unit `@std`.
    #[must_use]
    pub fn overlay(&self) -> StdlibOverlay {
        StdlibOverlay {
            unit: STDLIB_UNIT,
            package_name: self.package_name.clone(),
            package_version: self.package_version.clone(),
            modules: self
                .modules
                .iter()
                .map(|module| OverlayModule {
                    path: module.path.clone(),
                    segments: module
                        .segments()
                        .into_iter()
                        .map(str::to_owned)
                        .collect(),
                    source_id: module.source_id.clone(),
                    bytes: module.bytes.clone(),
                })
                .collect(),
            prelude: self.prelude.clone(),
        }
    }
}

/// A fault of the embedded standard library: where it is, when that is
/// known, and what it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Fault {
    origin: Option<(String, ByteSpan)>,
    note: String,
}

impl Fault {
    /// A fault at `span` of the document `source_id`.
    pub(crate) fn at(source_id: &str, span: ByteSpan, note: impl Into<String>) -> Self {
        Self {
            origin: Some((source_id.to_owned(), span)),
            note: note.into(),
        }
    }

    /// A fault with no location.
    pub(crate) fn bare(note: impl Into<String>) -> Self {
        Self {
            origin: None,
            note: note.into(),
        }
    }

    /// A diagnostic of a check, as a fault of the library.
    fn of_diagnostic(source_id: &str, diagnostic: &Diagnostic) -> Self {
        let mut note =
            format!("{}: {}", diagnostic.code().as_atom(), diagnostic.message());
        for extra in diagnostic.notes() {
            note.push_str("; ");
            note.push_str(extra);
        }
        Self::at(source_id, diagnostic.primary_span(), note)
    }
}

/// The embedded standard library does not hold together: a toolchain defect.
///
/// Its diagnostic is `@runtime.invalidCheckedProgram`, at the first fault's
/// origin when it has one, with every fault in its notes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StdlibError {
    diagnostic: Box<Diagnostic>,
}

impl StdlibError {
    fn new(faults: Vec<Fault>) -> Self {
        let origin = faults.iter().find_map(|fault| fault.origin.clone());
        let span = origin
            .as_ref()
            .map_or(ByteSpan::empty_at(0), |(_, span)| *span);
        let mut diagnostic = Diagnostic::new(
            DiagnosticCode::RuntimeInvalidCheckedProgram,
            span,
            "the embedded standard library is defective",
        );
        if let Some((source_id, _)) = origin {
            diagnostic = diagnostic.with_source_id(source_id);
        }
        for fault in faults {
            let note = match fault.origin {
                Some((source_id, span)) => format!(
                    "{source_id} {}..{}: {}",
                    span.start(),
                    span.end(),
                    fault.note
                ),
                None => fault.note,
            };
            diagnostic = diagnostic.with_note(note);
        }
        Self {
            diagnostic: Box::new(diagnostic),
        }
    }

    /// The defect: `@runtime.invalidCheckedProgram`, with each fault in a
    /// note.
    #[must_use]
    pub fn diagnostic(&self) -> &Diagnostic {
        &self.diagnostic
    }
}

impl fmt::Display for StdlibError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.diagnostic.message())?;
        for note in self.diagnostic.notes() {
            write!(formatter, "\n  {note}")?;
        }
        Ok(())
    }
}

impl std::error::Error for StdlibError {}

/// Loads the standard library embedded in this toolchain.
///
/// The embedded bytes are fixed for the life of the process, so they are
/// checked once and every later call returns that result.
///
/// # Errors
///
/// The embedded library is defective; see [`load_stdlib_from`].
pub fn load_stdlib() -> Result<Stdlib, StdlibError> {
    static LOADED: OnceLock<Result<Stdlib, StdlibError>> = OnceLock::new();
    LOADED
        .get_or_init(|| load_stdlib_from(&StdlibInputs::embedded()))
        .clone()
}

/// Loads the standard library from `inputs`, without reading anything else.
///
/// # Errors
///
/// Any fault of the inputs, reported as `@runtime.invalidCheckedProgram`
/// with every fault in its notes: a manifest that does not decode, modules or
/// symbols out of order, a module without an entry or an entry without a
/// module, a digest that differs, a module the reader reports, an external
/// declaration its registry rejects, a symbol the manifest does not list, a
/// role claimed by no declaration, by several, or by one of the wrong shape,
/// a core type of the wrong shape, an `Intrinsic` outside `@std.builtin`, and
/// a prelude name with no declaration.
pub fn load_stdlib_from(inputs: &StdlibInputs<'_>) -> Result<Stdlib, StdlibError> {
    let manifest = manifest::read(inputs.manifest).map_err(StdlibError::new)?;
    let mut faults = manifest_faults(&manifest);
    let mut admitted = Vec::new();
    for entry in &manifest.modules {
        let path = entry.path.text.as_str();
        let Some((_, bytes)) =
            inputs.modules.iter().find(|(module, _)| *module == path)
        else {
            faults.push(Fault::at(
                MANIFEST_SOURCE_ID,
                entry.path.span,
                format!("the module `{path}` is listed but not embedded"),
            ));
            continue;
        };
        let digest = identity::digest(bytes);
        if digest != entry.digest.text {
            faults.push(Fault::at(
                MANIFEST_SOURCE_ID,
                entry.digest.span,
                format!(
                    "the module `{path}` has the digest `{digest}`, not the recorded `{}`",
                    entry.digest.text
                ),
            ));
            continue;
        }
        admitted.push((entry, *bytes, source_id_of(path), digest));
    }
    for (path, _) in &inputs.modules {
        if !manifest
            .modules
            .iter()
            .any(|entry| entry.path.text == *path)
        {
            faults.push(Fault::bare(format!(
                "the embedded module `{path}` has no manifest entry"
            )));
        }
    }
    let documents = admitted
        .iter()
        .map(|(_, bytes, source_id, _)| {
            parse_source_with(Some(source_id), bytes, standard_library())
        })
        .collect::<Vec<_>>();
    let mut parsed = Vec::new();
    for ((entry, _, source_id, _), document) in admitted.iter().zip(&documents) {
        for diagnostic in document.diagnostics() {
            faults.push(Fault::of_diagnostic(source_id, diagnostic));
        }
        match module(document) {
            Some(view) if document.diagnostics().is_empty() => parsed.push(Parsed {
                path: &entry.path.text,
                source_id,
                module: view,
            }),
            Some(_) => {}
            None => faults.push(Fault::bare(format!(
                "the module `{}` has no source tree",
                entry.path.text
            ))),
        }
    }
    if !faults.is_empty() {
        return Err(StdlibError::new(faults));
    }
    let package = (
        manifest.package_name.as_str(),
        manifest.package_version.as_str(),
    );
    let (prelude, prelude_faults) = bind_prelude(&parsed, package);
    faults.extend(prelude_faults);
    let views = parsed
        .iter()
        .map(|parsed| (parsed.path, parsed.module))
        .collect::<Vec<_>>();
    let names = LibraryNames::new(
        &views,
        prelude
            .iter()
            .map(|(name, declaration)| {
                (
                    name.clone(),
                    (declaration.module.clone(), declaration.name.clone()),
                )
            })
            .collect(),
    );
    faults.extend(builtin_faults(&parsed, &names));
    faults.extend(binding_faults(&parsed, &names, &manifest));
    let (claims, role_faults) = check_roles(&parsed, &names);
    faults.extend(role_faults);
    faults.extend(language_role_faults(&manifest, &claims));
    let (core, core_faults) = check_core_types(&parsed);
    faults.extend(core_faults);
    if !faults.is_empty() {
        return Err(StdlibError::new(faults));
    }
    let declaration = |claim: &Claim| {
        StdlibDeclaration::new(package, &claim.module, &claim.name, claim.kind)
    };
    let roles = claims
        .iter()
        .map(|(role, claim)| (*role, declaration(claim)))
        .collect::<BTreeMap<_, _>>();
    let library = roles
        .iter()
        .map(|(role, declaration)| (role.library(), declaration.clone()))
        .chain(
            core.iter()
                .map(|(library, claim)| (*library, declaration(claim))),
        )
        .collect::<BTreeMap<_, _>>();
    let mut library_types = LibraryTypes::new();
    for (bound, declaration) in &library {
        match declaration.type_id() {
            Ok(id) => library_types = library_types.with(*bound, id),
            Err(error) => faults.push(Fault::bare(format!(
                "`{}` has no identity in the checked IR: {error}",
                declaration.path()
            ))),
        }
    }
    if !faults.is_empty() {
        return Err(StdlibError::new(faults));
    }
    let modules = admitted
        .iter()
        .map(|(entry, bytes, source_id, digest)| StdlibModule {
            path: entry.path.text.clone(),
            source_id: source_id.clone(),
            digest: digest.clone(),
            bytes: bytes.to_vec(),
            roles: Role::ALL
                .into_iter()
                .filter(|role| {
                    claims
                        .get(role)
                        .is_some_and(|claim| claim.module == entry.path.text)
                })
                .collect(),
        })
        .collect();
    let texts = |items: &[manifest::Spelled]| {
        items
            .iter()
            .map(|item| item.text.clone())
            .collect::<Vec<_>>()
    };
    Ok(Stdlib {
        package_name: manifest.package_name.clone(),
        package_version: manifest.package_version.clone(),
        modules,
        roles,
        library,
        library_types,
        prelude,
        compiler: texts(&manifest.compiler),
        host: texts(&manifest.host),
        native: texts(&manifest.native),
        names,
    })
}

/// The faults of the manifest on its own: its lists out of order, and
/// symbols no registry or native implementation has.
fn manifest_faults(manifest: &Manifest) -> Vec<Fault> {
    let paths = manifest
        .modules
        .iter()
        .map(|entry| entry.path.clone())
        .collect::<Vec<_>>();
    let mut faults = order_faults("the modules' paths", &paths);
    faults.extend(order_faults("the compiler symbols", &manifest.compiler));
    faults.extend(order_faults("the host symbols", &manifest.host));
    faults.extend(order_faults("the native symbols", &manifest.native));
    for symbol in &manifest.compiler {
        if CompilerIntrinsic::from_symbol(&symbol.text).is_none() {
            faults.push(Fault::at(
                MANIFEST_SOURCE_ID,
                symbol.span,
                format!("`{}` is not a row of the `@compiler` registry", symbol.text),
            ));
        }
    }
    for symbol in &manifest.host {
        if host_row(&symbol.text).is_none() {
            faults.push(Fault::at(
                MANIFEST_SOURCE_ID,
                symbol.span,
                format!("`{}` is not a row of the host registry", symbol.text),
            ));
        }
    }
    for symbol in &manifest.native {
        faults.push(Fault::at(
            MANIFEST_SOURCE_ID,
            symbol.span,
            format!(
                "`{}` names no native implementation: the toolchain implements none",
                symbol.text
            ),
        ));
    }
    for entry in &manifest.modules {
        faults.extend(order_faults("the language roles of a module", &entry.roles));
    }
    faults
}

/// The faults of the builtin types: an `Intrinsic` body outside
/// `@std.builtin`, and a builtin prelude name that is not the builtin type of
/// its wire tag.
fn builtin_faults(parsed: &[Parsed<'_>], names: &LibraryNames) -> Vec<Fault> {
    let mut faults = Vec::new();
    for module in parsed {
        if module.path == BUILTIN_MODULE {
            continue;
        }
        for declaration in module.module.declarations() {
            if let Declaration::Type(decl) = declaration
                && let Some(TypeBody::Intrinsic(tag)) = decl.body()
            {
                faults.push(Fault::at(
                    module.source_id,
                    tag.span(),
                    format!("only `{BUILTIN_MODULE}` may write `Intrinsic`"),
                ));
            }
        }
    }
    for name in BUILTIN_PRELUDE {
        let tag = format!("@{}", name.to_ascii_lowercase());
        let expected = intrinsic(&tag);
        let found = names.declaration(BUILTIN_MODULE, name);
        if expected.is_none() || found != expected.as_ref() {
            faults.push(Fault::bare(format!(
                "`{name}` is not a type of `{BUILTIN_MODULE}` whose body is `Intrinsic({tag})`"
            )));
        }
    }
    faults
}

/// The faults of the external declarations of every module, and of the
/// symbols they bind against the manifest's lists: a bound symbol the
/// manifest does not list, and any `native` section.
fn binding_faults(
    parsed: &[Parsed<'_>],
    names: &LibraryNames,
    manifest: &Manifest,
) -> Vec<Fault> {
    let listed = |items: &[manifest::Spelled]| {
        items
            .iter()
            .map(|item| item.text.clone())
            .collect::<BTreeSet<_>>()
    };
    let compiler = listed(&manifest.compiler);
    let host = listed(&manifest.host);
    let mut faults = Vec::new();
    for module in parsed {
        for diagnostic in check_externals(&module.module, module.path, names) {
            faults.push(Fault::of_diagnostic(module.source_id, &diagnostic));
        }
        for callable in callables(&module.module) {
            if let Some(native) = callable.native() {
                faults.push(Fault::at(
                    module.source_id,
                    native.node().span(),
                    "a `native` section names no native implementation: the toolchain implements none",
                ));
            }
            let (Some(provider), Some(symbol)) = (
                callable.external().and_then(|section| section.tag()),
                callable.symbol().and_then(|section| section.value()),
            ) else {
                continue;
            };
            let (list, what) = match provider.text() {
                "@compiler" => (&compiler, "compiler"),
                "@host" => (&host, "host"),
                _ => continue,
            };
            if !list.contains(&symbol) {
                faults.push(Fault::at(
                    module.source_id,
                    callable.construct().span(),
                    format!(
                        "the manifest does not list `{symbol}` among its {what} symbols"
                    ),
                ));
            }
        }
    }
    faults
}

/// Every named callable of `module`: top-level, members of types and
/// interfaces, and members of `impl` blocks.
fn callables<'a>(module: &Module<'a>) -> Vec<vibra_syntax::source::Callable<'a>> {
    let mut all = Vec::new();
    for declaration in module.declarations() {
        match declaration {
            Declaration::Fn(callable) | Declaration::Proc(callable) => {
                all.push(callable)
            }
            Declaration::Type(decl) => {
                all.extend(decl.members());
                for block in decl.impls() {
                    all.extend(block.members());
                }
            }
            Declaration::Interface(decl) => {
                all.extend(decl.members());
                for block in decl.impls() {
                    all.extend(block.members());
                }
            }
            Declaration::Use(_) | Declaration::Let(_) | Declaration::Test(_) => {}
        }
    }
    all
}

/// The faults of the manifest's `languageRoles`: each module's list names
/// exactly the roles its declarations claim.
fn language_role_faults(
    manifest: &Manifest,
    claims: &BTreeMap<Role, Claim>,
) -> Vec<Fault> {
    let mut faults = Vec::new();
    for entry in &manifest.modules {
        let listed = entry
            .roles
            .iter()
            .filter_map(|role| Role::from_atom(&role.text))
            .collect::<BTreeSet<_>>();
        let claimed = claims
            .iter()
            .filter(|(_, claim)| claim.module == entry.path.text)
            .map(|(role, _)| *role)
            .collect::<BTreeSet<_>>();
        for role in listed.symmetric_difference(&claimed) {
            faults.push(Fault::at(
                MANIFEST_SOURCE_ID,
                entry.path.span,
                format!(
                    "{}: the manifest's language roles of `{}` differ from the roles its declarations claim",
                    role.atom(),
                    entry.path.text
                ),
            ));
        }
    }
    faults
}

/// The declaration each prelude name resolves to: the one top-level
/// declaration of that name in the standard library. `Any` and `Self` are
/// predeclared.
fn bind_prelude(
    parsed: &[Parsed<'_>],
    package: (&str, &str),
) -> (BTreeMap<String, StdlibDeclaration>, Vec<Fault>) {
    let mut found: BTreeMap<&str, Vec<(&str, &'static str)>> = BTreeMap::new();
    for module in parsed {
        for declaration in module.module.declarations() {
            let (name, kind) = match &declaration {
                Declaration::Type(decl) => (decl.name(), "@type"),
                Declaration::Interface(decl) => (decl.name(), "@interface"),
                Declaration::Let(value) => (value.name(), "@value"),
                _ => continue,
            };
            let Some(name) = name else { continue };
            if PRELUDE.contains(&name.text()) {
                found
                    .entry(name.text())
                    .or_default()
                    .push((module.path, kind));
            }
        }
    }
    let mut prelude = BTreeMap::new();
    let mut faults = Vec::new();
    for name in PRELUDE {
        if PREDECLARED.contains(&name) || prelude.contains_key(name) {
            continue;
        }
        match found.get(name).map(Vec::as_slice) {
            Some([(module, kind)]) => {
                prelude.insert(
                    name.to_owned(),
                    StdlibDeclaration::new(package, module, name, kind),
                );
            }
            Some(several) if several.len() > 1 => faults.push(Fault::bare(format!(
                "the prelude name `{name}` is declared {} times",
                several.len()
            ))),
            _ => faults.push(Fault::bare(format!(
                "the prelude name `{name}` is declared nowhere in the standard library"
            ))),
        }
    }
    (prelude, faults)
}

#[cfg(test)]
mod tests {
    use super::source_id_of;

    #[test]
    fn a_module_path_names_its_file() {
        assert_eq!(source_id_of("@std.bool"), "stdlib/src/std/bool.vib");
        assert_eq!(source_id_of("@std.a.b"), "stdlib/src/std/a/b.vib");
    }
}

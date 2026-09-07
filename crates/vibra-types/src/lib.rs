//! Primitive type checking and lowering for the M2 Step 7 profile.
//!
//! The checker consumes the syntax AST and returns an immutable checked IR.
//! It admits primitive module values, direct local bindings, conditionals,
//! sequences, function values, closures, and fixed/labelled calls. A parsed
//! AST that contains a later-step form never crosses into `vibra-ir` or
//! `vibra-interp`.

#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used
    )
)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::Path;

use base64::Engine;
use ring::signature;
use sha2::{Digest, Sha256};
use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::{
    CheckedFunction, CheckedGlobal, CheckedProgram, Expr, FunctionSignature,
    LabelledParameter as IrLabelledParameter, PrimitiveType, SourceOrigin, Value,
    external::CompilerIntrinsic,
};
use vibra_syntax::{
    ApplicationBinding, Attribute, BindingFacts, Declaration, Expression,
    ExpressionKind, FloatSuffix, IntegerSuffix, Literal, NameKind, PatternKind,
    SourceAst, TypeExpr,
};

/// The result of checking one source document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckResult {
    program: Option<CheckedProgram>,
    diagnostics: Vec<Diagnostic>,
    bindings: Vec<ApplicationBinding>,
}

impl CheckResult {
    /// Creates a semantic result.  Callers normally use [`check_source`].
    #[must_use]
    pub fn new(program: Option<CheckedProgram>, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            program,
            diagnostics,
            bindings: Vec::new(),
        }
    }

    fn with_bindings(
        program: Option<CheckedProgram>,
        diagnostics: Vec<Diagnostic>,
        bindings: Vec<ApplicationBinding>,
    ) -> Self {
        Self {
            program,
            diagnostics,
            bindings,
        }
    }

    /// The executable checked program, when the source contains a function.
    ///
    /// A constant-only module can be accepted with no executable entry point,
    /// so callers must use [`Self::accepted`] separately from this accessor.
    #[must_use]
    pub const fn program(&self) -> Option<&CheckedProgram> {
        self.program.as_ref()
    }

    /// Diagnostics in deterministic source order.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Authoritative call binding facts for accepted applications.
    ///
    /// The facts are intentionally returned by the checker rather than
    /// inferred by the syntax or formatting crates.  A formatter may use
    /// these entries to normalize labelled operand order only after the
    /// corresponding call has passed semantic checking.
    #[must_use]
    pub fn application_bindings(&self) -> &[ApplicationBinding] {
        &self.bindings
    }

    /// Whether checking accepted this source document.
    #[must_use]
    pub fn accepted(&self) -> bool {
        self.diagnostics
            .iter()
            .all(|diagnostic| diagnostic.level() != vibra_diagnostics::Level::Error)
    }
}

/// Checks one `.vib` source document through the shared reader and lowers the
/// Step 7 subset to controlled IR.
pub fn check_source(source_id: impl AsRef<str>, source: &str) -> CheckResult {
    let source_id = source_id.as_ref();
    let document = match vibra_syntax::parse_source(Path::new(source_id), source) {
        Ok(document) => document,
        Err(error) => {
            let diagnostic = Diagnostic::new(
                DiagnosticCode::ModuleIoError,
                ByteSpan::empty_at(0),
                error.to_string(),
            )
            .with_source_id(source_id);
            return CheckResult::new(None, vec![diagnostic]);
        }
    };

    let mut diagnostics = document
        .diagnostics()
        .iter()
        .cloned()
        .map(|diagnostic| diagnostic.with_source_id(source_id))
        .collect::<Vec<_>>();
    if !document.accepted() || document.recovered() {
        return CheckResult::new(None, diagnostics);
    }
    let Some(ast) = document.ast() else {
        if source.trim().is_empty() {
            unavailable(
                &mut diagnostics,
                source_id,
                ByteSpan::empty_at(0),
                "source contains no Step 7 module value or executable function",
            );
        }
        return CheckResult::new(None, diagnostics);
    };
    let (program, bindings) = check_ast_with_bindings(source_id, ast, &mut diagnostics);
    if !diagnostics
        .iter()
        .all(|diagnostic| diagnostic.level() != vibra_diagnostics::Level::Error)
    {
        return CheckResult::new(None, diagnostics);
    }
    CheckResult::with_bindings(program, diagnostics, bindings)
}

/// Checks an already decoded source AST.  This is useful to workspace
/// adapters that have already performed the reader phase.
pub fn check_ast(
    source_id: impl AsRef<str>,
    ast: &SourceAst,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<CheckedProgram> {
    check_ast_with_bindings(source_id, ast, diagnostics).0
}

/// Checks an already decoded source AST and returns semantic binding facts
/// alongside the checked program.
pub fn check_ast_with_bindings(
    source_id: impl AsRef<str>,
    ast: &SourceAst,
    diagnostics: &mut Vec<Diagnostic>,
) -> (Option<CheckedProgram>, Vec<ApplicationBinding>) {
    check_ast_with_bindings_authority(source_id.as_ref(), ast, diagnostics, false)
}

/// Checks the exact signed M2 text bootstrap module.
///
/// This entry point is intentionally separate from [`check_source`]: a source
/// package cannot gain compiler authority merely by copying an external
/// declaration. The verifier in this crate must approve the bootstrap bytes
/// before callers pass them here.
pub fn check_bootstrap_source(
    verification: &BootstrapVerification,
    source_id: impl AsRef<str>,
    source: &str,
) -> CheckResult {
    let source_id = source_id.as_ref();
    if source_id != BOOTSTRAP_TEXT_SOURCE_ID
        || source.as_bytes() != BOOTSTRAP_TEXT_BYTES
        || verification.artifact.is_empty()
    {
        let diagnostic = Diagnostic::new(
            DiagnosticCode::ToolUnavailable,
            ByteSpan::empty_at(0),
            "compiler externals require the verified M2 bootstrap module",
        )
        .with_source_id(source_id);
        return CheckResult::new(None, vec![diagnostic]);
    }
    let document = match vibra_syntax::parse_source(Path::new(source_id), source) {
        Ok(document) => document,
        Err(error) => {
            return CheckResult::new(
                None,
                vec![
                    Diagnostic::new(
                        DiagnosticCode::ModuleIoError,
                        ByteSpan::empty_at(0),
                        error.to_string(),
                    )
                    .with_source_id(source_id),
                ],
            );
        }
    };
    let mut diagnostics = document
        .diagnostics()
        .iter()
        .cloned()
        .map(|diagnostic| diagnostic.with_source_id(source_id))
        .collect::<Vec<_>>();
    if !document.accepted() || document.recovered() {
        return CheckResult::new(None, diagnostics);
    }
    let Some(ast) = document.ast() else {
        return CheckResult::new(None, diagnostics);
    };
    let (program, bindings) =
        check_ast_with_bindings_authority(source_id, ast, &mut diagnostics, true);
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.level() == vibra_diagnostics::Level::Error)
    {
        CheckResult::new(None, diagnostics)
    } else {
        CheckResult::with_bindings(program, diagnostics, bindings)
    }
}

/// Checks one source module with the narrowly supported explicit `@std.text`
/// import.  This adapter is capability-backed by [`BootstrapVerification`]; a
/// source file cannot manufacture that authority by copying declarations.
/// Only the exact signed `@std.text` map entry is exposed, and source external
/// declarations remain forbidden.
pub fn check_bootstrap_text_import(
    verification: &BootstrapVerification,
    source_id: impl AsRef<str>,
    source: &str,
) -> CheckResult {
    let source_id = source_id.as_ref();
    if verification.artifact.is_empty()
        || validate_signed_bootstrap_map(&verification.artifact).is_err()
    {
        return bootstrap_import_unavailable(
            source_id,
            "the @std.text import requires the verified M2 bootstrap map",
        );
    }
    let document = match vibra_syntax::parse_source(Path::new(source_id), source) {
        Ok(document) => document,
        Err(error) => {
            return bootstrap_import_unavailable(source_id, error.to_string());
        }
    };
    let mut diagnostics = document
        .diagnostics()
        .iter()
        .cloned()
        .map(|diagnostic| diagnostic.with_source_id(source_id))
        .collect::<Vec<_>>();
    if !document.accepted() || document.recovered() {
        return CheckResult::new(None, diagnostics);
    }
    let Some(ast) = document.ast() else {
        return bootstrap_import_unavailable(
            source_id,
            "the @std.text adapter requires a source module",
        );
    };
    let imports = ast
        .declarations()
        .iter()
        .filter_map(|declaration| match declaration {
            Declaration::Import(import) => Some(import),
            _ => None,
        })
        .collect::<Vec<_>>();
    let Some(import) = imports.first() else {
        return bootstrap_import_unavailable(
            source_id,
            "the source must explicitly import `(import text @std.text)`",
        );
    };
    let exact_import = import.alias().kind() == NameKind::Symbol
        && import.alias().value() == "text"
        && import.target().kind() == NameKind::Atom
        && import.target().value() == "std.text";
    if imports.len() != 1 || !exact_import {
        return bootstrap_import_unavailable(
            source_id,
            "only the exact `(import text @std.text)` import is available",
        );
    }
    if ast.declarations().iter().any(|declaration| {
        matches!(
            declaration,
            Declaration::Defn(function)
                if function
                    .attributes()
                    .items()
                    .iter()
                    .any(|attribute| matches!(attribute, Attribute::External(_)))
        )
    }) {
        return bootstrap_import_unavailable(
            source_id,
            "source external declarations cannot acquire bootstrap authority",
        );
    }
    if !ast
        .declarations()
        .iter()
        .any(|declaration| matches!(declaration, Declaration::Defn(_)))
    {
        return bootstrap_import_unavailable(
            source_id,
            "the @std.text adapter requires a source function entry",
        );
    }

    let (program, bindings) =
        check_ast_with_text_import_authority(source_id, ast, &mut diagnostics);
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.level() == vibra_diagnostics::Level::Error)
    {
        CheckResult::new(None, diagnostics)
    } else {
        CheckResult::with_bindings(program, diagnostics, bindings)
    }
}

fn bootstrap_import_unavailable(
    source_id: &str,
    message: impl Into<String>,
) -> CheckResult {
    let diagnostic = Diagnostic::new(
        DiagnosticCode::ToolUnavailable,
        ByteSpan::empty_at(0),
        message,
    )
    .with_source_id(source_id);
    CheckResult::new(None, vec![diagnostic])
}

/// The canonical path of the signed M2 text bootstrap module.
pub const BOOTSTRAP_TEXT_SOURCE_ID: &str = "stdlib/m2/src/std/text.vib";
const BOOTSTRAP_TEXT_BYTES: &[u8] =
    include_bytes!("../../../stdlib/m2/src/std/text.vib");

/// The result of verifying the repository's signed M2 bootstrap input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootstrapVerification {
    artifact: Vec<u8>,
}

impl BootstrapVerification {
    /// The exact signed artifact bytes.
    #[must_use]
    pub fn artifact(&self) -> &[u8] {
        &self.artifact
    }
}

/// A failure while checking the fixed offline bootstrap provenance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootstrapVerificationError(String);

impl fmt::Display for BootstrapVerificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for BootstrapVerificationError {}

const BOOTSTRAP_ARTIFACT_SHA256: &str =
    "8dd00d7ecbe068205775cd74a0fdf54ffd32f8c0710da362ab938edee567e103";
const BOOTSTRAP_SIGNATURE_SHA256: &str =
    "f6bad514c77cf8dac2dc2309df174cb3f25425c681258db276e240a4af2a5e63";
const BOOTSTRAP_PUBLIC_KEY_SHA256: &str =
    "fe5736bd57729053562bf6617fbe0acd1d81f66e9cb930341556c4808f3b1509";
const BOOTSTRAP_TEXT_SHA256: &str =
    "c796489f44636b7856c6ce21a12a753f95e59a5204a028e1ce2697afa6a28e44";
const BOOTSTRAP_ASSERT_SHA256: &str =
    "746e2f3152caf3f80026531385d7364a8457e5310cbc599e15b7fdbf3bc65007";
const BOOTSTRAP_MANIFEST: &[u8] =
    include_bytes!("../../../stdlib/m2/bootstrap-manifest.vibon");
const BOOTSTRAP_SIGNATURE: &[u8] =
    include_bytes!("../../../stdlib/m2/bootstrap.vibon.sig");
const BOOTSTRAP_PUBLIC_KEY: &[u8] =
    include_bytes!("../../../stdlib/m2/toolchain-ed25519.pub");

/// Verifies the exact checked-in M2 artifact, manifest, key, and signature.
///
/// All paths are fixed relative paths under `root`. The manifest is itself
/// pinned to the reviewed bytes, and Ed25519 verification is performed over
/// the artifact bytes before any source declaration is admitted.
pub fn verify_bootstrap(
    root: impl AsRef<Path>,
) -> Result<BootstrapVerification, BootstrapVerificationError> {
    let root = root.as_ref();
    let manifest = read_bootstrap_file(root, "stdlib/m2/bootstrap-manifest.vibon")?;
    if manifest != BOOTSTRAP_MANIFEST {
        return Err(BootstrapVerificationError(
            "M2 bootstrap manifest bytes do not match the reviewed manifest".to_owned(),
        ));
    }
    let artifact = read_bootstrap_file(root, "stdlib/m2/bootstrap.vibon")?;
    let signature_bytes = read_bootstrap_file(root, "stdlib/m2/bootstrap.vibon.sig")?;
    let public_key = read_bootstrap_file(root, "stdlib/m2/toolchain-ed25519.pub")?;
    check_digest("artifact", &artifact, BOOTSTRAP_ARTIFACT_SHA256)?;
    check_digest("signature", &signature_bytes, BOOTSTRAP_SIGNATURE_SHA256)?;
    check_digest("public key", &public_key, BOOTSTRAP_PUBLIC_KEY_SHA256)?;
    let text_module = read_bootstrap_file(root, BOOTSTRAP_TEXT_SOURCE_ID)?;
    let assert_module = read_bootstrap_file(root, "stdlib/m2/src/std/assert.vib")?;
    check_digest("text module", &text_module, BOOTSTRAP_TEXT_SHA256)?;
    check_digest("assertion module", &assert_module, BOOTSTRAP_ASSERT_SHA256)?;
    validate_signed_bootstrap_map(&artifact)?;
    if signature_bytes != BOOTSTRAP_SIGNATURE || public_key != BOOTSTRAP_PUBLIC_KEY {
        return Err(BootstrapVerificationError(
            "M2 bootstrap trust inputs do not match the reviewed bytes".to_owned(),
        ));
    }
    let signature_text = std::str::from_utf8(&signature_bytes).map_err(|_| {
        BootstrapVerificationError("bootstrap signature is not UTF-8".to_owned())
    })?;
    let signature = base64::engine::general_purpose::STANDARD
        .decode(signature_text.trim())
        .map_err(|_| {
            BootstrapVerificationError("bootstrap signature is not base64".to_owned())
        })?;
    let key_text = std::str::from_utf8(&public_key).map_err(|_| {
        BootstrapVerificationError("bootstrap public key is not UTF-8".to_owned())
    })?;
    let key_text = key_text
        .lines()
        .filter(|line| !line.starts_with("---"))
        .collect::<String>();
    let key = base64::engine::general_purpose::STANDARD
        .decode(key_text)
        .map_err(|_| {
            BootstrapVerificationError("bootstrap public key is not base64".to_owned())
        })?;
    let key = key.get(key.len().saturating_sub(32)..).ok_or_else(|| {
        BootstrapVerificationError(
            "bootstrap public key has no Ed25519 key bytes".to_owned(),
        )
    })?;
    let verifier = signature::UnparsedPublicKey::new(&signature::ED25519, key);
    verifier.verify(&artifact, &signature).map_err(|_| {
        BootstrapVerificationError(
            "M2 bootstrap Ed25519 signature is invalid".to_owned(),
        )
    })?;
    Ok(BootstrapVerification { artifact })
}

fn validate_signed_bootstrap_map(
    artifact: &[u8],
) -> Result<(), BootstrapVerificationError> {
    let text = std::str::from_utf8(artifact).map_err(|_| {
        BootstrapVerificationError("bootstrap artifact is not UTF-8".to_owned())
    })?;
    for fragment in [
        "@std.text",
        "stdlib/m2/src/std/text.vib",
        "sha256:c796489f44636b7856c6ce21a12a753f95e59a5204a028e1ce2697afa6a28e44",
        "@std.assert",
        "stdlib/m2/src/std/assert.vib",
        "sha256:746e2f3152caf3f80026531385d7364a8457e5310cbc599e15b7fdbf3bc65007",
        "text.concat",
        "text.length",
        "assert.equal-u64",
    ] {
        if !text.contains(fragment) {
            return Err(BootstrapVerificationError(format!(
                "signed bootstrap import map is missing `{fragment}`"
            )));
        }
    }
    Ok(())
}

fn read_bootstrap_file(
    root: &Path,
    relative: &str,
) -> Result<Vec<u8>, BootstrapVerificationError> {
    if path_contains_link(root) {
        return Err(BootstrapVerificationError(
            "bootstrap root contains a symlink or junction".to_owned(),
        ));
    }
    let root = fs::canonicalize(root).map_err(|error| {
        BootstrapVerificationError(format!("cannot resolve bootstrap root: {error}"))
    })?;
    let candidate = root.join(relative);
    if path_contains_link(&candidate) {
        return Err(BootstrapVerificationError(format!(
            "bootstrap path `{relative}` contains a symlink or junction"
        )));
    }
    let resolved = fs::canonicalize(&candidate).map_err(|error| {
        BootstrapVerificationError(format!(
            "cannot read bootstrap `{relative}`: {error}"
        ))
    })?;
    if !resolved.starts_with(&root) {
        return Err(BootstrapVerificationError(format!(
            "bootstrap path `{relative}` escapes its root"
        )));
    }
    fs::read(resolved).map_err(|error| {
        BootstrapVerificationError(format!(
            "cannot read bootstrap `{relative}`: {error}"
        ))
    })
}

fn path_contains_link(path: &Path) -> bool {
    let mut current = Path::new("").to_path_buf();
    for component in path.components() {
        current.push(component.as_os_str());
        let Ok(metadata) = fs::symlink_metadata(&current) else {
            continue;
        };
        if metadata.file_type().is_symlink() || is_windows_reparse_point(&metadata) {
            return true;
        }
    }
    false
}

#[cfg(windows)]
fn is_windows_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
const fn is_windows_reparse_point(_metadata: &fs::Metadata) -> bool {
    false
}

fn check_digest(
    label: &str,
    bytes: &[u8],
    expected: &str,
) -> Result<(), BootstrapVerificationError> {
    let actual = format!("{:x}", Sha256::digest(bytes));
    if actual != expected {
        return Err(BootstrapVerificationError(format!(
            "M2 bootstrap {label} digest mismatch: expected sha256:{expected}, got sha256:{actual}"
        )));
    }
    Ok(())
}

fn check_ast_with_bindings_authority(
    source_id: &str,
    ast: &SourceAst,
    diagnostics: &mut Vec<Diagnostic>,
    trusted_bootstrap: bool,
) -> (Option<CheckedProgram>, Vec<ApplicationBinding>) {
    let mut checker = Checker::new(source_id, diagnostics, ast, trusted_bootstrap);
    checker.collect_headers();
    checker.check_initializer_cycles();
    checker.check_function_cycles();
    checker.check_globals();
    checker.check_functions();
    let program = checker.finish();
    (program, checker.bindings)
}

fn check_ast_with_text_import_authority(
    source_id: &str,
    ast: &SourceAst,
    diagnostics: &mut Vec<Diagnostic>,
) -> (Option<CheckedProgram>, Vec<ApplicationBinding>) {
    let mut checker = Checker::new(source_id, diagnostics, ast, true);
    checker.text_import_authorized = true;
    checker.collect_headers();
    checker.check_initializer_cycles();
    checker.check_function_cycles();
    checker.check_globals();
    checker.check_functions();
    let program = checker.finish();
    (program, checker.bindings)
}

#[derive(Clone)]
struct GlobalHeader {
    name: String,
    value_type: PrimitiveType,
    expression: Expression,
    span: ByteSpan,
    function_index: Option<usize>,
    function_targets: FunctionTargetSet,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct FunctionTargetSet {
    known: BTreeSet<usize>,
    unknown: bool,
    has_closure: bool,
}

impl FunctionTargetSet {
    fn known(function: usize) -> Self {
        Self {
            known: BTreeSet::from([function]),
            unknown: false,
            has_closure: false,
        }
    }

    fn unknown() -> Self {
        Self {
            known: BTreeSet::new(),
            unknown: true,
            has_closure: false,
        }
    }

    fn union(&mut self, other: &Self) {
        self.known.extend(&other.known);
        self.unknown |= other.unknown;
        self.has_closure |= other.has_closure;
    }

    fn closure() -> Self {
        Self {
            known: BTreeSet::new(),
            unknown: false,
            has_closure: true,
        }
    }

    fn singleton_function(&self) -> Option<usize> {
        (!self.unknown && !self.has_closure && self.known.len() == 1)
            .then(|| self.known.iter().next().copied())
            .flatten()
    }
}

#[derive(Clone)]
struct FunctionHeader {
    declaration_index: usize,
    name: String,
    signature: FunctionSignature,
    external: Option<CompilerIntrinsic>,
    external_declared: bool,
}

const IMPORTED_FUNCTION_DECLARATION: usize = usize::MAX;

#[derive(Clone)]
struct LocalBinding {
    slot: usize,
    value_type: PrimitiveType,
    span: ByteSpan,
    function_targets: Option<FunctionTargetSet>,
}

struct Checker<'a> {
    source_id: &'a str,
    diagnostics: &'a mut Vec<Diagnostic>,
    ast: &'a SourceAst,
    globals: Vec<GlobalHeader>,
    global_indices: BTreeMap<String, usize>,
    functions: Vec<FunctionHeader>,
    function_indices: BTreeMap<String, usize>,
    module_names: BTreeMap<String, ByteSpan>,
    checked_globals: Vec<Option<CheckedGlobal>>,
    checked_functions: Vec<Option<CheckedFunction>>,
    bindings: Vec<ApplicationBinding>,
    trusted_bootstrap: bool,
    text_import_authorized: bool,
    text_import_span: Option<ByteSpan>,
    recursive_groups: Vec<Vec<usize>>,
}

impl<'a> Checker<'a> {
    fn new(
        source_id: &'a str,
        diagnostics: &'a mut Vec<Diagnostic>,
        ast: &'a SourceAst,
        trusted_bootstrap: bool,
    ) -> Self {
        Self {
            source_id,
            diagnostics,
            ast,
            globals: Vec::new(),
            global_indices: BTreeMap::new(),
            functions: Vec::new(),
            function_indices: BTreeMap::new(),
            module_names: BTreeMap::new(),
            checked_globals: Vec::new(),
            checked_functions: Vec::new(),
            bindings: Vec::new(),
            trusted_bootstrap,
            text_import_authorized: false,
            text_import_span: None,
            recursive_groups: Vec::new(),
        }
    }

    fn collect_headers(&mut self) {
        for (declaration_index, declaration) in
            self.ast.declarations().iter().enumerate()
        {
            match declaration {
                Declaration::Def(definition) => {
                    let Some(value_type) = primitive_type(definition.value_type())
                    else {
                        unavailable(
                            self.diagnostics,
                            self.source_id,
                            definition.span(),
                            "only monomorphic module values are available in Step 7",
                        );
                        continue;
                    };
                    let name = definition.name().value().to_owned();
                    if let Some(earlier) = self.module_names.get(&name).copied() {
                        redeclaration(
                            self.diagnostics,
                            self.source_id,
                            definition.name().value(),
                            definition.name().value(),
                            definition.span(),
                            earlier,
                        );
                        continue;
                    }
                    self.module_names.insert(name.clone(), definition.span());
                    let index = self.globals.len();
                    self.global_indices.insert(name.clone(), index);
                    self.globals.push(GlobalHeader {
                        name,
                        value_type,
                        expression: definition.expression().clone(),
                        span: definition.span(),
                        function_index: None,
                        function_targets: FunctionTargetSet::default(),
                    });
                }
                Declaration::Defn(function) => {
                    let Some(signature) =
                        check_signature(self.source_id, function, self.diagnostics)
                    else {
                        continue;
                    };
                    let name = function.name().value().to_owned();
                    if let Some(earlier) = self.module_names.get(&name).copied() {
                        redeclaration(
                            self.diagnostics,
                            self.source_id,
                            &name,
                            &name,
                            function.span(),
                            earlier,
                        );
                        continue;
                    }
                    self.module_names.insert(name.clone(), function.span());
                    let index = self.functions.len();
                    self.function_indices.insert(name.clone(), index);
                    self.functions.push(FunctionHeader {
                        declaration_index,
                        name,
                        signature,
                        external: compiler_intrinsic(
                            self.source_id,
                            function,
                            self.diagnostics,
                            self.trusted_bootstrap,
                        ),
                        external_declared: function.attributes().items().iter().any(
                            |attribute| matches!(attribute, Attribute::External(_)),
                        ),
                    });
                }
                Declaration::Import(import)
                    if self.text_import_authorized
                        && import.alias().kind() == NameKind::Symbol
                        && import.alias().value() == "text"
                        && import.target().kind() == NameKind::Atom
                        && import.target().value() == "std.text" =>
                {
                    if let Some(earlier) = self.module_names.get("text").copied() {
                        redeclaration(
                            self.diagnostics,
                            self.source_id,
                            "text",
                            "text",
                            import.span(),
                            earlier,
                        );
                    } else {
                        self.module_names.insert("text".to_owned(), import.span());
                    }
                    self.text_import_span = Some(import.span());
                }
                Declaration::Import(import) => unavailable(
                    self.diagnostics,
                    self.source_id,
                    import.span(),
                    "imports require the resolved multi-module checker",
                ),
                _ => unavailable(
                    self.diagnostics,
                    self.source_id,
                    declaration.span(),
                    "this declaration family is outside the Step 7 monomorphic profile",
                ),
            }
        }
        if self.text_import_authorized {
            let import_span = self.text_import_span.unwrap_or_else(|| self.ast.span());
            for (name, intrinsic) in [
                ("text.concat", CompilerIntrinsic::TextConcat),
                ("text.length", CompilerIntrinsic::TextLength),
            ] {
                let index = self.functions.len();
                self.function_indices.insert(name.to_owned(), index);
                self.functions.push(FunctionHeader {
                    declaration_index: IMPORTED_FUNCTION_DECLARATION,
                    name: name.to_owned(),
                    signature: intrinsic.signature(),
                    external: Some(intrinsic),
                    external_declared: true,
                });
            }
            self.text_import_span = Some(import_span);
        }
        for _ in 0..=self.globals.len() {
            let global_function_targets = self
                .globals
                .iter()
                .map(|global| {
                    syntax_function_targets(
                        &global.expression,
                        &self.global_indices,
                        &self.function_indices,
                        &self.globals,
                        &BTreeMap::new(),
                    )
                })
                .collect::<Vec<_>>();
            let changed = self
                .globals
                .iter()
                .zip(&global_function_targets)
                .any(|(global, targets)| global.function_targets != *targets);
            for (global, function_targets) in
                self.globals.iter_mut().zip(global_function_targets)
            {
                global.function_index = function_targets.singleton_function();
                global.function_targets = function_targets;
            }
            if !changed {
                break;
            }
        }
        self.checked_globals = vec![None; self.globals.len()];
        self.checked_functions = vec![None; self.functions.len()];
    }

    fn check_initializer_cycles(&mut self) {
        let mut states = vec![VisitState::Unvisited; self.globals.len()];
        for index in 0..self.globals.len() {
            self.visit_global(index, &mut states);
        }
    }

    fn check_function_cycles(&mut self) {
        let mut dependencies = vec![BTreeSet::new(); self.functions.len()];
        for (index, header) in self.functions.iter().enumerate() {
            let Some(Declaration::Defn(function)) =
                self.ast.declarations().get(header.declaration_index)
            else {
                continue;
            };
            let Some(function_dependencies) = dependencies.get_mut(index) else {
                continue;
            };
            for expression in function.expressions() {
                collect_function_dependencies(
                    expression,
                    &self.function_indices,
                    function_dependencies,
                );
                collect_function_alias_dependencies(
                    expression,
                    &self.global_indices,
                    &self.function_indices,
                    &self.globals,
                    function_dependencies,
                );
            }
        }
        let module_definitions = self
            .functions
            .iter()
            .map(|function| function.declaration_index != IMPORTED_FUNCTION_DECLARATION)
            .collect::<Vec<_>>();
        self.recursive_groups =
            find_recursive_groups(&dependencies, &module_definitions);
    }

    fn visit_global(&mut self, index: usize, states: &mut [VisitState]) {
        match states.get(index).copied() {
            Some(VisitState::Done) => return,
            Some(VisitState::Visiting) => {
                let Some(header) = self.globals.get(index) else {
                    return;
                };
                self.diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::TypeInitializerCycle,
                        header.span,
                        "module value initializers form a cycle",
                    )
                    .with_source_id(self.source_id),
                );
                return;
            }
            Some(VisitState::Unvisited) => {}
            None => return,
        }
        let Some(state) = states.get_mut(index) else {
            return;
        };
        *state = VisitState::Visiting;
        let mut dependencies = BTreeSet::new();
        let Some(expression) = self.globals.get(index).map(|header| &header.expression)
        else {
            return;
        };
        collect_global_dependencies(
            expression,
            &self.global_indices,
            &self.function_indices,
            &self.functions,
            self.ast,
            &mut dependencies,
        );
        collect_global_alias_dependencies(
            expression,
            &self.global_indices,
            &self.function_indices,
            &self.globals,
            &self.functions,
            self.ast,
            &mut dependencies,
        );
        for dependency in dependencies {
            self.visit_global(dependency, states);
        }
        if let Some(state) = states.get_mut(index) {
            *state = VisitState::Done;
        }
    }

    fn check_globals(&mut self) {
        for index in 0..self.globals.len() {
            let Some(header) = self.globals.get(index).cloned() else {
                continue;
            };
            let mut environment = CheckEnvironment::new(
                self.source_id,
                self.diagnostics,
                &self.global_indices,
                &self.globals,
                &self.functions,
                &self.function_indices,
                &self.module_names,
                &mut self.bindings,
                None,
                None,
            );
            let Some(expression) = check_expression(
                &mut environment,
                &header.expression,
                Some(header.value_type.clone()),
            ) else {
                continue;
            };
            let origin = SourceOrigin::new(self.source_id, header.span);
            match CheckedGlobal::new(
                header.name,
                header.value_type.clone(),
                expression,
                origin,
            ) {
                Ok(global) => {
                    if let Some(slot) = self.checked_globals.get_mut(index) {
                        *slot = Some(global);
                    }
                }
                Err(error) => unavailable(
                    self.diagnostics,
                    self.source_id,
                    header.span,
                    format!("checked IR construction failed: {error}"),
                ),
            }
        }
    }

    fn check_functions(&mut self) {
        for (index, header) in self.functions.clone().into_iter().enumerate() {
            if header.declaration_index == IMPORTED_FUNCTION_DECLARATION {
                let Some(intrinsic) = header.external else {
                    continue;
                };
                let origin = SourceOrigin::new(
                    self.source_id,
                    self.text_import_span.unwrap_or_else(|| self.ast.span()),
                );
                if let Ok(function) = CheckedFunction::new_external(
                    header.name,
                    header.signature,
                    intrinsic,
                    origin,
                ) && let Some(slot) = self.checked_functions.get_mut(index)
                {
                    *slot = Some(function);
                }
                continue;
            }
            let Some(Declaration::Defn(function)) =
                self.ast.declarations().get(header.declaration_index)
            else {
                continue;
            };
            if !header.external_declared
                && has_deferred_attributes(function.attributes().items())
            {
                unavailable(
                    self.diagnostics,
                    self.source_id,
                    function.span(),
                    "variadic, generic, external, and nonempty-effect attributes are unavailable in Step 7",
                );
                continue;
            }
            if let Some(intrinsic) = header.external {
                let origin = SourceOrigin::new(self.source_id, function.span());
                match CheckedFunction::new_external(
                    header.name,
                    header.signature,
                    intrinsic,
                    origin,
                ) {
                    Ok(function) => {
                        if let Some(slot) = self.checked_functions.get_mut(index) {
                            *slot = Some(function);
                        }
                    }
                    Err(error) => unavailable(
                        self.diagnostics,
                        self.source_id,
                        function.span(),
                        format!("checked IR construction failed: {error}"),
                    ),
                }
                continue;
            }
            if header.external_declared {
                continue;
            }
            let mut environment = CheckEnvironment::new(
                self.source_id,
                self.diagnostics,
                &self.global_indices,
                &self.globals,
                &self.functions,
                &self.function_indices,
                &self.module_names,
                &mut self.bindings,
                Some(index),
                self.recursive_groups.get(index).cloned(),
            );
            let mut parameters_valid = true;
            for (parameter_index, parameter) in function.parameters().iter().enumerate()
            {
                match parameter.parsed_pattern().kind() {
                    PatternKind::Binding(name) if name.is_discard() => {}
                    PatternKind::Binding(name) => {
                        if !environment.add_binding(
                            name.value(),
                            parameter.value_type(),
                            parameter.span(),
                        ) {
                            parameters_valid = false;
                        }
                    }
                    _ => {
                        parameters_valid = false;
                        unavailable(
                            environment.diagnostics,
                            environment.source_id,
                            parameter.span(),
                            "constructor and destructuring patterns are deferred until M3",
                        );
                    }
                }
                environment.next_slot = parameter_index.saturating_add(1);
            }
            let mut labelled_index = 0;
            for attribute in function.attributes().items() {
                let Attribute::Labelled(entries) = attribute else {
                    continue;
                };
                for entry in entries {
                    let Some(labelled) =
                        header.signature.labelled().get(labelled_index)
                    else {
                        continue;
                    };
                    labelled_index = labelled_index.saturating_add(1);
                    if !environment.add_binding_type(
                        labelled.name(),
                        labelled.value_type(),
                        entry.span(),
                    ) {
                        parameters_valid = false;
                    }
                }
            }
            if !parameters_valid {
                continue;
            }
            let Some(body) = check_sequence(
                &mut environment,
                function.expressions(),
                Some(header.signature.result()),
                function.span(),
                true,
            ) else {
                continue;
            };
            let origin = SourceOrigin::new(self.source_id, function.span());
            match CheckedFunction::with_slots(
                header.name,
                header.signature,
                body,
                origin,
                environment.next_slot,
            ) {
                Ok(function) => {
                    if let Some(slot) = self.checked_functions.get_mut(index) {
                        *slot = Some(function);
                    }
                }
                Err(error) => unavailable(
                    self.diagnostics,
                    self.source_id,
                    function.span(),
                    format!("checked IR construction failed: {error}"),
                ),
            }
        }
    }

    fn finish(&mut self) -> Option<CheckedProgram> {
        if self
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.level() == vibra_diagnostics::Level::Error)
        {
            return None;
        }
        let globals = std::mem::take(&mut self.checked_globals)
            .into_iter()
            .collect::<Option<Vec<_>>>()?;
        let functions = std::mem::take(&mut self.checked_functions)
            .into_iter()
            .collect::<Option<Vec<_>>>()?;
        if functions.is_empty() {
            if globals.is_empty() {
                unavailable(
                    self.diagnostics,
                    self.source_id,
                    self.ast.span(),
                    "source contains no Step 7 module value or executable function",
                );
            }
            return None;
        }
        match CheckedProgram::try_new_with_globals(globals, functions, 0) {
            Ok(program) => Some(program),
            Err(error) => {
                unavailable(
                    self.diagnostics,
                    self.source_id,
                    self.ast.span(),
                    format!("checked IR construction failed: {error}"),
                );
                None
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VisitState {
    Unvisited,
    Visiting,
    Done,
}

fn find_recursive_groups(
    dependencies: &[BTreeSet<usize>],
    module_definitions: &[bool],
) -> Vec<Vec<usize>> {
    (0..dependencies.len())
        .map(|start| {
            if !module_definitions.get(start).copied().unwrap_or(false) {
                return Vec::new();
            }
            reachable_functions(start, dependencies)
                .into_iter()
                .filter(|target| {
                    module_definitions.get(*target).copied().unwrap_or(false)
                })
                .collect()
        })
        .collect()
}

fn reachable_functions(
    start: usize,
    dependencies: &[BTreeSet<usize>],
) -> BTreeSet<usize> {
    let mut reached = BTreeSet::new();
    let mut pending = vec![start];
    while let Some(index) = pending.pop() {
        if !reached.insert(index) {
            continue;
        }
        if let Some(next) = dependencies.get(index) {
            pending.extend(next.iter().copied());
        }
    }
    reached
}

struct CheckEnvironment<'a> {
    source_id: &'a str,
    diagnostics: &'a mut Vec<Diagnostic>,
    global_indices: &'a BTreeMap<String, usize>,
    globals: &'a [GlobalHeader],
    functions: &'a [FunctionHeader],
    function_indices: &'a BTreeMap<String, usize>,
    module_names: &'a BTreeMap<String, ByteSpan>,
    bindings: &'a mut Vec<ApplicationBinding>,
    locals: BTreeMap<String, LocalBinding>,
    captures: BTreeMap<String, CaptureBinding>,
    capture_sources: Vec<Expr>,
    outer: Option<VisibleBindings>,
    next_slot: usize,
    current_function: Option<usize>,
    recursive_group: Option<Vec<usize>>,
}

#[derive(Clone)]
struct CaptureBinding {
    slot: usize,
    value_type: PrimitiveType,
    span: ByteSpan,
}

#[derive(Clone)]
enum VisibleStorage {
    Activation {
        slot: usize,
        value_type: PrimitiveType,
        span: ByteSpan,
    },
    Closure {
        slot: usize,
        value_type: PrimitiveType,
        span: ByteSpan,
    },
}

#[derive(Clone, Default)]
struct VisibleBindings {
    values: BTreeMap<String, VisibleStorage>,
}

impl<'a> CheckEnvironment<'a> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        source_id: &'a str,
        diagnostics: &'a mut Vec<Diagnostic>,
        global_indices: &'a BTreeMap<String, usize>,
        globals: &'a [GlobalHeader],
        functions: &'a [FunctionHeader],
        function_indices: &'a BTreeMap<String, usize>,
        module_names: &'a BTreeMap<String, ByteSpan>,
        bindings: &'a mut Vec<ApplicationBinding>,
        current_function: Option<usize>,
        recursive_group: Option<Vec<usize>>,
    ) -> Self {
        Self {
            source_id,
            diagnostics,
            global_indices,
            globals,
            functions,
            function_indices,
            module_names,
            bindings,
            locals: BTreeMap::new(),
            captures: BTreeMap::new(),
            capture_sources: Vec::new(),
            outer: None,
            next_slot: 0,
            current_function,
            recursive_group,
        }
    }

    fn add_binding(
        &mut self,
        name: &str,
        value_type: &TypeExpr,
        span: ByteSpan,
    ) -> bool {
        let Some(value_type) = primitive_type(value_type) else {
            unavailable(
                self.diagnostics,
                self.source_id,
                span,
                "only monomorphic binding types are available in Step 7",
            );
            return false;
        };
        self.add_binding_type(name, value_type, span)
    }

    fn add_binding_type(
        &mut self,
        name: &str,
        value_type: PrimitiveType,
        span: ByteSpan,
    ) -> bool {
        self.add_binding_type_with_function(name, value_type, span, None)
    }

    fn add_binding_type_with_function(
        &mut self,
        name: &str,
        value_type: PrimitiveType,
        span: ByteSpan,
        function_index: Option<usize>,
    ) -> bool {
        let function_targets = if matches!(&value_type, PrimitiveType::Function(_)) {
            Some(
                function_index
                    .map_or_else(FunctionTargetSet::unknown, FunctionTargetSet::known),
            )
        } else {
            None
        };
        self.add_binding_type_with_targets(name, value_type, span, function_targets)
    }

    fn add_binding_type_with_targets(
        &mut self,
        name: &str,
        value_type: PrimitiveType,
        span: ByteSpan,
        function_targets: Option<FunctionTargetSet>,
    ) -> bool {
        if let Some(earlier) = self.module_names.get(name).copied() {
            redeclaration(self.diagnostics, self.source_id, name, name, span, earlier);
            return false;
        }
        if let Some(earlier) = self.locals.get(name) {
            redeclaration(
                self.diagnostics,
                self.source_id,
                name,
                name,
                span,
                earlier.span,
            );
            return false;
        }
        if let Some(earlier) = self.captures.get(name) {
            redeclaration(
                self.diagnostics,
                self.source_id,
                name,
                name,
                span,
                earlier.span,
            );
            return false;
        }
        if let Some(outer) = &self.outer
            && let Some(storage) = outer.values.get(name)
        {
            let earlier = match storage {
                VisibleStorage::Activation { span, .. }
                | VisibleStorage::Closure { span, .. } => *span,
            };
            redeclaration(self.diagnostics, self.source_id, name, name, span, earlier);
            return false;
        }
        let slot = self.next_slot;
        self.locals.insert(
            name.to_owned(),
            LocalBinding {
                slot,
                value_type,
                span,
                function_targets,
            },
        );
        self.next_slot = self.next_slot.saturating_add(1);
        true
    }

    fn visible_bindings(&self) -> VisibleBindings {
        let mut values = BTreeMap::new();
        for (name, binding) in &self.locals {
            values.insert(
                name.clone(),
                VisibleStorage::Activation {
                    slot: binding.slot,
                    value_type: binding.value_type.clone(),
                    span: binding.span,
                },
            );
        }
        for (name, binding) in &self.captures {
            values.insert(
                name.clone(),
                VisibleStorage::Closure {
                    slot: binding.slot,
                    value_type: binding.value_type.clone(),
                    span: binding.span,
                },
            );
        }
        if let Some(outer) = &self.outer {
            for (name, storage) in &outer.values {
                values
                    .entry(name.clone())
                    .or_insert_with(|| storage.clone());
            }
        }
        VisibleBindings { values }
    }

    fn resolve_capture(
        &mut self,
        name: &str,
        span: ByteSpan,
    ) -> Option<CaptureBinding> {
        let storage = self.outer.as_ref()?.values.get(name)?.clone();
        if let Some(binding) = self.captures.get(name) {
            return Some(binding.clone());
        }
        let slot = self.capture_sources.len();
        let (value_type, source) = match storage {
            VisibleStorage::Activation {
                slot, value_type, ..
            } => (
                value_type.clone(),
                Expr::variable(
                    slot,
                    value_type,
                    SourceOrigin::new(self.source_id, span),
                ),
            ),
            VisibleStorage::Closure {
                slot, value_type, ..
            } => (
                value_type.clone(),
                Expr::captured(
                    slot,
                    value_type,
                    SourceOrigin::new(self.source_id, span),
                ),
            ),
        };
        self.capture_sources.push(source);
        let binding = CaptureBinding {
            slot,
            value_type,
            span,
        };
        self.captures.insert(name.to_owned(), binding.clone());
        Some(binding)
    }
}

fn types_match(left: &PrimitiveType, right: &PrimitiveType) -> bool {
    match (left, right) {
        (PrimitiveType::Function(left), PrimitiveType::Function(right)) => {
            left.same_shape(right)
        }
        _ => left == right,
    }
}

fn collect_value_names(expression: &Expression, names: &mut Vec<String>) {
    let add_name = |name: &vibra_syntax::Name, names: &mut Vec<String>| {
        if name.kind() == NameKind::Symbol
            && name.segments().len() == 1
            && !names.iter().any(|known| known == name.value())
        {
            names.push(name.value().to_owned());
        }
    };
    match expression.kind() {
        ExpressionKind::Name(name) => add_name(name, names),
        ExpressionKind::Application(application) => {
            collect_value_names(application.callee(), names);
            for argument in application.arguments() {
                collect_value_names(argument.value(), names);
            }
        }
        ExpressionKind::Do(expressions) => {
            for expression in expressions {
                collect_value_names(expression, names);
            }
        }
        ExpressionKind::Let { value, body, .. } => {
            collect_value_names(value, names);
            for expression in body {
                collect_value_names(expression, names);
            }
        }
        ExpressionKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_value_names(condition, names);
            collect_value_names(then_branch, names);
            collect_value_names(else_branch, names);
        }
        ExpressionKind::Lambda(lambda) => {
            for expression in lambda.body() {
                collect_value_names(expression, names);
            }
        }
        ExpressionKind::Match { scrutinee, arms } => {
            collect_value_names(scrutinee, names);
            for arm in arms {
                collect_value_names(arm.result(), names);
            }
        }
        ExpressionKind::As { operand, .. } | ExpressionKind::Try(operand) => {
            collect_value_names(operand, names);
        }
        ExpressionKind::Literal(_) => {}
    }
}

fn function_index_from_expr(
    expression: &Expr,
    environment: &CheckEnvironment<'_>,
) -> Option<usize> {
    let summary = function_targets_from_expr(expression, environment, &BTreeMap::new());
    (!summary.unknown && !summary.has_closure && summary.known.len() == 1)
        .then(|| summary.known.iter().next().copied())
        .flatten()
}

fn function_targets_from_expr(
    expression: &Expr,
    environment: &CheckEnvironment<'_>,
    aliases: &BTreeMap<usize, FunctionTargetSet>,
) -> FunctionTargetSet {
    match expression {
        Expr::Function { function, .. } => FunctionTargetSet::known(*function),
        Expr::Variable {
            slot, value_type, ..
        } => aliases
            .get(slot)
            .cloned()
            .or_else(|| {
                environment
                    .locals
                    .values()
                    .find(|binding| binding.slot == *slot)
                    .and_then(|binding| binding.function_targets.clone())
            })
            .unwrap_or_else(|| {
                if matches!(value_type, PrimitiveType::Function(_)) {
                    FunctionTargetSet::unknown()
                } else {
                    FunctionTargetSet::default()
                }
            }),
        Expr::Global {
            index, value_type, ..
        } => environment
            .globals
            .get(*index)
            .map(|global| global.function_targets.clone())
            .unwrap_or_else(|| {
                if matches!(value_type, PrimitiveType::Function(_)) {
                    FunctionTargetSet::unknown()
                } else {
                    FunctionTargetSet::default()
                }
            }),
        Expr::Let {
            value, body, slot, ..
        } => {
            let value_targets = function_targets_from_expr(value, environment, aliases);
            let mut nested = aliases.clone();
            if let Some(slot) = slot {
                nested.insert(*slot, value_targets);
            }
            function_targets_from_expr(body, environment, &nested)
        }
        Expr::Sequence { expressions, .. } => expressions
            .last()
            .map_or_else(FunctionTargetSet::default, |expression| {
                function_targets_from_expr(expression, environment, aliases)
            }),
        Expr::If {
            then_branch,
            else_branch,
            ..
        } => {
            let mut targets =
                function_targets_from_expr(then_branch, environment, aliases);
            targets.union(&function_targets_from_expr(
                else_branch,
                environment,
                aliases,
            ));
            targets
        }
        Expr::Closure { .. } => FunctionTargetSet::closure(),
        Expr::Captured { .. } => FunctionTargetSet::unknown(),
        // A call's result may itself be a function value.  The checker does
        // not have a recursive return-summary environment here, so preserve
        // the function-typed boundary conservatively instead of dropping it
        // to the empty set and manufacturing a singleton hint from a branch.
        Expr::Call {
            result: PrimitiveType::Function(_),
            ..
        } => FunctionTargetSet::unknown(),
        Expr::Call { .. }
        | Expr::Literal { .. }
        | Expr::External { .. }
        | Expr::Default { .. } => FunctionTargetSet::default(),
    }
}

/// Returns whether every statically known branch of a callable expression
/// proves that a labelled slot has no default.  An unknown callable keeps the
/// omitted slot in the IR so the interpreter can resolve the selected value's
/// default after evaluating the callee.
fn callable_default_is_missing(expression: &Expr, label: &str) -> bool {
    match expression {
        Expr::Function { signature, .. } | Expr::Closure { signature, .. } => signature
            .labelled()
            .iter()
            .find(|parameter| parameter.name() == label)
            .is_some_and(|parameter| parameter.default().is_none()),
        Expr::If {
            then_branch,
            else_branch,
            ..
        } => {
            callable_default_is_missing(then_branch, label)
                && callable_default_is_missing(else_branch, label)
        }
        Expr::Sequence { expressions, .. } => expressions
            .last()
            .is_some_and(|expression| callable_default_is_missing(expression, label)),
        _ => false,
    }
}

fn syntax_function_index(
    expression: &Expression,
    global_indices: &BTreeMap<String, usize>,
    function_indices: &BTreeMap<String, usize>,
    globals: &[GlobalHeader],
    aliases: &BTreeMap<String, usize>,
) -> Option<usize> {
    match expression.kind() {
        ExpressionKind::Name(name) if name.kind() == NameKind::Symbol => aliases
            .get(name.value())
            .copied()
            .or_else(|| function_indices.get(name.value()).copied())
            .or_else(|| {
                global_indices
                    .get(name.value())
                    .and_then(|index| globals.get(*index))
                    .and_then(|global| global.function_index)
            }),
        ExpressionKind::Do(expressions) => expressions.last().and_then(|expression| {
            syntax_function_index(
                expression,
                global_indices,
                function_indices,
                globals,
                aliases,
            )
        }),
        ExpressionKind::Let {
            pattern,
            value,
            body,
        } => {
            let mut scoped = aliases.clone();
            if let PatternKind::Binding(name) = pattern.kind()
                && !name.is_discard()
                && let Some(function) = syntax_function_index(
                    value,
                    global_indices,
                    function_indices,
                    globals,
                    aliases,
                )
            {
                scoped.insert(name.value().to_owned(), function);
            }
            body.last().and_then(|expression| {
                syntax_function_index(
                    expression,
                    global_indices,
                    function_indices,
                    globals,
                    &scoped,
                )
            })
        }
        ExpressionKind::If {
            then_branch,
            else_branch,
            ..
        } => {
            let then_function = syntax_function_index(
                then_branch,
                global_indices,
                function_indices,
                globals,
                aliases,
            );
            let else_function = syntax_function_index(
                else_branch,
                global_indices,
                function_indices,
                globals,
                aliases,
            );
            (then_function == else_function)
                .then_some(then_function)
                .flatten()
        }
        _ => None,
    }
}

fn syntax_function_targets(
    expression: &Expression,
    global_indices: &BTreeMap<String, usize>,
    function_indices: &BTreeMap<String, usize>,
    globals: &[GlobalHeader],
    aliases: &BTreeMap<String, FunctionTargetSet>,
) -> FunctionTargetSet {
    match expression.kind() {
        ExpressionKind::Name(name) if name.kind() == NameKind::Symbol => aliases
            .get(name.value())
            .cloned()
            .or_else(|| {
                function_indices
                    .get(name.value())
                    .copied()
                    .map(FunctionTargetSet::known)
            })
            .or_else(|| {
                global_indices
                    .get(name.value())
                    .and_then(|index| globals.get(*index))
                    .map(|global| global.function_targets.clone())
            })
            .unwrap_or_default(),
        ExpressionKind::Do(expressions) => {
            expressions
                .last()
                .map_or_else(FunctionTargetSet::default, |expression| {
                    syntax_function_targets(
                        expression,
                        global_indices,
                        function_indices,
                        globals,
                        aliases,
                    )
                })
        }
        ExpressionKind::Let {
            pattern,
            value,
            body,
        } => {
            let mut scoped = aliases.clone();
            if let PatternKind::Binding(name) = pattern.kind()
                && !name.is_discard()
            {
                let targets = syntax_function_targets(
                    value,
                    global_indices,
                    function_indices,
                    globals,
                    aliases,
                );
                if !targets.known.is_empty() || targets.unknown || targets.has_closure {
                    scoped.insert(name.value().to_owned(), targets);
                }
            }
            body.last()
                .map_or_else(FunctionTargetSet::default, |expression| {
                    syntax_function_targets(
                        expression,
                        global_indices,
                        function_indices,
                        globals,
                        &scoped,
                    )
                })
        }
        ExpressionKind::If {
            then_branch,
            else_branch,
            ..
        } => {
            let mut targets = syntax_function_targets(
                then_branch,
                global_indices,
                function_indices,
                globals,
                aliases,
            );
            targets.union(&syntax_function_targets(
                else_branch,
                global_indices,
                function_indices,
                globals,
                aliases,
            ));
            targets
        }
        ExpressionKind::Lambda(_) => FunctionTargetSet::closure(),
        ExpressionKind::Application(_) => FunctionTargetSet::unknown(),
        _ => FunctionTargetSet::default(),
    }
}

fn collect_global_dependencies(
    expression: &Expression,
    global_indices: &BTreeMap<String, usize>,
    function_indices: &BTreeMap<String, usize>,
    functions: &[FunctionHeader],
    ast: &SourceAst,
    dependencies: &mut BTreeSet<usize>,
) {
    let mut visited_functions = BTreeSet::new();
    collect_global_dependencies_inner(
        expression,
        global_indices,
        function_indices,
        functions,
        ast,
        dependencies,
        &mut visited_functions,
    );
}

fn collect_global_dependencies_inner(
    expression: &Expression,
    global_indices: &BTreeMap<String, usize>,
    function_indices: &BTreeMap<String, usize>,
    functions: &[FunctionHeader],
    ast: &SourceAst,
    dependencies: &mut BTreeSet<usize>,
    visited_functions: &mut BTreeSet<usize>,
) {
    match expression.kind() {
        ExpressionKind::Name(name)
            if name.kind() == NameKind::Symbol && name.segments().len() == 1 =>
        {
            if let Some(index) = global_indices.get(name.value()).copied() {
                dependencies.insert(index);
            }
        }
        ExpressionKind::Application(application) => {
            if let ExpressionKind::Name(name) = application.callee().kind()
                && name.kind() == NameKind::Symbol
                && let Some(function_index) =
                    function_indices.get(name.value()).copied()
                && visited_functions.insert(function_index)
                && let Some(header) = functions.get(function_index)
                && let Some(Declaration::Defn(function)) =
                    ast.declarations().get(header.declaration_index)
            {
                for expression in function.expressions() {
                    collect_global_dependencies_inner(
                        expression,
                        global_indices,
                        function_indices,
                        functions,
                        ast,
                        dependencies,
                        visited_functions,
                    );
                }
            }
            collect_global_dependencies_inner(
                application.callee(),
                global_indices,
                function_indices,
                functions,
                ast,
                dependencies,
                visited_functions,
            );
            for argument in application.arguments() {
                collect_global_dependencies_inner(
                    argument.value(),
                    global_indices,
                    function_indices,
                    functions,
                    ast,
                    dependencies,
                    visited_functions,
                );
            }
        }
        ExpressionKind::Do(expressions) => {
            for expression in expressions {
                collect_global_dependencies_inner(
                    expression,
                    global_indices,
                    function_indices,
                    functions,
                    ast,
                    dependencies,
                    visited_functions,
                );
            }
        }
        ExpressionKind::Let { value, body, .. } => {
            collect_global_dependencies_inner(
                value,
                global_indices,
                function_indices,
                functions,
                ast,
                dependencies,
                visited_functions,
            );
            for expression in body {
                collect_global_dependencies_inner(
                    expression,
                    global_indices,
                    function_indices,
                    functions,
                    ast,
                    dependencies,
                    visited_functions,
                );
            }
        }
        ExpressionKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_global_dependencies_inner(
                condition,
                global_indices,
                function_indices,
                functions,
                ast,
                dependencies,
                visited_functions,
            );
            collect_global_dependencies_inner(
                then_branch,
                global_indices,
                function_indices,
                functions,
                ast,
                dependencies,
                visited_functions,
            );
            collect_global_dependencies_inner(
                else_branch,
                global_indices,
                function_indices,
                functions,
                ast,
                dependencies,
                visited_functions,
            );
        }
        ExpressionKind::Lambda(lambda) => {
            for expression in lambda.body() {
                collect_global_dependencies_inner(
                    expression,
                    global_indices,
                    function_indices,
                    functions,
                    ast,
                    dependencies,
                    visited_functions,
                );
            }
        }
        ExpressionKind::Match { scrutinee, arms } => {
            collect_global_dependencies_inner(
                scrutinee,
                global_indices,
                function_indices,
                functions,
                ast,
                dependencies,
                visited_functions,
            );
            for arm in arms {
                collect_global_dependencies_inner(
                    arm.result(),
                    global_indices,
                    function_indices,
                    functions,
                    ast,
                    dependencies,
                    visited_functions,
                );
            }
        }
        ExpressionKind::As { operand, .. } | ExpressionKind::Try(operand) => {
            collect_global_dependencies_inner(
                operand,
                global_indices,
                function_indices,
                functions,
                ast,
                dependencies,
                visited_functions,
            );
        }
        ExpressionKind::Literal(_) | ExpressionKind::Name(_) => {}
    }
}

fn collect_function_dependencies(
    expression: &Expression,
    function_indices: &BTreeMap<String, usize>,
    dependencies: &mut BTreeSet<usize>,
) {
    match expression.kind() {
        ExpressionKind::Application(application) => {
            if let ExpressionKind::Name(name) = application.callee().kind()
                && name.kind() == NameKind::Symbol
                && let Some(index) = function_indices.get(name.value()).copied()
            {
                dependencies.insert(index);
            }
            collect_function_dependencies(
                application.callee(),
                function_indices,
                dependencies,
            );
            for argument in application.arguments() {
                collect_function_dependencies(
                    argument.value(),
                    function_indices,
                    dependencies,
                );
            }
        }
        ExpressionKind::Do(expressions) => {
            for expression in expressions {
                collect_function_dependencies(
                    expression,
                    function_indices,
                    dependencies,
                );
            }
        }
        ExpressionKind::Let { value, body, .. } => {
            collect_function_dependencies(value, function_indices, dependencies);
            for expression in body {
                collect_function_dependencies(
                    expression,
                    function_indices,
                    dependencies,
                );
            }
        }
        ExpressionKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_function_dependencies(condition, function_indices, dependencies);
            collect_function_dependencies(then_branch, function_indices, dependencies);
            collect_function_dependencies(else_branch, function_indices, dependencies);
        }
        ExpressionKind::Lambda(lambda) => {
            for expression in lambda.body() {
                collect_function_dependencies(
                    expression,
                    function_indices,
                    dependencies,
                );
            }
        }
        ExpressionKind::Match { scrutinee, arms } => {
            collect_function_dependencies(scrutinee, function_indices, dependencies);
            for arm in arms {
                collect_function_dependencies(
                    arm.result(),
                    function_indices,
                    dependencies,
                );
            }
        }
        ExpressionKind::As { operand, .. } | ExpressionKind::Try(operand) => {
            collect_function_dependencies(operand, function_indices, dependencies);
        }
        ExpressionKind::Literal(_) | ExpressionKind::Name(_) => {}
    }
}

fn collect_function_alias_dependencies(
    expression: &Expression,
    global_indices: &BTreeMap<String, usize>,
    function_indices: &BTreeMap<String, usize>,
    globals: &[GlobalHeader],
    dependencies: &mut BTreeSet<usize>,
) {
    collect_function_alias_dependencies_inner(
        expression,
        global_indices,
        function_indices,
        globals,
        dependencies,
        &BTreeMap::new(),
    );
}

fn collect_function_alias_dependencies_inner(
    expression: &Expression,
    global_indices: &BTreeMap<String, usize>,
    function_indices: &BTreeMap<String, usize>,
    globals: &[GlobalHeader],
    dependencies: &mut BTreeSet<usize>,
    aliases: &BTreeMap<String, FunctionTargetSet>,
) {
    match expression.kind() {
        ExpressionKind::Application(application) => {
            dependencies.extend(
                syntax_function_targets(
                    application.callee(),
                    global_indices,
                    function_indices,
                    globals,
                    aliases,
                )
                .known,
            );
            collect_function_alias_dependencies_inner(
                application.callee(),
                global_indices,
                function_indices,
                globals,
                dependencies,
                aliases,
            );
            for argument in application.arguments() {
                collect_function_alias_dependencies_inner(
                    argument.value(),
                    global_indices,
                    function_indices,
                    globals,
                    dependencies,
                    aliases,
                );
            }
        }
        ExpressionKind::Do(expressions) => {
            for expression in expressions {
                collect_function_alias_dependencies_inner(
                    expression,
                    global_indices,
                    function_indices,
                    globals,
                    dependencies,
                    aliases,
                );
            }
        }
        ExpressionKind::Let {
            pattern,
            value,
            body,
        } => {
            collect_function_alias_dependencies_inner(
                value,
                global_indices,
                function_indices,
                globals,
                dependencies,
                aliases,
            );
            let mut scoped = aliases.clone();
            if let PatternKind::Binding(name) = pattern.kind()
                && !name.is_discard()
            {
                let targets = syntax_function_targets(
                    value,
                    global_indices,
                    function_indices,
                    globals,
                    aliases,
                );
                if !targets.known.is_empty() || targets.unknown || targets.has_closure {
                    scoped.insert(name.value().to_owned(), targets);
                }
            }
            for expression in body {
                collect_function_alias_dependencies_inner(
                    expression,
                    global_indices,
                    function_indices,
                    globals,
                    dependencies,
                    &scoped,
                );
            }
        }
        ExpressionKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_function_alias_dependencies_inner(
                condition,
                global_indices,
                function_indices,
                globals,
                dependencies,
                aliases,
            );
            collect_function_alias_dependencies_inner(
                then_branch,
                global_indices,
                function_indices,
                globals,
                dependencies,
                aliases,
            );
            collect_function_alias_dependencies_inner(
                else_branch,
                global_indices,
                function_indices,
                globals,
                dependencies,
                aliases,
            );
        }
        ExpressionKind::Lambda(lambda) => {
            for expression in lambda.body() {
                collect_function_alias_dependencies_inner(
                    expression,
                    global_indices,
                    function_indices,
                    globals,
                    dependencies,
                    aliases,
                );
            }
        }
        ExpressionKind::Match { scrutinee, arms } => {
            collect_function_alias_dependencies_inner(
                scrutinee,
                global_indices,
                function_indices,
                globals,
                dependencies,
                aliases,
            );
            for arm in arms {
                collect_function_alias_dependencies_inner(
                    arm.result(),
                    global_indices,
                    function_indices,
                    globals,
                    dependencies,
                    aliases,
                );
            }
        }
        ExpressionKind::As { operand, .. } | ExpressionKind::Try(operand) => {
            collect_function_alias_dependencies_inner(
                operand,
                global_indices,
                function_indices,
                globals,
                dependencies,
                aliases,
            );
        }
        ExpressionKind::Literal(_) | ExpressionKind::Name(_) => {}
    }
}

fn collect_global_alias_dependencies(
    expression: &Expression,
    global_indices: &BTreeMap<String, usize>,
    function_indices: &BTreeMap<String, usize>,
    globals: &[GlobalHeader],
    functions: &[FunctionHeader],
    ast: &SourceAst,
    dependencies: &mut BTreeSet<usize>,
) {
    let mut visited_functions = BTreeSet::new();
    collect_global_alias_dependencies_inner(
        expression,
        global_indices,
        function_indices,
        globals,
        functions,
        ast,
        dependencies,
        &mut visited_functions,
        &BTreeMap::new(),
    );
}

#[allow(clippy::too_many_arguments)]
fn collect_global_alias_dependencies_inner(
    expression: &Expression,
    global_indices: &BTreeMap<String, usize>,
    function_indices: &BTreeMap<String, usize>,
    globals: &[GlobalHeader],
    functions: &[FunctionHeader],
    ast: &SourceAst,
    dependencies: &mut BTreeSet<usize>,
    visited_functions: &mut BTreeSet<usize>,
    aliases: &BTreeMap<String, usize>,
) {
    match expression.kind() {
        ExpressionKind::Name(name)
            if name.kind() == NameKind::Symbol && name.segments().len() == 1 =>
        {
            if let Some(index) = global_indices.get(name.value()).copied() {
                dependencies.insert(index);
            }
        }
        ExpressionKind::Application(application) => {
            if let Some(function_index) = syntax_function_index(
                application.callee(),
                global_indices,
                function_indices,
                globals,
                aliases,
            ) && visited_functions.insert(function_index)
                && let Some(header) = functions.get(function_index)
                && let Some(Declaration::Defn(function)) =
                    ast.declarations().get(header.declaration_index)
            {
                for expression in function.expressions() {
                    collect_global_alias_dependencies_inner(
                        expression,
                        global_indices,
                        function_indices,
                        globals,
                        functions,
                        ast,
                        dependencies,
                        visited_functions,
                        &BTreeMap::new(),
                    );
                }
            }
            collect_global_alias_dependencies_inner(
                application.callee(),
                global_indices,
                function_indices,
                globals,
                functions,
                ast,
                dependencies,
                visited_functions,
                aliases,
            );
            for argument in application.arguments() {
                collect_global_alias_dependencies_inner(
                    argument.value(),
                    global_indices,
                    function_indices,
                    globals,
                    functions,
                    ast,
                    dependencies,
                    visited_functions,
                    aliases,
                );
            }
        }
        ExpressionKind::Do(expressions) => {
            for expression in expressions {
                collect_global_alias_dependencies_inner(
                    expression,
                    global_indices,
                    function_indices,
                    globals,
                    functions,
                    ast,
                    dependencies,
                    visited_functions,
                    aliases,
                );
            }
        }
        ExpressionKind::Let {
            pattern,
            value,
            body,
        } => {
            collect_global_alias_dependencies_inner(
                value,
                global_indices,
                function_indices,
                globals,
                functions,
                ast,
                dependencies,
                visited_functions,
                aliases,
            );
            let mut scoped = aliases.clone();
            if let PatternKind::Binding(name) = pattern.kind()
                && !name.is_discard()
                && let Some(function) = syntax_function_index(
                    value,
                    global_indices,
                    function_indices,
                    globals,
                    aliases,
                )
            {
                scoped.insert(name.value().to_owned(), function);
            }
            for expression in body {
                collect_global_alias_dependencies_inner(
                    expression,
                    global_indices,
                    function_indices,
                    globals,
                    functions,
                    ast,
                    dependencies,
                    visited_functions,
                    &scoped,
                );
            }
        }
        ExpressionKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            for expression in [condition, then_branch, else_branch] {
                collect_global_alias_dependencies_inner(
                    expression,
                    global_indices,
                    function_indices,
                    globals,
                    functions,
                    ast,
                    dependencies,
                    visited_functions,
                    aliases,
                );
            }
        }
        ExpressionKind::Lambda(lambda) => {
            for expression in lambda.body() {
                collect_global_alias_dependencies_inner(
                    expression,
                    global_indices,
                    function_indices,
                    globals,
                    functions,
                    ast,
                    dependencies,
                    visited_functions,
                    aliases,
                );
            }
        }
        ExpressionKind::Match { scrutinee, arms } => {
            collect_global_alias_dependencies_inner(
                scrutinee,
                global_indices,
                function_indices,
                globals,
                functions,
                ast,
                dependencies,
                visited_functions,
                aliases,
            );
            for arm in arms {
                collect_global_alias_dependencies_inner(
                    arm.result(),
                    global_indices,
                    function_indices,
                    globals,
                    functions,
                    ast,
                    dependencies,
                    visited_functions,
                    aliases,
                );
            }
        }
        ExpressionKind::As { operand, .. } | ExpressionKind::Try(operand) => {
            collect_global_alias_dependencies_inner(
                operand,
                global_indices,
                function_indices,
                globals,
                functions,
                ast,
                dependencies,
                visited_functions,
                aliases,
            );
        }
        ExpressionKind::Literal(_) | ExpressionKind::Name(_) => {}
    }
}

fn ensure_expected(
    environment: &mut CheckEnvironment<'_>,
    span: ByteSpan,
    expected: Option<PrimitiveType>,
    actual: PrimitiveType,
) {
    if let Some(expected) = expected
        && !types_match(&expected, &actual)
    {
        mismatch(
            environment.diagnostics,
            environment.source_id,
            span,
            expected,
            actual,
            "expression type does not match the written expectation",
        );
    }
}

fn unknown_name(
    environment: &mut CheckEnvironment<'_>,
    expression: &Expression,
    name: &str,
) {
    environment.diagnostics.push(
        Diagnostic::new(
            DiagnosticCode::NameUnknownSymbol,
            expression.span(),
            format!("symbol `{name}` does not resolve to a visible value"),
        )
        .with_source_id(environment.source_id),
    );
}

fn call_contract_error(
    environment: &mut CheckEnvironment<'_>,
    span: ByteSpan,
    message: String,
) {
    environment.diagnostics.push(
        Diagnostic::new(DiagnosticCode::TypeArgumentMismatch, span, message)
            .with_source_id(environment.source_id),
    );
}

fn redeclaration(
    diagnostics: &mut Vec<Diagnostic>,
    source_id: &str,
    _name: &str,
    _earlier_name: &str,
    span: ByteSpan,
    earlier_span: ByteSpan,
) {
    diagnostics.push(
        Diagnostic::new(
            DiagnosticCode::NameRedeclaration,
            span,
            "a visible name is introduced more than once",
        )
        .with_source_id(source_id)
        .with_related_source(
            source_id,
            earlier_span,
            "the earlier binding is here",
        ),
    );
}

fn check_signature(
    source_id: &str,
    function: &vibra_syntax::FunctionDeclaration,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<FunctionSignature> {
    let mut parameters = Vec::with_capacity(function.parameters().len());
    let mut valid = true;
    for parameter in function.parameters() {
        match primitive_type(parameter.value_type()) {
            Some(value_type) => parameters.push(value_type),
            None => {
                valid = false;
                unavailable(
                    diagnostics,
                    source_id,
                    parameter.span(),
                    "only monomorphic parameter types are available in Step 7",
                );
            }
        }
    }
    let result = match primitive_type(function.result()) {
        Some(value_type) => value_type,
        None => {
            valid = false;
            unavailable(
                diagnostics,
                source_id,
                function.span(),
                "only monomorphic result types are available in Step 7",
            );
            PrimitiveType::Void
        }
    };
    let mut labelled = Vec::new();
    for attribute in function.attributes().items() {
        match attribute {
            Attribute::Labelled(entries) => {
                for entry in entries {
                    let Some(value_type) = primitive_type(entry.value_type()) else {
                        valid = false;
                        unavailable(
                            diagnostics,
                            source_id,
                            entry.span(),
                            "labelled parameter types must be monomorphic in Step 7",
                        );
                        continue;
                    };
                    let Some(default) = check_literal(
                        source_id,
                        entry.span(),
                        entry.default(),
                        Some(value_type.clone()),
                        diagnostics,
                    ) else {
                        valid = false;
                        continue;
                    };
                    labelled.push(IrLabelledParameter::new(
                        entry.name().value(),
                        value_type,
                        Some(default),
                    ));
                }
            }
            Attribute::Effects(row) if !row.references().is_empty() => {
                valid = false;
                unavailable(
                    diagnostics,
                    source_id,
                    row.span(),
                    "nonempty effect ceilings remain unavailable in M2",
                );
            }
            _ => {}
        }
    }
    valid.then(|| FunctionSignature::with_labelled(parameters, labelled, result))
}

fn compiler_intrinsic(
    source_id: &str,
    function: &vibra_syntax::FunctionDeclaration,
    diagnostics: &mut Vec<Diagnostic>,
    trusted_bootstrap: bool,
) -> Option<CompilerIntrinsic> {
    let provider = function.attributes().items().iter().find_map(|attribute| {
        if let Attribute::External(name) = attribute {
            Some(name.value())
        } else {
            None
        }
    });
    let provider = provider?;
    if !trusted_bootstrap {
        unavailable(
            diagnostics,
            source_id,
            function.span(),
            "external declarations require the verified M2 bootstrap module",
        );
        return None;
    }
    if provider != "compiler" {
        diagnostics.push(
            Diagnostic::new(
                DiagnosticCode::ExternalUnknownSymbol,
                function.span(),
                "only the closed @compiler registry is executable in M2",
            )
            .with_source_id(source_id),
        );
        return None;
    }
    let symbol = function.attributes().items().iter().find_map(|attribute| {
        if let Attribute::Symbol(Literal::String(value)) = attribute {
            Some(value.value())
        } else {
            None
        }
    });
    let symbol = symbol?;
    let Some(intrinsic) = CompilerIntrinsic::from_symbol(symbol) else {
        diagnostics.push(
            Diagnostic::new(
                DiagnosticCode::ExternalUnknownSymbol,
                function.span(),
                format!("compiler symbol `{symbol}` is outside the closed M2 registry"),
            )
            .with_source_id(source_id),
        );
        return None;
    };
    let expected = intrinsic.signature();
    let actual = FunctionSignature::new(
        function
            .parameters()
            .iter()
            .filter_map(|parameter| primitive_type(parameter.value_type()))
            .collect(),
        primitive_type(function.result()).unwrap_or(PrimitiveType::Void),
    );
    if !actual.same_shape(&expected) {
        diagnostics.push(
            Diagnostic::new(
                DiagnosticCode::TypeArgumentMismatch,
                function.span(),
                format!(
                    "compiler symbol `{symbol}` has signature {} -> {}",
                    expected
                        .parameters()
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(" "),
                    expected.result()
                ),
            )
            .with_source_id(source_id),
        );
        return None;
    }
    Some(intrinsic)
}

fn check_lambda_signature(
    source_id: &str,
    lambda: &vibra_syntax::LambdaExpression,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<FunctionSignature> {
    let mut valid = true;
    let parameters = lambda
        .parameters()
        .iter()
        .map(|parameter| {
            let value_type = primitive_type(parameter.value_type());
            if value_type.is_none() {
                valid = false;
                unavailable(
                    diagnostics,
                    source_id,
                    parameter.span(),
                    "lambda parameter types must be monomorphic in Step 7",
                );
            }
            value_type
        })
        .collect::<Option<Vec<_>>>()?;
    let Some(result) = primitive_type(lambda.result()) else {
        unavailable(
            diagnostics,
            source_id,
            lambda.span(),
            "lambda result types must be monomorphic in Step 7",
        );
        return None;
    };
    let mut labelled = Vec::new();
    for attribute in lambda.attributes().items() {
        match attribute {
            Attribute::Labelled(entries) => {
                for entry in entries {
                    let Some(value_type) = primitive_type(entry.value_type()) else {
                        valid = false;
                        unavailable(
                            diagnostics,
                            source_id,
                            entry.span(),
                            "labelled parameter types must be monomorphic in Step 7",
                        );
                        continue;
                    };
                    let Some(default) = check_literal(
                        source_id,
                        entry.span(),
                        entry.default(),
                        Some(value_type.clone()),
                        diagnostics,
                    ) else {
                        valid = false;
                        continue;
                    };
                    labelled.push(IrLabelledParameter::new(
                        entry.name().value(),
                        value_type,
                        Some(default),
                    ));
                }
            }
            Attribute::Effects(row) if !row.references().is_empty() => {
                valid = false;
                unavailable(
                    diagnostics,
                    source_id,
                    row.span(),
                    "nonempty effect ceilings remain unavailable in M2",
                );
            }
            Attribute::Variadic(parameter) => {
                valid = false;
                unavailable(
                    diagnostics,
                    source_id,
                    parameter.span(),
                    "variadic parameters remain unavailable until M3",
                );
            }
            _ => {}
        }
    }
    valid.then(|| FunctionSignature::with_labelled(parameters, labelled, result))
}

fn primitive_type(value: &TypeExpr) -> Option<PrimitiveType> {
    match value {
        TypeExpr::Void => Some(PrimitiveType::Void),
        TypeExpr::Name(name) => match name.value() {
            "bool" => Some(PrimitiveType::Bool),
            "char" => Some(PrimitiveType::Char),
            "str" => Some(PrimitiveType::Str),
            "bytes" => Some(PrimitiveType::Bytes),
            "atom" => Some(PrimitiveType::Atom),
            "i8" => Some(PrimitiveType::I8),
            "i16" => Some(PrimitiveType::I16),
            "i32" => Some(PrimitiveType::I32),
            "i64" => Some(PrimitiveType::I64),
            "u8" => Some(PrimitiveType::U8),
            "u16" => Some(PrimitiveType::U16),
            "u32" => Some(PrimitiveType::U32),
            "u64" => Some(PrimitiveType::U64),
            "f32" => Some(PrimitiveType::F32),
            "f64" => Some(PrimitiveType::F64),
            _ => None,
        },
        TypeExpr::Function(function)
            if function.effects().is_empty() && function.variadic().is_none() =>
        {
            let parameters = function
                .parameters()
                .iter()
                .map(primitive_type)
                .collect::<Option<Vec<_>>>()?;
            let labelled = function
                .labelled()
                .iter()
                .map(|slot| {
                    Some(IrLabelledParameter::new(
                        slot.name().value(),
                        primitive_type(slot.value_type())?,
                        None,
                    ))
                })
                .collect::<Option<Vec<_>>>()?;
            let result = primitive_type(function.result())?;
            Some(PrimitiveType::Function(Box::new(
                FunctionSignature::with_labelled(parameters, labelled, result),
            )))
        }
        TypeExpr::Applied { .. }
        | TypeExpr::Tuple(_)
        | TypeExpr::Array(_)
        | TypeExpr::Map(_, _)
        | TypeExpr::Function(_) => None,
    }
}

fn has_deferred_attributes(attributes: &[Attribute]) -> bool {
    attributes.iter().any(|attribute| match attribute {
        Attribute::Where(_)
        | Attribute::Variadic(_)
        | Attribute::External(_)
        | Attribute::Symbol(_) => true,
        Attribute::Labelled(_) => false,
        Attribute::Effects(row) => !row.references().is_empty(),
        Attribute::Visibility(_) | Attribute::Doc(_) => false,
    })
}

fn check_sequence(
    environment: &mut CheckEnvironment<'_>,
    expressions: &[Expression],
    expected: Option<PrimitiveType>,
    fallback_span: ByteSpan,
    tail_position: bool,
) -> Option<Expr> {
    if expressions.is_empty() {
        if expected
            .as_ref()
            .is_some_and(|expected| *expected != PrimitiveType::Void)
        {
            mismatch(
                environment.diagnostics,
                environment.source_id,
                fallback_span,
                expected.clone().unwrap_or(PrimitiveType::Void),
                PrimitiveType::Void,
                "an empty sequence returns void",
            );
            return None;
        }
        return Some(Expr::literal(
            Value::Void,
            SourceOrigin::new(environment.source_id, fallback_span),
        ));
    }

    let mut checked = Vec::with_capacity(expressions.len());
    let mut valid = true;
    for (index, expression) in expressions.iter().enumerate() {
        let expression_expected = (index + 1 == expressions.len())
            .then_some(expected.clone())
            .flatten();
        match check_expression_in_position(
            environment,
            expression,
            expression_expected,
            tail_position && index + 1 == expressions.len(),
        ) {
            Some(value) => checked.push(value),
            None => valid = false,
        }
    }
    if !valid {
        return None;
    }
    let origin = expressions
        .first()
        .zip(expressions.last())
        .map(|(first, last)| {
            SourceOrigin::new(environment.source_id, first.span().join(last.span()))
        })
        .unwrap_or_else(|| SourceOrigin::new(environment.source_id, fallback_span));
    Some(Expr::sequence(checked, origin))
}

fn check_expression(
    environment: &mut CheckEnvironment<'_>,
    expression: &Expression,
    expected: Option<PrimitiveType>,
) -> Option<Expr> {
    check_expression_in_position(environment, expression, expected, false)
}

fn check_expression_in_position(
    environment: &mut CheckEnvironment<'_>,
    expression: &Expression,
    expected: Option<PrimitiveType>,
    tail_position: bool,
) -> Option<Expr> {
    match expression.kind() {
        ExpressionKind::Literal(literal) => check_literal(
            environment.source_id,
            expression.span(),
            literal,
            expected,
            environment.diagnostics,
        )
        .map(|value| {
            Expr::literal(
                value,
                SourceOrigin::new(environment.source_id, expression.span()),
            )
        }),
        ExpressionKind::Name(name) if name.kind() == NameKind::Atom => {
            let actual = PrimitiveType::Atom;
            if expected
                .as_ref()
                .is_some_and(|expected| *expected != actual)
            {
                mismatch(
                    environment.diagnostics,
                    environment.source_id,
                    expression.span(),
                    expected.clone().unwrap_or(actual.clone()),
                    actual.clone(),
                    "an atom literal does not match the written result type",
                );
                return None;
            }
            Some(Expr::literal(
                Value::Atom(name.value().to_owned()),
                SourceOrigin::new(environment.source_id, expression.span()),
            ))
        }
        ExpressionKind::Name(name) if name.kind() == NameKind::Symbol => {
            if name.segments().len() != 1 {
                if let Some(index) =
                    environment.function_indices.get(name.value()).copied()
                    && let Some(header) = environment.functions.get(index)
                {
                    let actual =
                        PrimitiveType::Function(Box::new(header.signature.clone()));
                    ensure_expected(
                        environment,
                        expression.span(),
                        expected.clone(),
                        actual.clone(),
                    );
                    return (expected
                        .as_ref()
                        .is_none_or(|expected| types_match(expected, &actual)))
                    .then_some(Expr::function(
                        index,
                        header.signature.clone(),
                        SourceOrigin::new(environment.source_id, expression.span()),
                    ));
                }
                unknown_name(environment, expression, name.value());
                return None;
            }
            if let Some(binding) = environment.locals.get(name.value()).cloned() {
                ensure_expected(
                    environment,
                    expression.span(),
                    expected.clone(),
                    binding.value_type.clone(),
                );
                return (expected.as_ref().is_none_or(|expected| {
                    types_match(expected, &binding.value_type)
                }))
                .then_some(Expr::variable(
                    binding.slot,
                    binding.value_type.clone(),
                    SourceOrigin::new(environment.source_id, expression.span()),
                ));
            }
            if let Some(binding) = environment.captures.get(name.value()).cloned() {
                ensure_expected(
                    environment,
                    expression.span(),
                    expected.clone(),
                    binding.value_type.clone(),
                );
                return (expected.as_ref().is_none_or(|expected| {
                    types_match(expected, &binding.value_type)
                }))
                .then_some(Expr::captured(
                    binding.slot,
                    binding.value_type,
                    SourceOrigin::new(environment.source_id, expression.span()),
                ));
            }
            if let Some(binding) =
                environment.resolve_capture(name.value(), expression.span())
            {
                ensure_expected(
                    environment,
                    expression.span(),
                    expected.clone(),
                    binding.value_type.clone(),
                );
                return (expected.as_ref().is_none_or(|expected| {
                    types_match(expected, &binding.value_type)
                }))
                .then_some(Expr::captured(
                    binding.slot,
                    binding.value_type,
                    SourceOrigin::new(environment.source_id, expression.span()),
                ));
            }
            if let Some(index) = environment.global_indices.get(name.value()).copied() {
                let global = environment.globals.get(index)?;
                let actual = global.value_type.clone();
                ensure_expected(
                    environment,
                    expression.span(),
                    expected.clone(),
                    actual.clone(),
                );
                return (expected
                    .as_ref()
                    .is_none_or(|expected| types_match(expected, &actual)))
                .then_some(Expr::global(
                    index,
                    actual,
                    SourceOrigin::new(environment.source_id, expression.span()),
                ));
            }
            if let Some(index) = environment.function_indices.get(name.value()).copied()
            {
                let header = environment.functions.get(index)?;
                let actual =
                    PrimitiveType::Function(Box::new(header.signature.clone()));
                ensure_expected(
                    environment,
                    expression.span(),
                    expected.clone(),
                    actual.clone(),
                );
                return (expected
                    .as_ref()
                    .is_none_or(|expected| types_match(expected, &actual)))
                .then_some(Expr::function(
                    index,
                    header.signature.clone(),
                    SourceOrigin::new(environment.source_id, expression.span()),
                ));
            }
            unknown_name(environment, expression, name.value());
            None
        }
        ExpressionKind::Application(application) => {
            if application.type_arguments().is_some() {
                unavailable(
                    environment.diagnostics,
                    environment.source_id,
                    application.span(),
                    "generic type arguments are deferred until M3",
                );
                return None;
            }
            let callee = check_expression(environment, application.callee(), None)?;
            let PrimitiveType::Function(signature) = callee.result_type() else {
                environment.diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::TypeNotApplicable,
                        application.callee().span(),
                        "the statically resolved callee is not a function",
                    )
                    .with_source_id(environment.source_id),
                );
                return None;
            };
            let direct_function = match &callee {
                Expr::Function { function, .. } => Some(*function),
                _ => None,
            };
            let function_targets =
                function_targets_from_expr(&callee, environment, &BTreeMap::new());
            let known_function = direct_function
                .or_else(|| function_index_from_expr(&callee, environment));
            let tail_transfer = tail_position
                && !function_targets.known.is_empty()
                && environment.recursive_group.as_ref().is_some_and(|group| {
                    function_targets
                        .known
                        .iter()
                        .all(|index| group.contains(index))
                });
            let facts = BindingFacts::new(
                signature.parameters().len(),
                signature
                    .labelled()
                    .iter()
                    .map(|parameter| parameter.name().to_owned())
                    .collect(),
                None,
            );
            let ordered = match application.ordered_arguments(&facts) {
                Ok(arguments) => arguments,
                Err(error) => {
                    call_contract_error(
                        environment,
                        application.span(),
                        error.to_string(),
                    );
                    return None;
                }
            };
            let mut labelled = BTreeMap::new();
            for argument in ordered.iter().skip(signature.parameters().len()) {
                if let Some(label) = argument.label() {
                    labelled.insert(label.value().to_owned(), *argument);
                }
            }
            let mut arguments = Vec::with_capacity(signature.fixed_parameter_count());
            for (argument, value_type) in ordered
                .iter()
                .take(signature.parameters().len())
                .zip(signature.parameters())
            {
                arguments.push(check_expression(
                    environment,
                    argument.value(),
                    Some(value_type.clone()),
                )?);
            }
            for parameter in signature.labelled() {
                if let Some(argument) = labelled.remove(parameter.name()) {
                    arguments.push(check_expression(
                        environment,
                        argument.value(),
                        Some(parameter.value_type()),
                    )?);
                } else {
                    if callable_default_is_missing(&callee, parameter.name()) {
                        call_contract_error(
                            environment,
                            application.span(),
                            format!(
                                "labelled argument `{}` has no default",
                                parameter.name()
                            ),
                        );
                        return None;
                    }
                    arguments.push(Expr::default_value(
                        parameter.value_type(),
                        SourceOrigin::new(environment.source_id, application.span()),
                    ));
                }
            }
            let result = signature.result();
            ensure_expected(
                environment,
                application.span(),
                expected.clone(),
                result.clone(),
            );
            if expected
                .as_ref()
                .is_some_and(|expected| !types_match(expected, &result))
            {
                return None;
            }
            if ordered
                .iter()
                .zip(application.arguments())
                .any(|(left, right)| !std::ptr::eq(*left, right))
            {
                environment.diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::StyleArgumentOrder,
                        application.span(),
                        "application operands are not in canonical declaration order",
                    )
                    .with_source_id(environment.source_id),
                );
            }
            environment
                .bindings
                .push(ApplicationBinding::new(application.span(), facts));
            let origin = SourceOrigin::new(environment.source_id, application.span());
            Some(match direct_function {
                Some(function) if tail_transfer => {
                    Expr::tail_call(function, arguments, result, origin)
                }
                Some(function) => Expr::call(function, arguments, result, origin),
                None if tail_transfer => {
                    let function_hint = (function_targets.known.len() == 1
                        && !function_targets.unknown
                        && !function_targets.has_closure)
                        .then(|| function_targets.known.iter().next().copied())
                        .flatten();
                    Expr::indirect_tail_call_with_hint(
                        callee,
                        function_hint,
                        arguments,
                        result,
                        origin,
                    )
                }
                None => Expr::indirect_call(
                    callee,
                    known_function,
                    arguments,
                    result,
                    origin,
                ),
            })
        }
        ExpressionKind::Do(expressions) => check_sequence(
            environment,
            expressions,
            expected,
            expression.span(),
            tail_position,
        ),
        ExpressionKind::Let {
            pattern,
            value,
            body,
        } => {
            let value = check_expression(environment, value, None)?;
            let function_targets =
                matches!(value.result_type(), PrimitiveType::Function(_)).then(|| {
                    function_targets_from_expr(&value, environment, &BTreeMap::new())
                });
            let mut nested = CheckEnvironment {
                source_id: environment.source_id,
                diagnostics: environment.diagnostics,
                global_indices: environment.global_indices,
                globals: environment.globals,
                functions: environment.functions,
                function_indices: environment.function_indices,
                module_names: environment.module_names,
                bindings: &mut *environment.bindings,
                locals: environment.locals.clone(),
                captures: environment.captures.clone(),
                capture_sources: environment.capture_sources.clone(),
                outer: environment.outer.clone(),
                next_slot: environment.next_slot,
                current_function: environment.current_function,
                recursive_group: environment.recursive_group.clone(),
            };
            let slot = match pattern.kind() {
                PatternKind::Binding(name) if name.is_discard() => None,
                PatternKind::Binding(name) => {
                    if !nested.add_binding_type_with_targets(
                        name.value(),
                        value.result_type(),
                        pattern.span(),
                        function_targets,
                    ) {
                        return None;
                    }
                    Some(nested.next_slot.saturating_sub(1))
                }
                _ => {
                    unavailable(
                        nested.diagnostics,
                        nested.source_id,
                        pattern.span(),
                        "constructor and destructuring patterns are deferred until M3",
                    );
                    return None;
                }
            };
            let result = check_sequence(
                &mut nested,
                body,
                expected,
                expression.span(),
                tail_position,
            );
            let next_slot = nested.next_slot;
            let captures = nested.captures.clone();
            let capture_sources = nested.capture_sources.clone();
            drop(nested);
            environment.next_slot = environment.next_slot.max(next_slot);
            environment.captures = captures;
            environment.capture_sources = capture_sources;
            result.map(|body| {
                Expr::let_binding(
                    slot,
                    value,
                    body,
                    SourceOrigin::new(environment.source_id, expression.span()),
                )
            })
        }
        ExpressionKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let condition =
                check_expression(environment, condition, Some(PrimitiveType::Bool))?;
            let then_branch = check_expression_in_position(
                environment,
                then_branch,
                expected.clone(),
                tail_position,
            )?;
            let else_branch = check_expression_in_position(
                environment,
                else_branch,
                expected.clone(),
                tail_position,
            )?;
            if !types_match(&then_branch.result_type(), &else_branch.result_type()) {
                mismatch(
                    environment.diagnostics,
                    environment.source_id,
                    expression.span(),
                    then_branch.result_type(),
                    else_branch.result_type(),
                    "if branches must have identical types",
                );
                return None;
            }
            Some(Expr::if_expression(
                condition,
                then_branch,
                else_branch,
                SourceOrigin::new(environment.source_id, expression.span()),
            ))
        }
        ExpressionKind::Lambda(lambda) => {
            let signature = check_lambda_signature(
                environment.source_id,
                lambda,
                environment.diagnostics,
            )?;
            let outer = environment.visible_bindings();
            let mut nested = CheckEnvironment::new(
                environment.source_id,
                environment.diagnostics,
                environment.global_indices,
                environment.globals,
                environment.functions,
                environment.function_indices,
                environment.module_names,
                &mut *environment.bindings,
                None,
                None,
            );
            nested.outer = Some(outer);
            let mut referenced_names = Vec::new();
            for expression in lambda.body() {
                collect_value_names(expression, &mut referenced_names);
            }
            for name in referenced_names {
                if nested
                    .outer
                    .as_ref()
                    .is_some_and(|outer| outer.values.contains_key(&name))
                {
                    let _ = nested.resolve_capture(&name, lambda.span());
                }
            }
            let mut parameters_valid = true;
            let mut parameter_types = Vec::with_capacity(lambda.parameters().len());
            for (parameter_index, parameter) in lambda.parameters().iter().enumerate() {
                let Some(value_type) = primitive_type(parameter.value_type()) else {
                    parameters_valid = false;
                    nested.next_slot = parameter_index.saturating_add(1);
                    unavailable(
                        nested.diagnostics,
                        nested.source_id,
                        parameter.span(),
                        "lambda parameter types must be monomorphic in Step 7",
                    );
                    continue;
                };
                parameter_types.push(value_type.clone());
                match parameter.parsed_pattern().kind() {
                    PatternKind::Binding(name) if name.is_discard() => {}
                    PatternKind::Binding(name) => {
                        if !nested.add_binding_type(
                            name.value(),
                            value_type,
                            parameter.span(),
                        ) {
                            parameters_valid = false;
                        }
                    }
                    _ => {
                        parameters_valid = false;
                        unavailable(
                            nested.diagnostics,
                            nested.source_id,
                            parameter.span(),
                            "constructor and destructuring patterns are deferred until M3",
                        );
                    }
                }
                nested.next_slot = parameter_index.saturating_add(1);
            }
            let mut labelled_index = 0;
            for attribute in lambda.attributes().items() {
                let Attribute::Labelled(entries) = attribute else {
                    continue;
                };
                for entry in entries {
                    let Some(labelled) = signature.labelled().get(labelled_index)
                    else {
                        continue;
                    };
                    labelled_index = labelled_index.saturating_add(1);
                    if !nested.add_binding_type(
                        labelled.name(),
                        labelled.value_type(),
                        entry.span(),
                    ) {
                        parameters_valid = false;
                    }
                }
            }
            if !parameters_valid {
                return None;
            }
            let body = check_sequence(
                &mut nested,
                lambda.body(),
                Some(signature.result()),
                lambda.span(),
                true,
            )?;
            let capture_sources = std::mem::take(&mut nested.capture_sources);
            let slot_count = nested.next_slot;
            drop(nested);
            let actual = PrimitiveType::Function(Box::new(signature.clone()));
            ensure_expected(
                environment,
                expression.span(),
                expected.clone(),
                actual.clone(),
            );
            if expected
                .as_ref()
                .is_some_and(|expected| !types_match(expected, &actual))
            {
                return None;
            }
            Some(Expr::closure(
                signature,
                parameter_types,
                capture_sources,
                body,
                slot_count,
                SourceOrigin::new(environment.source_id, expression.span()),
            ))
        }
        ExpressionKind::Match { .. }
        | ExpressionKind::As { .. }
        | ExpressionKind::Try(_) => {
            unavailable(
                environment.diagnostics,
                environment.source_id,
                expression.span(),
                "this expression form is deferred until a later M2/M3 step",
            );
            None
        }
        ExpressionKind::Name(_) => {
            unknown_name(environment, expression, "<invalid name>");
            None
        }
    }
}

fn check_literal(
    source_id: &str,
    span: ByteSpan,
    literal: &Literal,
    expected: Option<PrimitiveType>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Value> {
    match literal {
        Literal::String(value) => expect_fixed(
            source_id,
            span,
            expected,
            PrimitiveType::Str,
            Value::Str(value.value().to_owned()),
            diagnostics,
        ),
        Literal::Character(value) => expect_fixed(
            source_id,
            span,
            expected,
            PrimitiveType::Char,
            Value::Char(value.value()),
            diagnostics,
        ),
        Literal::Boolean(value) => expect_fixed(
            source_id,
            span,
            expected,
            PrimitiveType::Bool,
            Value::Bool(value.value()),
            diagnostics,
        ),
        Literal::Void(_) => expect_fixed(
            source_id,
            span,
            expected,
            PrimitiveType::Void,
            Value::Void,
            diagnostics,
        ),
        Literal::Integer(value) => {
            check_integer(source_id, span, value, expected, diagnostics)
        }
        Literal::Float(value) => {
            check_float(source_id, span, value, expected, diagnostics)
        }
    }
}

fn expect_fixed(
    source_id: &str,
    span: ByteSpan,
    expected: Option<PrimitiveType>,
    actual: PrimitiveType,
    value: Value,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Value> {
    if expected
        .as_ref()
        .is_some_and(|expected| *expected != actual)
    {
        mismatch(
            diagnostics,
            source_id,
            span,
            expected.clone().unwrap_or(actual.clone()),
            actual,
            "literal type does not match the written result type",
        );
        None
    } else {
        Some(value)
    }
}

fn check_integer(
    source_id: &str,
    span: ByteSpan,
    literal: &vibra_syntax::IntegerLiteral,
    expected: Option<PrimitiveType>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Value> {
    let target = literal
        .suffix()
        .map(integer_suffix_type)
        .or_else(|| expected.clone().filter(|value| value.is_integer()));
    let Some(target) = target else {
        mismatch(
            diagnostics,
            source_id,
            span,
            expected.unwrap_or(PrimitiveType::I64),
            PrimitiveType::I64,
            "an unsuffixed integer needs one expected integer type",
        );
        return None;
    };
    let Some(magnitude) = literal.digits().parse::<u128>().ok() else {
        out_of_range(
            diagnostics,
            source_id,
            span,
            "integer digits exceed all v1 widths",
        );
        return None;
    };
    let Some(value) = integer_value(target.clone(), literal.is_negative(), magnitude)
    else {
        out_of_range(
            diagnostics,
            source_id,
            span,
            "integer is outside its exact primitive range",
        );
        return None;
    };
    if expected
        .as_ref()
        .is_some_and(|expected| *expected != target)
    {
        mismatch(
            diagnostics,
            source_id,
            span,
            expected.clone().unwrap_or(target.clone()),
            target,
            "numeric suffixes never request an implicit conversion",
        );
        return None;
    }
    Some(value)
}

fn check_float(
    source_id: &str,
    span: ByteSpan,
    literal: &vibra_syntax::FloatLiteral,
    expected: Option<PrimitiveType>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Value> {
    let target = literal
        .suffix()
        .map(float_suffix_type)
        .or_else(|| expected.clone().filter(|value| value.is_float()));
    let Some(target) = target else {
        mismatch(
            diagnostics,
            source_id,
            span,
            expected.unwrap_or(PrimitiveType::F64),
            PrimitiveType::F64,
            "an unsuffixed float needs one expected floating-point type",
        );
        return None;
    };
    let value = match target {
        PrimitiveType::F32 => literal
            .body()
            .parse::<f32>()
            .ok()
            .filter(|value| value.is_finite())
            .and_then(Value::f32),
        PrimitiveType::F64 => literal
            .body()
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .and_then(Value::f64),
        _ => None,
    };
    let Some(value) = value else {
        out_of_range(
            diagnostics,
            source_id,
            span,
            "finite float literal overflows its exact type",
        );
        return None;
    };
    if expected
        .as_ref()
        .is_some_and(|expected| *expected != target)
    {
        mismatch(
            diagnostics,
            source_id,
            span,
            expected.clone().unwrap_or(target.clone()),
            target,
            "numeric suffixes never request an implicit conversion",
        );
        return None;
    }
    Some(value)
}

fn integer_suffix_type(suffix: IntegerSuffix) -> PrimitiveType {
    match suffix {
        IntegerSuffix::I8 => PrimitiveType::I8,
        IntegerSuffix::I16 => PrimitiveType::I16,
        IntegerSuffix::I32 => PrimitiveType::I32,
        IntegerSuffix::I64 => PrimitiveType::I64,
        IntegerSuffix::U8 => PrimitiveType::U8,
        IntegerSuffix::U16 => PrimitiveType::U16,
        IntegerSuffix::U32 => PrimitiveType::U32,
        IntegerSuffix::U64 => PrimitiveType::U64,
    }
}

fn float_suffix_type(suffix: FloatSuffix) -> PrimitiveType {
    match suffix {
        FloatSuffix::F32 => PrimitiveType::F32,
        FloatSuffix::F64 => PrimitiveType::F64,
    }
}

fn integer_value(
    target: PrimitiveType,
    negative: bool,
    magnitude: u128,
) -> Option<Value> {
    match target {
        PrimitiveType::I8 => {
            signed_value(negative, magnitude, i8::MIN as i128, i8::MAX as i128)
                .map(|value| Value::I8(value as i8))
        }
        PrimitiveType::I16 => {
            signed_value(negative, magnitude, i16::MIN as i128, i16::MAX as i128)
                .map(|value| Value::I16(value as i16))
        }
        PrimitiveType::I32 => {
            signed_value(negative, magnitude, i32::MIN as i128, i32::MAX as i128)
                .map(|value| Value::I32(value as i32))
        }
        PrimitiveType::I64 => {
            signed_value(negative, magnitude, i64::MIN as i128, i64::MAX as i128)
                .map(|value| Value::I64(value as i64))
        }
        PrimitiveType::U8 => (!negative && magnitude <= u8::MAX as u128)
            .then_some(Value::U8(magnitude as u8)),
        PrimitiveType::U16 => (!negative && magnitude <= u16::MAX as u128)
            .then_some(Value::U16(magnitude as u16)),
        PrimitiveType::U32 => (!negative && magnitude <= u32::MAX as u128)
            .then_some(Value::U32(magnitude as u32)),
        PrimitiveType::U64 => (!negative && magnitude <= u64::MAX as u128)
            .then_some(Value::U64(magnitude as u64)),
        _ => None,
    }
}

fn signed_value(negative: bool, magnitude: u128, min: i128, max: i128) -> Option<i128> {
    if negative {
        let magnitude = i128::try_from(magnitude).ok()?;
        let value = magnitude.checked_neg()?;
        (value >= min).then_some(value)
    } else {
        let value = i128::try_from(magnitude).ok()?;
        (value <= max).then_some(value)
    }
}

fn mismatch(
    diagnostics: &mut Vec<Diagnostic>,
    source_id: &str,
    span: ByteSpan,
    expected: PrimitiveType,
    actual: PrimitiveType,
    message: impl Into<String>,
) {
    diagnostics.push(
        Diagnostic::new(
            DiagnosticCode::TypeArgumentMismatch,
            span,
            format!("{}: expected {expected}, found {actual}", message.into()),
        )
        .with_source_id(source_id),
    );
}

fn out_of_range(
    diagnostics: &mut Vec<Diagnostic>,
    source_id: &str,
    span: ByteSpan,
    message: &'static str,
) {
    diagnostics.push(
        Diagnostic::new(DiagnosticCode::TypeNumericOutOfRange, span, message)
            .with_source_id(source_id),
    );
}

fn unavailable(
    diagnostics: &mut Vec<Diagnostic>,
    source_id: &str,
    span: ByteSpan,
    message: impl Into<String>,
) {
    diagnostics.push(
        Diagnostic::new(DiagnosticCode::ToolUnavailable, span, message)
            .with_source_id(source_id),
    );
}

#[cfg(test)]
mod tests {
    use super::{
        BOOTSTRAP_TEXT_SOURCE_ID, check_bootstrap_source, check_bootstrap_text_import,
        check_source, verify_bootstrap,
    };
    use std::path::Path;
    use vibra_diagnostics::{ByteSpan, DiagnosticCode};

    #[test]
    fn checks_an_unsuffixed_integer_against_the_written_result() {
        let result = check_source("answer.vib", "(defn answer () i32 42)");
        assert!(result.accepted(), "{:?}", result.diagnostics());
        assert_eq!(
            result
                .program()
                .expect("program")
                .entry()
                .body()
                .result_type(),
            vibra_ir::PrimitiveType::I32
        );
    }

    #[test]
    fn rejects_an_integer_that_exceeds_its_suffix() {
        let result = check_source("answer.vib", "(defn answer () i8 128i8)");
        assert!(!result.accepted());
        assert!(result.diagnostics().iter().any(
            |diagnostic| diagnostic.code() == DiagnosticCode::TypeNumericOutOfRange
        ));
    }

    #[test]
    fn rejects_wrong_result_without_lowering() {
        let result = check_source("answer.vib", "(defn answer () str 1i32)");
        assert!(!result.accepted());
        assert!(result.program().is_none());
        assert!(result.diagnostics().iter().any(
            |diagnostic| diagnostic.code() == DiagnosticCode::TypeArgumentMismatch
        ));
    }

    #[test]
    fn preserves_direct_f32_rounding_without_a_f64_intermediate() {
        let result = check_source(
            "rounding.vib",
            "(defn answer () f32 1.00000011920928955078125)",
        );
        let value = result
            .program()
            .expect("checked rounding program")
            .entry()
            .body()
            .expressions()
            .first()
            .expect("expression")
            .literal_value()
            .expect("literal")
            .as_f32()
            .expect("f32 value");
        assert_eq!(value.to_bits(), 1.0000001_f32.to_bits());
    }

    #[test]
    fn rejects_negative_unsigned_and_float_overflow_before_lowering() {
        for source in ["(defn answer () u8 -1)", "(defn answer () f32 1e+39f32)"] {
            let result = check_source("range.vib", source);
            assert!(!result.accepted(), "{source}");
            assert!(result.program().is_none());
            assert!(result.diagnostics().iter().any(|diagnostic| {
                diagnostic.code() == DiagnosticCode::TypeNumericOutOfRange
            }));
        }
    }

    #[test]
    fn typed_module_values_enter_the_checked_program_boundary() {
        let result =
            check_source("deferred.vib", "(def value i32 1)\n(defn answer () i32 2)");
        assert!(result.accepted(), "{:?}", result.diagnostics());
        assert_eq!(result.program().expect("program").globals().len(), 1);
    }

    #[test]
    fn checks_bindings_conditionals_and_fixed_calls() {
        let result = check_source(
            "bindings.vib",
            "(def base i32 40)\n(defn answer () i32 (add base))\n(defn add (value i32) i32 (let next (do 1i8 2i8) (if true (do value 2i32) 0i32)))",
        );
        assert!(result.accepted(), "{:?}", result.diagnostics());
        let program = result.program().expect("program");
        assert_eq!(program.functions().len(), 2);
        assert!(matches!(
            program.entry().body().expressions().first(),
            Some(vibra_ir::Expr::Call { .. })
        ));
    }

    #[test]
    fn rejects_initializer_cycles_before_execution() {
        let result = check_source(
            "cycle.vib",
            "(def first i32 second)\n(def second i32 first)\n(defn answer () i32 first)",
        );
        assert!(!result.accepted());
        assert!(result.program().is_none());
        assert!(result.diagnostics().iter().any(|diagnostic| {
            diagnostic.code() == DiagnosticCode::TypeInitializerCycle
        }));
    }

    #[test]
    fn rejects_shadowing_and_non_boolean_condition() {
        let result = check_source(
            "scope.vib",
            "(def value i32 1)\n(defn answer (value i32) i32 (let value value (if value 1i32 2i32)))",
        );
        assert!(!result.accepted());
        assert!(result.program().is_none());
        assert!(result.diagnostics().iter().any(|diagnostic| {
            diagnostic.code() == DiagnosticCode::NameRedeclaration
        }));
        let condition = check_source(
            "condition.vib",
            "(defn answer (value i32) i32 (if value 1i32 2i32))",
        );
        assert!(condition.diagnostics().iter().any(|diagnostic| {
            diagnostic.code() == DiagnosticCode::TypeArgumentMismatch
        }));
    }

    #[test]
    fn local_shadowing_reports_binder_spans() {
        let source =
            "(defn answer (value i32) i32 (let first value (let first 1i32 first)))";
        let result = check_source("scope.vib", source);
        let diagnostic = result
            .diagnostics()
            .iter()
            .find(|diagnostic| diagnostic.code() == DiagnosticCode::NameRedeclaration)
            .expect("local redeclaration");
        assert_eq!(
            diagnostic.primary_span(),
            vibra_diagnostics::ByteSpan::new(51, 56)
        );
        assert_eq!(diagnostic.related().len(), 1);
        assert_eq!(
            diagnostic.related()[0].span,
            vibra_diagnostics::ByteSpan::new(34, 39)
        );
    }

    #[test]
    fn indirect_initializer_cycles_follow_fixed_calls() {
        let result = check_source(
            "indirect-cycle.vib",
            "(def first i32 (read))\n(def second i32 first)\n(defn read () i32 second)\n(defn answer () i32 first)",
        );
        assert!(!result.accepted());
        assert!(result.diagnostics().iter().any(|diagnostic| {
            diagnostic.code() == DiagnosticCode::TypeInitializerCycle
        }));
    }

    #[test]
    fn ordinary_source_cannot_authorize_a_compiler_external() {
        let source = r#"(defn length (value str) u64
  external: @compiler
  symbol: "text.length")"#;
        let checked = check_source("copied.vib", source);
        assert!(!checked.accepted());
        assert!(checked.program().is_none());
        assert!(checked.diagnostics().iter().any(|diagnostic| {
            diagnostic.code() == DiagnosticCode::ToolUnavailable
        }));
    }

    #[test]
    fn the_exact_bootstrap_text_module_admits_only_closed_intrinsics() {
        let source = include_str!("../../../stdlib/m2/src/std/text.vib");
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let verification = verify_bootstrap(repository).expect("bootstrap provenance");
        let checked =
            check_bootstrap_source(&verification, BOOTSTRAP_TEXT_SOURCE_ID, source);
        assert!(checked.accepted(), "{:?}", checked.diagnostics());
        let program = checked.program().expect("bootstrap program");
        assert!(program.canonical_vibon().contains("text.length"));
    }

    #[test]
    fn signed_bootstrap_verifies_exact_bytes_and_ed25519_signature() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let verification =
            verify_bootstrap(repository).expect("checked-in bootstrap provenance");
        assert_eq!(
            verification.artifact(),
            include_bytes!("../../../stdlib/m2/bootstrap.vibon")
        );
    }

    #[test]
    fn bootstrap_rejects_tampered_declared_module_bytes() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        // macOS exposes the temporary directory through `/var`, a symlink to
        // `/private/var`.  Canonicalize the parent so the verifier can inspect
        // this fixture without mistaking the ambient path for a fixture link.
        let temporary_directory =
            std::fs::canonicalize(std::env::temp_dir()).expect("temporary directory");
        let root = temporary_directory
            .join(format!("vibra-bootstrap-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for relative in [
            "stdlib/m2/bootstrap-manifest.vibon",
            "stdlib/m2/bootstrap.vibon",
            "stdlib/m2/bootstrap.vibon.sig",
            "stdlib/m2/toolchain-ed25519.pub",
            "stdlib/m2/src/std/text.vib",
            "stdlib/m2/src/std/assert.vib",
        ] {
            let destination = root.join(relative);
            std::fs::create_dir_all(destination.parent().expect("module parent"))
                .expect("module directory");
            std::fs::copy(repository.join(relative), &destination)
                .expect("module copy");
        }
        std::fs::write(
            root.join("stdlib/m2/src/std/text.vib"),
            b"; modified trusted module\n",
        )
        .expect("tamper module");
        let error = verify_bootstrap(&root).expect_err("tampered module");
        assert!(error.to_string().contains("text module digest mismatch"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn admits_mutual_recursive_calls_and_marks_tail_transfers() {
        let result = check_source(
            "recursive.vib",
            "(defn first () i32 (second))\n(defn second () i32 (first))\n(defn answer () i32 (first))",
        );
        assert!(result.accepted(), "{:?}", result.diagnostics());
        let program = result.program().expect("recursive program");
        assert_eq!(
            program.recursive_groups(),
            &[vec![0, 1], vec![0, 1], vec![0, 1, 2]]
        );
        assert!(program.canonical_vibon().contains("tail: true"));
    }

    #[test]
    fn imports_do_not_cross_the_source_only_checked_boundary() {
        let result =
            check_source("imports.vib", "(import io @std.io)\n(defn answer () i32 2)");
        assert!(!result.accepted());
        assert!(result.program().is_none());
        assert!(result.diagnostics().iter().any(|diagnostic| {
            diagnostic.code() == DiagnosticCode::ToolUnavailable
        }));
    }

    #[test]
    fn verified_text_import_lowers_qualified_calls_through_checked_ir() {
        let source = r#"(import text @std.text)
(defn answer () u64
  (text.length (text.concat "A😀" "")))"#;
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let verification = verify_bootstrap(repository).expect("bootstrap provenance");
        let checked =
            check_bootstrap_text_import(&verification, "app/main.vib", source);
        assert!(checked.accepted(), "{:?}", checked.diagnostics());
        let program = checked.program().expect("checked imported program");
        assert!(program.canonical_vibon().contains("text.concat"));
        assert!(program.canonical_vibon().contains("text.length"));
        assert!(!program.canonical_vibon().contains("tail: true"));
    }

    #[test]
    fn text_import_rejects_alias_target_and_extra_imports() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let verification = verify_bootstrap(repository).expect("bootstrap provenance");
        for source in [
            "(import wrong @std.text)\n(defn answer () u64 1u64)",
            "(import text @std.assert)\n(defn answer () u64 1u64)",
            "(import text @std.text)\n(import assert @std.assert)\n(defn answer () u64 1u64)",
        ] {
            let checked =
                check_bootstrap_text_import(&verification, "app/main.vib", source);
            assert!(!checked.accepted());
            assert!(checked.program().is_none());
            assert!(
                checked.diagnostics().iter().any(|diagnostic| {
                    diagnostic.code() == DiagnosticCode::ToolUnavailable
                }),
                "{:?}",
                checked.diagnostics()
            );
        }
    }

    #[test]
    fn trusted_text_alias_collisions_report_the_import_span() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let verification = verify_bootstrap(repository).expect("bootstrap provenance");
        let module = check_bootstrap_text_import(
            &verification,
            "app/main.vib",
            "(import text @std.text)\n(def text str \"shadow\")\n(defn answer () str text)",
        );
        assert!(!module.accepted());
        let diagnostic = module
            .diagnostics()
            .iter()
            .find(|diagnostic| diagnostic.code() == DiagnosticCode::NameRedeclaration)
            .expect("module alias collision");
        assert_eq!(diagnostic.related().len(), 1);
        assert_eq!(diagnostic.related()[0].span, ByteSpan::new(0, 23));

        let local = check_bootstrap_text_import(
            &verification,
            "app/main.vib",
            "(import text @std.text)\n(defn answer () u64 (let text 1u64 text))",
        );
        assert!(!local.accepted());
        let diagnostic = local
            .diagnostics()
            .iter()
            .find(|diagnostic| diagnostic.code() == DiagnosticCode::NameRedeclaration)
            .expect("local alias collision");
        assert_eq!(diagnostic.related().len(), 1);
        assert_eq!(diagnostic.related()[0].span, ByteSpan::new(0, 23));
    }

    #[test]
    fn trusted_checker_rejects_unknown_external_provider_symbol_and_signature() {
        for source in [
            r#"(defn read () str
  external: @host
  symbol: "text.concat")"#,
            r#"(defn read () str
  external: @compiler
  symbol: "text.unknown")"#,
            r#"(defn read (value str) str
  external: @compiler
  symbol: "text.length")"#,
        ] {
            let document = vibra_syntax::parse_source(Path::new("trusted.vib"), source)
                .expect("parse trusted boundary source");
            let mut diagnostics = document.diagnostics().to_vec();
            let ast = document.ast().expect("trusted boundary AST");
            let _ = super::check_ast_with_bindings_authority(
                "trusted.vib",
                ast,
                &mut diagnostics,
                true,
            );
            assert!(diagnostics.iter().any(|diagnostic| {
                diagnostic.code() == DiagnosticCode::ExternalUnknownSymbol
                    || diagnostic.code() == DiagnosticCode::TypeArgumentMismatch
            }));
        }
    }

    #[test]
    fn trusted_text_import_rejects_source_body_and_effect_external_declarations() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let verification = verify_bootstrap(repository).expect("bootstrap provenance");
        for source in [
            r#"(import text @std.text)
(defn answer () str "spoof"
  external: @compiler
  symbol: "text.concat")"#,
            r#"(import text @std.text)
(defn answer () str
  effects: (@std.io)
  "spoof")"#,
        ] {
            let checked =
                check_bootstrap_text_import(&verification, "app/main.vib", source);
            assert!(!checked.accepted());
            assert!(checked.program().is_none());
            assert!(
                checked.diagnostics().iter().any(|diagnostic| {
                    diagnostic.code() == DiagnosticCode::ToolUnavailable
                        || diagnostic.code() == DiagnosticCode::SyntaxInvalidAttribute
                        || diagnostic.code() == DiagnosticCode::EffectInvalidReference
                }),
                "{:?}",
                checked.diagnostics()
            );
        }
    }
}

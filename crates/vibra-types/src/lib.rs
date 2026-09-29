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
use std::path::Path;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::{
    CheckedFunction, CheckedGlobal, CheckedProgram, Expr, FunctionSignature, IrError,
    LabelledParameter as IrLabelledParameter, SourceOrigin, TestAssertion, Type, Value,
    external::CompilerIntrinsic,
};
use vibra_syntax::{
    Application, ApplicationBinding, Attribute, BindingFacts, CallArgument,
    Declaration, Expression, ExpressionKind, FloatSuffix, IntegerSuffix, Literal,
    NameKind, PatternKind, SourceAst, TypeExpr, TypeMember,
};

mod construct;
mod infer;
mod nominal;
mod resolved;
mod standard;
mod stdlib;

pub use resolved::{ResolvedCheckResult, check_resolved};
pub use standard::builtin_member_names;
pub use stdlib::{
    STDLIB_ASSERT_SOURCE_ID, STDLIB_BUILTIN_SOURCE_ID, STDLIB_OPTION_SOURCE_ID,
    STDLIB_TEXT_SOURCE_ID, Stdlib, StdlibError, StdlibInputs, StdlibModule,
    load_stdlib, load_stdlib_bytes,
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
    verification: &Stdlib,
    source_id: impl AsRef<str>,
    source: &str,
) -> CheckResult {
    let source_id = source_id.as_ref();
    if source_id != STDLIB_TEXT_SOURCE_ID
        || verification
            .module("std.text")
            .map(stdlib::StdlibModule::bytes)
            != Some(source.as_bytes())
        || !verification.maps("std.text", STDLIB_TEXT_SOURCE_ID)
    {
        let diagnostic = Diagnostic::new(
            DiagnosticCode::ToolUnavailable,
            ByteSpan::empty_at(0),
            "compiler externals require the embedded standard library",
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
/// import.  This adapter is capability-backed by [`Stdlib`]; a
/// source file cannot manufacture that authority by copying declarations.
/// Only the exact signed `@std.text` map entry is exposed, and source external
/// declarations remain forbidden.
pub fn check_bootstrap_text_import(
    verification: &Stdlib,
    source_id: impl AsRef<str>,
    source: &str,
) -> CheckResult {
    let source_id = source_id.as_ref();
    let document = match vibra_syntax::parse_source(Path::new(source_id), source) {
        Ok(document) => document,
        Err(error) => {
            return bootstrap_import_unavailable(
                source_id,
                ByteSpan::empty_at(0),
                error.to_string(),
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
        return bootstrap_import_unavailable(
            source_id,
            ByteSpan::empty_at(0),
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
            ByteSpan::empty_at(0),
            "the source must explicitly import `(import text @std.text)`",
        );
    };
    let exact_import = import.alias().kind() == NameKind::Symbol
        && import.alias().value() == "text"
        && import.target().kind() == NameKind::Atom
        && import.target().value() == "std.text";
    if imports.len() != 1 || !exact_import {
        let rejected_import = if exact_import {
            imports.get(1).copied().unwrap_or(import)
        } else {
            import
        };
        return bootstrap_import_unavailable(
            source_id,
            rejected_import.span(),
            "only the exact `(import text @std.text)` import is available",
        );
    }
    if !verification.maps("std.text", STDLIB_TEXT_SOURCE_ID) {
        return bootstrap_import_unavailable(
            source_id,
            import.span(),
            "the @std.text import requires the embedded standard library",
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
        let source_external = ast.declarations().iter().find_map(|declaration| {
            if let Declaration::Defn(function) = declaration
                && function
                    .attributes()
                    .items()
                    .iter()
                    .any(|attribute| matches!(attribute, Attribute::External(_)))
            {
                Some(function.span())
            } else {
                None
            }
        });
        return bootstrap_import_unavailable(
            source_id,
            source_external.unwrap_or(import.span()),
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
            ast.declarations()
                .first()
                .map_or(import.span(), Declaration::span),
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

/// The embedded module source ID a single-source import of a
/// standard-library type module names, such as `(import option @std.option)`.
fn standard_type_import(
    import: &vibra_syntax::ImportDeclaration,
) -> Option<&'static str> {
    (import.alias().kind() == NameKind::Symbol
        && import.target().kind() == NameKind::Atom
        && import.target().value() == "std.option")
        .then_some(STDLIB_OPTION_SOURCE_ID)
}

fn bootstrap_import_unavailable(
    source_id: &str,
    span: ByteSpan,
    message: impl Into<String>,
) -> CheckResult {
    let diagnostic = Diagnostic::new(DiagnosticCode::ToolUnavailable, span, message)
        .with_source_id(source_id);
    CheckResult::new(None, vec![diagnostic])
}

fn check_ast_with_bindings_authority(
    source_id: &str,
    ast: &SourceAst,
    diagnostics: &mut Vec<Diagnostic>,
    trusted_bootstrap: bool,
) -> (Option<CheckedProgram>, Vec<ApplicationBinding>) {
    let mut checker = Checker::new(source_id, diagnostics, ast, trusted_bootstrap);
    checker.collect_headers();
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
    checker.check_globals();
    checker.check_functions();
    let program = checker.finish();
    (program, checker.bindings)
}

#[derive(Clone)]
struct GlobalHeader {
    name: String,
    value_type: Type,
    expression: Expression,
    span: ByteSpan,
    source_id: String,
    module_index: usize,
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
    module_index: usize,
    source_id: String,
    name: String,
    signature: FunctionSignature,
    external: Option<CompilerIntrinsic>,
    external_declared: bool,
    test: Option<vibra_syntax::TestDeclaration>,
    test_assertion: Option<TestAssertion>,
    /// For a nested method, its index among the owning `deftype`'s members;
    /// `declaration_index` then names the `deftype`.
    member_index: Option<usize>,
    /// The receiver type of a nested method.
    self_type: Option<Type>,
    /// The complete generic parameter list: the owner's, then the function's.
    type_parameters: Vec<String>,
}

pub(crate) const IMPORTED_FUNCTION_DECLARATION: usize = usize::MAX;

#[derive(Clone)]
struct LocalBinding {
    slot: usize,
    value_type: Type,
    span: ByteSpan,
    function_targets: Option<FunctionTargetSet>,
    /// The quantified `where:` names of a bound generic `lambda`, in order.
    generics: Vec<String>,
}

struct Checker<'a> {
    source_id: &'a str,
    diagnostics: &'a mut Vec<Diagnostic>,
    ast: &'a SourceAst,
    types: nominal::TypeNames,
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
            types: nominal::TypeNames::default(),
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
        }
    }

    fn collect_headers(&mut self) {
        // Standard-library type modules first, so declared types may name
        // their types: `(import option @std.option)` sees the embedded module.
        for declaration in self.ast.declarations() {
            if let Declaration::Import(import) = declaration
                && let Some(target) = standard_type_import(import)
            {
                let alias = import.alias().value();
                if let Some(earlier) = self.module_names.get(alias).copied() {
                    redeclaration(
                        self.diagnostics,
                        self.source_id,
                        alias,
                        alias,
                        import.span(),
                        earlier,
                    );
                    continue;
                }
                self.module_names.insert(alias.to_owned(), import.span());
                self.types.import(self.source_id, alias, target);
            }
        }
        // Declared types first: any signature below may name one. A single
        // source has no package, so a type's identity and path are its name.
        let mut type_declarations = Vec::new();
        for declaration in self.ast.declarations() {
            if let Declaration::Deftype(value) = declaration {
                let name = value.name().value();
                if let Some(earlier) = self.module_names.get(name).copied() {
                    redeclaration(
                        self.diagnostics,
                        self.source_id,
                        name,
                        name,
                        value.span(),
                        earlier,
                    );
                    continue;
                }
                self.module_names.insert(name.to_owned(), value.span());
                let index = self.types.declare(
                    self.source_id,
                    value,
                    vibra_ir::TypeId::new(format!("{}:{name}", self.source_id), name),
                );
                type_declarations.push((index, value));
            }
        }
        standard::declare_standard_types(&mut self.types, &mut type_declarations);
        self.types
            .lower_bodies(&type_declarations, self.diagnostics);
        for (declaration_index, declaration) in
            self.ast.declarations().iter().enumerate()
        {
            match declaration {
                Declaration::Deftype(value) => {
                    let Some(type_index) = self
                        .types
                        .declared()
                        .iter()
                        .position(|declared| declared.span == value.span())
                    else {
                        continue;
                    };
                    let Some((self_type, owner_generics)) =
                        self.types.get(type_index).map(|declared| {
                            (
                                nominal::declared_self_type(
                                    &declared.id,
                                    &declared.parameters,
                                ),
                                declared.parameters.clone(),
                            )
                        })
                    else {
                        continue;
                    };
                    for (member_index, member) in value.members().iter().enumerate() {
                        let TypeMember::Method(method) = member else {
                            continue;
                        };
                        let Some(generics) = nominal::function_generics(
                            &owner_generics,
                            method,
                            self.source_id,
                            self.diagnostics,
                        ) else {
                            continue;
                        };
                        let Some(signature) = check_signature(
                            self.source_id,
                            method,
                            self.diagnostics,
                            &self.types,
                            nominal::Scope::new(Some(&self_type), &generics),
                        ) else {
                            continue;
                        };
                        let name = format!(
                            "{}.{}",
                            value.name().value(),
                            method.name().value()
                        );
                        let index = self.functions.len();
                        self.function_indices.insert(name.clone(), index);
                        self.functions.push(FunctionHeader {
                            declaration_index,
                            module_index: 0,
                            source_id: self.source_id.to_owned(),
                            name,
                            signature,
                            external: None,
                            external_declared: false,
                            test: None,
                            test_assertion: None,
                            member_index: Some(member_index),
                            self_type: Some(self_type.clone()),
                            type_parameters: generics,
                        });
                    }
                }
                Declaration::Def(definition) => {
                    let Some(value_type) = self.types.lower_or_report(
                        self.source_id,
                        nominal::Scope::NONE,
                        definition.value_type(),
                        definition.span(),
                        self.diagnostics,
                    ) else {
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
                        source_id: self.source_id.to_owned(),
                        module_index: 0,
                        function_index: None,
                        function_targets: FunctionTargetSet::default(),
                    });
                }
                Declaration::Defn(function) => {
                    let Some(generics) = nominal::function_generics(
                        &[],
                        function,
                        self.source_id,
                        self.diagnostics,
                    ) else {
                        continue;
                    };
                    let Some(signature) = check_signature(
                        self.source_id,
                        function,
                        self.diagnostics,
                        &self.types,
                        nominal::Scope::new(None, &generics),
                    ) else {
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
                        module_index: 0,
                        source_id: self.source_id.to_owned(),
                        name,
                        external: compiler_intrinsic(
                            self.source_id,
                            function,
                            self.diagnostics,
                            self.trusted_bootstrap,
                            &signature,
                        ),
                        signature,
                        external_declared: function.attributes().items().iter().any(
                            |attribute| matches!(attribute, Attribute::External(_)),
                        ),
                        test: None,
                        test_assertion: None,
                        member_index: None,
                        self_type: None,
                        type_parameters: generics,
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
                // Registered with the types above.
                Declaration::Import(import)
                    if standard_type_import(import).is_some() => {}
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
                    module_index: 0,
                    source_id: self.source_id.to_owned(),
                    name: name.to_owned(),
                    signature: intrinsic.signature(),
                    external: Some(intrinsic),
                    external_declared: true,
                    test: None,
                    test_assertion: None,
                    member_index: None,
                    self_type: None,
                    type_parameters: Vec::new(),
                });
            }
            self.text_import_span = Some(import_span);
        }
        // Builtin static methods such as `array.of` are reached through the
        // type path with no import; only the members this module names join
        // its program.
        let paths = dotted_value_paths(self.ast);
        for member in standard::builtin_members(&self.types) {
            let path = member.path();
            if !paths.contains(&path) || self.function_indices.contains_key(&path) {
                continue;
            }
            let index = self.functions.len();
            self.function_indices.insert(path.clone(), index);
            self.functions.push(FunctionHeader {
                declaration_index: IMPORTED_FUNCTION_DECLARATION,
                module_index: 0,
                source_id: STDLIB_BUILTIN_SOURCE_ID.to_owned(),
                name: path,
                signature: member.signature,
                external: Some(member.intrinsic),
                external_declared: true,
                test: None,
                test_assertion: None,
                member_index: None,
                self_type: None,
                type_parameters: member.type_parameters,
            });
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
                &self.types,
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
            let Some(function) = header_function(self.ast.declarations(), &header)
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
                &self.types,
            );
            environment.self_type = header.self_type.clone();
            environment.generics = header.type_parameters.clone();
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
                            parameter.parsed_pattern().span(),
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
                        entry.name_span(),
                    ) {
                        parameters_valid = false;
                    }
                }
            }
            if !environment
                .bind_variadic(function.attributes().items(), &header.signature)
            {
                parameters_valid = false;
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
        let global_origins = globals
            .iter()
            .map(|global| global.origin().clone())
            .collect::<Vec<_>>();
        match CheckedProgram::try_new_with_types(
            self.types.definitions(),
            globals,
            functions,
            0,
        ) {
            Err(IrError::GlobalInitializerCycle(index)) => {
                if let Some(origin) = global_origins.get(index) {
                    self.diagnostics.push(initializer_cycle_diagnostic(origin));
                }
                None
            }
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

/// The `defn` a header was collected from: a top-level function or a nested
/// method of a `deftype`.
pub(crate) fn header_function<'a>(
    declarations: &'a [Declaration],
    header: &FunctionHeader,
) -> Option<&'a vibra_syntax::FunctionDeclaration> {
    match (
        declarations.get(header.declaration_index)?,
        header.member_index,
    ) {
        (Declaration::Defn(function), None) => Some(function),
        (Declaration::Deftype(value), Some(member)) => {
            match value.members().get(member)? {
                TypeMember::Method(function) => Some(function),
                TypeMember::Implementation(_) => None,
            }
        }
        _ => None,
    }
}

/// The diagnostic for a checked-IR initializer cycle through the global
/// declared at `global`.
pub(crate) fn initializer_cycle_diagnostic(global: &SourceOrigin) -> Diagnostic {
    Diagnostic::new(
        DiagnosticCode::TypeInitializerCycle,
        global.span(),
        "module value initializers form a cycle",
    )
    .with_source_id(global.source_id())
}

struct CheckEnvironment<'a> {
    source_id: &'a str,
    diagnostics: &'a mut Vec<Diagnostic>,
    /// Declared types and the names each module sees.
    types: &'a nominal::TypeNames,
    /// The receiver type inside a nested method.
    self_type: Option<Type>,
    /// The generic names in scope: the owner's and the function's own.
    generics: Vec<String>,
    /// Whether the expression being checked is an application's callee, where a
    /// generic function is instantiated by the application rather than here.
    callee_position: bool,
    /// Whether the expression being checked is a `let` value, where a generic
    /// `lambda` stays generic instead of being instantiated.
    keeps_generic: bool,
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
    resolved_targets:
        Option<&'a BTreeMap<(String, usize, usize), ResolvedReferenceTarget>>,
    /// Whether this checker reports lexical `@name.redeclaration`. The
    /// workspace path leaves that to the resolver, which owns name
    /// introduction there, so each introduction is reported exactly once.
    reports_redeclarations: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResolvedReferenceTarget {
    Global(usize),
    Function(usize),
    Unresolved,
}

#[derive(Clone)]
struct CaptureBinding {
    slot: usize,
    value_type: Type,
    span: ByteSpan,
    function_targets: Option<FunctionTargetSet>,
    generics: Vec<String>,
}

#[derive(Clone)]
enum VisibleStorage {
    Activation {
        slot: usize,
        value_type: Type,
        span: ByteSpan,
        function_targets: Option<FunctionTargetSet>,
        generics: Vec<String>,
    },
    Closure {
        slot: usize,
        value_type: Type,
        span: ByteSpan,
        function_targets: Option<FunctionTargetSet>,
        generics: Vec<String>,
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
        types: &'a nominal::TypeNames,
    ) -> Self {
        Self {
            source_id,
            diagnostics,
            types,
            self_type: None,
            generics: Vec::new(),
            callee_position: false,
            keeps_generic: false,
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
            resolved_targets: None,
            reports_redeclarations: true,
        }
    }

    /// Binds a parameter: `parameter_span` locates an unavailable type and
    /// `binder_span` is the introduced name.
    fn add_binding(
        &mut self,
        name: &str,
        value_type: &TypeExpr,
        parameter_span: ByteSpan,
        binder_span: ByteSpan,
    ) -> bool {
        let Some(value_type) = self.types.lower_or_report(
            self.source_id,
            nominal::Scope::new(self.self_type.as_ref(), &self.generics),
            value_type,
            parameter_span,
            self.diagnostics,
        ) else {
            return false;
        };
        self.add_binding_type(name, value_type, binder_span)
    }

    fn add_binding_type(
        &mut self,
        name: &str,
        value_type: Type,
        span: ByteSpan,
    ) -> bool {
        self.add_binding_type_with_function(name, value_type, span, None)
    }

    fn add_binding_type_with_function(
        &mut self,
        name: &str,
        value_type: Type,
        span: ByteSpan,
        function_index: Option<usize>,
    ) -> bool {
        let function_targets = if matches!(&value_type, Type::Function(_)) {
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
        value_type: Type,
        span: ByteSpan,
        function_targets: Option<FunctionTargetSet>,
    ) -> bool {
        // A repeated introduction relates the nearest earlier one: the
        // innermost lexical binding, else the module binding. The shadowing
        // binder is still bound so the rest of the scope keeps checking and
        // every later introduction is reported, as the resolver does.
        if self.reports_redeclarations
            && let Some(earlier) = self.earlier_introduction(name)
        {
            redeclaration(self.diagnostics, self.source_id, name, name, span, earlier);
        }
        let slot = self.next_slot;
        self.locals.insert(
            name.to_owned(),
            LocalBinding {
                slot,
                value_type,
                span,
                function_targets,
                generics: Vec::new(),
            },
        );
        self.next_slot = self.next_slot.saturating_add(1);
        true
    }

    /// Binds a variadic tail parameter to the slot after the labelled ones.
    /// A discarded tail still occupies its slot.
    fn bind_variadic(
        &mut self,
        attributes: &[Attribute],
        signature: &FunctionSignature,
    ) -> bool {
        let Some(tail) = signature.variadic() else {
            return true;
        };
        let Some(parameter) = attributes.iter().find_map(|attribute| match attribute {
            Attribute::Variadic(parameter) => Some(parameter),
            _ => None,
        }) else {
            return true;
        };
        if parameter.name().is_discard() {
            self.next_slot = self.next_slot.saturating_add(1);
            return true;
        }
        self.add_binding_type(parameter.name().value(), tail.clone(), parameter.span())
    }

    fn earlier_introduction(&self, name: &str) -> Option<ByteSpan> {
        if let Some(earlier) = self.locals.get(name) {
            return Some(earlier.span);
        }
        if let Some(earlier) = self.captures.get(name) {
            return Some(earlier.span);
        }
        if let Some(storage) =
            self.outer.as_ref().and_then(|outer| outer.values.get(name))
        {
            return Some(match storage {
                VisibleStorage::Activation { span, .. }
                | VisibleStorage::Closure { span, .. } => *span,
            });
        }
        self.module_names.get(name).copied()
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
                    function_targets: binding.function_targets.clone(),
                    generics: binding.generics.clone(),
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
                    function_targets: binding.function_targets.clone(),
                    generics: binding.generics.clone(),
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
        let (value_type, function_targets, generics, introduction, source) =
            match storage {
                VisibleStorage::Activation {
                    slot,
                    value_type,
                    span: introduction,
                    function_targets,
                    generics,
                } => (
                    value_type.clone(),
                    function_targets.clone(),
                    generics,
                    introduction,
                    Expr::variable(
                        slot,
                        value_type,
                        SourceOrigin::new(self.source_id, span),
                    ),
                ),
                VisibleStorage::Closure {
                    slot,
                    value_type,
                    span: introduction,
                    function_targets,
                    generics,
                } => (
                    value_type.clone(),
                    function_targets.clone(),
                    generics,
                    introduction,
                    Expr::captured(
                        slot,
                        value_type,
                        SourceOrigin::new(self.source_id, span),
                    ),
                ),
            };
        self.capture_sources.push(source);
        // The capture keeps the outer binder's introduction span so a
        // redeclaration relates the binder, not the capturing lambda.
        let binding = CaptureBinding {
            slot,
            value_type,
            span: introduction,
            function_targets,
            generics,
        };
        self.captures.insert(name.to_owned(), binding.clone());
        Some(binding)
    }
}

fn types_match(left: &Type, right: &Type) -> bool {
    left.same_shape(right)
}

/// Visits `expression` and every nested expression in pre-order.
///
/// This is the one syntax traversal the checker needs before lowering;
/// dependency, cycle, and recursive-group analysis happen once, over checked
/// IR, in `vibra-ir`.
fn walk_expressions<'e>(
    expression: &'e Expression,
    visit: &mut impl FnMut(&'e Expression),
) {
    let mut pending = vec![expression];
    while let Some(expression) = pending.pop() {
        visit(expression);
        let first_child = pending.len();
        match expression.kind() {
            ExpressionKind::Name(_) | ExpressionKind::Literal(_) => {}
            ExpressionKind::Application(application) => {
                pending.push(application.callee());
                pending.extend(
                    application
                        .arguments()
                        .iter()
                        .map(|argument| argument.value()),
                );
            }
            ExpressionKind::Do(expressions) => pending.extend(expressions),
            ExpressionKind::Let { value, body, .. } => {
                pending.push(value);
                pending.extend(body);
            }
            ExpressionKind::If {
                condition,
                then_branch,
                else_branch,
            } => pending.extend([&**condition, &**then_branch, &**else_branch]),
            ExpressionKind::Lambda(lambda) => pending.extend(lambda.body()),
            ExpressionKind::Match { scrutinee, arms } => {
                pending.push(scrutinee);
                pending.extend(arms.iter().map(|arm| arm.result()));
            }
            ExpressionKind::As { operand, .. } | ExpressionKind::Try(operand) => {
                pending.push(operand);
            }
            ExpressionKind::TupleOf(values) => pending.extend(values),
            ExpressionKind::RecordOf(fields) => {
                pending.extend(fields.iter().map(|field| field.value()));
            }
            ExpressionKind::EnumOf(variant) => pending.push(variant.value()),
        }
        // Children were pushed in source order; reverse them so they pop in
        // source order and the visit stays pre-order.
        if let Some(children) = pending.get_mut(first_child..) {
            children.reverse();
        }
    }
}

/// Distinct single-segment symbol names used anywhere in `expression`, in
/// first-use order.
fn collect_value_names(expression: &Expression, names: &mut Vec<String>) {
    walk_expressions(expression, &mut |expression| {
        if let ExpressionKind::Name(name) = expression.kind()
            && name.kind() == NameKind::Symbol
            && name.segments().len() == 1
            && !names.iter().any(|known| known == name.value())
        {
            names.push(name.value().to_owned());
        }
    });
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
        Expr::Record { .. }
        | Expr::Variant { .. }
        | Expr::Wrap { .. }
        | Expr::Tuple { .. }
        | Expr::Array { .. }
        | Expr::Map { .. }
        | Expr::Lookup { .. } => FunctionTargetSet::default(),
        Expr::Project { value_type, .. } | Expr::TupleProject { value_type, .. } => {
            if matches!(value_type, Type::Function(_)) {
                FunctionTargetSet::unknown()
            } else {
                FunctionTargetSet::default()
            }
        }
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
                if matches!(value_type, Type::Function(_)) {
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
                if matches!(value_type, Type::Function(_)) {
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
        Expr::Captured {
            slot, value_type, ..
        } => environment
            .captures
            .values()
            .find(|binding| binding.slot == *slot)
            .and_then(|binding| binding.function_targets.clone())
            .unwrap_or_else(|| {
                if matches!(value_type, Type::Function(_)) {
                    FunctionTargetSet::unknown()
                } else {
                    FunctionTargetSet::default()
                }
            }),
        // A call's result may itself be a function value.  The checker does
        // not have a recursive return-summary environment here, so preserve
        // the function-typed boundary conservatively instead of dropping it
        // to the empty set and manufacturing a singleton hint from a branch.
        Expr::Call {
            result: Type::Function(_),
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

fn ensure_expected(
    environment: &mut CheckEnvironment<'_>,
    span: ByteSpan,
    expected: Option<Type>,
    actual: Type,
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

/// A module-level function or method named as a value.
///
/// A generic function in callee position is returned uninstantiated, because
/// its application infers the arguments. Anywhere else it becomes a
/// monomorphic function value instantiated from the written expected `fn`
/// type, or `@type.ambiguous-inference` when that type does not fix every
/// argument.
fn function_value(
    environment: &mut CheckEnvironment<'_>,
    expression: &Expression,
    index: usize,
    expected: Option<Type>,
    callee_position: bool,
) -> Option<Expr> {
    let header = environment.functions.get(index)?;
    let signature = header.signature.clone();
    let parameters = header.type_parameters.clone();
    let origin = SourceOrigin::new(environment.source_id, expression.span());
    if !parameters.is_empty() && callee_position {
        return Some(Expr::function(index, signature, origin));
    }
    let signature = if parameters.is_empty() {
        signature
    } else {
        let Type::Function(instantiated) = instantiate_value(
            environment,
            expression.span(),
            &parameters,
            &Type::Function(Box::new(signature)),
            expected.as_ref(),
        )?
        else {
            return None;
        };
        *instantiated
    };
    let actual = Type::Function(Box::new(signature.clone()));
    ensure_expected(
        environment,
        expression.span(),
        expected.clone(),
        actual.clone(),
    );
    expected
        .as_ref()
        .is_none_or(|expected| types_match(expected, &actual))
        .then_some(Expr::function(index, signature, origin))
}

/// A local or captured binding named as a value.
///
/// A binding of a generic `lambda` stays generic: in callee position the
/// application instantiates it, and anywhere else it is instantiated from the
/// written expected type, like a generic function.
fn binding_value(
    environment: &mut CheckEnvironment<'_>,
    expression: &Expression,
    expected: Option<Type>,
    value_type: Type,
    generics: Vec<String>,
    callee_position: bool,
    build: impl FnOnce(Type, SourceOrigin) -> Expr,
) -> Option<Expr> {
    let parameters = generics;
    let actual = if parameters.is_empty() || callee_position {
        value_type
    } else {
        instantiate_value(
            environment,
            expression.span(),
            &parameters,
            &value_type,
            expected.as_ref(),
        )?
    };
    ensure_expected(
        environment,
        expression.span(),
        expected.clone(),
        actual.clone(),
    );
    expected
        .as_ref()
        .is_none_or(|expected| types_match(expected, &actual))
        .then(|| {
            build(
                actual,
                SourceOrigin::new(environment.source_id, expression.span()),
            )
        })
}

/// The quantified `where:` names of a generic `lambda` written at
/// `expression`, in order; empty for anything else. They match the names the
/// lambda arm substitutes, including ones its signature never mentions.
fn quantified_lambda_generics(expression: &Expression) -> Vec<String> {
    let ExpressionKind::Lambda(lambda) = expression.kind() else {
        return Vec::new();
    };
    nominal::generic_names(lambda.attributes().items())
        .iter()
        .enumerate()
        .map(|(index, name)| infer::quantified_name(name, index, lambda.span().start()))
        .collect()
}

/// The complete generic parameter list of a generic `lambda` in callee
/// position: written directly, or named through a local or captured binding.
fn callee_generics(
    environment: &CheckEnvironment<'_>,
    callee: &Expression,
) -> Vec<String> {
    if let ExpressionKind::Name(name) = callee.kind()
        && name.kind() == NameKind::Symbol
        && name.segments().len() == 1
    {
        if let Some(binding) = environment.locals.get(name.value()) {
            return binding.generics.clone();
        }
        if let Some(binding) = environment.captures.get(name.value()) {
            return binding.generics.clone();
        }
        return Vec::new();
    }
    quantified_lambda_generics(callee)
}

/// `value_type` with its generic `parameters` fixed by unification with the
/// written expected type. An expected type that contradicts the value is a
/// type mismatch; one that leaves any parameter open, including a parameter
/// the type never mentions, is `@type.ambiguous-inference`.
fn instantiate_value(
    environment: &mut CheckEnvironment<'_>,
    span: ByteSpan,
    parameters: &[String],
    value_type: &Type,
    expected: Option<&Type>,
) -> Option<Type> {
    let mut instantiation = infer::Instantiation::new(parameters);
    let opened = instantiation.open(value_type);
    if let Some(expected) = expected
        && !instantiation.unify(&opened, expected)
    {
        ensure_expected(
            environment,
            span,
            Some(expected.clone()),
            infer::display(value_type),
        );
        return None;
    }
    let unbound = instantiation.unbound();
    let resolved = instantiation
        .resolved(&opened)
        .filter(|_| unbound.is_empty());
    if resolved.is_none() {
        ambiguous_generic(environment, span, &unbound);
    }
    resolved
}

/// `@type.ambiguous-inference` for generic parameters no operand, written
/// result type, or `types:` list fixes.
pub(crate) fn ambiguous_generic(
    environment: &mut CheckEnvironment<'_>,
    span: ByteSpan,
    unbound: &[String],
) {
    // One note per missing constraint, as the inference rules require.
    let diagnostic = unbound.iter().fold(
        Diagnostic::new(
            DiagnosticCode::TypeAmbiguousInference,
            span,
            format!(
                "nothing fixes the generic argument{} {}; write `types:` or an expected type",
                if unbound.len() == 1 { "" } else { "s" },
                unbound
                    .iter()
                    .map(|name| infer::source_name(name))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ),
        |diagnostic, name| {
            diagnostic.with_note(format!(
                "no operand, written result type, or `types:` entry determines `{}`",
                infer::source_name(name)
            ))
        },
    );
    environment
        .diagnostics
        .push(diagnostic.with_source_id(environment.source_id));
}

/// Lowers an application's written `types:` list, if any. The outer `None`
/// means a type in the list failed to lower and was reported.
fn lower_type_arguments(
    environment: &mut CheckEnvironment<'_>,
    application: &Application,
) -> Option<Option<Vec<Type>>> {
    let Some(written) = application.type_arguments() else {
        return Some(None);
    };
    let span = application
        .type_arguments_span()
        .unwrap_or_else(|| application.span());
    let scope =
        nominal::Scope::new(environment.self_type.as_ref(), &environment.generics);
    written
        .iter()
        .map(|value| {
            environment.types.lower_or_report(
                environment.source_id,
                scope,
                value,
                span,
                environment.diagnostics,
            )
        })
        .collect::<Option<Vec<_>>>()
        .map(Some)
}

/// `@type.type-argument-mismatch` at an application's `types:` list.
fn type_argument_mismatch(
    environment: &mut CheckEnvironment<'_>,
    application: &Application,
    message: String,
) {
    environment.diagnostics.push(
        Diagnostic::new(
            DiagnosticCode::TypeTypeArgumentMismatch,
            application
                .type_arguments_span()
                .unwrap_or_else(|| application.span()),
            message,
        )
        .with_source_id(environment.source_id),
    );
}

/// Opens one instantiation of a generic entity at `application`: seeds it
/// from the written `types:` list and, when the written result type agrees,
/// from that. `result` is the entity's result type before opening. A
/// non-generic entity yields an empty instantiation.
pub(crate) fn start_instantiation(
    environment: &mut CheckEnvironment<'_>,
    application: &Application,
    parameters: &[String],
    result: &Type,
    type_arguments: Option<&[Type]>,
    expected: Option<&Type>,
) -> Option<infer::Instantiation> {
    let mut instantiation = infer::Instantiation::new(parameters);
    let result = instantiation.open(result);
    if let Some(arguments) = type_arguments {
        if !instantiation.is_generic() {
            type_argument_mismatch(
                environment,
                application,
                "the applied entity declares no generic parameters".to_owned(),
            );
            return None;
        }
        if arguments.len() != instantiation.len() {
            type_argument_mismatch(
                environment,
                application,
                format!(
                    "`types:` supplies {} type argument{}, but the entity takes {}",
                    arguments.len(),
                    if arguments.len() == 1 { "" } else { "s" },
                    instantiation.len()
                ),
            );
            return None;
        }
        instantiation.seed(arguments);
    }
    if let Some(expected) = expected {
        let mut probe = instantiation.clone();
        if probe.unify(&result, expected) {
            instantiation = probe;
        } else if type_arguments.is_some() && infer::has_variables(&result) {
            type_argument_mismatch(
                environment,
                application,
                format!(
                    "`types:` makes the result {}, but {expected} is expected",
                    infer::display(&instantiation.apply(&result))
                ),
            );
            return None;
        }
    }
    Some(instantiation)
}

/// Checks one operand against its opened parameter type `pattern`.
///
/// A pattern the instantiation already fixes is the operand's expected type,
/// and a mismatch against a type fixed by a written `types:` list is
/// `@type.type-argument-mismatch`. An open pattern is unified with the
/// operand's own type instead.
pub(crate) fn check_inferred_operand(
    environment: &mut CheckEnvironment<'_>,
    instantiation: &mut infer::Instantiation,
    operand: &Expression,
    pattern: &Type,
    types_written: bool,
) -> Option<Expr> {
    if let Some(fixed) = instantiation.resolved(pattern) {
        let before = environment.diagnostics.len();
        let checked = check_operand(environment, operand, Some(fixed));
        // Only an operand whose whole parameter type is a generic parameter
        // fixed by `types:` contradicts that list; a mismatch in a concrete
        // part of a parameter type stays an ordinary argument mismatch.
        let generic_parameter =
            matches!(pattern, Type::Param(name) if name.starts_with('?'));
        if types_written && generic_parameter {
            let span = operand.span();
            for diagnostic in environment.diagnostics.iter_mut().skip(before) {
                let inside = diagnostic.primary_span().start() >= span.start()
                    && diagnostic.primary_span().end() <= span.end();
                if matches!(
                    diagnostic.code(),
                    DiagnosticCode::TypeArgumentMismatch | DiagnosticCode::TypeMismatch
                ) && inside
                {
                    *diagnostic = diagnostic
                        .clone()
                        .with_code(DiagnosticCode::TypeTypeArgumentMismatch);
                }
            }
        }
        return checked;
    }
    let checked = check_operand(environment, operand, None)?;
    if !instantiation.unify(pattern, &checked.result_type()) {
        environment.diagnostics.push(
            Diagnostic::new(
                DiagnosticCode::TypeArgumentMismatch,
                operand.span(),
                format!(
                    "operand has type {}, which does not fit the parameter type {}",
                    infer::display(&checked.result_type()),
                    infer::display(&instantiation.apply(pattern))
                ),
            )
            .with_source_id(environment.source_id),
        );
        return None;
    }
    Some(checked)
}

/// The binding shape of a variadic tail type.
fn tail_binding(tail: &Type) -> vibra_syntax::VariadicBinding {
    if matches!(tail, Type::Map(_, _)) {
        vibra_syntax::VariadicBinding::Map
    } else {
        vibra_syntax::VariadicBinding::Array
    }
}

/// The expected type of each of `count` tail operands: the element type for
/// an array tail, alternating key and value types for a map tail.
fn tail_patterns(tail: &Type, count: usize) -> Vec<Type> {
    match tail {
        Type::Array(element) => vec![element.as_ref().clone(); count],
        Type::Map(key, value) => (0..count)
            .map(|index| {
                if index % 2 == 0 {
                    key.as_ref().clone()
                } else {
                    value.as_ref().clone()
                }
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Packs checked tail operands into the one argument a variadic slot takes.
fn pack_tail(tail: &Type, operands: Vec<Expr>, origin: SourceOrigin) -> Expr {
    match tail {
        Type::Map(_, _) => {
            let mut entries = Vec::with_capacity(operands.len() / 2);
            let mut operands = operands.into_iter();
            while let (Some(key), Some(value)) = (operands.next(), operands.next()) {
                entries.push((key, value));
            }
            Expr::Map {
                value_type: tail.clone(),
                entries,
                origin,
            }
        }
        _ => Expr::Array {
            value_type: tail.clone(),
            elements: operands,
            origin,
        },
    }
}

/// Reports every map type in `value_type` whose key is inadmissible, as
/// `@type.invalid-map-key` (or `@type.function-not-equatable` for a function
/// key) at the application that inferred it. Returns whether all are valid.
fn check_inferred_map_keys(
    environment: &mut CheckEnvironment<'_>,
    span: ByteSpan,
    value_type: &Type,
) -> bool {
    let mut valid = true;
    if let Type::Map(key, _) = value_type {
        let code = match nominal::map_key(key) {
            nominal::KeyVerdict::Admissible | nominal::KeyVerdict::Generic => None,
            nominal::KeyVerdict::Function => {
                Some(DiagnosticCode::TypeFunctionNotEquatable)
            }
            nominal::KeyVerdict::Invalid => Some(DiagnosticCode::TypeInvalidMapKey),
        };
        if let Some(code) = code {
            environment.diagnostics.push(
                Diagnostic::new(
                    code,
                    span,
                    format!("the inferred map key type {key} is not an admissible key"),
                )
                .with_source_id(environment.source_id),
            );
            valid = false;
        }
    }
    for component in value_type.components() {
        valid &= check_inferred_map_keys(environment, span, &component);
    }
    valid
}

/// The generic callee of one application and what the call site wrote.
struct GenericCall<'a> {
    parameters: &'a [String],
    signature: &'a FunctionSignature,
    type_arguments: Option<&'a [Type]>,
    expected: Option<&'a Type>,
}

/// Where one checked operand of a generic application belongs.
enum OperandSlot {
    Positional(usize),
    Labelled(String),
    Tail(usize),
}

/// The checked operands of a generic application.
struct GenericOperands {
    /// The instantiated signature.
    signature: FunctionSignature,
    /// Positional operands in order.
    positional: Vec<Expr>,
    /// Written labelled operands by name.
    labelled: BTreeMap<String, Expr>,
    /// Variadic tail operands in order, not yet packed.
    tail: Vec<Expr>,
}

/// Checks the written operands of a generic application and infers its
/// complete type-argument list from them, the written result type, and
/// `types:`. Omitted labelled operands stay in `labelled` for the caller's
/// default handling.
///
/// Operands whose parameter type is already fixed are checked against it, so
/// literals and lambdas see a concrete expectation; the rest are checked
/// alone and unified with their parameter type. Lambda operands wait until
/// every other operand has had the chance to fix their parameter type.
fn check_generic_operands(
    environment: &mut CheckEnvironment<'_>,
    application: &Application,
    call: GenericCall<'_>,
    ordered: &[&CallArgument],
    labelled: &mut BTreeMap<String, &CallArgument>,
    tail: &[&CallArgument],
) -> Option<GenericOperands> {
    let mut instantiation = start_instantiation(
        environment,
        application,
        call.parameters,
        &call.signature.result(),
        call.type_arguments,
        call.expected,
    )?;
    let opened = instantiation.open_signature(call.signature);

    let mut pending = Vec::new();
    for (index, (argument, pattern)) in ordered
        .iter()
        .take(opened.parameters().len())
        .zip(opened.parameters())
        .enumerate()
    {
        pending.push((OperandSlot::Positional(index), pattern.clone(), *argument));
    }
    for parameter in opened.labelled() {
        if let Some(argument) = labelled.remove(parameter.name()) {
            pending.push((
                OperandSlot::Labelled(parameter.name().to_owned()),
                parameter.value_type(),
                argument,
            ));
        }
    }
    if let Some(tail_type) = opened.variadic() {
        for (index, (argument, pattern)) in tail
            .iter()
            .zip(tail_patterns(tail_type, tail.len()))
            .enumerate()
        {
            pending.push((OperandSlot::Tail(index), pattern, *argument));
        }
    }
    let (lambdas, others): (Vec<_>, Vec<_>) =
        pending.into_iter().partition(|(_, _, argument)| {
            matches!(argument.value().kind(), ExpressionKind::Lambda(_))
        });

    let mut positional = BTreeMap::new();
    let mut labelled_values = BTreeMap::new();
    let mut tail_values = BTreeMap::new();
    for (slot, pattern, argument) in others.into_iter().chain(lambdas) {
        let checked = check_inferred_operand(
            environment,
            &mut instantiation,
            argument.value(),
            &pattern,
            call.type_arguments.is_some(),
        )?;
        match slot {
            OperandSlot::Positional(index) => {
                positional.insert(index, checked);
            }
            OperandSlot::Labelled(name) => {
                labelled_values.insert(name, checked);
            }
            OperandSlot::Tail(index) => {
                tail_values.insert(index, checked);
            }
        }
    }

    // Every parameter must be fixed, including one the signature never
    // mentions: inference is complete only when the argument list is unique.
    let unbound = instantiation.unbound();
    let Some(Type::Function(instantiated)) = instantiation
        .resolved(&Type::Function(Box::new(opened)))
        .filter(|_| unbound.is_empty())
    else {
        ambiguous_generic(environment, application.span(), &unbound);
        return None;
    };
    Some(GenericOperands {
        signature: *instantiated,
        positional: positional.into_values().collect(),
        labelled: labelled_values,
        tail: tail_values.into_values().collect(),
    })
}

/// Whether `name`, written at `expression`, denotes a value rather than a
/// constructor: a local, a capture, a module value, or a function.
fn names_value(
    environment: &CheckEnvironment<'_>,
    expression: &Expression,
    name: &str,
) -> bool {
    if environment.locals.contains_key(name)
        || environment.captures.contains_key(name)
        || environment
            .outer
            .as_ref()
            .is_some_and(|outer| outer.values.contains_key(name))
        || environment.global_indices.contains_key(name)
        || environment.function_indices.contains_key(name)
    {
        return true;
    }
    matches!(
        resolved_reference_target(environment, expression),
        Some(ResolvedReferenceTarget::Global(_) | ResolvedReferenceTarget::Function(_))
    )
}

fn unknown_name(
    environment: &mut CheckEnvironment<'_>,
    expression: &Expression,
    name: &str,
) {
    // A constructor is an application head, never a function value.
    if let ExpressionKind::Name(written) = expression.kind()
        && environment
            .types
            .constructor(environment.source_id, written)
            .is_some()
    {
        environment.diagnostics.push(
            Diagnostic::new(
                DiagnosticCode::NameWrongEntityKind,
                expression.span(),
                format!(
                    "`{name}` is a constructor, not a value; apply it to its operands"
                ),
            )
            .with_source_id(environment.source_id),
        );
        return;
    }
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
    types: &nominal::TypeNames,
    scope: nominal::Scope<'_>,
) -> Option<FunctionSignature> {
    let mut parameters = Vec::with_capacity(function.parameters().len());
    let mut valid = true;
    for parameter in function.parameters() {
        match types.lower_or_report(
            source_id,
            scope,
            parameter.value_type(),
            parameter.span(),
            diagnostics,
        ) {
            Some(value_type) => parameters.push(value_type),
            None => valid = false,
        }
    }
    let result = match types.lower_or_report(
        source_id,
        scope,
        function.result(),
        function.span(),
        diagnostics,
    ) {
        Some(value_type) => value_type,
        None => {
            valid = false;
            Type::Void
        }
    };
    let mut labelled = Vec::new();
    let mut variadic = None;
    for attribute in function.attributes().items() {
        match attribute {
            Attribute::Labelled(entries) => {
                for entry in entries {
                    let Some(value_type) = types.lower_or_report(
                        source_id,
                        scope,
                        entry.value_type(),
                        entry.span(),
                        diagnostics,
                    ) else {
                        valid = false;
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
                match types.lower_variadic(source_id, scope, parameter.value_type()) {
                    Ok(tail) => variadic = Some(tail),
                    Err(error) => {
                        valid = false;
                        nominal::report_lower_error(
                            diagnostics,
                            source_id,
                            parameter.span(),
                            &error,
                        );
                    }
                }
            }
            _ => {}
        }
    }
    valid.then(|| {
        with_tail(
            FunctionSignature::with_labelled(parameters, labelled, result),
            variadic,
        )
    })
}

fn compiler_intrinsic(
    source_id: &str,
    function: &vibra_syntax::FunctionDeclaration,
    diagnostics: &mut Vec<Diagnostic>,
    trusted_bootstrap: bool,
    actual: &FunctionSignature,
) -> Option<CompilerIntrinsic> {
    let provider = function.attributes().items().iter().find_map(|attribute| {
        if let Attribute::External(name) = attribute {
            Some(name.value())
        } else {
            None
        }
    });
    let provider = provider?;
    if provider == "host" {
        unavailable(
            diagnostics,
            source_id,
            function.span(),
            "the recognized @host provider remains unavailable until M4",
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
    if !trusted_bootstrap {
        unavailable(
            diagnostics,
            source_id,
            function.span(),
            "@compiler declarations require the embedded standard library",
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
    let Some(symbol) = symbol else {
        diagnostics.push(
            Diagnostic::new(
                DiagnosticCode::ExternalUnknownSymbol,
                function.span(),
                "a compiler external declaration requires a registered symbol",
            )
            .with_source_id(source_id),
        );
        return None;
    };
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
    // The registry signature is checked exactly, generic names included.
    let expected = intrinsic.signature();
    if !actual.same_shape(&expected) {
        diagnostics.push(
            Diagnostic::new(
                DiagnosticCode::TypeArgumentMismatch,
                function.span(),
                format!(
                    "compiler symbol `{symbol}` has signature {}",
                    Type::Function(Box::new(expected))
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
    types: &nominal::TypeNames,
    scope: nominal::Scope<'_>,
) -> Option<FunctionSignature> {
    let mut valid = true;
    let parameters = lambda
        .parameters()
        .iter()
        .map(|parameter| {
            let value_type = types.lower_or_report(
                source_id,
                scope,
                parameter.value_type(),
                parameter.span(),
                diagnostics,
            );
            if value_type.is_none() {
                valid = false;
            }
            value_type
        })
        .collect::<Option<Vec<_>>>()?;
    let result = types.lower_or_report(
        source_id,
        scope,
        lambda.result(),
        lambda.span(),
        diagnostics,
    )?;
    let mut labelled = Vec::new();
    let mut variadic = None;
    for attribute in lambda.attributes().items() {
        match attribute {
            Attribute::Labelled(entries) => {
                for entry in entries {
                    let Some(value_type) = types.lower_or_report(
                        source_id,
                        scope,
                        entry.value_type(),
                        entry.span(),
                        diagnostics,
                    ) else {
                        valid = false;
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
                match types.lower_variadic(source_id, scope, parameter.value_type()) {
                    Ok(tail) => variadic = Some(tail),
                    Err(error) => {
                        valid = false;
                        nominal::report_lower_error(
                            diagnostics,
                            source_id,
                            parameter.span(),
                            &error,
                        );
                    }
                }
            }
            _ => {}
        }
    }
    valid.then(|| {
        with_tail(
            FunctionSignature::with_labelled(parameters, labelled, result),
            variadic,
        )
    })
}

fn has_deferred_attributes(attributes: &[Attribute]) -> bool {
    attributes.iter().any(|attribute| match attribute {
        // `any`-bounded generics are checked from M3 Step 3; an interface
        // bound was already reported unavailable at the header.
        Attribute::External(_) | Attribute::Symbol(_) => true,
        Attribute::Where(_) | Attribute::Labelled(_) | Attribute::Variadic(_) => false,
        Attribute::Effects(row) => !row.references().is_empty(),
        Attribute::Visibility(_) | Attribute::Doc(_) => false,
    })
}

fn check_sequence(
    environment: &mut CheckEnvironment<'_>,
    expressions: &[Expression],
    expected: Option<Type>,
    fallback_span: ByteSpan,
    tail_position: bool,
) -> Option<Expr> {
    if expressions.is_empty() {
        if expected
            .as_ref()
            .is_some_and(|expected| *expected != Type::Void)
        {
            mismatch(
                environment.diagnostics,
                environment.source_id,
                fallback_span,
                expected.clone().unwrap_or(Type::Void),
                Type::Void,
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
    expected: Option<Type>,
) -> Option<Expr> {
    check_expression_in_position(environment, expression, expected, false)
}

/// Checks an operand bound to a parameter or constructor slot. A type
/// disagreement at the operand itself is `@type.argument-mismatch`; one nested
/// inside it, such as a differing branch, keeps `@type.mismatch`.
fn check_operand(
    environment: &mut CheckEnvironment<'_>,
    expression: &Expression,
    expected: Option<Type>,
) -> Option<Expr> {
    let before = environment.diagnostics.len();
    let checked = check_expression(environment, expression, expected);
    for diagnostic in environment.diagnostics.iter_mut().skip(before) {
        if diagnostic.code() == DiagnosticCode::TypeMismatch
            && diagnostic.primary_span() == expression.span()
        {
            *diagnostic = diagnostic
                .clone()
                .with_code(DiagnosticCode::TypeArgumentMismatch);
        }
    }
    checked
}

fn check_expression_in_position(
    environment: &mut CheckEnvironment<'_>,
    expression: &Expression,
    expected: Option<Type>,
    tail_position: bool,
) -> Option<Expr> {
    // Only the expression itself is a callee or a generic binding's value,
    // never its operands.
    let callee_position = std::mem::take(&mut environment.callee_position);
    let keeps_generic = std::mem::take(&mut environment.keeps_generic);
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
            let actual = Type::Atom;
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
                if let Some(target) = resolved_reference_target(environment, expression)
                {
                    return check_resolved_reference(
                        environment,
                        expression,
                        expected,
                        target,
                        callee_position,
                    );
                }
                if let Some(index) =
                    environment.function_indices.get(name.value()).copied()
                {
                    return function_value(
                        environment,
                        expression,
                        index,
                        expected,
                        callee_position,
                    );
                }
                unknown_name(environment, expression, name.value());
                return None;
            }
            if let Some(binding) = environment.locals.get(name.value()).cloned() {
                return binding_value(
                    environment,
                    expression,
                    expected,
                    binding.value_type,
                    binding.generics,
                    callee_position,
                    |value_type, origin| {
                        Expr::variable(binding.slot, value_type, origin)
                    },
                );
            }
            let binding = match environment.captures.get(name.value()).cloned() {
                Some(binding) => Some(binding),
                None => environment.resolve_capture(name.value(), expression.span()),
            };
            if let Some(binding) = binding {
                return binding_value(
                    environment,
                    expression,
                    expected,
                    binding.value_type,
                    binding.generics,
                    callee_position,
                    |value_type, origin| {
                        Expr::captured(binding.slot, value_type, origin)
                    },
                );
            }
            if let Some(target) = resolved_reference_target(environment, expression) {
                return check_resolved_reference(
                    environment,
                    expression,
                    expected,
                    target,
                    callee_position,
                );
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
                return function_value(
                    environment,
                    expression,
                    index,
                    expected,
                    callee_position,
                );
            }
            unknown_name(environment, expression, name.value());
            None
        }
        ExpressionKind::Application(application) => {
            if let ExpressionKind::Name(name) = application.callee().kind()
                && name.kind() == NameKind::Symbol
                && !names_value(environment, application.callee(), name.value())
                && let Some(target) =
                    environment.types.constructor(environment.source_id, name)
            {
                let type_arguments = lower_type_arguments(environment, application)?;
                return construct::check_constructor(
                    environment,
                    application,
                    &target,
                    type_arguments.as_deref(),
                    expected,
                );
            }
            let type_arguments = lower_type_arguments(environment, application)?;
            environment.callee_position = true;
            let mut callee = check_expression(environment, application.callee(), None)?;
            let generic = match &callee {
                Expr::Function { function, .. } => environment
                    .functions
                    .get(*function)
                    .filter(|header| !header.type_parameters.is_empty())
                    .map(|header| header.type_parameters.clone()),
                // A generic `lambda`, directly or through a binding.
                _ => Some(callee_generics(environment, application.callee()))
                    .filter(|parameters| !parameters.is_empty()),
            };
            if type_arguments.is_some() && generic.is_none() {
                type_argument_mismatch(
                    environment,
                    application,
                    "the applied callee declares no generic parameters".to_owned(),
                );
                return None;
            }
            if construct::record_fields(environment, &callee.result_type()).is_some() {
                return construct::check_projection(
                    environment,
                    application,
                    callee,
                    expected,
                );
            }
            let callee_type = callee.result_type();
            if let Some(components) =
                construct::tuple_components(environment, &callee_type)
            {
                return construct::check_tuple_projection(
                    environment,
                    application,
                    callee,
                    &components,
                    expected,
                );
            }
            if let Some((key_type, element)) = construct::lookup_types(&callee_type) {
                return construct::check_lookup(
                    environment,
                    application,
                    callee,
                    key_type,
                    element,
                    expected,
                );
            }
            let Type::Function(signature) = callee.result_type() else {
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
            // A call's own targets are reachable from the caller, so they are
            // in its recursive group unless they are compiler-intrinsic
            // wrappers. Checked IR computes the groups and decides whether a
            // marked transfer reuses the activation; the checker only marks
            // tail-position calls that can reach source code.
            let has_source_target = function_targets.known.iter().any(|index| {
                environment
                    .functions
                    .get(*index)
                    .is_some_and(|function| function.external.is_none())
            });
            let tail_transfer = tail_position
                && environment.current_function.is_some()
                && (has_source_target || function_targets.unknown);
            let facts = BindingFacts::new(
                signature.parameters().len(),
                signature
                    .labelled()
                    .iter()
                    .map(|parameter| parameter.name().to_owned())
                    .collect(),
                signature.variadic().map(tail_binding),
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
            let mut tail_operands = Vec::new();
            for argument in ordered.iter().skip(signature.parameters().len()) {
                if let Some(label) = argument.label() {
                    labelled.insert(label.value().to_owned(), *argument);
                } else {
                    tail_operands.push(*argument);
                }
            }
            let mut signature = signature;
            let mut arguments = Vec::with_capacity(signature.fixed_parameter_count());
            let mut checked_labelled = BTreeMap::new();
            let mut checked_tail = Vec::new();
            if let Some(parameters) = generic {
                let GenericOperands {
                    signature: instantiated,
                    positional,
                    labelled: labelled_values,
                    tail,
                } = check_generic_operands(
                    environment,
                    application,
                    GenericCall {
                        parameters: &parameters,
                        signature: &signature,
                        type_arguments: type_arguments.as_deref(),
                        expected: expected.as_ref(),
                    },
                    &ordered,
                    &mut labelled,
                    &tail_operands,
                )?;
                if !check_inferred_map_keys(
                    environment,
                    application.span(),
                    &Type::Function(Box::new(instantiated.clone())),
                ) {
                    return None;
                }
                checked_tail = tail;
                // A named function is rebuilt at its instantiation; a generic
                // closure value stays erased and the call carries the
                // instantiated operand and result types.
                if let Expr::Function { function, .. } = callee {
                    callee = Expr::function(
                        function,
                        instantiated.clone(),
                        SourceOrigin::new(
                            environment.source_id,
                            application.callee().span(),
                        ),
                    );
                }
                *signature = instantiated;
                arguments = positional;
                checked_labelled = labelled_values;
            } else {
                for (argument, value_type) in ordered
                    .iter()
                    .take(signature.parameters().len())
                    .zip(signature.parameters())
                {
                    arguments.push(check_operand(
                        environment,
                        argument.value(),
                        Some(value_type.clone()),
                    )?);
                }
                if let Some(tail_type) = signature.variadic() {
                    for (argument, value_type) in tail_operands
                        .iter()
                        .zip(tail_patterns(tail_type, tail_operands.len()))
                    {
                        checked_tail.push(check_operand(
                            environment,
                            argument.value(),
                            Some(value_type),
                        )?);
                    }
                }
            }
            for parameter in signature.labelled() {
                if let Some(argument) = checked_labelled.remove(parameter.name()) {
                    arguments.push(argument);
                } else if let Some(argument) = labelled.remove(parameter.name()) {
                    arguments.push(check_operand(
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
            if let Some(tail_type) = signature.variadic() {
                arguments.push(pack_tail(
                    tail_type,
                    checked_tail,
                    SourceOrigin::new(environment.source_id, application.span()),
                ));
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
            if application.type_arguments_after_operands()
                || ordered
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
            environment.keeps_generic =
                matches!(value.kind(), ExpressionKind::Lambda(_));
            let generics = quantified_lambda_generics(value);
            let value = check_expression(environment, value, None)?;
            let function_targets = matches!(value.result_type(), Type::Function(_))
                .then(|| {
                    function_targets_from_expr(&value, environment, &BTreeMap::new())
                });
            let mut nested = CheckEnvironment {
                source_id: environment.source_id,
                diagnostics: environment.diagnostics,
                types: environment.types,
                self_type: environment.self_type.clone(),
                generics: environment.generics.clone(),
                callee_position: false,
                keeps_generic: false,
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
                resolved_targets: environment.resolved_targets,
                reports_redeclarations: environment.reports_redeclarations,
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
                    if let Some(binding) = nested.locals.get_mut(name.value()) {
                        binding.generics = generics;
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
            let condition = check_expression(environment, condition, Some(Type::Bool))?;
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
            let attributes = lambda.attributes().items();
            if !nominal::report_interface_bounds(
                attributes,
                environment.source_id,
                environment.diagnostics,
            ) {
                return None;
            }
            let own_generics = nominal::generic_names(attributes);
            let mut generics = environment.generics.clone();
            generics.extend(own_generics.iter().cloned());
            let signature = check_lambda_signature(
                environment.source_id,
                lambda,
                environment.diagnostics,
                environment.types,
                nominal::Scope::new(environment.self_type.as_ref(), &generics),
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
                environment.types,
            );
            nested.self_type = environment.self_type.clone();
            nested.generics = generics;
            nested.resolved_targets = environment.resolved_targets;
            nested.reports_redeclarations = environment.reports_redeclarations;
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
                // The signature already lowered and reported every parameter type.
                let Some(value_type) =
                    signature.parameters().get(parameter_index).cloned()
                else {
                    parameters_valid = false;
                    nested.next_slot = parameter_index.saturating_add(1);
                    continue;
                };
                parameter_types.push(value_type.clone());
                match parameter.parsed_pattern().kind() {
                    PatternKind::Binding(name) if name.is_discard() => {}
                    PatternKind::Binding(name) => {
                        if !nested.add_binding_type(
                            name.value(),
                            value_type,
                            parameter.parsed_pattern().span(),
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
                        entry.name_span(),
                    ) {
                        parameters_valid = false;
                    }
                }
            }
            if !nested.bind_variadic(lambda.attributes().items(), &signature) {
                parameters_valid = false;
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
            // The body is checked with the lambda's own generic names rigid.
            // A `let` value or a callee stays generic under quantified names;
            // anywhere else the expected `fn` type instantiates it.
            let (signature, parameter_types) = if own_generics.is_empty() {
                (signature, parameter_types)
            } else if keeps_generic || callee_position {
                let quantified: BTreeMap<String, Type> = own_generics
                    .iter()
                    .enumerate()
                    .map(|(index, name)| {
                        (
                            name.clone(),
                            Type::Param(infer::quantified_name(
                                name,
                                index,
                                lambda.span().start(),
                            )),
                        )
                    })
                    .collect();
                (
                    signature.substitute(&quantified),
                    parameter_types
                        .iter()
                        .map(|value| value.substitute(&quantified))
                        .collect(),
                )
            } else {
                let Type::Function(instantiated) = instantiate_value(
                    environment,
                    expression.span(),
                    &own_generics,
                    &Type::Function(Box::new(signature)),
                    expected.as_ref(),
                )?
                else {
                    return None;
                };
                let parameters = instantiated.parameters().to_vec();
                (*instantiated, parameters)
            };
            let actual = Type::Function(Box::new(signature.clone()));
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
        ExpressionKind::RecordOf(fields) => {
            construct::check_recordof(environment, expression, fields, expected)
        }
        ExpressionKind::EnumOf(variant) => {
            construct::check_enumof(environment, expression, variant, expected)
        }
        ExpressionKind::TupleOf(components) => {
            construct::check_tupleof(environment, expression, components, expected)
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

fn resolved_reference_target(
    environment: &CheckEnvironment<'_>,
    expression: &Expression,
) -> Option<ResolvedReferenceTarget> {
    let references = environment.resolved_targets?;
    references
        .get(&(
            environment.source_id.to_owned(),
            expression.span().start(),
            expression.span().end(),
        ))
        .copied()
}

fn check_resolved_reference(
    environment: &mut CheckEnvironment<'_>,
    expression: &Expression,
    expected: Option<Type>,
    target: ResolvedReferenceTarget,
    callee_position: bool,
) -> Option<Expr> {
    match target {
        ResolvedReferenceTarget::Unresolved => {
            // The resolver reported an unknown path. A path that resolved to a
            // type or variant is a constructor, which is not a value.
            if let ExpressionKind::Name(name) = expression.kind()
                && environment
                    .types
                    .constructor(environment.source_id, name)
                    .is_some()
            {
                unknown_name(environment, expression, name.value());
            }
            None
        }
        ResolvedReferenceTarget::Global(index) => {
            let global = environment.globals.get(index)?;
            let actual = global.value_type.clone();
            ensure_expected(
                environment,
                expression.span(),
                expected.clone(),
                actual.clone(),
            );
            (expected
                .as_ref()
                .is_none_or(|expected| types_match(expected, &actual)))
            .then_some(Expr::global(
                index,
                actual,
                SourceOrigin::new(environment.source_id, expression.span()),
            ))
        }
        ResolvedReferenceTarget::Function(index) => {
            function_value(environment, expression, index, expected, callee_position)
        }
    }
}

fn check_literal(
    source_id: &str,
    span: ByteSpan,
    literal: &Literal,
    expected: Option<Type>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Value> {
    match literal {
        Literal::String(value) => expect_fixed(
            source_id,
            span,
            expected,
            Type::Str,
            Value::Str(value.value().to_owned()),
            diagnostics,
        ),
        Literal::Character(value) => expect_fixed(
            source_id,
            span,
            expected,
            Type::Char,
            Value::Char(value.value()),
            diagnostics,
        ),
        Literal::Boolean(value) => expect_fixed(
            source_id,
            span,
            expected,
            Type::Bool,
            Value::Bool(value.value()),
            diagnostics,
        ),
        Literal::Void(_) => expect_fixed(
            source_id,
            span,
            expected,
            Type::Void,
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
    expected: Option<Type>,
    actual: Type,
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
    expected: Option<Type>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Value> {
    let target = literal
        .suffix()
        .map(integer_suffix_type)
        .or_else(|| expected.clone().filter(|value| value.is_integer()));
    let Some(target) = target else {
        match expected {
            Some(expected) => mismatch(
                diagnostics,
                source_id,
                span,
                expected,
                Type::I64,
                "an unsuffixed integer is not a value of the expected type",
            ),
            None => ambiguous_literal(
                diagnostics,
                source_id,
                span,
                "an unsuffixed integer needs one expected integer type",
            ),
        }
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
    expected: Option<Type>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Value> {
    let target = literal
        .suffix()
        .map(float_suffix_type)
        .or_else(|| expected.clone().filter(|value| value.is_float()));
    let Some(target) = target else {
        match expected {
            Some(expected) => mismatch(
                diagnostics,
                source_id,
                span,
                expected,
                Type::F64,
                "an unsuffixed float is not a value of the expected type",
            ),
            None => ambiguous_literal(
                diagnostics,
                source_id,
                span,
                "an unsuffixed float needs one expected floating-point type",
            ),
        }
        return None;
    };
    let value = match target {
        Type::F32 => literal
            .body()
            .parse::<f32>()
            .ok()
            .filter(|value| value.is_finite())
            .and_then(Value::f32),
        Type::F64 => literal
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

fn integer_suffix_type(suffix: IntegerSuffix) -> Type {
    match suffix {
        IntegerSuffix::I8 => Type::I8,
        IntegerSuffix::I16 => Type::I16,
        IntegerSuffix::I32 => Type::I32,
        IntegerSuffix::I64 => Type::I64,
        IntegerSuffix::U8 => Type::U8,
        IntegerSuffix::U16 => Type::U16,
        IntegerSuffix::U32 => Type::U32,
        IntegerSuffix::U64 => Type::U64,
    }
}

fn float_suffix_type(suffix: FloatSuffix) -> Type {
    match suffix {
        FloatSuffix::F32 => Type::F32,
        FloatSuffix::F64 => Type::F64,
    }
}

fn integer_value(target: Type, negative: bool, magnitude: u128) -> Option<Value> {
    match target {
        Type::I8 => signed_value(negative, magnitude, i8::MIN as i128, i8::MAX as i128)
            .map(|value| Value::I8(value as i8)),
        Type::I16 => {
            signed_value(negative, magnitude, i16::MIN as i128, i16::MAX as i128)
                .map(|value| Value::I16(value as i16))
        }
        Type::I32 => {
            signed_value(negative, magnitude, i32::MIN as i128, i32::MAX as i128)
                .map(|value| Value::I32(value as i32))
        }
        Type::I64 => {
            signed_value(negative, magnitude, i64::MIN as i128, i64::MAX as i128)
                .map(|value| Value::I64(value as i64))
        }
        Type::U8 => (!negative && magnitude <= u8::MAX as u128)
            .then_some(Value::U8(magnitude as u8)),
        Type::U16 => (!negative && magnitude <= u16::MAX as u128)
            .then_some(Value::U16(magnitude as u16)),
        Type::U32 => (!negative && magnitude <= u32::MAX as u128)
            .then_some(Value::U32(magnitude as u32)),
        Type::U64 => (!negative && magnitude <= u64::MAX as u128)
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

/// `@type.ambiguous-inference` for a literal no expected type constrains.
fn ambiguous_literal(
    diagnostics: &mut Vec<Diagnostic>,
    source_id: &str,
    span: ByteSpan,
    message: &str,
) {
    diagnostics.push(
        Diagnostic::new(DiagnosticCode::TypeAmbiguousInference, span, message)
            .with_source_id(source_id),
    );
}

fn mismatch(
    diagnostics: &mut Vec<Diagnostic>,
    source_id: &str,
    span: ByteSpan,
    expected: Type,
    actual: Type,
    message: impl Into<String>,
) {
    diagnostics.push(
        Diagnostic::new(
            DiagnosticCode::TypeMismatch,
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

pub(crate) fn unavailable(
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

/// `signature` with its variadic tail, when it has one.
fn with_tail(signature: FunctionSignature, tail: Option<Type>) -> FunctionSignature {
    match tail {
        Some(tail) => signature.with_variadic(tail),
        None => signature,
    }
}

/// Every dotted value path written anywhere in `ast`'s executable bodies.
fn dotted_value_paths(ast: &SourceAst) -> BTreeSet<String> {
    let mut bodies: Vec<&Expression> = Vec::new();
    for declaration in ast.declarations() {
        match declaration {
            Declaration::Def(value) => bodies.push(value.expression()),
            Declaration::Defn(function) => bodies.extend(function.expressions()),
            Declaration::Deftype(value) => {
                for member in value.members() {
                    if let TypeMember::Method(method) = member {
                        bodies.extend(method.expressions());
                    }
                }
            }
            Declaration::Test(test) => bodies.extend(test.expressions()),
            _ => {}
        }
    }
    let mut paths = BTreeSet::new();
    for body in bodies {
        walk_expressions(body, &mut |expression| {
            if let ExpressionKind::Name(name) = expression.kind()
                && name.kind() == NameKind::Symbol
                && name.segments().len() > 1
            {
                paths.insert(name.value().to_owned());
            }
        });
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::{
        STDLIB_TEXT_SOURCE_ID, check_bootstrap_source, check_bootstrap_text_import,
        check_source, load_stdlib,
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
            vibra_ir::Type::I32
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
        assert!(
            result
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code() == DiagnosticCode::TypeMismatch)
        );
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
        assert!(
            condition.diagnostics().iter().any(|diagnostic| {
                diagnostic.code() == DiagnosticCode::TypeMismatch
            })
        );
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
        let source = include_str!("../../../stdlib/src/std/text.vib");
        let verification = load_stdlib().expect("bootstrap provenance");
        let checked =
            check_bootstrap_source(&verification, STDLIB_TEXT_SOURCE_ID, source);
        assert!(checked.accepted(), "{:?}", checked.diagnostics());
        let program = checked.program().expect("bootstrap program");
        assert!(program.canonical_vibon().contains("text.length"));
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
            [vec![0, 1], vec![0, 1], vec![0, 1, 2]]
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
        let verification = load_stdlib().expect("bootstrap provenance");
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
        let verification = load_stdlib().expect("bootstrap provenance");
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
    fn unavailable_explicit_text_import_uses_import_span() {
        let verification = load_stdlib().expect("bootstrap provenance");
        let source = "(import wrong @std.text)\n(defn answer () u64 1u64)";
        let checked =
            check_bootstrap_text_import(&verification, "app/main.vib", source);
        let diagnostic = checked
            .diagnostics()
            .iter()
            .find(|diagnostic| diagnostic.code() == DiagnosticCode::ToolUnavailable)
            .expect("unavailable import diagnostic");

        assert!(!checked.accepted());
        assert_eq!(diagnostic.source_id(), Some("app/main.vib"));
        assert_eq!(
            diagnostic.primary_span(),
            ByteSpan::new(0, source.find('\n').unwrap())
        );
    }

    #[test]
    fn trusted_text_alias_collisions_report_the_import_span() {
        let verification = load_stdlib().expect("bootstrap provenance");
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
            let expected = if source.contains("@host") {
                DiagnosticCode::ToolUnavailable
            } else if source.contains("text.unknown") {
                DiagnosticCode::ExternalUnknownSymbol
            } else {
                DiagnosticCode::TypeArgumentMismatch
            };
            assert!(
                diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code() == expected)
            );
        }
    }

    #[test]
    fn trusted_text_import_rejects_source_body_and_effect_external_declarations() {
        let verification = load_stdlib().expect("bootstrap provenance");
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

    #[test]
    fn variadic_array_and_map_calls_bind_zero_or_more_tail_operands() {
        for (label, source) in [
            (
                "array",
                "(defn use-array () i32\n  (do (collect-array 1i32) (collect-array 1i32 2i32)))\n(defn collect-array (first i32) i32\n  variadic: (rest (array i32))\n  first)",
            ),
            (
                "map",
                "(defn use-map () i32\n  (do (collect-map 1i32) (collect-map 1i32 \"key\" 2i32)))\n(defn collect-map (first i32) i32\n  variadic: (rest (map str i32))\n  first)",
            ),
        ] {
            let result = check_source(format!("{label}.vib"), source);
            assert!(result.accepted(), "{label}: {:?}", result.diagnostics());
        }
    }

    #[test]
    fn variadic_targets_survive_lambda_alias_captures() {
        let source = "(defn use-alias () i32\n  (let alias collect-array (alias 1i32)))\n(defn collect-array (first i32) i32\n  variadic: (rest (array i32))\n  first)\n(defn use-direct () (fn () i32)\n  (let alias collect-array\n    (lambda () i32 (alias 1i32))))\n(defn use-nested () (fn () (fn () i32))\n  (let alias collect-array\n    (lambda () (fn () i32)\n      (lambda () i32 (alias 1i32)))))";
        let result = check_source("captured-variadic.vib", source);
        assert!(result.accepted(), "{:?}", result.diagnostics());
        let program = result.program().expect("program");
        // Every call through an alias packs its tail into one array argument.
        assert_eq!(
            program
                .canonical_vibon()
                .matches("(record kind: @array")
                .count(),
            3
        );
    }

    #[test]
    fn higher_order_call_flow_converges_through_a_long_chain() {
        // Each function forwards its callable parameter to the next, so the
        // entry's argument must flow through every parameter summary.
        // The single-source entry is the first function.
        let depth = 200;
        let mut source = format!(
            "(defn answer () i32 (step{} leaf))\n(defn leaf () i32 1i32)\n(defn step0 (g (fn () i32)) i32 (g))\n",
            depth - 1
        );
        for index in 1..depth {
            // `step` rather than `f`: `f32` and `f64` are builtin type names.
            source.push_str(&format!(
                "(defn step{index} (g (fn () i32)) i32 (step{} g))\n",
                index - 1
            ));
        }
        let started = std::time::Instant::now();
        let result = check_source("chain.vib", &source);
        assert!(result.accepted(), "{:?}", result.diagnostics());
        assert!(
            started.elapsed() < std::time::Duration::from_secs(30),
            "call-flow analysis took {:?}",
            started.elapsed()
        );
    }
}

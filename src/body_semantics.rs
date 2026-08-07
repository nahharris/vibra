//! Shared control-flow validation over lowered body IR.
//!
//! Both source frontends use these return-termination and affine-task checks.
//! Expression inference remains owned by the legacy lowerer until its
//! enum/alias dependencies can be extracted as one semantic unit.

use crate::ast::{
    AnnotationKind, Expr as AstExpr, ExprKind as AstExprKind, Pattern as AstPattern,
    PatternKind as AstPatternKind, TopLevel,
};
use crate::frontend::SurfaceProgram;
use crate::lower::{
    Call, Expr, FunctionBody, FunctionSig, LetValue, Pattern, RuntimeValue, Statement, TypeAlias,
    TypeRef,
};
use crate::type_semantics::substitute_type;
use anyhow::{bail, Result};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceSpan {
    pub path: PathBuf,
    pub span: crate::syntax::Span,
}

impl SourceSpan {
    fn new(path: &Path, span: crate::syntax::Span) -> Self {
        Self {
            path: path.to_path_buf(),
            span,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BodySourceKind {
    User,
    TrustedStdlib,
}

impl Default for BodySourceKind {
    fn default() -> Self {
        Self::User
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct BodySource {
    pub kind: BodySourceKind,
    pub statements: Vec<StatementSource>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct StatementSource {
    pub span: Option<SourceSpan>,
    pub binder: Option<SourceSpan>,
    pub patterns: Vec<PatternSource>,
    pub bodies: Vec<BodySource>,
}

#[derive(Debug, Clone)]
pub(crate) enum PatternSource {
    None,
    Bind(SourceSpan),
    Constructor(Vec<PatternSource>),
    Record(std::collections::BTreeMap<String, PatternSource>),
    Items(Vec<PatternSource>),
    Map(Vec<(PatternSource, PatternSource)>),
    Newtype(Box<PatternSource>),
    Interface(Box<PatternSource>),
}

impl BodySource {
    pub(crate) fn from_ast(path: &Path, body: &[AstExpr], kind: BodySourceKind) -> Self {
        let mut statements = Vec::new();
        push_ast_statements(path, body, kind, &mut statements);
        Self { kind, statements }
    }
}

fn push_ast_statements(
    path: &Path,
    body: &[AstExpr],
    kind: BodySourceKind,
    out: &mut Vec<StatementSource>,
) {
    for expr in body {
        if let AstExprKind::Do(inner) = &expr.value {
            push_ast_statements(path, inner, kind, out);
        } else {
            out.push(statement_source(path, expr, kind));
        }
    }
}

fn statement_source(path: &Path, expr: &AstExpr, kind: BodySourceKind) -> StatementSource {
    let mut source = StatementSource {
        span: Some(SourceSpan::new(path, expr.span)),
        ..StatementSource::default()
    };
    match &expr.value {
        AstExprKind::Let { name, .. } => {
            source.binder = Some(SourceSpan::new(path, name.span));
        }
        AstExprKind::If {
            then_body,
            else_body,
            ..
        } => {
            source
                .bodies
                .push(BodySource::from_ast(path, then_body, kind));
            source
                .bodies
                .push(BodySource::from_ast(path, else_body, kind));
        }
        AstExprKind::While { body, .. }
        | AstExprKind::For { body, .. }
        | AstExprKind::Task { body, .. } => {
            source.bodies.push(BodySource::from_ast(path, body, kind));
        }
        AstExprKind::Match { cases, .. } => {
            source.patterns = cases
                .iter()
                .map(|case| pattern_source(path, &case.pattern))
                .collect();
            source.bodies = cases
                .iter()
                .map(|case| BodySource::from_ast(path, &case.body, kind))
                .collect();
        }
        _ => {}
    }
    source
}

fn pattern_source(path: &Path, pattern: &AstPattern) -> PatternSource {
    match &pattern.value {
        AstPatternKind::Bind(name) => PatternSource::Bind(SourceSpan::new(path, name.span)),
        AstPatternKind::Constructor { arguments, .. } => PatternSource::Constructor(
            arguments
                .iter()
                .map(|pattern| pattern_source(path, pattern))
                .collect(),
        ),
        AstPatternKind::Record(fields) => PatternSource::Record(
            fields
                .iter()
                .map(|field| {
                    (
                        field.name.value.clone(),
                        pattern_source(path, &field.pattern),
                    )
                })
                .collect(),
        ),
        AstPatternKind::Tuple(items) | AstPatternKind::Array(items) => PatternSource::Items(
            items
                .iter()
                .map(|pattern| pattern_source(path, pattern))
                .collect(),
        ),
        AstPatternKind::Map(items) => PatternSource::Map(
            items
                .iter()
                .map(|(key, value)| (pattern_source(path, key), pattern_source(path, value)))
                .collect(),
        ),
        AstPatternKind::Newtype { pattern, .. } | AstPatternKind::Interface { pattern, .. } => {
            let inner = pattern_source(path, pattern);
            if matches!(&pattern.value, AstPatternKind::Newtype { .. }) {
                PatternSource::Newtype(Box::new(inner))
            } else {
                PatternSource::Interface(Box::new(inner))
            }
        }
        AstPatternKind::Literal(_) | AstPatternKind::Wildcard => PatternSource::None,
    }
}

/// Collect source origins for every ordinary body that can be lowered into a
/// function or test. The map uses the same qualified keys as the legacy
/// lowering registry, so imported bodies retain their defining file.
pub(crate) fn collect_body_sources(program: &SurfaceProgram) -> HashMap<String, BodySource> {
    let mut sources = HashMap::new();
    let mut visited = std::collections::HashSet::new();
    let trusted_stdlib_root = crate::project::locate_stdlib_source()
        .ok()
        .and_then(|path| std::fs::canonicalize(path).ok());
    collect_module_sources(
        program,
        &program.entry,
        "",
        &mut visited,
        &mut sources,
        trusted_stdlib_root.as_deref(),
    );
    sources
}

fn collect_module_sources(
    program: &SurfaceProgram,
    path: &Path,
    alias: &str,
    visited: &mut std::collections::HashSet<(PathBuf, String)>,
    sources: &mut HashMap<String, BodySource>,
    trusted_stdlib_root: Option<&Path>,
) {
    if !visited.insert((path.to_path_buf(), alias.to_string())) {
        return;
    }
    let Some(module) = program.modules.get(path) else {
        return;
    };
    for part in &module.parts {
        let kind = source_kind(&part.path, trusted_stdlib_root);
        for form in &part.module.forms {
            collect_form_source(alias, &part.path, form, kind, sources);
        }
    }
    let imports = program
        .imports
        .get(path)
        .into_iter()
        .flat_map(|imports| imports.iter())
        .map(|(child_alias, child_path)| (child_alias.clone(), child_path.clone()))
        .collect::<Vec<_>>();
    for (child_alias, child_path) in imports {
        let child_alias = if alias.is_empty() {
            child_alias
        } else {
            format!("{alias}.{child_alias}")
        };
        collect_module_sources(
            program,
            &child_path,
            &child_alias,
            visited,
            sources,
            trusted_stdlib_root,
        );
    }
}

fn source_kind(path: &Path, trusted_stdlib_root: Option<&Path>) -> BodySourceKind {
    trusted_stdlib_root
        .is_some_and(|root| path.starts_with(root))
        .then_some(BodySourceKind::TrustedStdlib)
        .unwrap_or_default()
}

fn collect_form_source(
    alias: &str,
    path: &Path,
    form: &TopLevel,
    source_kind: BodySourceKind,
    sources: &mut HashMap<String, BodySource>,
) {
    let key = |name: &str| {
        if alias.is_empty() {
            name.to_string()
        } else {
            format!("{alias}.{name}")
        }
    };
    match form {
        TopLevel::Function(function) => {
            sources.insert(
                key(&function.name.value),
                BodySource::from_ast(path, &function.body, source_kind),
            );
        }
        TopLevel::Test(test) => {
            sources.insert(
                key(&test.name.value),
                BodySource::from_ast(path, &test.body, source_kind),
            );
        }
        TopLevel::TestScenario(scenario) => {
            for test in &scenario.cases {
                let name = format!("{}::{}", scenario.name.value, test.name.value);
                sources.insert(
                    key(&name),
                    BodySource::from_ast(path, &test.body, source_kind),
                );
            }
        }
        TopLevel::Definition(definition) => {
            for annotation in &definition.annotations {
                let AnnotationKind::Definitions(functions) = &annotation.value else {
                    continue;
                };
                for function in functions {
                    let name = format!("{}.{}", definition.name.value, function.name.value);
                    sources.insert(
                        key(&name),
                        BodySource::from_ast(path, &function.body, source_kind),
                    );
                }
            }
        }
        TopLevel::Deffect(deffect) => {
            for operation in &deffect.operations {
                let name = format!("{}.{}", deffect.name.value, operation.function.name.value);
                sources.insert(
                    key(&name),
                    BodySource::from_ast(path, &operation.function.body, source_kind),
                );
            }
        }
        TopLevel::Import(_) | TopLevel::Constant(_) | TopLevel::Macro(_) => {}
    }
}

#[derive(Debug, Clone)]
pub(crate) struct BodyDiagnostic {
    pub code: &'static str,
    pub message: String,
    pub span: Option<SourceSpan>,
    pub fix: Option<SourceSpan>,
}

pub(crate) const UNHANDLED_RESULT_CODE: &str = "W-RESULT-001";
pub(crate) const UNUSED_BINDING_CODE: &str = "W-BIND-001";

pub(crate) fn validate_function_body(
    statements: &[Statement],
    return_type: &TypeRef,
) -> Result<()> {
    if *return_type != TypeRef::Void && !body_terminates(statements) {
        bail!(
            "non-void function must end with `$return`, or with `$match` whose every arm ends with `$return`"
        );
    }
    Ok(())
}

pub(crate) fn validate_task_handles(statements: &[Statement]) -> Result<()> {
    fn walk(statements: &[Statement], mut live: BTreeSet<String>) -> Result<BTreeSet<String>> {
        for statement in statements {
            match statement {
                Statement::Spawn { handle, .. } => {
                    if !live.insert(handle.clone()) {
                        bail!("task handle `{handle}` is spawned more than once");
                    }
                }
                Statement::Join { handle, .. } => {
                    if !live.remove(handle) {
                        bail!("task handle `{handle}` is joined more than once or out of scope");
                    }
                }
                Statement::If {
                    then_body,
                    else_body,
                    ..
                } => {
                    let then_live = walk(then_body, live.clone())?;
                    let else_live = walk(else_body, live.clone())?;
                    if then_live != else_live {
                        bail!("both branches must consume the same task handles");
                    }
                    live = then_live;
                }
                Statement::Match { arms, .. } => {
                    let mut merged = None;
                    for arm in arms {
                        let arm_live = walk(&arm.body, live.clone())?;
                        if merged.as_ref().is_some_and(|prior| prior != &arm_live) {
                            bail!("every match arm must consume the same task handles");
                        }
                        merged = Some(arm_live);
                    }
                    if let Some(arm_live) = merged {
                        live = arm_live;
                    }
                }
                Statement::While { body, .. } | Statement::For { body, .. } => {
                    if walk(body, live.clone())? != live {
                        bail!("loops cannot create or consume task handles across iterations");
                    }
                }
                Statement::Task { body, .. } => {
                    let nested = walk(body, BTreeSet::new())?;
                    if !nested.is_empty() {
                        bail!("nested task left unjoined handles");
                    }
                }
                _ => {}
            }
        }
        Ok(live)
    }
    let live = walk(statements, BTreeSet::new())?;
    if !live.is_empty() {
        bail!("task handles must be joined before leaving their scope");
    }
    Ok(())
}

fn body_terminates(statements: &[Statement]) -> bool {
    match statements.last() {
        Some(Statement::Return(_)) => true,
        Some(Statement::Match { arms, .. }) => arms.iter().all(|arm| body_terminates(&arm.body)),
        Some(Statement::If {
            then_body,
            else_body,
            ..
        }) => body_terminates(then_body) && body_terminates(else_body),
        _ => false,
    }
}

/// Check the source-level must-use contract over the shared body IR.
///
/// This pass deliberately runs after either frontend has produced typed
/// statements.  A result/option value is only unhandled when it is itself the
/// value of a non-final expression statement; `let`, `let-as`, `return`, and
/// `match` already give the value a consumer.  Binding reads are collected by
/// binding identity, so a read on any match arm is enough to keep that binding
/// live.
pub(crate) fn validate_unhandled_values(
    statements: &[Statement],
    functions: &HashMap<String, FunctionSig>,
    type_aliases: &HashMap<String, TypeAlias>,
    sources: &HashMap<String, BodySource>,
    main_source: Option<&BodySource>,
    warnings: &mut Vec<String>,
    diagnostics: &mut Vec<BodyDiagnostic>,
) {
    let mut function_keys: Vec<_> = functions
        .iter()
        .filter_map(|(key, signature)| {
            (matches!(&signature.body, FunctionBody::User { .. })
                && !sources
                    .get(key)
                    .is_some_and(|source| source.kind == BodySourceKind::TrustedStdlib))
            .then_some(key)
        })
        .collect();
    function_keys.sort();

    for key in function_keys {
        let Some(FunctionSig {
            body: FunctionBody::User { statements },
            parameters,
            ..
        }) = functions.get(key)
        else {
            continue;
        };
        validate_unhandled_body(
            statements,
            parameters,
            functions,
            type_aliases,
            sources.get(key),
            warnings,
            diagnostics,
        );
    }

    // `main` is lowered separately from ordinary user functions by the legacy
    // frontend.  The typed frontend materializes it as a normal user function,
    // so this call is harmless there and keeps both paths covered.
    validate_unhandled_body(
        statements,
        &[],
        functions,
        type_aliases,
        main_source,
        warnings,
        diagnostics,
    );
}

pub(crate) fn validate_unhandled_body(
    statements: &[Statement],
    parameters: &[crate::lower::Parameter],
    functions: &HashMap<String, FunctionSig>,
    type_aliases: &HashMap<String, TypeAlias>,
    source: Option<&BodySource>,
    warnings: &mut Vec<String>,
    diagnostics: &mut Vec<BodyDiagnostic>,
) {
    let mut analysis = BindingAnalysis::default();
    let mut scopes = vec![Scope::default()];
    for parameter in parameters {
        analysis.declare(
            scopes.last_mut().expect("parameter scope exists"),
            &parameter.name,
            Some(parameter.ty.clone()),
            false,
            None,
        );
    }
    analysis.walk_scope(
        statements,
        &mut scopes,
        functions,
        type_aliases,
        source,
        warnings,
        diagnostics,
    );
}

#[derive(Default)]
struct Scope {
    bindings: HashMap<String, usize>,
    declared: Vec<usize>,
}

struct BindingState {
    name: String,
    ty: Option<TypeRef>,
    span: Option<SourceSpan>,
    read: bool,
    report: bool,
    reported: bool,
}

#[derive(Default)]
struct BindingAnalysis {
    next_binding: usize,
    bindings: HashMap<usize, BindingState>,
}

impl BindingAnalysis {
    fn declare(
        &mut self,
        scope: &mut Scope,
        name: &str,
        ty: Option<TypeRef>,
        report: bool,
        span: Option<SourceSpan>,
    ) {
        if name == "_" {
            return;
        }
        let id = self.next_binding;
        self.next_binding += 1;
        scope.bindings.insert(name.to_string(), id);
        scope.declared.push(id);
        self.bindings.insert(
            id,
            BindingState {
                name: name.to_string(),
                ty,
                span,
                read: false,
                report,
                reported: false,
            },
        );
    }

    fn resolve(scopes: &[Scope], name: &str) -> Option<usize> {
        scopes
            .iter()
            .rev()
            .find_map(|scope| scope.bindings.get(name).copied())
    }

    fn mark_read(&mut self, scopes: &[Scope], name: &str) {
        let Some(id) = Self::resolve(scopes, name) else {
            return;
        };
        if let Some(binding) = self.bindings.get_mut(&id) {
            binding.read = true;
        }
    }

    fn type_of(&self, scopes: &[Scope], name: &str) -> Option<TypeRef> {
        let id = Self::resolve(scopes, name)?;
        self.bindings.get(&id)?.ty.clone()
    }

    fn finish_scope(
        &mut self,
        scope: &Scope,
        warnings: &mut Vec<String>,
        diagnostics: &mut Vec<BodyDiagnostic>,
    ) {
        for id in &scope.declared {
            let Some(binding) = self.bindings.get_mut(id) else {
                continue;
            };
            if binding.report && !binding.read && !binding.reported {
                binding.reported = true;
                let message = format!(
                    "{UNUSED_BINDING_CODE}: binding `{}` is never read; remove it or replace it with `_`",
                    binding.name
                );
                push_warning(
                    UNUSED_BINDING_CODE,
                    message,
                    binding.span.clone(),
                    binding.span.clone(),
                    warnings,
                    diagnostics,
                );
            }
        }
    }

    fn walk_scope(
        &mut self,
        statements: &[Statement],
        scopes: &mut Vec<Scope>,
        functions: &HashMap<String, FunctionSig>,
        type_aliases: &HashMap<String, TypeAlias>,
        source: Option<&BodySource>,
        warnings: &mut Vec<String>,
        diagnostics: &mut Vec<BodyDiagnostic>,
    ) {
        scopes.push(Scope::default());
        self.walk_sequence(
            statements,
            scopes,
            functions,
            type_aliases,
            source,
            warnings,
            diagnostics,
        );
        let scope = scopes.pop().expect("walk scope pushed a scope");
        self.finish_scope(&scope, warnings, diagnostics);
    }

    fn walk_sequence(
        &mut self,
        statements: &[Statement],
        scopes: &mut Vec<Scope>,
        functions: &HashMap<String, FunctionSig>,
        type_aliases: &HashMap<String, TypeAlias>,
        source: Option<&BodySource>,
        warnings: &mut Vec<String>,
        diagnostics: &mut Vec<BodyDiagnostic>,
    ) {
        for (index, statement) in statements.iter().enumerate() {
            self.walk_statement(
                statement,
                index + 1 == statements.len(),
                source.and_then(|source| source.statements.get(index)),
                scopes,
                functions,
                type_aliases,
                warnings,
                diagnostics,
            );
        }
    }

    fn walk_statement(
        &mut self,
        statement: &Statement,
        is_final: bool,
        source: Option<&StatementSource>,
        scopes: &mut Vec<Scope>,
        functions: &HashMap<String, FunctionSig>,
        type_aliases: &HashMap<String, TypeAlias>,
        warnings: &mut Vec<String>,
        diagnostics: &mut Vec<BodyDiagnostic>,
    ) {
        match statement {
            Statement::Call(call) => {
                for argument in &call.args {
                    self.walk_expr_reads(argument, scopes);
                }
                if !is_final {
                    self.warn_if_unhandled(
                        call_return_type(call, functions),
                        call.callee_key.as_str(),
                        type_aliases,
                        source.and_then(|source| source.span.as_ref()),
                        warnings,
                        diagnostics,
                    );
                }
            }
            Statement::Eval(expr) => {
                self.walk_expr_reads(expr, scopes);
                if !is_final {
                    self.warn_if_unhandled(
                        self.expr_type(expr, scopes),
                        "expression",
                        type_aliases,
                        source.and_then(|source| source.span.as_ref()),
                        warnings,
                        diagnostics,
                    );
                }
            }
            Statement::Let { var, value } => {
                let value_type = match value {
                    LetValue::Call(call) => {
                        for argument in &call.args {
                            self.walk_expr_reads(argument, scopes);
                        }
                        call_return_type(call, functions)
                    }
                    LetValue::Expr(expr) => {
                        self.walk_expr_reads(expr, scopes);
                        self.expr_type(expr, scopes)
                    }
                };
                self.declare(
                    scopes.last_mut().expect("let has a scope"),
                    var,
                    value_type,
                    true,
                    source.and_then(|source| source.binder.clone()),
                );
            }
            Statement::Set { var, value } => {
                self.walk_expr_reads(value, scopes);
                self.mark_read(scopes, var);
            }
            Statement::Return(expr) => self.walk_expr_reads(expr, scopes),
            Statement::Match { target, arms } => {
                self.walk_expr_reads(target, scopes);
                for (index, arm) in arms.iter().enumerate() {
                    scopes.push(Scope::default());
                    let target_type = self.expr_type(target, scopes);
                    self.walk_pattern(
                        &arm.pattern,
                        target_type.as_ref(),
                        source.and_then(|source| source.patterns.get(index)),
                        scopes,
                    );
                    self.walk_sequence(
                        &arm.body,
                        scopes,
                        functions,
                        type_aliases,
                        source.and_then(|source| source.bodies.get(index)),
                        warnings,
                        diagnostics,
                    );
                    let scope = scopes.pop().expect("match arm pushed a scope");
                    self.finish_scope(&scope, warnings, diagnostics);
                }
            }
            Statement::If {
                cond,
                then_body,
                else_body,
            } => {
                self.walk_expr_reads(cond, scopes);
                self.walk_scope(
                    then_body,
                    scopes,
                    functions,
                    type_aliases,
                    source.and_then(|source| source.bodies.first()),
                    warnings,
                    diagnostics,
                );
                self.walk_scope(
                    else_body,
                    scopes,
                    functions,
                    type_aliases,
                    source.and_then(|source| source.bodies.get(1)),
                    warnings,
                    diagnostics,
                );
            }
            Statement::While { cond, body } => {
                self.walk_expr_reads(cond, scopes);
                self.walk_scope(
                    body,
                    scopes,
                    functions,
                    type_aliases,
                    source.and_then(|source| source.bodies.first()),
                    warnings,
                    diagnostics,
                );
            }
            Statement::For {
                var,
                source: iter_source,
                body,
            } => {
                self.walk_expr_reads(iter_source, scopes);
                scopes.push(Scope::default());
                self.declare(
                    scopes.last_mut().expect("for has a scope"),
                    var,
                    None,
                    false,
                    None,
                );
                self.walk_sequence(
                    body,
                    scopes,
                    functions,
                    type_aliases,
                    source.and_then(|source| source.bodies.first()),
                    warnings,
                    diagnostics,
                );
                let scope = scopes.pop().expect("for pushed a scope");
                self.finish_scope(&scope, warnings, diagnostics);
            }
            Statement::Task { captures, body } => {
                for capture in captures {
                    self.mark_read(scopes, capture);
                }
                self.walk_scope(
                    body,
                    scopes,
                    functions,
                    type_aliases,
                    source.and_then(|source| source.bodies.first()),
                    warnings,
                    diagnostics,
                );
            }
            Statement::Spawn {
                captures, value, ..
            } => {
                for capture in captures {
                    self.mark_read(scopes, capture);
                }
                self.walk_expr_reads(value, scopes);
            }
            Statement::Join { handle, .. } => self.mark_read(scopes, handle),
            Statement::Break | Statement::Continue => {}
        }
    }

    fn walk_pattern(
        &mut self,
        pattern: &Pattern,
        expected_type: Option<&TypeRef>,
        source: Option<&PatternSource>,
        scopes: &mut Vec<Scope>,
    ) {
        match pattern {
            Pattern::Wildcard | Pattern::Literal(_) | Pattern::Interface(_) => {}
            Pattern::Bind(name) => self.declare(
                scopes.last_mut().expect("pattern has a scope"),
                name,
                expected_type.cloned(),
                true,
                pattern_source_span(source),
            ),
            Pattern::Enum { payload, .. } => {
                if let Some(payload) = payload {
                    self.walk_pattern(payload, None, pattern_source_child(source, 0), scopes);
                }
            }
            Pattern::Record(fields) => {
                for (name, field) in fields {
                    self.walk_pattern(field, None, pattern_source_field(source, name), scopes);
                }
            }
            Pattern::Tuple(items) | Pattern::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    self.walk_pattern(item, None, pattern_source_child(source, index), scopes);
                }
            }
            Pattern::Map(items) => {
                for (index, (key, value)) in items.iter().enumerate() {
                    self.walk_pattern(key, None, pattern_source_child(source, index * 2), scopes);
                    self.walk_pattern(
                        value,
                        None,
                        pattern_source_child(source, index * 2 + 1),
                        scopes,
                    );
                }
            }
            Pattern::Newtype { inner, .. } => self.walk_pattern(
                inner,
                expected_type,
                pattern_source_child(source, 0),
                scopes,
            ),
        }
    }

    fn walk_expr_reads(&mut self, expr: &Expr, scopes: &[Scope]) {
        match expr {
            Expr::Value(_) => {}
            Expr::VarRef(name) => self.mark_read(scopes, name),
            Expr::Mutable(inner) | Expr::Cast { from: inner, .. } => {
                self.walk_expr_reads(inner, scopes)
            }
            Expr::Reference { target, .. } => self.walk_expr_reads(target, scopes),
            Expr::Call { call, .. } => {
                for argument in &call.args {
                    self.walk_expr_reads(argument, scopes);
                }
            }
            Expr::HostCall { args, .. } | Expr::Primitive { args, .. } => {
                for argument in args {
                    self.walk_expr_reads(argument, scopes);
                }
            }
            Expr::EnumConstructor { payload, .. } => {
                if let Some(payload) = payload {
                    self.walk_expr_reads(payload, scopes);
                }
            }
            Expr::Record(fields) => {
                for value in fields.values() {
                    self.walk_expr_reads(value, scopes);
                }
            }
            Expr::Tuple(items) | Expr::Array(items) => {
                for item in items {
                    self.walk_expr_reads(item, scopes);
                }
            }
            Expr::Map(items) => {
                for (key, value) in items {
                    self.walk_expr_reads(key, scopes);
                    self.walk_expr_reads(value, scopes);
                }
            }
            Expr::Range { start, end, step } => {
                self.walk_expr_reads(start, scopes);
                self.walk_expr_reads(end, scopes);
                self.walk_expr_reads(step, scopes);
            }
            Expr::If {
                cond,
                then_e,
                else_e,
            } => {
                self.walk_expr_reads(cond, scopes);
                self.walk_expr_reads(then_e, scopes);
                self.walk_expr_reads(else_e, scopes);
            }
        }
    }

    fn expr_type(&self, expr: &Expr, scopes: &[Scope]) -> Option<TypeRef> {
        match expr {
            Expr::Value(RuntimeValue::Typed { type_ref, .. }) => Some(type_ref.clone()),
            Expr::Value(RuntimeValue::Enum { enum_key, .. }) => {
                Some(TypeRef::Named(enum_key.clone()))
            }
            Expr::VarRef(name) => self.type_of(scopes, name),
            Expr::Mutable(inner) => self.expr_type(inner, scopes),
            Expr::Reference { target, .. } => self.expr_type(target, scopes),
            Expr::Call { return_type, .. }
            | Expr::HostCall { return_type, .. }
            | Expr::Primitive { return_type, .. } => Some(return_type.clone()),
            Expr::Cast { target, .. } => Some(target.clone()),
            Expr::EnumConstructor { enum_key, .. } => Some(TypeRef::Named(enum_key.clone())),
            Expr::If { then_e, else_e, .. } => {
                let then_type = self.expr_type(then_e, scopes)?;
                let else_type = self.expr_type(else_e, scopes)?;
                (then_type == else_type).then_some(then_type)
            }
            Expr::Value(_) | Expr::Record(_) | Expr::Tuple(_) | Expr::Array(_) | Expr::Map(_) => {
                None
            }
            Expr::Range { .. } => Some(TypeRef::Range),
        }
    }

    fn warn_if_unhandled(
        &self,
        ty: Option<TypeRef>,
        source: &str,
        type_aliases: &HashMap<String, TypeAlias>,
        span: Option<&SourceSpan>,
        warnings: &mut Vec<String>,
        diagnostics: &mut Vec<BodyDiagnostic>,
    ) {
        let Some(kind) = ty.as_ref().and_then(|ty| fallible_kind(ty, type_aliases)) else {
            return;
        };
        let message = format!(
            "{UNHANDLED_RESULT_CODE}: {kind} value from `{source}` is not handled in statement position; use `match`, bind it with `let`, or discard it with `(let _ ...)`"
        );
        push_warning(
            UNHANDLED_RESULT_CODE,
            message,
            span.cloned(),
            span.cloned(),
            warnings,
            diagnostics,
        );
    }
}

fn push_warning(
    code: &'static str,
    message: String,
    span: Option<SourceSpan>,
    fix: Option<SourceSpan>,
    warnings: &mut Vec<String>,
    diagnostics: &mut Vec<BodyDiagnostic>,
) {
    warnings.push(message.clone());
    diagnostics.push(BodyDiagnostic {
        code,
        message,
        span,
        fix,
    });
}

fn pattern_source_span(source: Option<&PatternSource>) -> Option<SourceSpan> {
    match source {
        Some(PatternSource::Bind(span)) => Some(span.clone()),
        _ => None,
    }
}

fn pattern_source_child(source: Option<&PatternSource>, index: usize) -> Option<&PatternSource> {
    match source {
        Some(PatternSource::Constructor(items)) | Some(PatternSource::Items(items)) => {
            items.get(index)
        }
        Some(PatternSource::Map(items)) => items
            .get(index / 2)
            .and_then(|(key, value)| (index % 2 == 0).then_some(key).or_else(|| Some(value))),
        Some(PatternSource::Newtype(inner)) | Some(PatternSource::Interface(inner)) => {
            (index == 0).then_some(inner.as_ref())
        }
        _ => None,
    }
}

fn pattern_source_field<'a>(
    source: Option<&'a PatternSource>,
    name: &str,
) -> Option<&'a PatternSource> {
    match source {
        Some(PatternSource::Record(fields)) => fields.get(name),
        _ => None,
    }
}

fn call_return_type(call: &Call, functions: &HashMap<String, FunctionSig>) -> Option<TypeRef> {
    let signature = functions.get(&call.callee_key)?;
    let substitutions: HashMap<_, _> = signature
        .type_params
        .iter()
        .cloned()
        .zip(call.type_args.iter().cloned())
        .collect();
    Some(substitute_type(&signature.return_type, &substitutions))
}

fn fallible_kind<'a>(
    ty: &'a TypeRef,
    type_aliases: &HashMap<String, TypeAlias>,
) -> Option<&'static str> {
    let base = match ty {
        TypeRef::Mutable(inner) | TypeRef::Reference { inner, .. } => {
            return fallible_kind(inner, type_aliases)
        }
        TypeRef::Named(name) => name,
        TypeRef::Instantiated { base, .. } => base,
        _ => return None,
    };
    let alias = type_aliases.get(base)?;
    let TypeRef::Enum(tags) = &alias.body else {
        return None;
    };
    match base.rsplit('.').next()? {
        "result"
            if alias.type_params.len() == 2
                && tags.contains_key("ok")
                && tags.contains_key("err") =>
        {
            Some("result")
        }
        "option"
            if alias.type_params.len() == 1
                && tags.contains_key("some")
                && tags.contains_key("none") =>
        {
            Some("option")
        }
        _ => None,
    }
}

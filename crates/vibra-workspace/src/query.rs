//! Semantic source-position facts over one immutable workspace snapshot.
//!
//! The workspace owns the join between M1's lossless structural query, the
//! resolver's declaration identities, and the checked program's primitive
//! types.  Every lookup is made against captured bytes and is keyed by source
//! identity and spans; token spelling is never used as an identity key.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use vibra_diagnostics::{ByteSpan, DocumentRevision};
use vibra_ir::{Expr, FunctionSignature, PrimitiveType};
use vibra_resolve::{DeclarationId, EntityKind, ResolvedReference, ResolvedSnapshot};
use vibra_syntax::{
    Application, Attribute, Declaration, Expression, ExpressionKind, FloatSuffix,
    FunctionDeclaration, IntegerSuffix, Literal, NameKind, Pattern, PatternKind,
    SourceAst, StructuralQuery, TypeExpr,
};
use vibra_types::check_source;

use crate::WorkspaceSnapshot;

/// The independent availability of one semantic fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SemanticFactStatus {
    /// The fact is complete for the selected snapshot and position.
    Exact,
    /// The fact is associated with recovered syntax or a partial semantic
    /// observation.
    Recovered,
    /// The producer cannot provide this fact at the selected position.
    Unavailable,
}

impl SemanticFactStatus {
    /// Stable wire spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Recovered => "recovered",
            Self::Unavailable => "unavailable",
        }
    }
}

/// One semantic fact and its independent availability status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticFact<T> {
    status: SemanticFactStatus,
    value: Option<T>,
}

impl<T> SemanticFact<T> {
    /// Creates an exact fact, including an exact empty collection.
    #[must_use]
    pub fn exact(value: T) -> Self {
        Self {
            status: SemanticFactStatus::Exact,
            value: Some(value),
        }
    }

    /// Creates a recovered fact with an optional partial value.
    #[must_use]
    pub fn recovered(value: Option<T>) -> Self {
        Self {
            status: SemanticFactStatus::Recovered,
            value,
        }
    }

    /// Creates an unavailable fact.
    #[must_use]
    pub const fn unavailable() -> Self {
        Self {
            status: SemanticFactStatus::Unavailable,
            value: None,
        }
    }

    /// Availability status.
    #[must_use]
    pub const fn status(&self) -> SemanticFactStatus {
        self.status
    }

    /// Value, when the producer has one.
    #[must_use]
    pub const fn value(&self) -> Option<&T> {
        self.value.as_ref()
    }
}

/// A canonical declaration or lexical identity returned by a query.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct QueryIdentity {
    kind: String,
    canonical: String,
}

impl QueryIdentity {
    /// Creates an identity from its closed entity kind and canonical spelling.
    #[must_use]
    pub fn new(kind: impl Into<String>, canonical: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            canonical: canonical.into(),
        }
    }

    /// Entity kind spelling.
    #[must_use]
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// Canonical identity spelling.
    #[must_use]
    pub fn canonical(&self) -> &str {
        &self.canonical
    }
}

/// A structured primitive or function type for tooling consumers.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SemanticType {
    kind: SemanticTypeKind,
    name: String,
    parameters: Vec<Self>,
    result: Option<Box<Self>>,
    labelled: Vec<LabelledType>,
}

/// The closed type shape used by the M2 semantic query schema.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SemanticTypeKind {
    /// A primitive type such as `str` or `i32`.
    Primitive,
    /// A monomorphic function type.
    Function,
}

impl SemanticType {
    /// Creates a primitive type.
    #[must_use]
    pub fn primitive(name: impl Into<String>) -> Self {
        Self {
            kind: SemanticTypeKind::Primitive,
            name: name.into(),
            parameters: Vec::new(),
            result: None,
            labelled: Vec::new(),
        }
    }

    /// Creates a function type.
    #[must_use]
    pub fn function(
        parameters: Vec<Self>,
        result: Self,
        labelled: Vec<LabelledType>,
    ) -> Self {
        Self {
            kind: SemanticTypeKind::Function,
            name: "fn".to_owned(),
            parameters,
            result: Some(Box::new(result)),
            labelled,
        }
    }

    /// Type shape.
    #[must_use]
    pub const fn kind(&self) -> SemanticTypeKind {
        self.kind
    }

    /// Primitive name or `fn`.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Function parameters, empty for primitives.
    #[must_use]
    pub fn parameters(&self) -> &[Self] {
        &self.parameters
    }

    /// Function result, absent for primitives.
    #[must_use]
    pub fn result(&self) -> Option<&Self> {
        self.result.as_deref()
    }

    /// Labelled function slots, empty for primitives.
    #[must_use]
    pub fn labelled(&self) -> &[LabelledType] {
        &self.labelled
    }
}

/// One labelled slot in a function type or application contract.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LabelledType {
    name: String,
    value_type: SemanticType,
}

impl LabelledType {
    /// Creates a labelled slot.
    #[must_use]
    pub fn new(name: impl Into<String>, value_type: SemanticType) -> Self {
        Self {
            name: name.into(),
            value_type,
        }
    }

    /// Label without its trailing colon.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Labelled slot type.
    #[must_use]
    pub const fn value_type(&self) -> &SemanticType {
        &self.value_type
    }
}

/// A visible lexical binder at a source position.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LocalBinding {
    name: String,
    identity: String,
}

impl LocalBinding {
    /// Creates a lexical binder record.
    #[must_use]
    pub fn new(name: impl Into<String>, identity: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            identity: identity.into(),
        }
    }

    /// Source spelling of the local name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Revision-scoped binder identity.
    #[must_use]
    pub fn identity(&self) -> &str {
        &self.identity
    }
}

/// An imported module alias visible at a source position.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ImportAlias {
    alias: String,
    module: Option<String>,
    source_id: String,
    span: ByteSpan,
}

impl ImportAlias {
    /// Creates an import observation.
    #[must_use]
    pub fn new(
        alias: impl Into<String>,
        module: Option<String>,
        source_id: impl Into<String>,
        span: ByteSpan,
    ) -> Self {
        Self {
            alias: alias.into(),
            module,
            source_id: source_id.into(),
            span,
        }
    }

    /// Local alias without its marker.
    #[must_use]
    pub fn alias(&self) -> &str {
        &self.alias
    }

    /// Canonical module identity, when resolution succeeded.
    #[must_use]
    pub fn module(&self) -> Option<&str> {
        self.module.as_deref()
    }

    /// Source identity of the import declaration.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// Import declaration span.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

/// The supported M2 function application contract.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ApplicationContract {
    callee: Option<QueryIdentity>,
    callee_type: Option<SemanticType>,
    positional: Vec<SemanticType>,
    labelled: Vec<LabelledType>,
    result_type: SemanticType,
}

impl ApplicationContract {
    /// Creates a supported function contract.
    #[must_use]
    pub fn new(
        callee: Option<QueryIdentity>,
        callee_type: Option<SemanticType>,
        positional: Vec<SemanticType>,
        labelled: Vec<LabelledType>,
        result_type: SemanticType,
    ) -> Self {
        Self {
            callee,
            callee_type,
            positional,
            labelled,
            result_type,
        }
    }

    /// Resolved callee identity, when available.
    #[must_use]
    pub const fn callee(&self) -> Option<&QueryIdentity> {
        self.callee.as_ref()
    }

    /// Resolved callable type, when available.
    #[must_use]
    pub const fn callee_type(&self) -> Option<&SemanticType> {
        self.callee_type.as_ref()
    }

    /// Positional operand contract.
    #[must_use]
    pub fn positional(&self) -> &[SemanticType] {
        &self.positional
    }

    /// Labelled operand contract.
    #[must_use]
    pub fn labelled(&self) -> &[LabelledType] {
        &self.labelled
    }

    /// Exact result type.
    #[must_use]
    pub const fn result_type(&self) -> &SemanticType {
        &self.result_type
    }
}

/// One complete semantic source-position query result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspacePositionQuery {
    structural: StructuralQuery,
    workspace_revision: DocumentRevision,
    source_id: String,
    node_id: String,
    role: SemanticFact<String>,
    context: SemanticFact<String>,
    identity: SemanticFact<QueryIdentity>,
    expected_type: SemanticFact<SemanticType>,
    observed_type: SemanticFact<SemanticType>,
    visible_locals: SemanticFact<Vec<LocalBinding>>,
    visible_imports: SemanticFact<Vec<ImportAlias>>,
    declaration_candidates: SemanticFact<Vec<QueryIdentity>>,
    application: SemanticFact<ApplicationContract>,
}

impl WorkspacePositionQuery {
    /// Embedded M1 structural result.
    #[must_use]
    pub const fn structural(&self) -> &StructuralQuery {
        &self.structural
    }

    /// Exact snapshot revision used for every fact.
    #[must_use]
    pub const fn workspace_revision(&self) -> &DocumentRevision {
        &self.workspace_revision
    }

    /// Source identity owning the query.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// Revision-scoped selected-node locator.
    #[must_use]
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    /// Role fact.
    #[must_use]
    pub const fn role(&self) -> &SemanticFact<String> {
        &self.role
    }

    /// Context fact.
    #[must_use]
    pub const fn context(&self) -> &SemanticFact<String> {
        &self.context
    }

    /// Identity fact.
    #[must_use]
    pub const fn identity(&self) -> &SemanticFact<QueryIdentity> {
        &self.identity
    }

    /// Expected type fact.
    #[must_use]
    pub const fn expected_type(&self) -> &SemanticFact<SemanticType> {
        &self.expected_type
    }

    /// Observed type fact.
    #[must_use]
    pub const fn observed_type(&self) -> &SemanticFact<SemanticType> {
        &self.observed_type
    }

    /// Visible lexical locals.
    #[must_use]
    pub const fn visible_locals(&self) -> &SemanticFact<Vec<LocalBinding>> {
        &self.visible_locals
    }

    /// Visible imported aliases.
    #[must_use]
    pub const fn visible_imports(&self) -> &SemanticFact<Vec<ImportAlias>> {
        &self.visible_imports
    }

    /// Candidate declaration identities.
    #[must_use]
    pub const fn declaration_candidates(&self) -> &SemanticFact<Vec<QueryIdentity>> {
        &self.declaration_candidates
    }

    /// Supported function application contract.
    #[must_use]
    pub const fn application(&self) -> &SemanticFact<ApplicationContract> {
        &self.application
    }
}

/// A failure while querying a captured workspace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceQueryError {
    /// The requested source identity is not in the snapshot.
    UnknownSource(String),
    /// A captured source module is not UTF-8.
    InvalidUtf8(String),
    /// The source document could not be loaded by the selected parser.
    Parse(String),
    /// The M1 structural query rejected the position.
    Structural(vibra_syntax::QueryError),
    /// The workspace orchestration failed before semantic facts were joined.
    Workspace(String),
}

impl fmt::Display for WorkspaceQueryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownSource(source_id) => {
                write!(
                    formatter,
                    "source `{source_id}` is not in the workspace snapshot"
                )
            }
            Self::InvalidUtf8(source_id) => {
                write!(formatter, "source `{source_id}` is not valid UTF-8")
            }
            Self::Parse(message) => formatter.write_str(message),
            Self::Structural(error) => error.fmt(formatter),
            Self::Workspace(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for WorkspaceQueryError {}

/// Queries one captured source module without touching the filesystem.
pub fn query_position(
    workspace: &WorkspaceSnapshot,
    source_id: &str,
    offset: usize,
) -> Result<WorkspacePositionQuery, WorkspaceQueryError> {
    let document = workspace
        .source()
        .documents()
        .find(|document| document.source_id() == source_id)
        .ok_or_else(|| WorkspaceQueryError::UnknownSource(source_id.to_owned()))?;
    let source = std::str::from_utf8(document.bytes())
        .map_err(|_| WorkspaceQueryError::InvalidUtf8(source_id.to_owned()))?;
    let syntax = vibra_syntax::parse_source(Path::new(source_id), source)
        .map_err(|error| WorkspaceQueryError::Parse(error.to_string()))?;
    let structural = syntax
        .query_position(offset)
        .map_err(WorkspaceQueryError::Structural)?;
    let resolved = workspace
        .resolve()
        .map_err(|error| WorkspaceQueryError::Workspace(error.to_string()))?;
    let catalog = CallableCatalog::build(workspace, &resolved);
    let mut collector =
        SemanticCollector::new(source_id, &resolved, &catalog, source.len());
    if let Some(ast) = syntax.ast() {
        collector.collect_ast(ast);
    }
    let checked = check_source(source_id, source);
    if let Some(program) = checked.program() {
        collector.collect_ir(program.entry().body());
        for global in program.globals() {
            collector.collect_ir(global.initializer());
        }
        for function in program.functions() {
            collector.collect_ir(function.body());
        }
    }
    Ok(collector.finish(structural, workspace.revision().clone()))
}

#[derive(Clone, Debug)]
struct CallableInfo {
    identity: QueryIdentity,
    ty: SemanticType,
}

#[derive(Clone, Debug, Default)]
struct CallableCatalog {
    by_identity: BTreeMap<String, CallableInfo>,
}

impl CallableCatalog {
    fn build(workspace: &WorkspaceSnapshot, resolved: &ResolvedSnapshot) -> Self {
        let package = workspace.project().project().package();
        let mut catalog = Self::default();
        for unit in workspace.source().units() {
            for document in unit.modules() {
                let Ok(source) = std::str::from_utf8(document.bytes()) else {
                    continue;
                };
                let Ok(parsed) =
                    vibra_syntax::parse_source(Path::new(document.source_id()), source)
                else {
                    continue;
                };
                let Some(ast) = parsed.ast() else {
                    continue;
                };
                for declaration in ast.declarations() {
                    let Declaration::Defn(function) = declaration else {
                        continue;
                    };
                    let Some(ty) = function_type(function) else {
                        continue;
                    };
                    let id = DeclarationId::new(
                        package.name().value(),
                        package.version().value(),
                        unit.name(),
                        document.module_segments().iter().cloned(),
                        [function.name().value().to_owned()],
                        EntityKind::Function,
                    );
                    let canonical = id.canonical();
                    let kind = id.kind().as_str().to_owned();
                    let identity = QueryIdentity::new(kind, canonical.clone());
                    if resolved
                        .declarations()
                        .iter()
                        .any(|declaration| declaration.id().canonical() == canonical)
                    {
                        catalog
                            .by_identity
                            .insert(canonical, CallableInfo { identity, ty });
                    }
                }
            }
        }
        catalog
    }
}

#[derive(Clone, Debug)]
struct ExpressionSite {
    span: ByteSpan,
    role: String,
    context: String,
    identity: Option<QueryIdentity>,
    expected_type: Option<SemanticType>,
    observed_type: Option<SemanticType>,
    application: Option<ApplicationContract>,
}

#[derive(Clone, Debug)]
struct BinderSite {
    span: ByteSpan,
    binding: LocalBinding,
}

#[derive(Clone, Debug)]
struct ScopeSite {
    span: ByteSpan,
    locals: Vec<LocalBinding>,
}

#[derive(Clone, Debug)]
struct PatternSite {
    span: ByteSpan,
    role: String,
    context: String,
}

#[derive(Clone, Debug)]
struct SemanticCollector<'a> {
    source_id: &'a str,
    resolved: &'a ResolvedSnapshot,
    catalog: &'a CallableCatalog,
    source_length: usize,
    expressions: Vec<ExpressionSite>,
    binders: Vec<BinderSite>,
    scopes: Vec<ScopeSite>,
    patterns: Vec<PatternSite>,
    ir_sites: Vec<IrSite>,
    ir_expected_sites: Vec<IrExpectedSite>,
}

impl<'a> SemanticCollector<'a> {
    fn new(
        source_id: &'a str,
        resolved: &'a ResolvedSnapshot,
        catalog: &'a CallableCatalog,
        source_length: usize,
    ) -> Self {
        Self {
            source_id,
            resolved,
            catalog,
            source_length,
            expressions: Vec::new(),
            binders: Vec::new(),
            scopes: Vec::new(),
            patterns: Vec::new(),
            ir_sites: Vec::new(),
            ir_expected_sites: Vec::new(),
        }
    }

    fn collect_ast(&mut self, ast: &SourceAst) {
        for declaration in ast.declarations() {
            match declaration {
                Declaration::Def(value) => {
                    self.collect_expression(
                        value.expression(),
                        &[],
                        "initializer",
                        semantic_type_expr(value.value_type()),
                    );
                }
                Declaration::Defn(function) => self.collect_function(function),
                Declaration::Test(test) => {
                    for (index, expression) in test.expressions().iter().enumerate() {
                        self.collect_expression(
                            expression,
                            &[],
                            if index + 1 == test.expressions().len() {
                                "result"
                            } else {
                                "function"
                            },
                            None,
                        );
                    }
                }
                Declaration::Import(_)
                | Declaration::Deftype(_)
                | Declaration::Defint(_)
                | Declaration::Deffect(_) => {}
            }
        }
    }

    fn collect_function(&mut self, function: &FunctionDeclaration) {
        let mut locals = Vec::new();
        for parameter in function.parameters() {
            self.collect_pattern_binding(
                parameter.parsed_pattern(),
                &mut locals,
                "parameter",
            );
        }
        for attribute in function.attributes().items() {
            if let Attribute::Labelled(parameters) = attribute {
                for parameter in parameters {
                    if parameter.name().kind() != NameKind::Discard {
                        let binding = self
                            .local_binding(parameter.name().value(), parameter.span());
                        self.binders.push(BinderSite {
                            span: parameter.span(),
                            binding: binding.clone(),
                        });
                        locals.push(binding);
                    } else {
                        self.patterns.push(PatternSite {
                            span: parameter.span(),
                            role: "@discard".to_owned(),
                            context: "parameter".to_owned(),
                        });
                    }
                }
            }
        }
        self.scopes.push(ScopeSite {
            span: function.span(),
            locals: locals.clone(),
        });
        for (index, expression) in function.expressions().iter().enumerate() {
            self.collect_expression(
                expression,
                &locals,
                if index + 1 == function.expressions().len() {
                    "result"
                } else {
                    "function"
                },
                if index + 1 == function.expressions().len() {
                    semantic_type_expr(function.result())
                } else {
                    None
                },
            );
        }
    }

    fn collect_pattern_binding(
        &mut self,
        pattern: &Pattern,
        locals: &mut Vec<LocalBinding>,
        context: &str,
    ) {
        match pattern.kind() {
            PatternKind::Binding(name) if !name.is_discard() => {
                let binding = self.local_binding(name.value(), pattern.span());
                self.binders.push(BinderSite {
                    span: pattern.span(),
                    binding: binding.clone(),
                });
                locals.push(binding);
            }
            PatternKind::Binding(_) => {
                self.patterns.push(PatternSite {
                    span: pattern.span(),
                    role: "@discard".to_owned(),
                    context: context.to_owned(),
                });
            }
            PatternKind::Tuple(patterns) | PatternKind::Array(patterns) => {
                for pattern in patterns {
                    self.collect_pattern_binding(pattern, locals, context);
                }
            }
            PatternKind::Constructor { arguments, .. } => {
                for argument in arguments {
                    self.collect_pattern_binding(argument.pattern(), locals, context);
                }
            }
            PatternKind::As { pattern, .. } => {
                self.collect_pattern_binding(pattern, locals, context);
            }
            PatternKind::Literal(_) => {}
        }
    }

    fn local_binding(&self, name: &str, span: ByteSpan) -> LocalBinding {
        LocalBinding::new(
            name,
            format!("binder:{}:{}-{}", self.source_id, span.start(), span.end()),
        )
    }

    fn collect_expression(
        &mut self,
        expression: &Expression,
        locals: &[LocalBinding],
        context: &str,
        expected_type: Option<SemanticType>,
    ) {
        let (role, identity) = self.role_and_identity(expression, locals);
        let application = match expression.kind() {
            ExpressionKind::Application(application) => {
                self.application_contract(application)
            }
            _ => None,
        };
        self.expressions.push(ExpressionSite {
            span: expression.span(),
            role,
            context: context.to_owned(),
            identity,
            expected_type: expected_type.clone(),
            observed_type: expression_observed_type(expression, expected_type.as_ref()),
            application,
        });

        match expression.kind() {
            ExpressionKind::Application(application) => {
                self.collect_expression(application.callee(), locals, "function", None);
                let contract = self.application_contract(application);
                let mut positional_index = 0;
                for argument in application.arguments() {
                    let expected = if let Some(label) = argument.label() {
                        contract.as_ref().and_then(|contract| {
                            contract
                                .labelled()
                                .iter()
                                .find(|slot| slot.name() == label.value())
                                .map(|slot| slot.value_type().clone())
                        })
                    } else {
                        let expected = contract.as_ref().and_then(|contract| {
                            contract.positional().get(positional_index).cloned()
                        });
                        positional_index = positional_index.saturating_add(1);
                        expected
                    };
                    self.collect_expression(
                        argument.value(),
                        locals,
                        "argument",
                        expected,
                    );
                }
            }
            ExpressionKind::Do(expressions) => {
                for (index, expression) in expressions.iter().enumerate() {
                    self.collect_expression(
                        expression,
                        locals,
                        context,
                        if index + 1 == expressions.len() {
                            expected_type.clone()
                        } else {
                            None
                        },
                    );
                }
            }
            ExpressionKind::Let {
                pattern,
                value,
                body,
            } => {
                self.collect_expression(value, locals, "let-value", None);
                let mut nested = locals.to_vec();
                self.collect_pattern_binding(pattern, &mut nested, "let-value");
                if let (Some(first), Some(last)) = (body.first(), body.last()) {
                    self.scopes.push(ScopeSite {
                        span: first.span().join(last.span()),
                        locals: nested.clone(),
                    });
                }
                for (index, expression) in body.iter().enumerate() {
                    self.collect_expression(
                        expression,
                        &nested,
                        "let-body",
                        if index + 1 == body.len() {
                            expected_type.clone()
                        } else {
                            None
                        },
                    );
                }
            }
            ExpressionKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.collect_expression(
                    condition,
                    locals,
                    "branch",
                    Some(SemanticType::primitive("bool")),
                );
                self.collect_expression(
                    then_branch,
                    locals,
                    "branch",
                    expected_type.clone(),
                );
                self.collect_expression(else_branch, locals, "branch", expected_type);
            }
            ExpressionKind::Lambda(lambda) => {
                let mut nested = locals.to_vec();
                for parameter in lambda.parameters() {
                    self.collect_pattern_binding(
                        parameter.parsed_pattern(),
                        &mut nested,
                        "parameter",
                    );
                }
                self.scopes.push(ScopeSite {
                    span: lambda.span(),
                    locals: nested.clone(),
                });
                for (index, body) in lambda.body().iter().enumerate() {
                    self.collect_expression(
                        body,
                        &nested,
                        "lambda",
                        if index + 1 == lambda.body().len() {
                            semantic_type_expr(lambda.result())
                        } else {
                            None
                        },
                    );
                }
            }
            ExpressionKind::As {
                value_type,
                operand,
            } => {
                self.collect_expression(
                    operand,
                    locals,
                    context,
                    semantic_type_expr(value_type),
                );
            }
            ExpressionKind::Try(operand) => {
                self.collect_expression(operand, locals, context, expected_type);
            }
            ExpressionKind::Literal(_)
            | ExpressionKind::Name(_)
            | ExpressionKind::Match { .. } => {}
        }
    }

    fn role_and_identity(
        &self,
        expression: &Expression,
        locals: &[LocalBinding],
    ) -> (String, Option<QueryIdentity>) {
        match expression.kind() {
            ExpressionKind::Literal(_) => ("@literal".to_owned(), None),
            ExpressionKind::Application(_) => ("@application".to_owned(), None),
            ExpressionKind::Name(name) if name.is_discard() => {
                ("@discard".to_owned(), None)
            }
            ExpressionKind::Name(name) if name.kind() == NameKind::Atom => {
                ("@atom-value".to_owned(), None)
            }
            ExpressionKind::Name(name) if name.kind() == NameKind::Symbol => {
                if let Some(binding) = locals.iter().rev().find(|binding| {
                    binding.name() == name.segments().first().map_or("", String::as_str)
                }) {
                    return (
                        "@local-binding".to_owned(),
                        Some(QueryIdentity::new("binder", binding.identity())),
                    );
                }
                if let Some(reference) = self.reference_for_span(expression.span())
                    && let Some(target) = reference.target()
                {
                    return (
                        "@entity-reference".to_owned(),
                        Some(query_identity(target)),
                    );
                }
                ("@unknown".to_owned(), None)
            }
            ExpressionKind::Do(_)
            | ExpressionKind::Let { .. }
            | ExpressionKind::If { .. }
            | ExpressionKind::Lambda(_)
            | ExpressionKind::Match { .. }
            | ExpressionKind::As { .. }
            | ExpressionKind::Try(_) => ("@unknown".to_owned(), None),
            ExpressionKind::Name(_) => ("@unknown".to_owned(), None),
        }
    }

    fn reference_for_span(&self, span: ByteSpan) -> Option<&ResolvedReference> {
        self.resolved
            .references()
            .iter()
            .find(|reference| {
                reference.source_id() == self.source_id && reference.span() == span
            })
            .or_else(|| {
                self.resolved.references().iter().find(|reference| {
                    reference.source_id() == self.source_id
                        && reference.span().start() <= span.start()
                        && reference.span().end() >= span.end()
                })
            })
    }

    fn application_contract(
        &self,
        application: &Application,
    ) -> Option<ApplicationContract> {
        let reference = self.reference_for_span(application.callee().span())?;
        let target = reference.target()?;
        let info = self.catalog.by_identity.get(&target.canonical())?;
        let SemanticTypeKind::Function = info.ty.kind() else {
            return None;
        };
        Some(ApplicationContract::new(
            Some(info.identity.clone()),
            Some(info.ty.clone()),
            info.ty.parameters().to_vec(),
            info.ty.labelled().to_vec(),
            info.ty.result()?.clone(),
        ))
    }

    fn collect_ir(&mut self, expression: &Expr) {
        let application = ir_application_contract(expression);
        if let Expr::Call {
            callee: Some(callee),
            arguments,
            ..
        } = expression
            && let PrimitiveType::Function(signature) = callee.result_type()
        {
            let mut expected_types = signature.parameters().to_vec();
            expected_types.extend(
                signature
                    .labelled()
                    .iter()
                    .map(|parameter| parameter.value_type().clone()),
            );
            for (argument, expected_type) in arguments.iter().zip(expected_types) {
                self.ir_expected_sites.push(IrExpectedSite {
                    source_id: argument.origin().source_id().to_owned(),
                    span: argument.origin().span(),
                    expected_type: semantic_type_primitive(&expected_type),
                });
            }
        }
        // The IR is immutable and every node carries its own source origin.
        // Recursion follows only the checked enum, never source text.
        match expression {
            Expr::Literal { .. }
            | Expr::Default { .. }
            | Expr::Variable { .. }
            | Expr::Global { .. }
            | Expr::Function { .. }
            | Expr::Captured { .. } => {}
            Expr::External { arguments, .. }
            | Expr::Sequence {
                expressions: arguments,
                ..
            } => {
                for argument in arguments {
                    self.collect_ir(argument);
                }
            }
            Expr::Closure { captures, body, .. } => {
                for capture in captures {
                    self.collect_ir(capture);
                }
                self.collect_ir(body);
            }
            Expr::Let { value, body, .. } => {
                self.collect_ir(value);
                self.collect_ir(body);
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.collect_ir(condition);
                self.collect_ir(then_branch);
                self.collect_ir(else_branch);
            }
            Expr::Call {
                arguments, callee, ..
            } => {
                if let Some(callee) = callee {
                    self.collect_ir(callee);
                }
                for argument in arguments {
                    self.collect_ir(argument);
                }
            }
        }
        // Keep IR facts in a side list through the expression site shape. The
        // final lookup uses the origin directly and does not mutate syntax
        // facts, so recovered siblings remain independent.
        self.ir_sites.push(IrSite {
            source_id: expression.origin().source_id().to_owned(),
            span: expression.origin().span(),
            ty: semantic_type_primitive(&expression.result_type()),
            application,
        });
    }

    fn finish(
        self,
        structural: StructuralQuery,
        revision: DocumentRevision,
    ) -> WorkspacePositionQuery {
        let source_id = self.source_id.to_owned();
        let span = structural.span();
        let node_id = format!("{}#{}-{}", source_id, span.start(), span.end());
        let structural_status = match structural.status() {
            vibra_syntax::FactStatus::Exact => SemanticFactStatus::Exact,
            vibra_syntax::FactStatus::Recovered => SemanticFactStatus::Recovered,
            vibra_syntax::FactStatus::Unavailable => SemanticFactStatus::Unavailable,
        };
        let ir_observation = self
            .best_ir(structural.offset())
            .map(|site| (site.ty.clone(), site.application.clone()));
        let (role, context, identity, expected_type, application) = if structural_status
            == SemanticFactStatus::Unavailable
        {
            (
                SemanticFact::exact("@unknown".to_owned()),
                SemanticFact::exact("trivia".to_owned()),
                SemanticFact::unavailable(),
                SemanticFact::unavailable(),
                SemanticFact::unavailable(),
            )
        } else if structural_status == SemanticFactStatus::Recovered {
            (
                SemanticFact::recovered(Some("@unknown".to_owned())),
                SemanticFact::recovered(Some("recovery".to_owned())),
                SemanticFact::unavailable(),
                SemanticFact::unavailable(),
                SemanticFact::unavailable(),
            )
        } else {
            let expression = self.best_expression(structural.offset());
            let binder = self.best_binder(structural.offset());
            let pattern = self.best_pattern(structural.offset());
            let declaration = self.declaration_at(structural.offset());
            let role = pattern
                .map(|site| site.role.clone())
                .or_else(|| binder.map(|_| "@local-binding".to_owned()))
                .or_else(|| expression.map(|site| site.role.clone()))
                .or_else(|| declaration.as_ref().map(|_| "@declaration".to_owned()))
                .unwrap_or_else(|| "@unknown".to_owned());
            let context = pattern
                .map(|site| site.context.clone())
                .or_else(|| expression.map(|site| site.context.clone()))
                .or_else(|| binder.map(|_| "parameter".to_owned()))
                .or_else(|| declaration.as_ref().map(|_| "module".to_owned()))
                .unwrap_or_else(|| "module".to_owned());
            let is_discard = pattern.is_some_and(|site| site.role == "@discard");
            let identity = (!is_discard)
                .then(|| {
                    binder
                        .map(|site| {
                            QueryIdentity::new("binder", site.binding.identity())
                        })
                        .or_else(|| expression.and_then(|site| site.identity.clone()))
                        .or_else(|| {
                            (binder.is_none() && expression.is_none())
                                .then(|| declaration.map(|(_, identity)| identity))
                                .flatten()
                        })
                })
                .flatten();
            let expected = (!is_discard)
                .then(|| {
                    expression
                        .and_then(|site| site.expected_type.clone())
                        .or_else(|| self.best_ir_expected(structural.offset()))
                })
                .flatten();
            let application = (!is_discard)
                .then(|| {
                    expression
                        .and_then(|site| site.application.clone())
                        .or_else(|| {
                            ir_observation
                                .as_ref()
                                .and_then(|(_, application)| application.clone())
                        })
                })
                .flatten();
            (
                SemanticFact::exact(role),
                SemanticFact::exact(context),
                identity.map_or_else(SemanticFact::unavailable, SemanticFact::exact),
                expected.map_or_else(SemanticFact::unavailable, SemanticFact::exact),
                application.map_or_else(SemanticFact::unavailable, SemanticFact::exact),
            )
        };

        let visible_locals = if structural_status == SemanticFactStatus::Exact {
            SemanticFact::exact(
                self.scopes
                    .iter()
                    .filter(|scope| {
                        contains(scope.span, structural.offset(), self.source_length)
                    })
                    .min_by_key(|scope| scope.span.len())
                    .map(|scope| scope.locals.clone())
                    .unwrap_or_default(),
            )
        } else {
            SemanticFact::unavailable()
        };
        let visible_imports = if structural_status == SemanticFactStatus::Exact {
            SemanticFact::exact(
                self.resolved
                    .imports()
                    .iter()
                    .filter(|import| import.source_id() == self.source_id)
                    .map(|import| {
                        ImportAlias::new(
                            import.alias(),
                            import.module().map(|module| module.as_atom()),
                            import.source_id(),
                            import.span(),
                        )
                    })
                    .collect(),
            )
        } else {
            SemanticFact::unavailable()
        };
        let candidates = if structural_status == SemanticFactStatus::Exact {
            identity
                .value()
                .cloned()
                .filter(|identity| identity.kind() != "binder")
                .map(|identity| vec![identity])
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let declaration_candidates = if structural_status == SemanticFactStatus::Exact {
            SemanticFact::exact(candidates)
        } else {
            SemanticFact::unavailable()
        };
        let observed_type = self
            .best_pattern(structural.offset())
            .is_none()
            .then(|| {
                ir_observation
                    .as_ref()
                    .and_then(|(ty, _)| ty.clone())
                    .or_else(|| {
                        self.best_expression(structural.offset())
                            .and_then(|site| site.observed_type.clone())
                    })
            })
            .flatten()
            .map_or_else(SemanticFact::unavailable, SemanticFact::exact);
        WorkspacePositionQuery {
            structural,
            workspace_revision: revision,
            source_id,
            node_id,
            role,
            context,
            identity,
            expected_type,
            observed_type,
            visible_locals,
            visible_imports,
            declaration_candidates,
            application,
        }
    }

    fn best_expression(&self, offset: usize) -> Option<&ExpressionSite> {
        self.expressions
            .iter()
            .filter(|site| contains(site.span, offset, self.source_length))
            .min_by_key(|site| site.span.len())
    }

    fn best_binder(&self, offset: usize) -> Option<&BinderSite> {
        self.binders
            .iter()
            .filter(|site| contains(site.span, offset, self.source_length))
            .min_by_key(|site| site.span.len())
    }

    fn best_pattern(&self, offset: usize) -> Option<&PatternSite> {
        self.patterns
            .iter()
            .filter(|site| contains(site.span, offset, self.source_length))
            .min_by_key(|site| site.span.len())
    }

    fn declaration_at(&self, offset: usize) -> Option<(ByteSpan, QueryIdentity)> {
        self.resolved
            .declarations()
            .iter()
            .filter(|declaration| {
                declaration.source_id() == self.source_id
                    && contains(declaration.span(), offset, self.source_length)
            })
            .min_by_key(|declaration| declaration.span().len())
            .map(|declaration| (declaration.span(), query_identity(declaration.id())))
    }

    fn best_ir(&self, offset: usize) -> Option<&IrSite> {
        self.ir_sites
            .iter()
            .filter(|site| {
                site.source_id == self.source_id
                    && contains(site.span, offset, self.source_length)
            })
            .min_by_key(|site| site.span.len())
    }

    fn best_ir_expected(&self, offset: usize) -> Option<SemanticType> {
        self.ir_expected_sites
            .iter()
            .filter(|site| {
                site.source_id == self.source_id
                    && contains(site.span, offset, self.source_length)
            })
            .min_by_key(|site| site.span.len())
            .and_then(|site| site.expected_type.clone())
    }
}

#[derive(Clone, Debug)]
struct IrSite {
    source_id: String,
    span: ByteSpan,
    ty: Option<SemanticType>,
    application: Option<ApplicationContract>,
}

#[derive(Clone, Debug)]
struct IrExpectedSite {
    source_id: String,
    span: ByteSpan,
    expected_type: Option<SemanticType>,
}

fn contains(span: ByteSpan, offset: usize, source_length: usize) -> bool {
    !span.is_empty()
        && span.start() <= offset
        && (offset < span.end()
            || (offset == source_length && span.end() == source_length))
}

fn query_identity(id: &DeclarationId) -> QueryIdentity {
    QueryIdentity::new(id.kind().as_str(), id.canonical())
}

fn semantic_type_expr(value: &TypeExpr) -> Option<SemanticType> {
    match value {
        TypeExpr::Void => Some(SemanticType::primitive("void")),
        TypeExpr::Name(name) => {
            primitive_name(name.value()).map(SemanticType::primitive)
        }
        TypeExpr::Function(function) => {
            let parameters = function
                .parameters()
                .iter()
                .map(semantic_type_expr)
                .collect::<Option<Vec<_>>>()?;
            let labelled = function
                .labelled()
                .iter()
                .map(|slot| {
                    Some(LabelledType::new(
                        slot.name().value(),
                        semantic_type_expr(slot.value_type())?,
                    ))
                })
                .collect::<Option<Vec<_>>>()?;
            Some(SemanticType::function(
                parameters,
                semantic_type_expr(function.result())?,
                labelled,
            ))
        }
        TypeExpr::Applied { .. }
        | TypeExpr::Tuple(_)
        | TypeExpr::Array(_)
        | TypeExpr::Map(_, _) => None,
    }
}

fn primitive_name(value: &str) -> Option<&'static str> {
    Some(match value {
        "bool" => "bool",
        "char" => "char",
        "str" => "str",
        "bytes" => "bytes",
        "atom" => "atom",
        "i8" => "i8",
        "i16" => "i16",
        "i32" => "i32",
        "i64" => "i64",
        "u8" => "u8",
        "u16" => "u16",
        "u32" => "u32",
        "u64" => "u64",
        "f32" => "f32",
        "f64" => "f64",
        _ => return None,
    })
}

fn semantic_type_primitive(value: &PrimitiveType) -> Option<SemanticType> {
    match value {
        PrimitiveType::Function(signature) => Some(semantic_type_signature(signature)),
        _ => Some(SemanticType::primitive(value.as_str())),
    }
}

fn semantic_type_signature(signature: &FunctionSignature) -> SemanticType {
    let labelled: Vec<LabelledType> = signature
        .labelled()
        .iter()
        .map(|parameter| {
            LabelledType::new(
                parameter.name(),
                semantic_type_primitive(&parameter.value_type())
                    .unwrap_or_else(|| SemanticType::primitive("void")),
            )
        })
        .collect();
    SemanticType::function(
        signature
            .parameters()
            .iter()
            .map(|parameter| {
                semantic_type_primitive(parameter)
                    .unwrap_or_else(|| SemanticType::primitive("void"))
            })
            .collect(),
        semantic_type_primitive(&signature.result())
            .unwrap_or_else(|| SemanticType::primitive("void")),
        labelled,
    )
}

fn expression_observed_type(
    expression: &Expression,
    expected: Option<&SemanticType>,
) -> Option<SemanticType> {
    match expression.kind() {
        ExpressionKind::Literal(literal) => literal_type(literal, expected),
        ExpressionKind::Name(name) if name.kind() == NameKind::Atom => {
            Some(SemanticType::primitive("atom"))
        }
        ExpressionKind::Application(_) => expected.cloned(),
        _ => None,
    }
}

fn literal_type(
    literal: &Literal,
    expected: Option<&SemanticType>,
) -> Option<SemanticType> {
    let primitive = match literal {
        Literal::String(_) => Some("str"),
        Literal::Character(_) => Some("char"),
        Literal::Boolean(_) => Some("bool"),
        Literal::Void(_) => Some("void"),
        Literal::Integer(value) => value
            .suffix()
            .map(integer_suffix_name)
            .or_else(|| expected.and_then(numeric_expected_name)),
        Literal::Float(value) => value
            .suffix()
            .map(float_suffix_name)
            .or_else(|| expected.and_then(float_expected_name)),
    };
    primitive.map(SemanticType::primitive)
}

fn integer_suffix_name(suffix: IntegerSuffix) -> &'static str {
    suffix.as_str()
}

fn float_suffix_name(suffix: FloatSuffix) -> &'static str {
    suffix.as_str()
}

fn numeric_expected_name(value: &SemanticType) -> Option<&str> {
    matches!(
        value.name(),
        "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64"
    )
    .then_some(value.name())
}

fn float_expected_name(value: &SemanticType) -> Option<&str> {
    matches!(value.name(), "f32" | "f64").then_some(value.name())
}

fn ir_application_contract(expression: &Expr) -> Option<ApplicationContract> {
    let Expr::Call {
        callee: Some(callee),
        ..
    } = expression
    else {
        return None;
    };
    let PrimitiveType::Function(signature) = callee.result_type() else {
        return None;
    };
    let callee_type = semantic_type_signature(signature.as_ref());
    Some(ApplicationContract::new(
        None,
        Some(callee_type),
        signature
            .parameters()
            .iter()
            .map(|parameter| {
                semantic_type_primitive(parameter)
                    .unwrap_or_else(|| SemanticType::primitive("void"))
            })
            .collect(),
        signature
            .labelled()
            .iter()
            .map(|parameter| {
                LabelledType::new(
                    parameter.name(),
                    semantic_type_primitive(&parameter.value_type())
                        .unwrap_or_else(|| SemanticType::primitive("void")),
                )
            })
            .collect(),
        semantic_type_primitive(&signature.result())
            .unwrap_or_else(|| SemanticType::primitive("void")),
    ))
}

fn function_type(function: &FunctionDeclaration) -> Option<SemanticType> {
    let parameters = function
        .parameters()
        .iter()
        .map(|parameter| semantic_type_expr(parameter.value_type()))
        .collect::<Option<Vec<_>>>()?;
    let mut labelled = Vec::new();
    for attribute in function.attributes().items() {
        if let Attribute::Labelled(entries) = attribute {
            for entry in entries {
                labelled.push(LabelledType::new(
                    entry.name().value(),
                    semantic_type_expr(entry.value_type())?,
                ));
            }
        }
    }
    Some(SemanticType::function(
        parameters,
        semantic_type_expr(function.result())?,
        labelled,
    ))
}

use std::{cell::Cell, collections::BTreeSet, fmt, path::Path, sync::Arc};

use crate::syntax::{Atom, Document, Node, NodeKind, Span};

/// Stable identity for a source document across snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocumentId(u64);

impl DocumentId {
    pub const ANONYMOUS: Self = Self(0);

    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }

    /// Derive a deterministic ID from a normalized document path.
    pub fn from_path(path: &Path) -> Self {
        let normalized = path.to_string_lossy().replace('\\', "/");
        Self(stable_hash(normalized.as_bytes()))
    }
}

/// Stable identity of one typed node within a parsed document snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AstId(u64);

impl AstId {
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }

    fn from_span(span: Span) -> Self {
        let mut bytes = [0_u8; 16];
        bytes[..8].copy_from_slice(&(span.start as u64).to_le_bytes());
        bytes[8..].copy_from_slice(&(span.end as u64).to_le_bytes());
        Self(stable_hash(&bytes))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceLocation {
    pub document: DocumentId,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// Compatibility origin for callers constructing detached AST fragments.
    Source(Span),
    DocumentSource {
        document: DocumentId,
        ast_id: AstId,
        span: Span,
    },
    Expansion {
        call_site: Span,
        definition: Span,
        parent: Arc<Origin>,
    },
    DocumentExpansion {
        ast_id: AstId,
        call_site: SourceLocation,
        definition: SourceLocation,
        parent: Arc<Origin>,
    },
}

impl Origin {
    pub fn primary_span(&self) -> Span {
        match self {
            Self::Source(span) => *span,
            Self::DocumentSource { span, .. } => *span,
            Self::Expansion { call_site, .. } => *call_site,
            Self::DocumentExpansion { call_site, .. } => call_site.span,
        }
    }

    pub fn document_id(&self) -> Option<DocumentId> {
        match self {
            Self::DocumentSource { document, .. } => Some(*document),
            Self::DocumentExpansion { call_site, .. } => Some(call_site.document),
            Self::Source(_) | Self::Expansion { .. } => None,
        }
    }

    pub fn ast_id(&self) -> Option<AstId> {
        match self {
            Self::DocumentSource { ast_id, .. } | Self::DocumentExpansion { ast_id, .. } => {
                Some(*ast_id)
            }
            Self::Source(_) | Self::Expansion { .. } => None,
        }
    }
}

thread_local! {
    static CURRENT_DOCUMENT: Cell<DocumentId> = const { Cell::new(DocumentId::ANONYMOUS) };
}

struct DocumentGuard(DocumentId);

impl DocumentGuard {
    fn enter(document: DocumentId) -> Self {
        Self(CURRENT_DOCUMENT.with(|current| current.replace(document)))
    }
}

impl Drop for DocumentGuard {
    fn drop(&mut self) {
        CURRENT_DOCUMENT.with(|current| current.set(self.0));
    }
}

fn source_origin(span: Span) -> Origin {
    Origin::DocumentSource {
        document: CURRENT_DOCUMENT.with(Cell::get),
        ast_id: AstId::from_span(span),
        span,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Spanned<T> {
    pub value: T,
    pub span: Span,
    pub origin: Origin,
}

impl<T> Spanned<T> {
    fn source(value: T, span: Span) -> Self {
        Self {
            value,
            span,
            origin: source_origin(span),
        }
    }

    pub fn map<U>(self, map: impl FnOnce(T) -> U) -> Spanned<U> {
        Spanned {
            value: map(self.value),
            span: self.span,
            origin: self.origin,
        }
    }

    /// Construct a typed rewrite result while retaining macro provenance.
    pub fn expanded(
        value: T,
        span: Span,
        call_site: Span,
        definition: Span,
        parent: Origin,
    ) -> Self {
        Self {
            value,
            span,
            origin: Origin::Expansion {
                call_site,
                definition,
                parent: Arc::new(parent),
            },
        }
    }

    pub fn expanded_in(
        value: T,
        span: Span,
        call_site: SourceLocation,
        definition: SourceLocation,
        parent: Origin,
    ) -> Self {
        Self {
            value,
            span,
            origin: Origin::DocumentExpansion {
                ast_id: AstId::from_span(span),
                call_site,
                definition,
                parent: Arc::new(parent),
            },
        }
    }

    pub fn with_origin(mut self, origin: Origin) -> Self {
        self.origin = origin;
        self
    }

    pub fn ast_id(&self) -> Option<AstId> {
        self.origin.ast_id()
    }
}

pub type Name = Spanned<String>;
pub type TypeExpr = Spanned<TypeExprKind>;
pub type Expr = Spanned<ExprKind>;
pub type Pattern = Spanned<PatternKind>;
pub type Annotation = Spanned<AnnotationKind>;
pub type MacroExpr = Spanned<MacroExprKind>;

#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    pub forms: Vec<TopLevel>,
    pub span: Span,
    pub document_id: DocumentId,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TopLevel {
    Import(Import),
    Definition(Definition),
    Constant(Constant),
    Function(Function),
    Macro(Macro),
    Test(Test),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Private,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Import {
    pub alias: Name,
    pub path: Spanned<String>,
    pub span: Span,
    pub origin: Origin,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Definition {
    pub visibility: Visibility,
    pub name: Name,
    pub body: TypeExpr,
    pub annotations: Vec<Annotation>,
    pub span: Span,
    pub origin: Origin,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Constant {
    pub visibility: Visibility,
    pub name: Name,
    pub ty: TypeExpr,
    pub value: Expr,
    pub annotations: Vec<Annotation>,
    pub span: Span,
    pub origin: Origin,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub visibility: Visibility,
    pub name: Name,
    pub parameters: Vec<Parameter>,
    pub return_type: TypeExpr,
    pub body: Vec<Expr>,
    pub annotations: Vec<Annotation>,
    pub span: Span,
    pub origin: Origin,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Macro {
    pub visibility: Visibility,
    pub name: Name,
    pub parameters: Vec<MacroParameter>,
    pub result: Spanned<SyntaxCategory>,
    pub body: Vec<MacroExpr>,
    pub annotations: Vec<Annotation>,
    pub span: Span,
    pub origin: Origin,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MacroParameter {
    pub name: Name,
    pub category: Spanned<SyntaxCategory>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntaxCategory {
    Expression,
    Type,
    Pattern,
    Definition,
    Module,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MacroExprKind {
    Atom(Atom),
    Let {
        name: Name,
        value: Box<MacroExpr>,
    },
    If {
        condition: Box<MacroExpr>,
        then_body: Vec<MacroExpr>,
        else_body: Vec<MacroExpr>,
    },
    Quote {
        category: Spanned<SyntaxCategory>,
        /// Quoted syntax remains CST until macro-category validation and
        /// expansion. It is never coerced into a runtime expression tree.
        form: Node,
    },
    Unquote(Name),
    Splice(Name),
    Capture(Name),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    pub name: Name,
    pub ty: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Test {
    pub name: Name,
    pub profile: Name,
    pub body: Vec<Expr>,
    pub metadata: Vec<TestMeta>,
    pub span: Span,
    pub origin: Origin,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TestMeta {
    Tags(Vec<Name>),
    ExpectError {
        phase: Name,
        code: Name,
        message: Option<Spanned<String>>,
    },
    Clock {
        mode: Name,
        millis: Spanned<i64>,
    },
    Benchmark(Node),
    Workspace(Name),
}

#[derive(Debug, Clone, Copy)]
struct AttributeRef<'a> {
    label: &'a Node,
    value: &'a Node,
    span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeExprKind {
    Named(String),
    Application {
        constructor: Name,
        arguments: Vec<TypeExpr>,
    },
    Record(Vec<TypeMember>),
    Tuple(Vec<TypeExpr>),
    Array(Box<TypeExpr>),
    Map(Box<TypeExpr>, Box<TypeExpr>),
    Union(Vec<TypeExpr>),
    Enum(Vec<TypeMember>),
    Interface(Vec<TypeMember>),
    Function {
        parameters: Vec<TypeExpr>,
        result: Box<TypeExpr>,
    },
    Newtype(Box<TypeExpr>),
    Mutable(Box<TypeExpr>),
    Reference(Box<TypeExpr>),
    MutableReference(Box<TypeExpr>),
    Intersect(Vec<TypeExpr>),
    /// Capability, handle, policy, and ABI forms have a fixed semantic head,
    /// but their detailed inventory is owned by the type checker.
    Domain {
        head: Name,
        arguments: Vec<TypeExpr>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeMember {
    pub name: Name,
    pub ty: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    String(String),
    Bool(bool),
    Int(i64),
    Float(f64),
    Unit,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Literal(Literal),
    Reference(String),
    Call {
        callee: Name,
        arguments: Vec<Expr>,
    },
    Do(Vec<Expr>),
    Let {
        name: Name,
        ty: Option<TypeExpr>,
        value: Box<Expr>,
    },
    Set {
        name: Name,
        value: Box<Expr>,
    },
    Return(Option<Box<Expr>>),
    If {
        condition: Box<Expr>,
        then_body: Vec<Expr>,
        else_body: Vec<Expr>,
    },
    While {
        condition: Box<Expr>,
        body: Vec<Expr>,
    },
    For {
        binding: Name,
        source: Box<Expr>,
        body: Vec<Expr>,
    },
    Match {
        target: Box<Expr>,
        cases: Vec<MatchCase>,
    },
    Break,
    Continue,
    Record(Vec<ExprField>),
    Tuple(Vec<Expr>),
    Array(Vec<Expr>),
    Map(Vec<(Expr, Expr)>),
    Mutable(Box<Expr>),
    ReferenceOf(Box<Expr>),
    Range(Box<Expr>, Box<Expr>, Box<Expr>),
    Convert {
        value: Box<Expr>,
        into: TypeExpr,
        fallback: Literal,
    },
    Cast {
        value: Box<Expr>,
        into: TypeExpr,
    },
    Task {
        captures: Vec<Name>,
        body: Vec<Expr>,
    },
    Spawn {
        handle: Name,
        captures: Vec<Name>,
        value: Box<Expr>,
    },
    Join {
        handle: Name,
        binding: Name,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExprField {
    pub name: Name,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchCase {
    pub pattern: Pattern,
    pub body: Vec<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PatternKind {
    Literal(Literal),
    Wildcard,
    Bind(Name),
    Constructor {
        constructor: Name,
        arguments: Vec<Pattern>,
    },
    Record(Vec<PatternField>),
    Tuple(Vec<Pattern>),
    Array(Vec<Pattern>),
    Map(Vec<(Pattern, Pattern)>),
    Newtype {
        ty: TypeExpr,
        pattern: Box<Pattern>,
    },
    Interface {
        ty: TypeExpr,
        pattern: Box<Pattern>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct PatternField {
    pub name: Name,
    pub pattern: Pattern,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AnnotationKind {
    Doc(String),
    Where(Vec<TypeParameter>),
    Definitions(Vec<Function>),
    Implementation {
        interface: TypeExpr,
        items: Vec<ImplItem>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeParameter {
    pub name: Name,
    pub bounds: Vec<TypeExpr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImplItem {
    Types(Vec<TypeExpr>),
    Method {
        name: Name,
        binding: MethodBinding,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum MethodBinding {
    Reference(Name),
    Function(Function),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstError {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
    pub related: Vec<(String, Span)>,
}

impl AstError {
    fn new(code: &'static str, message: impl Into<String>, span: Span) -> Self {
        Self {
            code,
            message: message.into(),
            span,
            related: Vec::new(),
        }
    }
}

impl fmt::Display for AstError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} at bytes {}..{}: {}",
            self.code, self.span.start, self.span.end, self.message
        )
    }
}

impl std::error::Error for AstError {}

pub fn lower_document(document: &Document) -> Result<Module, AstError> {
    lower_document_with_id(document, DocumentId::ANONYMOUS)
}

pub fn lower_document_with_id(
    document: &Document,
    document_id: DocumentId,
) -> Result<Module, AstError> {
    let _guard = DocumentGuard::enter(document_id);
    let mut forms = Vec::new();
    for node in semantic_nodes(&document.nodes) {
        forms.push(parse_top(node)?);
    }
    Ok(Module {
        forms,
        span: document.span,
        document_id,
    })
}

pub(crate) fn lower_expression_node_with_id(
    node: &Node,
    document_id: DocumentId,
) -> Result<Expr, AstError> {
    let _guard = DocumentGuard::enter(document_id);
    parse_expr(node)
}

pub(crate) fn lower_type_node_with_id(
    node: &Node,
    document_id: DocumentId,
) -> Result<TypeExpr, AstError> {
    let _guard = DocumentGuard::enter(document_id);
    parse_type(node)
}

pub(crate) fn lower_pattern_node_with_id(
    node: &Node,
    document_id: DocumentId,
) -> Result<Pattern, AstError> {
    let _guard = DocumentGuard::enter(document_id);
    parse_pattern(node)
}

pub(crate) fn lower_top_level_node_with_id(
    node: &Node,
    document_id: DocumentId,
) -> Result<TopLevel, AstError> {
    let _guard = DocumentGuard::enter(document_id);
    parse_top(node)
}

fn parse_top(node: &Node) -> Result<TopLevel, AstError> {
    let (head, args) = headed(node)?;
    match head.value.as_str() {
        "import" => parse_import(node, args).map(TopLevel::Import),
        "def" => parse_definition(node, args, Visibility::Public).map(TopLevel::Definition),
        "const" => parse_constant(node, args, Visibility::Public).map(TopLevel::Constant),
        "fn" => parse_function(node, args, Visibility::Public).map(TopLevel::Function),
        "macro" => parse_macro(node, args, Visibility::Public).map(TopLevel::Macro),
        "test" => parse_test(node, args).map(TopLevel::Test),
        "private" => parse_private(node, args),
        _ => Err(AstError::new(
            "E-SYN-007",
            format!("`{}` is not a valid top-level form", head.value),
            head.span,
        )),
    }
}

fn parse_import<'a>(node: &Node, args: impl AsRef<[&'a Node]>) -> Result<Import, AstError> {
    let args = args.as_ref();
    exact_arity("import", args, 2, node.span)?;
    Ok(Import {
        alias: name(args[0])?,
        path: string(args[1])?,
        span: node.span,
        origin: source_origin(node.span),
    })
}

fn parse_definition<'a>(
    node: &Node,
    args: impl AsRef<[&'a Node]>,
    visibility: Visibility,
) -> Result<Definition, AstError> {
    let args = args.as_ref();
    let attributes = trailing_attributes("def", args, 2, node.span)?;
    Ok(Definition {
        visibility,
        name: name(args[0])?,
        body: parse_type(args[1])?,
        annotations: parse_annotations(&attributes)?,
        span: node.span,
        origin: source_origin(node.span),
    })
}

fn parse_constant<'a>(
    node: &Node,
    args: impl AsRef<[&'a Node]>,
    visibility: Visibility,
) -> Result<Constant, AstError> {
    let args = args.as_ref();
    let attributes = trailing_attributes("const", args, 3, node.span)?;
    Ok(Constant {
        visibility,
        name: name(args[0])?,
        ty: parse_type(args[1])?,
        value: parse_expr(args[2])?,
        annotations: parse_annotations(&attributes)?,
        span: node.span,
        origin: source_origin(node.span),
    })
}

fn parse_function<'a>(
    node: &Node,
    args: impl AsRef<[&'a Node]>,
    visibility: Visibility,
) -> Result<Function, AstError> {
    let args = args.as_ref();
    let attributes = trailing_attributes("fn", args, 4, node.span)?;
    Ok(Function {
        visibility,
        name: name(args[0])?,
        parameters: parse_parameters(args[1])?,
        return_type: parse_type(args[2])?,
        body: parse_body(args[3])?,
        annotations: parse_annotations(&attributes)?,
        span: node.span,
        origin: source_origin(node.span),
    })
}

fn parse_macro<'a>(
    node: &Node,
    args: impl AsRef<[&'a Node]>,
    visibility: Visibility,
) -> Result<Macro, AstError> {
    let args = args.as_ref();
    let attributes = trailing_attributes("macro", args, 4, node.span)?;
    Ok(Macro {
        visibility,
        name: name(args[0])?,
        parameters: parse_macro_parameters(args[1])?,
        result: parse_syntax_category(args[2])?,
        body: parse_macro_body(args[3])?,
        annotations: parse_annotations(&attributes)?,
        span: node.span,
        origin: source_origin(node.span),
    })
}

fn parse_macro_parameters(node: &Node) -> Result<Vec<MacroParameter>, AstError> {
    semantic_nodes(list(node)?)
        .map(|parameter| {
            let values = semantic_nodes(list(parameter)?).collect::<Vec<_>>();
            exact_arity("macro parameter", &values, 2, parameter.span)?;
            Ok(MacroParameter {
                name: name(values[0])?,
                category: parse_syntax_category(values[1])?,
                span: parameter.span,
            })
        })
        .collect()
}

fn parse_syntax_category(node: &Node) -> Result<Spanned<SyntaxCategory>, AstError> {
    let category = match symbol(node) {
        Some("expr-syntax") => SyntaxCategory::Expression,
        Some("type-syntax") => SyntaxCategory::Type,
        Some("pattern-syntax") => SyntaxCategory::Pattern,
        Some("definition-syntax") => SyntaxCategory::Definition,
        Some("module-syntax") => SyntaxCategory::Module,
        Some(value) => {
            return Err(AstError::new(
                "E-SYN-008",
                format!("unknown syntax category `{value}`"),
                node.span,
            ));
        }
        None => {
            return Err(AstError::new(
                "E-SYN-010",
                "expected a syntax category symbol",
                node.span,
            ));
        }
    };
    Ok(Spanned::source(category, node.span))
}

fn parse_macro_body(node: &Node) -> Result<Vec<MacroExpr>, AstError> {
    let (head, args) = headed(node)?;
    if head.value != "do" {
        return Err(expected_head("do", &head));
    }
    min_arity("macro do", &args, 1, node.span)?;
    args.iter().map(|node| parse_macro_expr(node)).collect()
}

fn parse_macro_expr(node: &Node) -> Result<MacroExpr, AstError> {
    if let NodeKind::Atom(atom) = &node.kind {
        if matches!(atom, Atom::Label(_)) {
            return Err(AstError::new(
                "E-SYN-011",
                "attribute label is not a macro expression",
                node.span,
            ));
        }
        return Ok(Spanned::source(
            MacroExprKind::Atom(atom.clone()),
            node.span,
        ));
    }
    let (head, args) = headed(node)?;
    let value = match head.value.as_str() {
        "let" => {
            exact_arity("macro let", &args, 2, node.span)?;
            MacroExprKind::Let {
                name: name(args[0])?,
                value: Box::new(parse_macro_expr(args[1])?),
            }
        }
        "if" => {
            exact_arity("macro if", &args, 3, node.span)?;
            MacroExprKind::If {
                condition: Box::new(parse_macro_expr(args[0])?),
                then_body: parse_macro_body(args[1])?,
                else_body: parse_macro_body(args[2])?,
            }
        }
        "quote" => {
            exact_arity("quote", &args, 2, node.span)?;
            MacroExprKind::Quote {
                category: parse_syntax_category(args[0])?,
                form: args[1].clone(),
            }
        }
        "unquote" | "splice" | "capture" => {
            exact_arity(&head.value, &args, 1, node.span)?;
            let target = name(args[0])?;
            match head.value.as_str() {
                "unquote" => MacroExprKind::Unquote(target),
                "splice" => MacroExprKind::Splice(target),
                _ => MacroExprKind::Capture(target),
            }
        }
        _ => {
            return Err(AstError::new(
                "E-SYN-008",
                format!("unknown macro expression `{}`", head.value),
                head.span,
            ));
        }
    };
    Ok(Spanned::source(value, node.span))
}

fn parse_private<'a>(node: &Node, args: impl AsRef<[&'a Node]>) -> Result<TopLevel, AstError> {
    let args = args.as_ref();
    exact_arity("private", args, 1, node.span)?;
    let inner = args[0];
    let (head, inner_args) = headed(inner)?;
    match head.value.as_str() {
        "def" => {
            let mut definition = parse_definition(inner, inner_args, Visibility::Private)?;
            definition.span = node.span;
            definition.origin = source_origin(node.span);
            Ok(TopLevel::Definition(definition))
        }
        "const" => {
            let mut constant = parse_constant(inner, inner_args, Visibility::Private)?;
            constant.span = node.span;
            constant.origin = source_origin(node.span);
            Ok(TopLevel::Constant(constant))
        }
        "fn" => {
            let mut function = parse_function(inner, inner_args, Visibility::Private)?;
            function.span = node.span;
            function.origin = source_origin(node.span);
            Ok(TopLevel::Function(function))
        }
        "macro" => {
            let mut definition = parse_macro(inner, inner_args, Visibility::Private)?;
            definition.span = node.span;
            definition.origin = source_origin(node.span);
            Ok(TopLevel::Macro(definition))
        }
        _ => Err(AstError::new(
            "E-SYN-008",
            "`private` accepts exactly one def, const, fn, or macro",
            head.span,
        )),
    }
}

fn parse_test<'a>(node: &Node, args: impl AsRef<[&'a Node]>) -> Result<Test, AstError> {
    let args = args.as_ref();
    let attributes = trailing_attributes("test", args, 3, node.span)?;
    Ok(Test {
        name: name(args[0])?,
        profile: name(args[1])?,
        body: parse_body(args[2])?,
        metadata: attributes
            .iter()
            .map(parse_test_meta)
            .collect::<Result<_, _>>()?,
        span: node.span,
        origin: source_origin(node.span),
    })
}

fn parse_parameters(node: &Node) -> Result<Vec<Parameter>, AstError> {
    let children = list(node)?;
    semantic_nodes(children)
        .map(|parameter| {
            let values = list(parameter)?;
            let values = semantic_nodes(values).collect::<Vec<_>>();
            exact_arity("parameter", &values, 2, parameter.span)?;
            Ok(Parameter {
                name: name(values[0])?,
                ty: parse_type(values[1])?,
                span: parameter.span,
            })
        })
        .collect()
}

fn parse_body(node: &Node) -> Result<Vec<Expr>, AstError> {
    let (head, args) = headed(node)?;
    if head.value != "do" {
        return Err(expected_head("do", &head));
    }
    args.iter().map(|node| parse_expr(node)).collect()
}

fn parse_type(node: &Node) -> Result<TypeExpr, AstError> {
    if let Some(symbol) = symbol(node) {
        return Ok(Spanned::source(
            TypeExprKind::Named(symbol.to_string()),
            node.span,
        ));
    }
    let (head, args) = headed(node)?;
    let value = match head.value.as_str() {
        "record" => TypeExprKind::Record(parse_type_members(args)?),
        "tuple" => TypeExprKind::Tuple(parse_types(args)?),
        "array" => {
            exact_arity("array type", &args, 1, node.span)?;
            TypeExprKind::Array(Box::new(parse_type(args[0])?))
        }
        "map" => {
            exact_arity("map type", &args, 2, node.span)?;
            TypeExprKind::Map(
                Box::new(parse_type(args[0])?),
                Box::new(parse_type(args[1])?),
            )
        }
        "union" => {
            min_arity("union", &args, 1, node.span)?;
            TypeExprKind::Union(parse_types(args)?)
        }
        "enum" => TypeExprKind::Enum(parse_type_members(args)?),
        "interface" => TypeExprKind::Interface(parse_type_members(args)?),
        "fn-type" => {
            exact_arity("fn-type", &args, 2, node.span)?;
            TypeExprKind::Function {
                parameters: semantic_nodes(list(args[0])?)
                    .map(parse_type)
                    .collect::<Result<_, _>>()?,
                result: Box::new(parse_type(args[1])?),
            }
        }
        "newtype" | "mut" | "ref" | "mut-ref" => {
            exact_arity(&head.value, &args, 1, node.span)?;
            let inner = Box::new(parse_type(args[0])?);
            match head.value.as_str() {
                "newtype" => TypeExprKind::Newtype(inner),
                "mut" => TypeExprKind::Mutable(inner),
                "ref" => TypeExprKind::Reference(inner),
                _ => TypeExprKind::MutableReference(inner),
            }
        }
        "intersect" => {
            min_arity("intersect", &args, 1, node.span)?;
            TypeExprKind::Intersect(parse_types(args)?)
        }
        "capability" | "handle" | "policy" | "wasm" => TypeExprKind::Domain {
            head,
            arguments: parse_types(args)?,
        },
        "inst" => {
            return Err(AstError::new(
                "E-SYN-008",
                "`inst` was removed; place the generic type constructor directly in head position",
                head.span,
            ));
        }
        _ => {
            min_arity("generic type application", &args, 1, node.span)?;
            TypeExprKind::Application {
                constructor: head,
                arguments: parse_types(args)?,
            }
        }
    };
    Ok(Spanned::source(value, node.span))
}

fn parse_type_members<'a>(args: impl AsRef<[&'a Node]>) -> Result<Vec<TypeMember>, AstError> {
    let args = args.as_ref();
    args.iter()
        .map(|node| {
            let values = semantic_nodes(list(node)?).collect::<Vec<_>>();
            exact_arity("type member", &values, 2, node.span)?;
            Ok(TypeMember {
                name: name(values[0])?,
                ty: parse_type(values[1])?,
                span: node.span,
            })
        })
        .collect()
}

fn parse_types<'a>(args: impl AsRef<[&'a Node]>) -> Result<Vec<TypeExpr>, AstError> {
    let args = args.as_ref();
    args.iter().map(|node| parse_type(node)).collect()
}

fn parse_expr(node: &Node) -> Result<Expr, AstError> {
    if let NodeKind::Atom(atom) = &node.kind {
        let value = match atom {
            Atom::Symbol(value) => ExprKind::Reference(value.clone()),
            Atom::Label(_) => {
                return Err(AstError::new(
                    "E-SYN-011",
                    "labels are declaration/test attributes, not named call arguments",
                    node.span,
                ));
            }
            atom => ExprKind::Literal(literal(atom).expect("non-symbol atom is literal")),
        };
        return Ok(Spanned::source(value, node.span));
    }
    let (head, args) = headed(node)?;
    let value = match head.value.as_str() {
        "do" => ExprKind::Do(parse_exprs(args)?),
        "let" => {
            exact_arity("let", &args, 2, node.span)?;
            ExprKind::Let {
                name: name(args[0])?,
                ty: None,
                value: Box::new(parse_expr(args[1])?),
            }
        }
        "let-as" => {
            exact_arity("let-as", &args, 3, node.span)?;
            ExprKind::Let {
                name: name(args[0])?,
                ty: Some(parse_type(args[1])?),
                value: Box::new(parse_expr(args[2])?),
            }
        }
        "set" => {
            exact_arity("set", &args, 2, node.span)?;
            ExprKind::Set {
                name: name(args[0])?,
                value: Box::new(parse_expr(args[1])?),
            }
        }
        "return" => {
            range_arity("return", &args, 0, 1, node.span)?;
            ExprKind::Return(
                args.first()
                    .map(|node| parse_expr(node))
                    .transpose()?
                    .map(Box::new),
            )
        }
        "if" => {
            exact_arity("if", &args, 3, node.span)?;
            ExprKind::If {
                condition: Box::new(parse_expr(args[0])?),
                then_body: parse_body(args[1])?,
                else_body: parse_body(args[2])?,
            }
        }
        "while" => {
            exact_arity("while", &args, 2, node.span)?;
            ExprKind::While {
                condition: Box::new(parse_expr(args[0])?),
                body: parse_body(args[1])?,
            }
        }
        "for" => {
            exact_arity("for", &args, 3, node.span)?;
            ExprKind::For {
                binding: name(args[0])?,
                source: Box::new(parse_expr(args[1])?),
                body: parse_body(args[2])?,
            }
        }
        "match" => {
            min_arity("match", &args, 2, node.span)?;
            ExprKind::Match {
                target: Box::new(parse_expr(args[0])?),
                cases: args[1..]
                    .iter()
                    .map(|node| parse_case(node))
                    .collect::<Result<_, _>>()?,
            }
        }
        "break" | "continue" => {
            exact_arity(&head.value, args, 0, node.span)?;
            if head.value == "break" {
                ExprKind::Break
            } else {
                ExprKind::Continue
            }
        }
        "record" => ExprKind::Record(parse_expr_fields(args)?),
        "tuple" => ExprKind::Tuple(parse_exprs(args)?),
        "array" => ExprKind::Array(parse_exprs(args)?),
        "map" => ExprKind::Map(parse_pairs(args, parse_expr)?),
        "mut" | "ref" => {
            exact_arity(&head.value, &args, 1, node.span)?;
            let inner = Box::new(parse_expr(args[0])?);
            if head.value == "mut" {
                ExprKind::Mutable(inner)
            } else {
                ExprKind::ReferenceOf(inner)
            }
        }
        "range" => {
            exact_arity("range", &args, 3, node.span)?;
            ExprKind::Range(
                Box::new(parse_expr(args[0])?),
                Box::new(parse_expr(args[1])?),
                Box::new(parse_expr(args[2])?),
            )
        }
        "convert" => {
            exact_arity("convert", &args, 3, node.span)?;
            ExprKind::Convert {
                value: Box::new(parse_expr(args[0])?),
                into: parse_type(args[1])?,
                fallback: literal_node(args[2])?,
            }
        }
        "cast" => {
            exact_arity("cast", &args, 2, node.span)?;
            ExprKind::Cast {
                value: Box::new(parse_expr(args[0])?),
                into: parse_type(args[1])?,
            }
        }
        "task" => {
            exact_arity("task", &args, 2, node.span)?;
            ExprKind::Task {
                captures: parse_captures(args[0])?,
                body: parse_body(args[1])?,
            }
        }
        "spawn" => {
            exact_arity("spawn", &args, 3, node.span)?;
            ExprKind::Spawn {
                handle: name(args[0])?,
                captures: parse_captures(args[1])?,
                value: Box::new(parse_expr(args[2])?),
            }
        }
        "join" => {
            exact_arity("join", &args, 2, node.span)?;
            ExprKind::Join {
                handle: name(args[0])?,
                binding: name(args[1])?,
            }
        }
        _ => ExprKind::Call {
            callee: head,
            arguments: parse_exprs(args)?,
        },
    };
    Ok(Spanned::source(value, node.span))
}

fn parse_exprs<'a>(args: impl AsRef<[&'a Node]>) -> Result<Vec<Expr>, AstError> {
    let args = args.as_ref();
    args.iter().map(|node| parse_expr(node)).collect()
}

fn parse_expr_fields<'a>(args: impl AsRef<[&'a Node]>) -> Result<Vec<ExprField>, AstError> {
    let args = args.as_ref();
    args.iter()
        .map(|node| {
            let values = semantic_nodes(list(node)?).collect::<Vec<_>>();
            exact_arity("record field", &values, 2, node.span)?;
            Ok(ExprField {
                name: name(values[0])?,
                value: parse_expr(values[1])?,
                span: node.span,
            })
        })
        .collect()
}

fn parse_pairs<'a, T>(
    args: impl AsRef<[&'a Node]>,
    parse: fn(&Node) -> Result<T, AstError>,
) -> Result<Vec<(T, T)>, AstError> {
    let args = args.as_ref();
    args.iter()
        .map(|node| {
            let values = semantic_nodes(list(node)?).collect::<Vec<_>>();
            exact_arity("pair", &values, 2, node.span)?;
            Ok((parse(values[0])?, parse(values[1])?))
        })
        .collect()
}

fn parse_case(node: &Node) -> Result<MatchCase, AstError> {
    let (head, args) = headed(node)?;
    if head.value != "case" {
        return Err(expected_head("case", &head));
    }
    exact_arity("case", &args, 2, node.span)?;
    Ok(MatchCase {
        pattern: parse_pattern(args[0])?,
        body: parse_body(args[1])?,
        span: node.span,
    })
}

fn parse_pattern(node: &Node) -> Result<Pattern, AstError> {
    if let NodeKind::Atom(atom) = &node.kind {
        let value = match atom {
            Atom::Symbol(value) if value == "_" => PatternKind::Wildcard,
            Atom::Symbol(value) => PatternKind::Constructor {
                constructor: Spanned::source(value.clone(), node.span),
                arguments: Vec::new(),
            },
            Atom::Label(_) => {
                return Err(AstError::new(
                    "E-SYN-011",
                    "attribute label is not a pattern",
                    node.span,
                ));
            }
            atom => PatternKind::Literal(literal(atom).expect("non-symbol atom is literal")),
        };
        return Ok(Spanned::source(value, node.span));
    }
    let (head, args) = headed(node)?;
    let value = match head.value.as_str() {
        "bind" => {
            exact_arity("bind", &args, 1, node.span)?;
            PatternKind::Bind(name(args[0])?)
        }
        "record" => PatternKind::Record(
            args.iter()
                .map(|node| {
                    let values = semantic_nodes(list(node)?).collect::<Vec<_>>();
                    exact_arity("pattern field", &values, 2, node.span)?;
                    Ok(PatternField {
                        name: name(values[0])?,
                        pattern: parse_pattern(values[1])?,
                        span: node.span,
                    })
                })
                .collect::<Result<_, AstError>>()?,
        ),
        "tuple" => PatternKind::Tuple(parse_patterns(args)?),
        "array" => PatternKind::Array(parse_patterns(args)?),
        "map" => PatternKind::Map(parse_pairs(args, parse_pattern)?),
        "newtype" | "interface" => {
            exact_arity(&head.value, &args, 2, node.span)?;
            let ty = parse_type(args[0])?;
            let pattern = Box::new(parse_pattern(args[1])?);
            if head.value == "newtype" {
                PatternKind::Newtype { ty, pattern }
            } else {
                PatternKind::Interface { ty, pattern }
            }
        }
        _ => PatternKind::Constructor {
            constructor: head,
            arguments: parse_patterns(args)?,
        },
    };
    Ok(Spanned::source(value, node.span))
}

fn parse_patterns<'a>(args: impl AsRef<[&'a Node]>) -> Result<Vec<Pattern>, AstError> {
    let args = args.as_ref();
    args.iter().map(|node| parse_pattern(node)).collect()
}

fn parse_captures(node: &Node) -> Result<Vec<Name>, AstError> {
    let (head, args) = headed(node)?;
    if head.value != "captures" {
        return Err(expected_head("captures", &head));
    }
    args.iter().map(|node| name(node)).collect()
}

fn parse_annotations(attributes: &[AttributeRef<'_>]) -> Result<Vec<Annotation>, AstError> {
    let mut annotations = Vec::new();
    for attribute in attributes {
        let label = label(attribute.label)?;
        match label.value.as_str() {
            "doc" => annotations.push(Spanned::source(
                AnnotationKind::Doc(string(attribute.value)?.value),
                attribute.span,
            )),
            "where" => annotations.push(Spanned::source(
                AnnotationKind::Where(parse_where_value(attribute.value)?),
                attribute.span,
            )),
            "defs" => annotations.push(Spanned::source(
                AnnotationKind::Definitions(parse_defs_value(attribute.value)?),
                attribute.span,
            )),
            "impls" => {
                for implementation in semantic_nodes(list(attribute.value)?) {
                    annotations.push(Spanned::source(
                        parse_impl_value(implementation)?,
                        implementation.span,
                    ));
                }
            }
            _ => {
                return Err(AstError::new(
                    "E-SYN-011",
                    format!("unknown declaration attribute `{}:`", label.value),
                    label.span,
                ));
            }
        }
    }
    Ok(annotations)
}

fn parse_where_value(node: &Node) -> Result<Vec<TypeParameter>, AstError> {
    semantic_nodes(list(node)?)
        .map(|node| {
            let values = semantic_nodes(list(node)?).collect::<Vec<_>>();
            min_arity("type parameter", &values, 1, node.span)?;
            Ok(TypeParameter {
                name: name(values[0])?,
                bounds: parse_types(&values[1..])?,
                span: node.span,
            })
        })
        .collect()
}

fn parse_defs_value(node: &Node) -> Result<Vec<Function>, AstError> {
    semantic_nodes(list(node)?)
        .map(|node| {
            let (head, args) = headed(node)?;
            if head.value != "fn" {
                return Err(expected_head("fn", &head));
            }
            parse_function(node, args, Visibility::Public)
        })
        .collect()
}

fn parse_impl_value(node: &Node) -> Result<AnnotationKind, AstError> {
    let (head, args) = headed(node)?;
    if head.value != "impl" {
        return Err(expected_head("impl", &head));
    }
    let attributes = trailing_attributes("impl", &args, 1, node.span)?;
    let mut items = Vec::new();
    for attribute in attributes {
        let label = label(attribute.label)?;
        match label.value.as_str() {
            "types" => items.push(ImplItem::Types(
                semantic_nodes(list(attribute.value)?)
                    .map(parse_type)
                    .collect::<Result<_, _>>()?,
            )),
            "methods" => {
                for method in semantic_nodes(list(attribute.value)?) {
                    items.push(parse_method(method)?);
                }
            }
            _ => {
                return Err(AstError::new(
                    "E-SYN-011",
                    format!("unknown implementation attribute `{}:`", label.value),
                    label.span,
                ));
            }
        }
    }
    Ok(AnnotationKind::Implementation {
        interface: parse_type(args[0])?,
        items,
    })
}

fn parse_method(node: &Node) -> Result<ImplItem, AstError> {
    let (head, args) = headed(node)?;
    if head.value != "method" {
        return Err(expected_head("method", &head));
    }
    exact_arity("method", &args, 2, node.span)?;
    let binding = if symbol(args[1]).is_some() {
        MethodBinding::Reference(name(args[1])?)
    } else {
        let (fn_head, fn_args) = headed(args[1])?;
        if fn_head.value != "fn" {
            return Err(expected_head("fn", &fn_head));
        }
        MethodBinding::Function(parse_function(args[1], fn_args, Visibility::Public)?)
    };
    Ok(ImplItem::Method {
        name: name(args[0])?,
        binding,
        span: node.span,
    })
}

fn parse_test_meta(attribute: &AttributeRef<'_>) -> Result<TestMeta, AstError> {
    let label = label(attribute.label)?;
    match label.value.as_str() {
        "tags" => Ok(TestMeta::Tags(
            semantic_nodes(list(attribute.value)?)
                .map(name)
                .collect::<Result<_, _>>()?,
        )),
        "expect-error" => {
            let args = semantic_nodes(list(attribute.value)?).collect::<Vec<_>>();
            range_arity("expect-error", &args, 2, 3, attribute.span)?;
            Ok(TestMeta::ExpectError {
                phase: name(args[0])?,
                code: name(args[1])?,
                message: args.get(2).map(|node| string(node)).transpose()?,
            })
        }
        "clock" => {
            let args = semantic_nodes(list(attribute.value)?).collect::<Vec<_>>();
            exact_arity("clock", &args, 2, attribute.span)?;
            Ok(TestMeta::Clock {
                mode: name(args[0])?,
                millis: integer(args[1])?,
            })
        }
        "benchmark" => {
            list(attribute.value)?;
            Ok(TestMeta::Benchmark(attribute.value.clone()))
        }
        "workspace" => Ok(TestMeta::Workspace(name(attribute.value)?)),
        _ => Err(AstError::new(
            "E-SYN-011",
            format!("unknown test attribute `{}:`", label.value),
            label.span,
        )),
    }
}

fn trailing_attributes<'a>(
    form: &str,
    args: &[&'a Node],
    positional: usize,
    span: Span,
) -> Result<Vec<AttributeRef<'a>>, AstError> {
    if args.len() < positional {
        return Err(AstError::new(
            "E-SYN-009",
            format!(
                "`{form}` expects at least {positional} positional operands, found {}",
                args.len()
            ),
            span,
        ));
    }
    if let Some(node) = args[..positional]
        .iter()
        .find(|node| matches!(node.kind, NodeKind::Atom(Atom::Label(_))))
    {
        return Err(AstError::new(
            "E-SYN-011",
            "attribute label appears before all positional operands",
            node.span,
        ));
    }
    let tail = &args[positional..];
    let mut attributes = Vec::new();
    let mut seen = BTreeSet::new();
    let mut index = 0;
    while index < tail.len() {
        let label_node = tail[index];
        let label = label(label_node)?;
        let Some(value) = tail.get(index + 1) else {
            return Err(AstError::new(
                "E-SYN-011",
                format!("attribute `{}:` is missing its value", label.value),
                label.span,
            ));
        };
        if !seen.insert(label.value.clone()) {
            return Err(AstError::new(
                "E-SYN-011",
                format!("duplicate attribute `{}:`", label.value),
                label.span,
            ));
        }
        attributes.push(AttributeRef {
            label: label_node,
            value,
            span: label_node.span.cover(value.span),
        });
        index += 2;
    }
    Ok(attributes)
}

fn literal_node(node: &Node) -> Result<Literal, AstError> {
    match &node.kind {
        NodeKind::Atom(atom) => {
            literal(atom).ok_or_else(|| AstError::new("E-SYN-010", "expected a literal", node.span))
        }
        _ => Err(AstError::new("E-SYN-010", "expected a literal", node.span)),
    }
}

fn literal(atom: &Atom) -> Option<Literal> {
    match atom {
        Atom::String(value) => Some(Literal::String(value.clone())),
        Atom::Bool(value) => Some(Literal::Bool(*value)),
        Atom::Int(value) => Some(Literal::Int(*value)),
        Atom::Float(value) => Some(Literal::Float(*value)),
        Atom::Unit => Some(Literal::Unit),
        Atom::Symbol(_) | Atom::Label(_) => None,
    }
}

fn headed(node: &Node) -> Result<(Name, Vec<&Node>), AstError> {
    let children = semantic_nodes(list(node)?).collect::<Vec<_>>();
    if children.is_empty() {
        return Err(AstError::new(
            "E-SYN-008",
            "a semantic list must have a symbol head",
            node.span,
        ));
    }
    Ok((name(children[0])?, children[1..].to_vec()))
}

fn list(node: &Node) -> Result<&[Node], AstError> {
    match &node.kind {
        NodeKind::List(children) => Ok(children),
        _ => Err(AstError::new("E-SYN-010", "expected a list", node.span)),
    }
}

fn semantic_nodes(nodes: &[Node]) -> impl Iterator<Item = &Node> {
    nodes
        .iter()
        .filter(|node| !matches!(node.kind, NodeKind::Comment(_)))
}

fn symbol(node: &Node) -> Option<&str> {
    match &node.kind {
        NodeKind::Atom(Atom::Symbol(value)) => Some(value),
        _ => None,
    }
}

fn label(node: &Node) -> Result<Name, AstError> {
    match &node.kind {
        NodeKind::Atom(Atom::Label(value)) => Ok(Spanned::source(value.to_string(), node.span)),
        _ => Err(AstError::new(
            "E-SYN-011",
            "expected a trailing attribute label",
            node.span,
        )),
    }
}

fn name(node: &Node) -> Result<Name, AstError> {
    symbol(node)
        .map(|value| Spanned::source(value.to_string(), node.span))
        .ok_or_else(|| AstError::new("E-SYN-010", "expected a symbol", node.span))
}

fn string(node: &Node) -> Result<Spanned<String>, AstError> {
    match &node.kind {
        NodeKind::Atom(Atom::String(value)) => Ok(Spanned::source(value.clone(), node.span)),
        _ => Err(AstError::new("E-SYN-010", "expected a string", node.span)),
    }
}

fn integer(node: &Node) -> Result<Spanned<i64>, AstError> {
    match &node.kind {
        NodeKind::Atom(Atom::Int(value)) => Ok(Spanned::source(*value, node.span)),
        _ => Err(AstError::new("E-SYN-010", "expected an integer", node.span)),
    }
}

fn exact_arity<'a>(
    form: &str,
    args: impl AsRef<[&'a Node]>,
    expected: usize,
    span: Span,
) -> Result<(), AstError> {
    let args = args.as_ref();
    range_arity(form, args, expected, expected, span)
}

fn min_arity<'a>(
    form: &str,
    args: impl AsRef<[&'a Node]>,
    min: usize,
    span: Span,
) -> Result<(), AstError> {
    let args = args.as_ref();
    if args.len() < min {
        Err(AstError::new(
            "E-SYN-009",
            format!(
                "`{form}` expects at least {min} operands, found {}",
                args.len()
            ),
            span,
        ))
    } else {
        Ok(())
    }
}

fn range_arity<'a>(
    form: &str,
    args: impl AsRef<[&'a Node]>,
    min: usize,
    max: usize,
    span: Span,
) -> Result<(), AstError> {
    let args = args.as_ref();
    if (min..=max).contains(&args.len()) {
        Ok(())
    } else {
        let expected = if min == max {
            min.to_string()
        } else {
            format!("{min}..={max}")
        };
        Err(AstError::new(
            "E-SYN-009",
            format!("`{form}` expects {expected} operands, found {}", args.len()),
            span,
        ))
    }
}

fn expected_head(expected: &str, actual: &Name) -> AstError {
    AstError::new(
        "E-SYN-008",
        format!("expected `{expected}` form, found `{}`", actual.value),
        actual.span,
    )
}

fn stable_hash(bytes: &[u8]) -> u64 {
    // FNV-1a is deliberately fixed here: IDs must not depend on Rust's
    // randomized HashMap state or compiler version.
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::parse;

    fn module(source: &str) -> Result<Module, AstError> {
        lower_document(&parse(source).unwrap())
    }

    #[test]
    fn lowers_complete_module_surface_with_source_origins() {
        let source = r#"
(import io "./io.vibra")
(def option (enum (some t) (none void)) where: ((t)) doc: "Optional.")
(private (const limit int64 10))
(fn choose ((value bool) (fallback bool)) bool
  (do (if value (do (return value)) (do (return fallback)))))
(test works core (do (test.assert true)) tags: (fast language) clock: (fixed 0))
"#;
        let module = module(source).unwrap();
        assert_eq!(module.forms.len(), 5);
        assert!(matches!(
            module.forms[2],
            TopLevel::Constant(Constant {
                visibility: Visibility::Private,
                ..
            })
        ));
        let TopLevel::Function(function) = &module.forms[3] else {
            panic!("expected function");
        };
        assert_eq!(function.parameters.len(), 2);
        assert_eq!(function.span, function.origin.primary_span());
    }

    #[test]
    fn lowers_expression_type_pattern_and_annotation_roles() {
        let source = r#"
(fn unwrap ((input (option int64))) int64
  (do
    (let-as fallback int64 0)
    (match input
      (case (option.some (bind value)) (do (return value)))
      (case (option.none) (do (return fallback)))))
  doc: "unwrap"
  where: ((t comparable))
  impls: ((impl display
    types: (int64)
    methods: ((method show display.show)))))
"#;
        let module = module(source).unwrap();
        let TopLevel::Function(function) = &module.forms[0] else {
            panic!("expected function");
        };
        assert_eq!(function.annotations.len(), 3);
        let ExprKind::Match { cases, .. } = &function.body[1].value else {
            panic!("expected match");
        };
        assert_eq!(cases.len(), 2);
        assert!(matches!(
            cases[0].pattern.value,
            PatternKind::Constructor { .. }
        ));
    }

    #[test]
    fn validates_top_heads_arity_and_required_node_kinds_at_exact_spans() {
        let error = module("(wat x)").unwrap_err();
        assert_eq!(error.code, "E-SYN-007");
        assert_eq!(error.span, Span::new(1, 4));

        let error = module("(import io)").unwrap_err();
        assert_eq!(error.code, "E-SYN-009");
        assert_eq!(error.span, Span::new(0, 11));

        let error = module("(import \"io\" \"x\")").unwrap_err();
        assert_eq!(error.code, "E-SYN-010");
        assert_eq!(error.span, Span::new(8, 12));
    }

    #[test]
    fn rejects_invalid_private_target_body_and_special_form_arity() {
        assert_eq!(
            module("(private (import io \"x\"))").unwrap_err().code,
            "E-SYN-008"
        );
        assert_eq!(
            module("(fn f () void (array))").unwrap_err().code,
            "E-SYN-008"
        );
        assert_eq!(
            module("(fn f () void (do (break 1)))").unwrap_err().code,
            "E-SYN-009"
        );
    }

    #[test]
    fn ignores_reader_comments_in_all_grammar_positions() {
        let module =
            module("(fn ; head\n f (; params\n (x int64)) int64 (do ; body\n (return x)))")
                .unwrap();
        let TopLevel::Function(function) = &module.forms[0] else {
            panic!("expected function");
        };
        assert_eq!(function.parameters.len(), 1);
        assert_eq!(function.body.len(), 1);
    }

    #[test]
    fn expansion_origins_retain_call_definition_and_parent_chain() {
        let source = Origin::Source(Span::new(20, 30));
        let expanded = Spanned::expanded(
            ExprKind::Break,
            Span::new(100, 107),
            Span::new(4, 10),
            Span::new(20, 30),
            source,
        );
        assert_eq!(expanded.origin.primary_span(), Span::new(4, 10));
        assert_eq!(expanded.span, Span::new(100, 107));
    }

    #[test]
    fn generic_types_use_direct_head_application_only() {
        let parsed = module("(const value (option int64) unit)").unwrap();
        let TopLevel::Constant(constant) = &parsed.forms[0] else {
            panic!("expected constant");
        };
        assert!(matches!(
            constant.ty.value,
            TypeExprKind::Application { ref constructor, ref arguments }
                if constructor.value == "option" && arguments.len() == 1
        ));

        let error = module("(const value (inst option int64) unit)").unwrap_err();
        assert_eq!(error.code, "E-SYN-008");
        let error = module("(const value (option) unit)").unwrap_err();
        assert_eq!(error.code, "E-SYN-009");
    }

    #[test]
    fn lowers_typed_macros_and_retains_quoted_cst() {
        let source = r#"
(private
  (macro unless ((condition expr-syntax) (body expr-syntax)) expr-syntax
    (do
      (let fallback (quote expr-syntax unit))
      (if condition
        (do (quote expr-syntax (if (unquote condition) (do unit) (do (splice body)))))
        (do (capture caller))))
    doc: "Conditional syntax."))
"#;
        let parsed = module(source).unwrap();
        let TopLevel::Macro(definition) = &parsed.forms[0] else {
            panic!("expected macro");
        };
        assert_eq!(definition.visibility, Visibility::Private);
        assert_eq!(definition.parameters.len(), 2);
        assert_eq!(definition.result.value, SyntaxCategory::Expression);
        assert_eq!(definition.body.len(), 2);
        assert_eq!(definition.span, Span::new(1, source.trim_end().len()));
        let MacroExprKind::Let { value, .. } = &definition.body[0].value else {
            panic!("expected macro let");
        };
        let MacroExprKind::Quote { category, form } = &value.value else {
            panic!("expected quote");
        };
        assert_eq!(category.value, SyntaxCategory::Expression);
        assert!(matches!(form.kind, NodeKind::Atom(Atom::Unit)));
    }

    #[test]
    fn validates_macro_categories_body_and_operator_arity() {
        let error = module("(macro m ((x value-syntax)) expr-syntax (do x))").unwrap_err();
        assert_eq!(error.code, "E-SYN-008");

        let error = module("(macro m () expr-syntax (do))").unwrap_err();
        assert_eq!(error.code, "E-SYN-009");

        let error = module("(macro m () expr-syntax (do (quote expr-syntax a b)))").unwrap_err();
        assert_eq!(error.code, "E-SYN-009");

        let error = module("(macro m () expr-syntax (do (unquote)))").unwrap_err();
        assert_eq!(error.code, "E-SYN-009");

        let error = module("(private (test no core (do)))").unwrap_err();
        assert_eq!(error.code, "E-SYN-008");
    }

    #[test]
    fn identical_list_has_contextual_type_expression_and_pattern_meaning() {
        let parsed = module(
            "(const typed (option int64) (option int64))\n\
             (fn inspect ((x int64)) bool\n\
               (do (match x (case (option int64) (do true)))))",
        )
        .unwrap();
        let TopLevel::Constant(constant) = &parsed.forms[0] else {
            panic!("expected constant");
        };
        assert!(matches!(
            constant.ty.value,
            TypeExprKind::Application { .. }
        ));
        assert!(matches!(constant.value.value, ExprKind::Call { .. }));
        let TopLevel::Function(function) = &parsed.forms[1] else {
            panic!("expected function");
        };
        let ExprKind::Match { cases, .. } = &function.body[0].value else {
            panic!("expected match");
        };
        assert!(matches!(
            cases[0].pattern.value,
            PatternKind::Constructor { .. }
        ));
    }

    #[test]
    fn validates_trailing_attributes_and_keeps_calls_positional() {
        for source in [
            "(fn f () doc: \"early\" void (do))",
            "(fn f () void (do) doc:)",
            "(fn f () void (do) doc: \"a\" doc: \"b\")",
            "(fn f () void (do) unknown: unit)",
            "(test t core (do) tags: (fast) tags: (slow))",
            "(fn f () void (do (call value: 1)))",
        ] {
            let error = module(source).unwrap_err();
            assert_eq!(error.code, "E-SYN-011", "source: {source}");
        }
    }

    #[test]
    fn lowers_all_test_attribute_roles() {
        let parsed = module(
            "(test measured core (do)\n\
             tags: (fast arithmetic)\n\
             expect-error: (compile E-OP-002 \"overflow\")\n\
             clock: (fixed 0)\n\
             benchmark: (iterations 100)\n\
             workspace: temp)",
        )
        .unwrap();
        let TopLevel::Test(test) = &parsed.forms[0] else {
            panic!("expected test");
        };
        assert_eq!(test.metadata.len(), 5);
        assert!(matches!(test.metadata[3], TestMeta::Benchmark(_)));
        assert!(matches!(test.metadata[4], TestMeta::Workspace(_)));
    }

    #[test]
    fn document_and_ast_ids_are_stable_per_snapshot_and_document_qualified() {
        let syntax = parse("(fn main () void (do (return)))").unwrap();
        let document_a = DocumentId::from_raw(41);
        let document_b = DocumentId::from_raw(42);

        let first = lower_document_with_id(&syntax, document_a).unwrap();
        let second = lower_document_with_id(&syntax, document_a).unwrap();
        let other_document = lower_document_with_id(&syntax, document_b).unwrap();

        let TopLevel::Function(first_fn) = &first.forms[0] else {
            panic!("expected function");
        };
        let TopLevel::Function(second_fn) = &second.forms[0] else {
            panic!("expected function");
        };
        let TopLevel::Function(other_fn) = &other_document.forms[0] else {
            panic!("expected function");
        };

        assert_eq!(first.document_id, document_a);
        assert_eq!(first_fn.origin.document_id(), Some(document_a));
        assert_eq!(other_fn.origin.document_id(), Some(document_b));
        assert_eq!(first_fn.origin.ast_id(), second_fn.origin.ast_id());
        assert_eq!(first_fn.origin.ast_id(), other_fn.origin.ast_id());
        assert_eq!(
            first_fn.body[0].ast_id(),
            second_fn.body[0].ast_id(),
            "the same snapshot assigns the same node identity"
        );
        assert_ne!(
            first_fn.origin.ast_id(),
            first_fn.body[0].ast_id(),
            "distinct source nodes have distinct identities"
        );
    }

    #[test]
    fn document_expansion_retains_both_document_locations() {
        let call_site = SourceLocation {
            document: DocumentId::from_raw(1),
            span: Span::new(10, 20),
        };
        let definition = SourceLocation {
            document: DocumentId::from_raw(2),
            span: Span::new(30, 40),
        };
        let expanded = Spanned::expanded_in(
            ExprKind::Break,
            Span::new(100, 107),
            call_site,
            definition,
            Origin::Source(definition.span),
        );
        let Origin::DocumentExpansion {
            call_site: actual_call,
            definition: actual_definition,
            ..
        } = &expanded.origin
        else {
            panic!("expected document-qualified expansion");
        };
        assert_eq!(*actual_call, call_site);
        assert_eq!(*actual_definition, definition);
        assert_eq!(expanded.origin.document_id(), Some(call_site.document));
        assert!(expanded.ast_id().is_some());
    }

    #[test]
    fn path_document_ids_are_separator_normalized() {
        assert_eq!(
            DocumentId::from_path(Path::new("src\\main.vibra")),
            DocumentId::from_path(Path::new("src/main.vibra"))
        );
        assert_ne!(
            DocumentId::from_path(Path::new("src/main.vibra")),
            DocumentId::from_path(Path::new("src/other.vibra"))
        );
    }
}

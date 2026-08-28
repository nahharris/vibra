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
    Deffect(Deffect),
    Macro(Macro),
    Test(TestCase),
    TestScenario(TestScenario),
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

/// A nominal effect root and its operations.
///
/// Operations deliberately retain the ordinary function AST as their body so
/// the type checker and formatter can share the same expression machinery. The
/// owner root is supplied by the containing [`Deffect`]; an operation's
/// `effects:` annotation is additive rather than a replacement for that owner.
#[derive(Debug, Clone, PartialEq)]
pub struct Deffect {
    pub visibility: Visibility,
    pub name: Name,
    pub operations: Vec<EffectOperation>,
    pub span: Span,
    pub origin: Origin,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EffectOperation {
    pub function: Function,
    pub effects: EffectRow,
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
    pub variadic: bool,
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
    pub variadic: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionTypeParameter {
    pub name: Name,
    pub ty: TypeExpr,
    pub variadic: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TestScenario {
    pub name: Spanned<String>,
    pub cases: Vec<TestCase>,
    pub span: Span,
    pub origin: Origin,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TestCase {
    pub name: Spanned<String>,
    pub profile: Name,
    pub body: Vec<Expr>,
    pub metadata: Vec<TestMeta>,
    pub span: Span,
    pub origin: Origin,
}

pub type Test = TestCase;

#[derive(Debug, Clone, PartialEq)]
pub enum TestMeta {
    Tags(Vec<Name>),
    TimeoutMillis(Spanned<i64>),
    RandomSeed(Spanned<i64>),
    Skip(Spanned<String>),
    ExpectError(ExpectedError),
    Clock {
        unix_millis: Spanned<i64>,
        monotonic_millis: Spanned<i64>,
    },
    Workspace(Name),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExpectedError {
    Load {
        code: Name,
        message: Option<Spanned<String>>,
    },
    Compile {
        code: Name,
        message: Option<Spanned<String>>,
    },
    Runtime {
        message: Spanned<String>,
    },
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
    Literal(Literal),
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
        parameters: Vec<FunctionTypeParameter>,
        result: Box<TypeExpr>,
        /// The ceiling an implementation of this signature may not exceed.
        effects: EffectRow,
    },
    Newtype(Box<TypeExpr>),
    Mutable(Box<TypeExpr>),
    Reference(Box<TypeExpr>),
    MutableReference(Box<TypeExpr>),
    Intersect(Vec<TypeExpr>),
    Handle(Spanned<HandleAccess>),
    WasmValue(Name),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleAccess {
    Read,
    Write,
    ReadWrite,
    Any,
    Process,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeMember {
    pub name: Name,
    pub ty: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Atom(String),
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
        arguments: Vec<CallArgument>,
        type_arguments: Vec<TypeExpr>,
        /// Position of the `types:` labelled operand among value arguments.
        /// This keeps the labelled group boundary available to resolved
        /// binders without making type arguments runtime call values.
        type_arguments_position: Option<usize>,
    },
    AnonymousFunction {
        parameters: Vec<Parameter>,
        return_type: TypeExpr,
        body: Vec<Expr>,
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
    Try(Box<Expr>),
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
        fallback: Spanned<Literal>,
    },
    Cast {
        value: Box<Expr>,
        into: TypeExpr,
    },
    Embed {
        path: Spanned<String>,
        format: Spanned<EmbedFormat>,
    },
    Template {
        path: Spanned<String>,
        bindings: Vec<ExprField>,
    },
    /// A closed, compiler-known operation. Unlike `wasm`, the intrinsic name
    /// is an atom and its argument/result types are checked against the
    /// intrinsic registry rather than an arbitrary imported symbol.
    Intrinsic {
        name: Spanned<String>,
        arguments: Vec<Expr>,
    },
    Wasm {
        import: WasmImport,
        arguments: Vec<WasmArgument>,
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
pub enum CallArgument {
    Positional(Expr),
    Labelled { label: Name, value: Expr },
}

impl CallArgument {
    pub fn value(&self) -> &Expr {
        match self {
            Self::Positional(value) | Self::Labelled { value, .. } => value,
        }
    }

    pub fn value_mut(&mut self) -> &mut Expr {
        match self {
            Self::Positional(value) | Self::Labelled { value, .. } => value,
        }
    }

    pub fn map_value<E>(self, map: impl FnOnce(Expr) -> Result<Expr, E>) -> Result<Self, E> {
        Ok(match self {
            Self::Positional(value) => Self::Positional(map(value)?),
            Self::Labelled { label, value } => Self::Labelled {
                label,
                value: map(value)?,
            },
        })
    }

    pub fn into_value(self) -> Expr {
        match self {
            Self::Positional(value) | Self::Labelled { value, .. } => value,
        }
    }
}

impl std::ops::Deref for CallArgument {
    type Target = Expr;

    fn deref(&self) -> &Self::Target {
        self.value()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbedFormat {
    Auto,
    Text,
    Binary,
    Json,
    Toml,
    Xml,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WasmImport {
    pub module: Spanned<String>,
    pub name: Spanned<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WasmArgument {
    Parameter(Name),
    ConstInt(Spanned<i64>),
    ConstString(Spanned<String>),
    Expression(Expr),
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
    Effects(EffectRow),
    Implementation {
        interface: TypeExpr,
        items: Vec<ImplItem>,
    },
}

/// The complete set of effects a function's body may perform.
///
/// Each label is a nominal root name such as `fs.read`.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectRow {
    pub labels: Vec<TypeExpr>,
    /// Reserved for effect polymorphism. Always `None`: first-class functions are
    /// still rejected (`E-FN-001`), so nothing can propagate a callee's effects yet,
    /// and no surface spelling for a tail has been allocated. The field exists now
    /// because retrofitting the row shape once higher-order functions land is the
    /// single hardest change in the effect design.
    pub tail: Option<Name>,
    pub span: Span,
}

/// Which declaration form an annotation list belongs to. Only functions accept
/// `effects:`, and `parse_annotations` is shared by five callers, so the owner has
/// to be threaded in to reject it at the surface -- the only layer with spans.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AnnotationOwner {
    Definition,
    Constant,
    Function,
    Macro,
}

impl AnnotationOwner {
    fn allows_effects(self) -> bool {
        matches!(self, Self::Function)
    }

    fn label(self) -> &'static str {
        match self {
            Self::Definition => "def",
            Self::Constant => "const",
            Self::Function => "defn",
            Self::Macro => "macro",
        }
    }
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
    let mut scenario_names = std::collections::BTreeSet::new();
    for node in semantic_nodes(&document.nodes) {
        match parse_top(node)? {
            TopLevel::TestScenario(scenario) => {
                if !scenario_names.insert(scenario.name.value.clone()) {
                    return Err(AstError::new(
                        "E-TEST-010",
                        format!("duplicate test scenario `{}`", scenario.name.value),
                        scenario.name.span,
                    ));
                }
                forms.push(TopLevel::TestScenario(scenario));
            }
            form => forms.push(form),
        }
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
        "defn" => parse_function(node, args, Visibility::Public).map(TopLevel::Function),
        "deffect" => parse_deffect(node, args, Visibility::Public).map(TopLevel::Deffect),
        "macro" => parse_macro(node, args, Visibility::Public).map(TopLevel::Macro),
        "test.scenario" => parse_test_scenario(node, args).map(TopLevel::TestScenario),
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
    let visibility = parse_visibility(&attributes, visibility)?;
    let annotations = attributes
        .iter()
        .copied()
        .filter(|attribute| label(attribute.label).is_ok_and(|name| name.value != "visibility"))
        .collect::<Vec<_>>();
    Ok(Definition {
        visibility,
        name: name(args[0])?,
        body: parse_type(args[1])?,
        annotations: parse_annotations(&annotations, AnnotationOwner::Definition)?,
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
    let visibility = parse_visibility(&attributes, visibility)?;
    let annotations = attributes
        .iter()
        .copied()
        .filter(|attribute| label(attribute.label).is_ok_and(|name| name.value != "visibility"))
        .collect::<Vec<_>>();
    Ok(Constant {
        visibility,
        name: name(args[0])?,
        ty: parse_type(args[1])?,
        value: parse_expr(args[2])?,
        annotations: parse_annotations(&annotations, AnnotationOwner::Constant)?,
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
    if args.len() < 3 {
        return Err(AstError::new(
            "E-SYN-009",
            format!(
                "`defn` expects at least 3 positional operands, found {}",
                args.len()
            ),
            node.span,
        ));
    }
    let (attributes, body_nodes) = split_labeled_remainder("defn", args, 3, node.span)?;
    let visibility = parse_visibility(&attributes, visibility)?;
    let annotation_attributes = attributes
        .iter()
        .copied()
        .filter(|attribute| label(attribute.label).is_ok_and(|name| name.value != "visibility"))
        .collect::<Vec<_>>();
    Ok(Function {
        visibility,
        name: name(args[0])?,
        parameters: parse_parameters(args[1])?,
        return_type: parse_type(args[2])?,
        body: parse_owned_body(&body_nodes)?,
        annotations: parse_annotations(&annotation_attributes, AnnotationOwner::Function)?,
        span: node.span,
        origin: source_origin(node.span),
    })
}

fn parse_deffect<'a>(
    node: &Node,
    args: impl AsRef<[&'a Node]>,
    visibility: Visibility,
) -> Result<Deffect, AstError> {
    let args = args.as_ref();
    if args.len() < 2 {
        return Err(AstError::new(
            "E-DEFFECT-001",
            "`deffect` expects a root name and at least one operation",
            node.span,
        ));
    }
    let name = name(args[0])?;
    let mut operations = Vec::with_capacity(args.len() - 1);
    let mut seen = std::collections::BTreeSet::new();
    for operation_node in &args[1..] {
        let (head, operation_args) = headed(operation_node)?;
        if head.value != "defn" {
            return Err(AstError::new(
                "E-DEFFECT-002",
                "a `deffect` body may contain only `defn` operations",
                head.span,
            ));
        }
        let function = parse_function(operation_node, operation_args, Visibility::Public)?;
        let mut function = function;
        if function.body.len() == 1
            && matches!(function.body[0].value, ExprKind::Intrinsic { .. })
            && !matches!(&function.return_type.value, TypeExprKind::Named(name) if name == "void")
        {
            let intrinsic = function.body.remove(0);
            function.body.push(Spanned {
                value: ExprKind::Return(Some(Box::new(intrinsic))),
                span: function.span,
                origin: function.origin.clone(),
            });
        }
        if !seen.insert(function.name.value.clone()) {
            return Err(AstError::new(
                "E-DEFFECT-003",
                format!("duplicate deffect operation `{}`", function.name.value),
                function.name.span,
            ));
        }
        let effects = function
            .annotations
            .iter()
            .find_map(|annotation| match &annotation.value {
                AnnotationKind::Effects(effects) => Some(effects.clone()),
                _ => None,
            })
            .unwrap_or_else(|| EffectRow {
                labels: Vec::new(),
                tail: None,
                span: function.span,
            });
        operations.push(EffectOperation {
            span: function.span,
            origin: function.origin.clone(),
            function,
            effects,
        });
    }
    Ok(Deffect {
        visibility,
        name,
        operations,
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
    if args.len() < 4 {
        return Err(AstError::new(
            "E-SYN-009",
            format!("`macro` expects at least 4 operands, found {}", args.len()),
            node.span,
        ));
    }
    let (attributes, body_nodes) = split_labeled_remainder("macro", args, 3, node.span)?;
    Ok(Macro {
        visibility: parse_visibility(&attributes, visibility)?,
        name: name(args[0])?,
        parameters: parse_macro_parameters(args[1])?,
        result: parse_syntax_category(args[2])?,
        body: parse_owned_macro_body(&body_nodes)?,
        annotations: parse_annotations(
            &attributes
                .iter()
                .copied()
                .filter(|attribute| {
                    label(attribute.label).is_ok_and(|name| name.value != "visibility")
                })
                .collect::<Vec<_>>(),
            AnnotationOwner::Macro,
        )?,
        span: node.span,
        origin: source_origin(node.span),
    })
}

fn parse_macro_parameters(node: &Node) -> Result<Vec<MacroParameter>, AstError> {
    let values = semantic_nodes(list(node)?).collect::<Vec<_>>();
    if values.len() % 2 != 0 {
        return Err(AstError::new(
            "E-SYN-009",
            "macro parameter list expects flat `name category` pairs",
            node.span,
        ));
    }
    let parameter_count = values.len() / 2;
    values
        .chunks_exact(2)
        .enumerate()
        .map(|(index, values)| {
            let raw_name = name(values[0])?;
            let variadic = raw_name.value.ends_with("...");
            if variadic && index + 1 != parameter_count {
                return Err(AstError::new(
                    "E-SYN-008",
                    "variadic macro parameter must be final",
                    raw_name.span,
                ));
            }
            let parameter_name = raw_name
                .value
                .strip_suffix("...")
                .unwrap_or(&raw_name.value);
            if parameter_name == "types" {
                return Err(AstError::new(
                    "E-SYN-008",
                    "`types` is reserved for explicit generic call arguments",
                    raw_name.span,
                ));
            }
            Ok(MacroParameter {
                name: Spanned::source(parameter_name.to_string(), raw_name.span),
                category: parse_syntax_category(values[1])?,
                variadic,
                span: values[0].span.cover(values[1].span),
            })
        })
        .collect()
}

fn parse_owned_macro_body(nodes: &[&Node]) -> Result<Vec<MacroExpr>, AstError> {
    let semantic = nodes
        .iter()
        .copied()
        .filter(|node| !matches!(node.kind, NodeKind::Comment(_)))
        .collect::<Vec<_>>();
    if let [only] = semantic.as_slice() {
        if headed(only).is_ok_and(|(head, _)| head.value == "do") {
            return parse_macro_body(only);
        }
    }
    semantic.iter().map(|node| parse_macro_expr(node)).collect()
}

fn parse_syntax_category(node: &Node) -> Result<Spanned<SyntaxCategory>, AstError> {
    let word = contextual_atom(
        node,
        "syntax category",
        &[
            "expr-syntax",
            "type-syntax",
            "pattern-syntax",
            "definition-syntax",
            "module-syntax",
        ],
    )?;
    let category = match word.value.as_str() {
        "expr-syntax" => SyntaxCategory::Expression,
        "type-syntax" => SyntaxCategory::Type,
        "pattern-syntax" => SyntaxCategory::Pattern,
        "definition-syntax" => SyntaxCategory::Definition,
        _ => SyntaxCategory::Module,
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

fn parse_test_scenario<'a>(
    node: &Node,
    args: impl AsRef<[&'a Node]>,
) -> Result<TestScenario, AstError> {
    let args = args.as_ref();
    if args.len() < 2 {
        return Err(AstError::new(
            "E-SYN-009",
            "`test.scenario` expects a name and at least one `test.case`",
            node.span,
        ));
    }
    let scenario_name = string(args[0])?;
    let cases = args[1..]
        .iter()
        .map(|case| parse_test_case(case))
        .collect::<Result<Vec<_>, _>>()?;
    let mut names = std::collections::BTreeSet::new();
    for case in &cases {
        if !names.insert(case.name.value.clone()) {
            return Err(AstError::new(
                "E-TEST-011",
                format!(
                    "duplicate test case `{}` in scenario `{}`",
                    case.name.value, scenario_name.value
                ),
                case.name.span,
            ));
        }
    }
    Ok(TestScenario {
        name: scenario_name,
        cases,
        span: node.span,
        origin: source_origin(node.span),
    })
}

fn parse_test_case(node: &Node) -> Result<TestCase, AstError> {
    let (head, args) = headed(node)?;
    if head.value != "test.case" {
        return Err(expected_head("test.case", &head));
    }
    if args.len() < 2 {
        return Err(AstError::new(
            "E-SYN-009",
            "`test.case` expects a name and at least one expression",
            node.span,
        ));
    }
    let (attributes, body_nodes) = split_labeled_remainder("test.case", &args, 1, node.span)?;
    let mut profile = None;
    for attribute in &attributes {
        if label(attribute.label)?.value == "profile" {
            profile = Some(open_atom(attribute.value, "`profile:`")?);
        }
    }
    let profile = profile.unwrap_or_else(|| Spanned::source("core".to_string(), node.span));
    Ok(TestCase {
        name: string(args[0])?,
        profile,
        body: parse_owned_body(&body_nodes)?,
        metadata: attributes
            .iter()
            .filter(|attribute| label(attribute.label).is_ok_and(|name| name.value != "profile"))
            .map(parse_test_meta)
            .collect::<Result<_, _>>()?,
        span: node.span,
        origin: source_origin(node.span),
    })
}

fn parse_parameters(node: &Node) -> Result<Vec<Parameter>, AstError> {
    let values = semantic_nodes(list(node)?).collect::<Vec<_>>();
    if values.len() % 2 != 0 {
        return Err(AstError::new(
            "E-SYN-009",
            "parameter list expects flat `name type` pairs",
            node.span,
        ));
    }
    let parameter_count = values.len() / 2;
    values
        .chunks_exact(2)
        .enumerate()
        .map(|(index, values)| {
            let raw_name = name(values[0])?;
            let variadic = raw_name.value.ends_with("...");
            if variadic && index + 1 != parameter_count {
                return Err(AstError::new(
                    "E-SYN-008",
                    "variadic parameter must be the final parameter",
                    raw_name.span,
                ));
            }
            let parameter_name = raw_name
                .value
                .strip_suffix("...")
                .unwrap_or(&raw_name.value);
            if parameter_name.is_empty() {
                return Err(AstError::new(
                    "E-SYN-008",
                    "variadic parameter requires a name before `...`",
                    raw_name.span,
                ));
            }
            if parameter_name == "types" {
                return Err(AstError::new(
                    "E-SYN-008",
                    "`types` is reserved for explicit generic call arguments",
                    raw_name.span,
                ));
            }
            Ok(Parameter {
                name: Spanned::source(parameter_name.to_string(), raw_name.span),
                ty: parse_type(values[1])?,
                variadic,
                span: values[0].span.cover(values[1].span),
            })
        })
        .collect()
}

fn parse_owned_body(nodes: &[&Node]) -> Result<Vec<Expr>, AstError> {
    let semantic = nodes
        .iter()
        .copied()
        .filter(|node| !matches!(node.kind, NodeKind::Comment(_)))
        .collect::<Vec<_>>();
    if let [only] = semantic.as_slice() {
        if headed(only).is_ok_and(|(head, _)| head.value == "do") {
            return parse_body(only);
        }
    }
    parse_exprs(semantic)
}

fn parse_visibility(
    attributes: &[AttributeRef<'_>],
    default: Visibility,
) -> Result<Visibility, AstError> {
    let Some(attribute) = attributes
        .iter()
        .find(|attribute| label(attribute.label).is_ok_and(|name| name.value == "visibility"))
    else {
        return Ok(default);
    };
    let visibility = contextual_atom(attribute.value, "visibility", &["public", "private"])?;
    Ok(if visibility.value == "public" {
        Visibility::Public
    } else {
        Visibility::Private
    })
}

fn parse_body(node: &Node) -> Result<Vec<Expr>, AstError> {
    let (head, args) = headed(node)?;
    if head.value != "do" {
        return Err(expected_head("do", &head));
    }
    args.iter().map(|node| parse_expr(node)).collect()
}

fn parse_type(node: &Node) -> Result<TypeExpr, AstError> {
    if let NodeKind::Atom(Atom::Atom(value)) = &node.kind {
        return Ok(Spanned::source(
            TypeExprKind::Literal(Literal::Atom(value.clone())),
            node.span,
        ));
    }
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
            // An interface method's effects are part of its contract, so `fn-type`
            // takes an optional trailing `effects:` attribute after its two
            // positional operands.
            let attributes = trailing_attributes("fn-type", &args, 2, node.span)?;
            let mut effects = EffectRow {
                labels: Vec::new(),
                tail: None,
                span: node.span,
            };
            for attribute in &attributes {
                let label = label(attribute.label)?;
                if label.value != "effects" {
                    return Err(AstError::new(
                        "E-SYN-011",
                        format!("unknown `fn-type` attribute `{}:`", label.value),
                        label.span,
                    ));
                }
                effects = parse_effects_value(attribute.value)?;
            }
            TypeExprKind::Function {
                parameters: parse_function_type_parameters(args[0])?,
                result: Box::new(parse_type(args[1])?),
                effects,
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
        "handle" => {
            exact_arity("handle", &args, 1, node.span)?;
            TypeExprKind::Handle(parse_handle_access(args[0])?)
        }
        "effect" => {
            return Err(AstError::new(
                "E-EFFECT-008",
                "structural `(effect @domain @action)` syntax was removed; use a nominal root name such as `fs.read`",
                head.span,
            ));
        }
        "wasm" => {
            exact_arity("wasm type", &args, 1, node.span)?;
            TypeExprKind::WasmValue(name(args[0])?)
        }
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

fn parse_function_type_parameters(node: &Node) -> Result<Vec<FunctionTypeParameter>, AstError> {
    let values = semantic_nodes(list(node)?).collect::<Vec<_>>();
    if values.len() % 2 != 0 {
        return Err(AstError::new(
            "E-SYN-009",
            "fn-type parameter list expects flat `name type` pairs",
            node.span,
        ));
    }
    let parameter_count = values.len() / 2;
    values
        .chunks_exact(2)
        .enumerate()
        .map(|(index, values)| {
            let raw_name = name(values[0])?;
            let variadic = raw_name.value.ends_with("...");
            if variadic && index + 1 != parameter_count {
                return Err(AstError::new(
                    "E-SYN-008",
                    "variadic fn-type parameter must be final",
                    raw_name.span,
                ));
            }
            let parameter_name = raw_name
                .value
                .strip_suffix("...")
                .unwrap_or(&raw_name.value);
            Ok(FunctionTypeParameter {
                name: Spanned::source(parameter_name.to_string(), raw_name.span),
                ty: parse_type(values[1])?,
                variadic,
                span: values[0].span.cover(values[1].span),
            })
        })
        .collect()
}

fn parse_handle_access(node: &Node) -> Result<Spanned<HandleAccess>, AstError> {
    let word = contextual_atom(
        node,
        "handle access",
        &["read", "write", "read-write", "any", "process"],
    )?;
    let access = match word.value.as_str() {
        "read" => HandleAccess::Read,
        "write" => HandleAccess::Write,
        "read-write" => HandleAccess::ReadWrite,
        "any" => HandleAccess::Any,
        _ => HandleAccess::Process,
    };
    Ok(Spanned::source(access, node.span))
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
        "fn" => {
            min_arity("fn", &args, 3, node.span)?;
            let (attributes, body_nodes) = split_labeled_remainder("fn", &args, 2, node.span)?;
            if let Some(attribute) = attributes.first() {
                return Err(AstError::new(
                    "E-SYN-011",
                    "anonymous `fn` does not accept labelled attributes",
                    attribute.label.span,
                ));
            }
            ExprKind::AnonymousFunction {
                parameters: parse_parameters(args[0])?,
                return_type: parse_type(args[1])?,
                body: parse_owned_body(&body_nodes)?,
            }
        }
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
        "try" => {
            exact_arity("try", &args, 1, node.span)?;
            ExprKind::Try(Box::new(parse_expr(args[0])?))
        }
        "if" => {
            exact_arity("if", &args, 3, node.span)?;
            ExprKind::If {
                condition: Box::new(parse_expr(args[0])?),
                then_body: vec![parse_expr(args[1])?],
                else_body: vec![parse_expr(args[2])?],
            }
        }
        "while" => {
            min_arity("while", &args, 2, node.span)?;
            ExprKind::While {
                condition: Box::new(parse_expr(args[0])?),
                body: parse_owned_body(&args[1..])?,
            }
        }
        "for" => {
            min_arity("for", &args, 3, node.span)?;
            ExprKind::For {
                binding: name(args[0])?,
                source: Box::new(parse_expr(args[1])?),
                body: parse_owned_body(&args[2..])?,
            }
        }
        "match" => {
            min_arity("match", &args, 2, node.span)?;
            if (args.len() - 1) % 2 != 0 {
                return Err(AstError::new(
                    "E-SYN-009",
                    "match expects alternating `pattern expression` arms",
                    node.span,
                ));
            }
            ExprKind::Match {
                target: Box::new(parse_expr(args[0])?),
                cases: args[1..]
                    .chunks_exact(2)
                    .map(|arm| {
                        Ok(MatchCase {
                            pattern: parse_pattern(arm[0])?,
                            body: vec![parse_expr(arm[1])?],
                            span: arm[0].span.cover(arm[1].span),
                        })
                    })
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
        "embed" => {
            let attributes = trailing_attributes("embed", &args, 1, node.span)?;
            let path = string(args[0])?;
            let mut format = Spanned::source(EmbedFormat::Auto, path.span);
            for attribute in attributes {
                let attribute_name = label(attribute.label)?;
                if attribute_name.value != "format" {
                    return Err(AstError::new(
                        "E-SYN-011",
                        format!("unknown embed attribute `{}:`", attribute_name.value),
                        attribute_name.span,
                    ));
                }
                format = parse_embed_format(attribute.value)?;
            }
            if format.value == EmbedFormat::Auto
                && matches!(
                    Path::new(&path.value)
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .map(str::to_ascii_lowercase)
                        .as_deref(),
                    Some("yaml" | "yml")
                )
            {
                return Err(AstError::new(
                    "E-SYN-008",
                    "YAML embed format was removed",
                    path.span,
                ));
            }
            ExprKind::Embed { path, format }
        }
        "template" => {
            let attributes = trailing_attributes("template", &args, 1, node.span)?;
            let mut bindings = None;
            for attribute in attributes {
                let attribute_name = label(attribute.label)?;
                if attribute_name.value != "with" {
                    return Err(AstError::new(
                        "E-SYN-011",
                        format!("unknown template attribute `{}:`", attribute_name.value),
                        attribute_name.span,
                    ));
                }
                let (record_head, record_args) = headed(attribute.value)?;
                if record_head.value != "record" {
                    return Err(expected_head("record", &record_head));
                }
                bindings = Some(parse_expr_fields(record_args)?);
            }
            ExprKind::Template {
                path: string(args[0])?,
                bindings: bindings.ok_or_else(|| {
                    AstError::new(
                        "E-SYN-011",
                        "template is missing required `with:` attribute",
                        node.span,
                    )
                })?,
            }
        }
        "intrinsic" => {
            if args.is_empty() {
                return Err(AstError::new(
                    "E-INTRINSIC-002",
                    "`intrinsic` expects an atom name and zero or more arguments",
                    node.span,
                ));
            }
            ExprKind::Intrinsic {
                name: open_atom(args[0], "intrinsic name")?,
                arguments: args[1..]
                    .iter()
                    .map(|argument| parse_expr(argument))
                    .collect::<Result<_, _>>()?,
            }
        }
        "wasm" => parse_wasm_expr(node, &args)?,
        "task" => {
            let (attributes, body_nodes) = split_labeled_remainder("task", &args, 0, node.span)?;
            let captures = required_capture_attribute("task", &attributes, node.span)?;
            ExprKind::Task {
                captures,
                body: parse_owned_body(&body_nodes)?,
            }
        }
        "spawn" => {
            let attributes = trailing_attributes("spawn", &args, 2, node.span)?;
            let captures = required_capture_attribute("spawn", &attributes, node.span)?;
            ExprKind::Spawn {
                handle: name(args[0])?,
                captures,
                value: Box::new(parse_expr(args[1])?),
            }
        }
        "join" => {
            exact_arity("join", &args, 2, node.span)?;
            ExprKind::Join {
                handle: name(args[0])?,
                binding: name(args[1])?,
            }
        }
        "apply" => {
            return Err(AstError::new(
                "E-SYN-008",
                "`apply` was removed; pass explicit generic arguments with `types:`",
                head.span,
            ));
        }
        _ => {
            let (arguments, type_arguments, type_arguments_position) =
                parse_call_arguments(&args, node.span)?;
            ExprKind::Call {
                callee: head,
                arguments,
                type_arguments,
                type_arguments_position,
            }
        }
    };
    Ok(Spanned::source(value, node.span))
}

fn parse_call_arguments(
    args: &[&Node],
    call_span: Span,
) -> Result<(Vec<CallArgument>, Vec<TypeExpr>, Option<usize>), AstError> {
    let mut arguments = Vec::new();
    let mut type_arguments = Vec::new();
    let mut type_arguments_position = None;
    let mut index = 0;
    let mut saw_types = false;
    let mut saw_label = false;
    let mut saw_variadic = false;
    while index < args.len() {
        if let NodeKind::Atom(Atom::Label(label_name)) = &args[index].kind {
            if saw_variadic {
                return Err(AstError::new(
                    "E-SYN-013",
                    "labelled arguments must precede the remainder and types operands",
                    args[index].span,
                ));
            }
            let Some(value) = args.get(index + 1) else {
                return Err(AstError::new(
                    "E-SYN-011",
                    format!("call argument `{label_name}:` is missing its value"),
                    args[index].span,
                ));
            };
            if label_name == "types" {
                if saw_types {
                    return Err(AstError::new(
                        "E-SYN-011",
                        "duplicate call argument `types:`",
                        args[index].span,
                    ));
                }
                saw_types = true;
                type_arguments_position = Some(arguments.len());
                type_arguments = semantic_nodes(list(value)?)
                    .map(parse_type)
                    .collect::<Result<_, _>>()?;
                index += 2;
                continue;
            }
            saw_label = true;
            arguments.push(CallArgument::Labelled {
                label: Spanned::source(label_name.clone(), args[index].span),
                value: parse_expr(value)?,
            });
            index += 2;
        } else {
            if saw_label || saw_types {
                saw_variadic = true;
            }
            arguments.push(CallArgument::Positional(parse_expr(args[index])?));
            index += 1;
        }
    }
    if arguments.is_empty() && type_arguments.is_empty() && args.is_empty() {
        let _ = call_span;
    }
    Ok((arguments, type_arguments, type_arguments_position))
}

fn parse_embed_format(node: &Node) -> Result<Spanned<EmbedFormat>, AstError> {
    if matches!(atom_word(node), Some("yaml" | "yml")) {
        return Err(AstError::new(
            "E-SYN-008",
            "YAML embed format was removed",
            node.span,
        ));
    }
    let word = contextual_atom(
        node,
        "embed format",
        &["auto", "text", "binary", "json", "toml", "xml"],
    )?;
    let format = match word.value.as_str() {
        "auto" => EmbedFormat::Auto,
        "text" => EmbedFormat::Text,
        "binary" => EmbedFormat::Binary,
        "json" => EmbedFormat::Json,
        "toml" => EmbedFormat::Toml,
        _ => EmbedFormat::Xml,
    };
    Ok(Spanned::source(format, node.span))
}

fn parse_wasm_expr(node: &Node, args: &[&Node]) -> Result<ExprKind, AstError> {
    if args.len() < 2 {
        return Err(AstError::new(
            "E-SYN-009",
            "`wasm` expects a module, symbol, and optional arguments",
            node.span,
        ));
    }
    Ok(ExprKind::Wasm {
        import: WasmImport {
            module: string(args[0])?,
            name: string(args[1])?,
            span: node.span,
        },
        arguments: args[2..]
            .iter()
            .map(|argument| match &argument.kind {
                NodeKind::Atom(Atom::Symbol(_)) => name(argument).map(WasmArgument::Parameter),
                NodeKind::Atom(Atom::Int(value)) => Ok(WasmArgument::ConstInt(Spanned::source(
                    *value,
                    argument.span,
                ))),
                NodeKind::Atom(Atom::String(value)) => Ok(WasmArgument::ConstString(
                    Spanned::source(value.clone(), argument.span),
                )),
                _ => parse_expr(argument).map(WasmArgument::Expression),
            })
            .collect::<Result<_, _>>()?,
    })
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

fn required_capture_attribute(
    owner: &str,
    attributes: &[AttributeRef<'_>],
    span: Span,
) -> Result<Vec<Name>, AstError> {
    let mut captures = None;
    for attribute in attributes {
        let attribute_name = label(attribute.label)?;
        if attribute_name.value != "captures" {
            return Err(AstError::new(
                "E-SYN-011",
                format!("unknown {owner} attribute `{}:`", attribute_name.value),
                attribute_name.span,
            ));
        }
        if captures.is_some() {
            return Err(AstError::new(
                "E-SYN-011",
                format!("duplicate `{owner}` captures attribute"),
                attribute.span,
            ));
        }
        captures = Some(
            semantic_nodes(list(attribute.value)?)
                .map(name)
                .collect::<Result<_, _>>()?,
        );
    }
    captures.ok_or_else(|| {
        AstError::new(
            "E-SYN-011",
            format!("{owner} is missing required `captures:` attribute"),
            span,
        )
    })
}

fn parse_annotations(
    attributes: &[AttributeRef<'_>],
    owner: AnnotationOwner,
) -> Result<Vec<Annotation>, AstError> {
    let mut annotations = Vec::new();
    for attribute in attributes {
        let label = label(attribute.label)?;
        match label.value.as_str() {
            "effects" if !owner.allows_effects() => {
                return Err(AstError::new(
                    "E-EFFECT-006",
                    format!(
                        "`effects:` declares what a function body may perform and has no meaning on `{}`",
                        owner.label()
                    ),
                    label.span,
                ));
            }
            "effects" => annotations.push(Spanned::source(
                AnnotationKind::Effects(parse_effects_value(attribute.value)?),
                attribute.span,
            )),
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

/// `effects: (fs.read stream.write)` is a flat list of nominal root names.
/// An empty list is legal and means the same as omitting the attribute.
fn parse_effects_value(node: &Node) -> Result<EffectRow, AstError> {
    let labels = semantic_nodes(list(node)?)
        .map(parse_type)
        .collect::<Result<Vec<_>, _>>()?;
    for label in &labels {
        if !matches!(label.value, TypeExprKind::Named(_)) {
            return Err(AstError::new(
                "E-EFFECT-004",
                "each `effects:` element must be a nominal root name such as `fs.read`",
                label.span,
            ));
        }
    }
    Ok(EffectRow {
        labels,
        tail: None,
        span: node.span,
    })
}

fn parse_where_value(node: &Node) -> Result<Vec<TypeParameter>, AstError> {
    let values = semantic_nodes(list(node)?).collect::<Vec<_>>();
    if values.len() % 2 != 0 {
        return Err(AstError::new(
            "E-SYN-009",
            "where expects flat `name bound` pairs",
            node.span,
        ));
    }
    values
        .chunks_exact(2)
        .map(|values| {
            Ok(TypeParameter {
                name: name(values[0])?,
                bounds: vec![parse_type(values[1])?],
                span: values[0].span.cover(values[1].span),
            })
        })
        .collect()
}

fn parse_defs_value(node: &Node) -> Result<Vec<Function>, AstError> {
    semantic_nodes(list(node)?)
        .map(|node| {
            let (head, args) = headed(node)?;
            if head.value != "defn" {
                return Err(expected_head("defn", &head));
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
    let method_name = name(args[0])?;
    let binding = if symbol(args[1]).is_some() {
        MethodBinding::Reference(name(args[1])?)
    } else {
        let (fn_head, fn_args) = headed(args[1])?;
        if fn_head.value != "fn" {
            return Err(expected_head("fn", &fn_head));
        }
        if fn_args.len() < 3 {
            return Err(AstError::new(
                "E-SYN-009",
                "inline `fn` expects parameters, return type, and at least one body expression",
                args[1].span,
            ));
        }
        let (attributes, body_nodes) = split_labeled_remainder("fn", &fn_args, 2, args[1].span)?;
        MethodBinding::Function(Function {
            visibility: Visibility::Public,
            name: method_name.clone(),
            parameters: parse_parameters(fn_args[0])?,
            return_type: parse_type(fn_args[1])?,
            body: parse_owned_body(&body_nodes)?,
            annotations: parse_annotations(&attributes, AnnotationOwner::Function)?,
            span: args[1].span,
            origin: source_origin(args[1].span),
        })
    };
    Ok(ImplItem::Method {
        name: method_name,
        binding,
        span: node.span,
    })
}

fn parse_test_meta(attribute: &AttributeRef<'_>) -> Result<TestMeta, AstError> {
    let label = label(attribute.label)?;
    match label.value.as_str() {
        "tags" => Ok(TestMeta::Tags(
            semantic_nodes(list(attribute.value)?)
                .map(|node| open_atom(node, "`tags:` entry"))
                .collect::<Result<_, _>>()?,
        )),
        "timeout-ms" => {
            let value = integer(attribute.value)?;
            if value.value <= 0 {
                return Err(AstError::new(
                    "E-SYN-008",
                    "`timeout-ms:` must be greater than zero",
                    value.span,
                ));
            }
            Ok(TestMeta::TimeoutMillis(value))
        }
        "random-seed" => {
            let value = integer(attribute.value)?;
            if value.value < 0 {
                return Err(AstError::new(
                    "E-SYN-008",
                    "`random-seed:` must be non-negative",
                    value.span,
                ));
            }
            Ok(TestMeta::RandomSeed(value))
        }
        "skip" => {
            let reason = string(attribute.value)?;
            if reason.value.is_empty() {
                return Err(AstError::new(
                    "E-SYN-008",
                    "`skip:` must be a non-empty string",
                    reason.span,
                ));
            }
            Ok(TestMeta::Skip(reason))
        }
        "expect-error" => {
            let args = semantic_nodes(list(attribute.value)?).collect::<Vec<_>>();
            min_arity("expect-error", &args, 1, attribute.span)?;
            let phase = contextual_atom(
                args[0],
                "expect-error phase",
                &["load", "compile", "runtime"],
            )?;
            let expected = match phase.value.as_str() {
                "runtime" => {
                    exact_arity("runtime expect-error", &args, 2, attribute.span)?;
                    ExpectedError::Runtime {
                        message: non_empty_string(args[1], "`expect-error` message")?,
                    }
                }
                phase => {
                    range_arity("load/compile expect-error", &args, 2, 3, attribute.span)?;
                    let code = name(args[1])?;
                    let message = args
                        .get(2)
                        .map(|node| non_empty_string(node, "`expect-error` message"))
                        .transpose()?;
                    if phase == "load" {
                        ExpectedError::Load { code, message }
                    } else {
                        ExpectedError::Compile { code, message }
                    }
                }
            };
            Ok(TestMeta::ExpectError(expected))
        }
        "clock" => {
            let args = semantic_nodes(list(attribute.value)?).collect::<Vec<_>>();
            exact_arity("clock", &args, 3, attribute.span)?;
            contextual_atom(args[0], "clock mode", &["fixed"])?;
            let unix_millis = integer(args[1])?;
            let monotonic_millis = integer(args[2])?;
            if unix_millis.value < 0 || monotonic_millis.value < 0 {
                return Err(AstError::new(
                    "E-SYN-008",
                    "fixed clock milliseconds must be non-negative",
                    if unix_millis.value < 0 {
                        unix_millis.span
                    } else {
                        monotonic_millis.span
                    },
                ));
            }
            Ok(TestMeta::Clock {
                unix_millis,
                monotonic_millis,
            })
        }
        "workspace" => Ok(TestMeta::Workspace(contextual_atom(
            attribute.value,
            "workspace mode",
            &["temp"],
        )?)),
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

/// Split a form's fixed operands, labelled attributes, and trailing remainder.
///
/// Body-bearing forms use this ordering so attributes can configure the form
/// before its variadic body. Once the first remainder expression is seen, a
/// later label is unambiguously misplaced and is rejected rather than being
/// silently reordered by tooling.
fn split_labeled_remainder<'a>(
    form: &str,
    args: &[&'a Node],
    positional: usize,
    span: Span,
) -> Result<(Vec<AttributeRef<'a>>, Vec<&'a Node>), AstError> {
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

    let mut attributes = Vec::new();
    let mut remainder = Vec::new();
    let mut seen = BTreeSet::new();
    let mut remainder_started = false;
    let mut index = positional;
    while index < args.len() {
        let node = args[index];
        if matches!(node.kind, NodeKind::Atom(Atom::Label(_))) {
            if remainder_started {
                return Err(AstError::new(
                    "E-SYN-013",
                    format!(
                        "labelled attribute appears after `{form}` remainder operands; labels must precede the body"
                    ),
                    node.span,
                ));
            }
            let label = label(node)?;
            let Some(value) = args.get(index + 1) else {
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
                label: node,
                value,
                span: node.span.cover(value.span),
            });
            index += 2;
        } else {
            remainder_started = true;
            remainder.push(node);
            index += 1;
        }
    }
    Ok((attributes, remainder))
}

fn literal_node(node: &Node) -> Result<Spanned<Literal>, AstError> {
    match &node.kind {
        NodeKind::Atom(atom) => literal(atom)
            .map(|value| Spanned::source(value, node.span))
            .ok_or_else(|| AstError::new("E-SYN-010", "expected a literal", node.span)),
        _ => Err(AstError::new("E-SYN-010", "expected a literal", node.span)),
    }
}

fn literal(atom: &Atom) -> Option<Literal> {
    match atom {
        Atom::Atom(value) => Some(Literal::Atom(value.clone())),
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

fn atom_word(node: &Node) -> Option<&str> {
    match &node.kind {
        NodeKind::Atom(Atom::Atom(value)) => Some(value),
        _ => None,
    }
}

/// Contextual keyword positions accept only atoms; a bare symbol there is the
/// pre-atom spelling and must be rejected rather than silently accepted.
fn open_atom(node: &Node, context: &str) -> Result<Name, AstError> {
    atom_word(node)
        .map(|value| Spanned::source(value.to_string(), node.span))
        .ok_or_else(|| {
            AstError::new(
                "E-ATOM-003",
                format!("{context} must be an atom such as `@name`"),
                node.span,
            )
        })
}

fn contextual_atom(node: &Node, context: &str, allowed: &[&str]) -> Result<Name, AstError> {
    let expected = allowed
        .iter()
        .map(|value| format!("`@{value}`"))
        .collect::<Vec<_>>()
        .join(", ");
    match atom_word(node) {
        Some(value) if allowed.contains(&value) => {
            Ok(Spanned::source(value.to_string(), node.span))
        }
        Some(value) => Err(AstError::new(
            "E-ATOM-002",
            format!("unknown {context} `@{value}`; expected one of {expected}"),
            node.span,
        )),
        None => Err(AstError::new(
            "E-ATOM-003",
            format!("{context} must be an atom; expected one of {expected}"),
            node.span,
        )),
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

fn non_empty_string(node: &Node, field: &str) -> Result<Spanned<String>, AstError> {
    let value = string(node)?;
    if value.value.is_empty() {
        return Err(AstError::new(
            "E-SYN-008",
            format!("{field} must be a non-empty string"),
            value.span,
        ));
    }
    Ok(value)
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
(import io "./io.vib")
(def option (enum (some t) (none void)) where: (t any) doc: "Optional.")
(const limit int64 10 visibility: @private)
(defn
  choose
  (value bool fallback bool)
  bool
  (do (if value (do (return value)) (do (return fallback))))
)
(test.scenario "suite" (test.case "works" tags: (@fast @language) clock: (@fixed 0 0) (test.assert true)  ))
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
    fn lowers_defn_with_flat_variadic_parameters_direct_body_and_visibility() {
        let parsed = module(
            r#"(defn collect (head int64 rest... int64) void
  visibility: @private doc: "Collect values." (return)

  )"#,
        )
        .unwrap();
        let TopLevel::Function(function) = &parsed.forms[0] else {
            panic!("expected function definition");
        };
        assert_eq!(function.visibility, Visibility::Private);
        assert_eq!(function.parameters.len(), 2);
        assert_eq!(function.parameters[0].name.value, "head");
        assert!(!function.parameters[0].variadic);
        assert_eq!(function.parameters[1].name.value, "rest");
        assert!(function.parameters[1].variadic);
        assert!(matches!(function.body[0].value, ExprKind::Return(None)));
    }

    #[test]
    fn accepts_labels_before_function_body_and_rejects_labels_after_remainder() {
        let parsed = module(r#"(defn documented () void doc: "A function" (return unit))"#)
            .expect("labels before the body are canonical");
        let TopLevel::Function(function) = &parsed.forms[0] else {
            panic!("expected function definition");
        };
        assert_eq!(function.body.len(), 1);
        assert!(matches!(
            function.annotations[0].value,
            AnnotationKind::Doc(_)
        ));

        let error = module(r#"(defn invalid () void (return unit) doc: "Late")"#)
            .expect_err("labels after a body remainder must be rejected");
        assert_eq!(error.code, "E-SYN-013");
    }

    #[test]
    fn rejects_late_labels_for_all_body_bearing_structural_forms() {
        for source in [
            "(macro m () @expr-syntax (quote @expr-syntax unit) doc: \"late\")",
            "(defn outer () void (fn () void unit doc: \"late\"))",
            "(defn outer () void (task unit captures: ()))",
            "(test.scenario \"late\" (test.case \"late-label\" unit tags: (@core)))",
        ] {
            let error = module(source).expect_err("late labels must fail directly");
            assert_eq!(error.code, "E-SYN-013", "source: {source}");
        }
    }

    #[test]
    fn preserves_mixed_labelled_call_arguments_and_explicit_types_in_source_order() {
        let parsed =
            module("(defn caller () void (target 2 first: 1 second: 3 types: (int64)))").unwrap();
        let TopLevel::Function(function) = &parsed.forms[0] else {
            panic!("expected function definition");
        };
        let ExprKind::Call {
            arguments,
            type_arguments,
            ..
        } = &function.body[0].value
        else {
            panic!("expected call");
        };
        assert!(matches!(arguments[0], CallArgument::Positional(_)));
        assert!(
            matches!(arguments[1], CallArgument::Labelled { ref label, .. } if label.value == "first")
        );
        assert!(
            matches!(arguments[2], CallArgument::Labelled { ref label, .. } if label.value == "second")
        );
        assert_eq!(type_arguments.len(), 1);
    }

    #[test]
    fn parses_try_as_a_single_argument_propagation_form() {
        let parsed = module("(defn read () int64 (return (try value)))").unwrap();
        let TopLevel::Function(function) = &parsed.forms[0] else {
            panic!("expected function");
        };
        let ExprKind::Return(Some(value)) = &function.body[0].value else {
            panic!("expected return");
        };
        assert!(matches!(
            &value.value,
            ExprKind::Try(inner) if matches!(&inner.value, ExprKind::Reference(name) if name == "value")
        ));
    }

    #[test]
    fn lowers_macro_with_flat_variadic_parameters_and_direct_body() {
        let parsed = module(
            "(macro emit (head @expr-syntax tail... @expr-syntax) @expr-syntax (quote @expr-syntax head))",
        )
        .unwrap();
        let TopLevel::Macro(definition) = &parsed.forms[0] else {
            panic!("expected macro definition");
        };
        assert_eq!(definition.parameters.len(), 2);
        assert_eq!(definition.parameters[0].name.value, "head");
        assert!(!definition.parameters[0].variadic);
        assert_eq!(definition.parameters[1].name.value, "tail");
        assert!(definition.parameters[1].variadic);
        assert_eq!(definition.body.len(), 1);
    }

    #[test]
    fn fn_type_uses_named_flat_parameters_and_final_variadic_status() {
        let parsed = module("(def callback (fn-type (value int64 rest... str) bool))").unwrap();
        let TopLevel::Definition(definition) = &parsed.forms[0] else {
            panic!("expected definition");
        };
        let TypeExprKind::Function { parameters, .. } = &definition.body.value else {
            panic!("expected function type");
        };
        assert_eq!(parameters[0].name.value, "value");
        assert!(!parameters[0].variadic);
        assert_eq!(parameters[1].name.value, "rest");
        assert!(parameters[1].variadic);
    }

    #[test]
    fn where_attribute_uses_flat_name_bound_pairs_and_any_sentinel() {
        let parsed = module("(def box (record (value t)) where: (t any))").unwrap();
        let TopLevel::Definition(definition) = &parsed.forms[0] else {
            panic!("expected definition");
        };
        let AnnotationKind::Where(parameters) = &definition.annotations[0].value else {
            panic!("expected where annotation");
        };
        assert_eq!(parameters[0].name.value, "t");
        assert!(
            matches!(parameters[0].bounds[0].value, TypeExprKind::Named(ref name) if name == "any")
        );
    }

    #[test]
    fn body_forms_use_direct_expressions_flat_match_arms_and_capture_attributes() {
        let parsed = module(
            r#"(defn flow (value bool items (array int64)) void
  (if value (return) (return))
  (while value (set value false) (continue))
  (for item items (let seen item) (continue))
  (match value true (return) false (return))
  (task captures: (value) (return)))"#,
        )
        .unwrap();
        let TopLevel::Function(function) = &parsed.forms[0] else {
            panic!("expected function");
        };
        assert!(
            matches!(&function.body[0].value, ExprKind::If { then_body, else_body, .. } if then_body.len() == 1 && else_body.len() == 1)
        );
        assert!(matches!(&function.body[1].value, ExprKind::While { body, .. } if body.len() == 2));
        assert!(matches!(&function.body[2].value, ExprKind::For { body, .. } if body.len() == 2));
        assert!(
            matches!(&function.body[3].value, ExprKind::Match { cases, .. } if cases.len() == 2)
        );
        assert!(
            matches!(&function.body[4].value, ExprKind::Task { captures, body } if captures.len() == 1 && body.len() == 1)
        );
    }

    #[test]
    fn anonymous_function_is_a_typed_value_node() {
        let parsed =
            module("(const callback any (fn (value int64) int64 (return value)))").unwrap();
        let TopLevel::Constant(constant) = &parsed.forms[0] else {
            panic!("expected constant");
        };
        assert!(matches!(
            &constant.value.value,
            ExprKind::AnonymousFunction { parameters, body, .. }
                if parameters.len() == 1 && body.len() == 1
        ));
    }

    #[test]
    fn inline_implementation_method_uses_anonymous_fn_syntax() {
        let parsed = module(
            r#"(def display (interface (show (fn-type (self self) str))))
(def box (record (value str))
  impls: ((impl display methods: ((method show (fn (value self) str (return "box")))))))"#,
        )
        .unwrap();
        let TopLevel::Definition(definition) = &parsed.forms[1] else {
            panic!("expected definition");
        };
        let AnnotationKind::Implementation { items, .. } = &definition.annotations[0].value else {
            panic!("expected implementation");
        };
        assert!(matches!(
            items[0],
            ImplItem::Method {
                binding: MethodBinding::Function(_),
                ..
            }
        ));
    }

    #[test]
    fn visibility_attribute_replaces_private_wrapper_for_all_declarations() {
        let parsed = module(
            r#"(def hidden (record) visibility: @private)
(const secret int64 1 visibility: @private)
(macro concealed () @expr-syntax visibility: @private (quote @expr-syntax 1))"#,
        )
        .unwrap();
        assert!(
            matches!(&parsed.forms[0], TopLevel::Definition(value) if value.visibility == Visibility::Private)
        );
        assert!(
            matches!(&parsed.forms[1], TopLevel::Constant(value) if value.visibility == Visibility::Private)
        );
        assert!(
            matches!(&parsed.forms[2], TopLevel::Macro(value) if value.visibility == Visibility::Private)
        );
        let error = module("(private (const legacy int64 1))").unwrap_err();
        assert_eq!(error.code, "E-SYN-007");
    }

    #[test]
    fn removed_declaration_call_and_helper_spellings_are_rejected() {
        for source in [
            "(fn named () void unit)",
            "(test sample unit)",
            "(apply identity (int64) 1)",
            "(defn main () void (spawn worker (captures) 1))",
            "(defn main () void (match 1 (case 1 unit)))",
            "(defn main () void (wasm import: (host symbol) args: ()))",
        ] {
            assert!(module(source).is_err(), "accepted removed syntax: {source}");
        }
    }

    #[test]
    fn lowers_embed_template_and_wasm_bodies_as_typed_nodes() {
        let parsed = module(
            r#"
(defn
  assets
  ()
  void
  (do
    (embed "message.txt" format: @text)
    (template "message.mustache" with: (record (name "Vibra")))
    (wasm "vibra:host/abi@1" "io_write" output 1 "suffix")))"#,
        )
        .unwrap();
        let TopLevel::Function(function) = &parsed.forms[0] else {
            panic!("expected function");
        };
        assert!(matches!(
            function.body[0].value,
            ExprKind::Embed {
                format: Spanned {
                    value: EmbedFormat::Text,
                    ..
                },
                ..
            }
        ));
        assert!(matches!(
            function.body[1].value,
            ExprKind::Template { ref bindings, .. } if bindings.len() == 1
        ));
        assert!(matches!(
            function.body[2].value,
            ExprKind::Wasm { ref arguments, .. } if arguments.len() == 3
        ));

        let yaml =
            module(r#"(defn bad () void (do (embed "legacy.yaml" format: @yaml)))"#).unwrap_err();
        assert_eq!(yaml.code, "E-SYN-008");
        assert_eq!(
            module(r#"(defn bad () void (do (embed "legacy.YML")))"#)
                .unwrap_err()
                .code,
            "E-SYN-008"
        );
    }

    #[test]
    fn convert_preserves_the_fallback_literals_own_span() {
        let source = "(defn narrow (value int64) int32 (do (return (convert value int32 5))))";
        let parsed = module(source).unwrap();
        let TopLevel::Function(function) = &parsed.forms[0] else {
            panic!("expected function");
        };
        let ExprKind::Return(Some(convert_expr)) = &function.body[0].value else {
            panic!("expected return");
        };
        let ExprKind::Convert { fallback, .. } = &convert_expr.value else {
            panic!("expected convert, got {:?}", convert_expr.value);
        };
        assert_eq!(fallback.value, Literal::Int(5));
        // Exact half-open span of the `5` token, not the enclosing `convert`
        // form: this is the origin problem the fallback field exists to fix.
        let start = source.rfind('5').expect("fallback literal present");
        assert_eq!(fallback.span, Span::new(start, start + 1));
        assert_eq!(fallback.span, fallback.origin.primary_span());
    }

    #[test]
    fn lowers_explicit_security_types_and_validates_their_closed_shapes() {
        let parsed = module(r#"(def input-handle (handle @read))"#).unwrap();
        assert!(matches!(
            parsed.forms[0],
            TopLevel::Definition(Definition {
                body: Spanned {
                    value: TypeExprKind::Handle(Spanned {
                        value: HandleAccess::Read,
                        ..
                    }),
                    ..
                },
                ..
            })
        ));
    }

    #[test]
    fn deffect_operations_are_parsed_as_nominal_effect_declarations() {
        let parsed = module(
            r#"(deffect read
  (defn
    open
    (path path)
    (result reader fs-error)
    effects:
    ()
    (intrinsic @fs-open-read path))
  (defn
    file
    (path path)
    (result str fs-error)
    effects:
    (stream.read stream.manage)
    (return path)))"#,
        )
        .unwrap();
        let TopLevel::Deffect(deffect) = &parsed.forms[0] else {
            panic!("expected deffect");
        };
        assert_eq!(deffect.name.value, "read");
        assert_eq!(deffect.operations.len(), 2);
        assert_eq!(deffect.operations[0].function.name.value, "open");
        assert!(matches!(
            deffect.operations[0].function.body.first().map(|body| &body.value),
            Some(ExprKind::Return(Some(value)))
                if matches!(&value.value, ExprKind::Intrinsic { name, arguments }
                    if name.value == "fs-open-read" && arguments.len() == 1)
        ));
        assert!(matches!(
            deffect.operations[1].effects.labels.first().map(|label| &label.value),
            Some(TypeExprKind::Named(name)) if name == "stream.read"
        ));
    }

    #[test]
    fn lowers_complete_test_metadata_with_trailing_labels() {
        let parsed = module(
            r#"(test.scenario "suite" (test.case "complete" profile: @fs tags: (@filesystem) timeout-ms: 25 random-seed: 42 skip: "sandbox unavailable"
  expect-error: (@compile E-FS-001 "denied") clock: (@fixed 1000 7) workspace: @temp (test.assert true)






  ))
"#,
        )
        .unwrap();
        let TopLevel::TestScenario(scenario) = &parsed.forms[0] else {
            panic!("expected scenario");
        };
        let test = &scenario.cases[0];
        assert_eq!(test.metadata.len(), 7);
        assert!(matches!(test.metadata[1], TestMeta::TimeoutMillis(_)));
        assert!(matches!(test.metadata[2], TestMeta::RandomSeed(_)));
        assert!(matches!(test.metadata[3], TestMeta::Skip(_)));

        assert_eq!(
            module("(test.scenario \"suite\" (test.case \"bad\" timeout-ms: 0 (do)))")
                .unwrap_err()
                .code,
            "E-SYN-008"
        );
        assert_eq!(
            module(r#"(test.scenario "suite" (test.case "bad" expect-error: (@runtime E-RUN "boom") (do) ))"#)
                .unwrap_err()
                .code,
            "E-SYN-009"
        );
        let runtime = module(
            r#"(test.scenario "suite" (test.case "runtime" expect-error: (@runtime "boom") (do) ))"#,
        )
        .unwrap();
        let TopLevel::TestScenario(scenario) = &runtime.forms[0] else {
            panic!("expected scenario");
        };
        let runtime = &scenario.cases[0];
        assert!(matches!(
            runtime.metadata[0],
            TestMeta::ExpectError(ExpectedError::Runtime { .. })
        ));
        for source in [
            r#"(test.scenario "suite" (test.case "bad" expect-error: (@runtime "") (do) ))"#,
            r#"(test.scenario "suite" (test.case "bad" expect-error: (@load E-LOAD-001 "") (do) ))"#,
            r#"
(test.scenario "suite" (test.case "bad" expect-error: (@compile E-COMPILE-001 "") (do) ))"#,
        ] {
            let error = module(source).unwrap_err();
            assert_eq!(error.code, "E-SYN-008");
            assert_eq!(
                error.message,
                "`expect-error` message must be a non-empty string"
            );
        }
    }

    #[test]
    fn lowers_expression_type_pattern_and_annotation_roles() {
        let source = r#"
(defn
  unwrap
  (input (option int64))
  int64
  doc: "unwrap" where: (t comparable) impls: ((impl display types: (int64) methods: ((method show display.show)))) (do (let-as fallback int64 0) (match input (option.some (bind value)) (do (return value)) (option.none) (do (return fallback))))



)
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
            "E-SYN-007"
        );
        assert_eq!(
            module("(defn f () void (do (break 1)))").unwrap_err().code,
            "E-SYN-009"
        );
    }

    #[test]
    fn ignores_reader_comments_in_all_grammar_positions() {
        let module =
            module("(defn ; head\n f (; params\n x int64) int64 (do ; body\n (return x)))")
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
(macro
  unless
  (condition @expr-syntax body @expr-syntax)
  @expr-syntax
  doc: "Conditional syntax." visibility: @private (do (let fallback (quote @expr-syntax unit)) (if condition (do (quote @expr-syntax (if (unquote condition) (do unit) (do (splice body))))) (do (capture caller))))


)
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
        let error = module("(macro m (x @value-syntax) @expr-syntax (do x))").unwrap_err();
        assert_eq!(error.code, "E-ATOM-002");

        let error = module("(macro m (x expr-syntax) @expr-syntax (do x))").unwrap_err();
        assert_eq!(error.code, "E-ATOM-003");

        let error = module("(macro m () @expr-syntax (do))").unwrap_err();
        assert_eq!(error.code, "E-SYN-009");

        let error = module("(macro m () @expr-syntax (do (quote @expr-syntax a b)))").unwrap_err();
        assert_eq!(error.code, "E-SYN-009");

        let error = module("(macro m () @expr-syntax (do (unquote)))").unwrap_err();
        assert_eq!(error.code, "E-SYN-009");

        let error = module("(private (test no core (do)))").unwrap_err();
        assert_eq!(error.code, "E-SYN-007");
    }

    #[test]
    fn identical_list_has_contextual_type_expression_and_pattern_meaning() {
        let parsed = module(
            "(const typed (option int64) (option int64))\n\
             (defn inspect (x int64) bool\n\
               (do (match x (option int64) (do true))))",
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
        for (source, expected) in [
            ("(defn f () void (do) doc:)", "E-SYN-013"),
            (
                "(const f any (fn () void (return unit) doc: \"late\"))",
                "E-SYN-013",
            ),
            ("(defn f () void doc: \"a\" doc: \"b\" (do))", "E-SYN-011"),
            ("(defn f () void unknown: unit (do))", "E-SYN-011"),
            (
                "(test.scenario \"suite\" (test.case \"t\" tags: (@fast) tags: (@slow) (do)))",
                "E-SYN-011",
            ),
        ] {
            let error = module(source).unwrap_err();
            assert_eq!(error.code, expected, "source: {source}");
        }
    }

    #[test]
    fn lowers_all_test_attribute_roles() {
        let parsed = module(
            "(test.scenario \"suite\" (test.case \"measured\" tags: (@fast @arithmetic) expect-error: (@compile E-OP-002 \"overflow\") clock: (@fixed 0 0) workspace: @temp (do)))",
        )
        .unwrap();
        let TopLevel::TestScenario(scenario) = &parsed.forms[0] else {
            panic!("expected scenario");
        };
        let test = &scenario.cases[0];
        assert_eq!(test.metadata.len(), 4);
        assert!(matches!(test.metadata[3], TestMeta::Workspace(_)));
    }

    #[test]
    fn document_and_ast_ids_are_stable_per_snapshot_and_document_qualified() {
        let syntax = parse("(defn main () void (do (return)))").unwrap();
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
            DocumentId::from_path(Path::new("src\\main.vib")),
            DocumentId::from_path(Path::new("src/main.vib"))
        );
        assert_ne!(
            DocumentId::from_path(Path::new("src/main.vib")),
            DocumentId::from_path(Path::new("src/other.vib"))
        );
    }
}

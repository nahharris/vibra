//! Contextual declaration, type, expression, and pattern structure for the
//! Step 8–9 source surface.
//!
//! This layer consumes the lossless reader CST without lowering declarations
//! to applications or resolving names. Raw CST slices remain alongside the
//! owned contextual views where callers still need source-preserving access.

use std::collections::BTreeSet;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};

use crate::literal::{Literal, LiteralClassification};
use crate::name::{Name, NameClassification, NameKind};
use crate::reader::{CstNode, SyntaxKind};

const RESERVED_TYPE_HEADS: &[&str] = &[
    "record", "enum", "union", "newtype", "tuple", "array", "map", "fn",
];
const RESERVED_VALUE_SPELLINGS: &[&str] = &["map", "array", "tuple"];
const RETIRED_EXPRESSION_HEADS: &[&str] = &[
    "while", "for", "break", "continue", "return", "bind", "case",
];
const MAX_CONTEXTUAL_DEPTH: usize = 256;

/// A lossless CST slice retained alongside a contextual grammar view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawNode {
    span: ByteSpan,
    source: String,
}

/// One expression parsed in a source expression position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expression {
    kind: ExpressionKind,
    span: ByteSpan,
}

impl Expression {
    /// The contextual expression variant.
    #[must_use]
    pub const fn kind(&self) -> &ExpressionKind {
        &self.kind
    }

    /// The half-open source span of the expression.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

/// Contextual expression variants implemented by Step 9.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExpressionKind {
    /// A decoded literal value.
    Literal(Literal),
    /// A symbol or atom value; resolution is later work.
    Name(Name),
    /// A nonempty executable application.
    Application(Application),
    /// An anonymous function expression.
    Lambda(LambdaExpression),
    /// A direct expression sequence.
    Do(Vec<Expression>),
    /// An immutable binding form.
    Let {
        /// The binding pattern.
        pattern: Pattern,
        /// The expression producing the bound value.
        value: Box<Expression>,
        /// Expressions evaluated after the binding.
        body: Vec<Expression>,
    },
    /// A three-operand conditional.
    If {
        /// The condition.
        condition: Box<Expression>,
        /// The true branch.
        then_branch: Box<Expression>,
        /// The false branch.
        else_branch: Box<Expression>,
    },
    /// A flat pattern/result match.
    Match {
        /// The value being matched.
        scrutinee: Box<Expression>,
        /// Pattern/result arms in written order.
        arms: Vec<MatchArm>,
    },
    /// A static expression ascription.
    As {
        /// The written expected type.
        value_type: TypeExpr,
        /// The operand expression.
        operand: Box<Expression>,
    },
    /// The single early-exit form.
    Try(Box<Expression>),
}

/// A nonempty application and its written operand groups.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Application {
    callee: Box<Expression>,
    type_arguments: Option<Vec<TypeExpr>>,
    type_arguments_span: Option<ByteSpan>,
    type_arguments_after_operands: bool,
    arguments: Vec<CallArgument>,
    span: ByteSpan,
}

impl Application {
    /// The arbitrary expression in the callee position.
    #[must_use]
    pub const fn callee(&self) -> &Expression {
        &self.callee
    }

    /// The optional reserved `types:` argument, in written type order.
    #[must_use]
    pub fn type_arguments(&self) -> Option<&[TypeExpr]> {
        self.type_arguments.as_deref()
    }

    /// The source span of the complete `types:` group, when present.
    #[must_use]
    pub const fn type_arguments_span(&self) -> Option<ByteSpan> {
        self.type_arguments_span
    }

    /// Whether the written `types:` group followed an ordinary operand.
    ///
    /// Canonical format places the group before every ordinary operand, but
    /// the reader accepts it in any unambiguous position.
    #[must_use]
    pub const fn type_arguments_after_operands(&self) -> bool {
        self.type_arguments_after_operands
    }

    /// Operands in their original written order.
    #[must_use]
    pub fn arguments(&self) -> &[CallArgument] {
        &self.arguments
    }

    /// The application span, including its delimiters.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    /// Returns operands in canonical binding order using supplied facts.
    ///
    /// The syntax reader never infers these facts from a callee spelling.
    /// Callers provide the fixed positional count, declared labelled names,
    /// and optional variadic shape from a resolver or another authoritative
    /// signature source.
    pub fn ordered_arguments<'a>(
        &'a self,
        facts: &BindingFacts,
    ) -> Result<Vec<&'a CallArgument>, BindingError> {
        let mut positional = Vec::new();
        let mut labelled = Vec::new();
        let mut variadic = Vec::new();
        let mut seen_labels = BTreeSet::new();

        for argument in &self.arguments {
            if let Some(label) = argument.label() {
                if !seen_labels.insert(label.value().to_owned()) {
                    return Err(BindingError::DuplicateLabel(label.value().to_owned()));
                }
                let Some(order) = facts
                    .labelled()
                    .iter()
                    .position(|declared| declared == label.value())
                else {
                    return Err(BindingError::UnknownLabel(label.value().to_owned()));
                };
                labelled.push((order, argument));
            } else {
                positional.push(argument);
            }
        }

        if positional.len() < facts.positional_count() {
            return Err(BindingError::MissingPositional {
                expected: facts.positional_count(),
                actual: positional.len(),
            });
        }
        let fixed = positional
            .drain(..facts.positional_count())
            .collect::<Vec<_>>();
        variadic.extend(positional);
        match facts.variadic() {
            Some(VariadicBinding::Array) => {}
            Some(VariadicBinding::Map) if !variadic.len().is_multiple_of(2) => {
                return Err(BindingError::OddMapVariadic(variadic.len()));
            }
            Some(VariadicBinding::Map) => {}
            None if !variadic.is_empty() => {
                return Err(BindingError::UnexpectedPositional(variadic.len()));
            }
            None => {}
        }

        labelled.sort_by_key(|(order, _)| *order);
        let mut ordered = fixed;
        ordered.extend(labelled.into_iter().map(|(_, argument)| argument));
        ordered.extend(variadic);
        Ok(ordered)
    }
}

/// One application operand, with an optional written label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallArgument {
    label: Option<Name>,
    value: Expression,
    span: ByteSpan,
}

impl CallArgument {
    /// The optional label, including its lexical label category.
    #[must_use]
    pub const fn label(&self) -> Option<&Name> {
        self.label.as_ref()
    }

    /// The operand expression.
    #[must_use]
    pub const fn value(&self) -> &Expression {
        &self.value
    }

    /// The operand group span, including its label when present.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

/// An anonymous function expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LambdaExpression {
    parameters: Vec<Parameter>,
    result: TypeExpr,
    attributes: FunctionAttributes,
    body: Vec<Expression>,
    span: ByteSpan,
}

impl LambdaExpression {
    /// Flat pattern/type parameter pairs.
    #[must_use]
    pub fn parameters(&self) -> &[Parameter] {
        &self.parameters
    }

    /// The result type.
    #[must_use]
    pub const fn result(&self) -> &TypeExpr {
        &self.result
    }

    /// Lambda-only attributes in source order.
    #[must_use]
    pub const fn attributes(&self) -> &FunctionAttributes {
        &self.attributes
    }

    /// Body expressions in source order.
    #[must_use]
    pub fn body(&self) -> &[Expression] {
        &self.body
    }

    /// The lambda span.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

/// One pattern/result arm in a match expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchArm {
    pattern: Pattern,
    result: Expression,
    span: ByteSpan,
}

impl MatchArm {
    /// The arm pattern.
    #[must_use]
    pub const fn pattern(&self) -> &Pattern {
        &self.pattern
    }

    /// The arm result expression.
    #[must_use]
    pub const fn result(&self) -> &Expression {
        &self.result
    }

    /// The arm span from pattern start through result end.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

/// One pattern parsed in a binding or match position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pattern {
    kind: PatternKind,
    span: ByteSpan,
}

impl Pattern {
    /// The contextual pattern variant.
    #[must_use]
    pub const fn kind(&self) -> &PatternKind {
        &self.kind
    }

    /// The half-open source span of the pattern.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

/// Contextual pattern variants implemented by Step 9.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatternKind {
    /// A local binding or discard.
    Binding(Name),
    /// A literal pattern.
    Literal(Literal),
    /// A qualified or named constructor pattern.
    Constructor {
        /// The written constructor path.
        head: Name,
        /// Positional and labelled subpatterns in written order.
        arguments: Vec<PatternArgument>,
    },
    /// A tuple pattern.
    Tuple(Vec<Pattern>),
    /// A fixed-length array pattern.
    Array(Vec<Pattern>),
    /// A union-narrowing pattern ascription.
    As {
        /// The member type selected by the pattern.
        value_type: TypeExpr,
        /// The payload pattern.
        pattern: Box<Pattern>,
    },
}

/// One constructor-pattern operand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatternArgument {
    label: Option<Name>,
    pattern: Pattern,
    span: ByteSpan,
}

impl PatternArgument {
    /// The optional field label.
    #[must_use]
    pub const fn label(&self) -> Option<&Name> {
        self.label.as_ref()
    }

    /// The nested pattern.
    #[must_use]
    pub const fn pattern(&self) -> &Pattern {
        &self.pattern
    }

    /// The argument span.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

/// The shape of a supplied variadic binding contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VariadicBinding {
    /// Remaining operands are independent array elements.
    Array,
    /// Remaining operands are alternating key/value pairs.
    Map,
}

/// Authoritative call-site binding facts supplied by a resolver or test.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindingFacts {
    positional_count: usize,
    labelled: Vec<String>,
    variadic: Option<VariadicBinding>,
}

/// A binding-facts entry associated with one application span.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplicationBinding {
    span: ByteSpan,
    facts: BindingFacts,
}

impl ApplicationBinding {
    /// Associates authoritative facts with an application span.
    #[must_use]
    pub const fn new(span: ByteSpan, facts: BindingFacts) -> Self {
        Self { span, facts }
    }

    /// The application span selected by this entry.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    /// The authoritative binding facts.
    #[must_use]
    pub const fn facts(&self) -> &BindingFacts {
        &self.facts
    }
}

impl BindingFacts {
    /// Creates binding facts without inferring anything from a callee name.
    #[must_use]
    pub fn new(
        positional_count: usize,
        labelled: Vec<String>,
        variadic: Option<VariadicBinding>,
    ) -> Self {
        Self {
            positional_count,
            labelled,
            variadic,
        }
    }

    /// Number of fixed positional operands.
    #[must_use]
    pub const fn positional_count(&self) -> usize {
        self.positional_count
    }

    /// Label names in declaration order, without `:`.
    #[must_use]
    pub fn labelled(&self) -> &[String] {
        &self.labelled
    }

    /// The optional variadic tail shape.
    #[must_use]
    pub const fn variadic(&self) -> Option<VariadicBinding> {
        self.variadic
    }
}

/// A structural failure while applying supplied binding facts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingError {
    /// A supplied label is not in the authoritative signature.
    UnknownLabel(String),
    /// A label occurs more than once.
    DuplicateLabel(String),
    /// Too few fixed positional operands were supplied.
    MissingPositional {
        /// Required fixed positional count.
        expected: usize,
        /// Supplied fixed positional count.
        actual: usize,
    },
    /// Positional operands remain without a variadic tail.
    UnexpectedPositional(usize),
    /// A map variadic tail has an odd number of operands.
    OddMapVariadic(usize),
}

impl std::fmt::Display for BindingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownLabel(label) => {
                write!(formatter, "unknown labelled operand `{label}`")
            }
            Self::DuplicateLabel(label) => {
                write!(formatter, "duplicate labelled operand `{label}`")
            }
            Self::MissingPositional { expected, actual } => {
                write!(
                    formatter,
                    "expected {expected} positional operands, got {actual}"
                )
            }
            Self::UnexpectedPositional(count) => {
                write!(formatter, "{count} unexpected positional operands")
            }
            Self::OddMapVariadic(count) => {
                write!(formatter, "map variadic tail has odd operand count {count}")
            }
        }
    }
}

impl std::error::Error for BindingError {}

impl RawNode {
    fn from_cst(node: &CstNode) -> Self {
        Self {
            span: node.span(),
            source: node.to_source(),
        }
    }

    /// The source span of the retained subtree.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    /// The exact source represented by the retained subtree.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }
}

/// One parsed source module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceAst {
    declarations: Vec<Declaration>,
    span: ByteSpan,
}

impl SourceAst {
    /// Native top-level declarations in source order.
    #[must_use]
    pub fn declarations(&self) -> &[Declaration] {
        &self.declarations
    }

    /// The source span covered by the module.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

/// The result of contextual source decoding.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceDecode {
    ast: Option<SourceAst>,
    diagnostics: Vec<Diagnostic>,
    recognized: bool,
}

impl SourceDecode {
    /// The contextual source AST for a recognized root.
    ///
    /// When diagnostics contain errors, malformed declarations are omitted but
    /// intact sibling declarations remain available. Consumers that require a
    /// complete AST must also check the document's acceptance state.
    #[must_use]
    pub const fn ast(&self) -> Option<&SourceAst> {
        self.ast.as_ref()
    }

    /// Diagnostics emitted by contextual declaration/type validation.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Whether the source contained at least one recognized top-form head.
    #[must_use]
    pub const fn recognized(&self) -> bool {
        self.recognized
    }
}

/// One of the seven native v1 top forms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Declaration {
    /// A module import alias.
    Import(ImportDeclaration),
    /// A nominal type declaration.
    Deftype(DeftypeDeclaration),
    /// An interface declaration.
    Defint(DefintDeclaration),
    /// A nominal effect root declaration.
    Deffect(DeffectDeclaration),
    /// An immutable module value.
    Def(DefDeclaration),
    /// A named function declaration.
    Defn(FunctionDeclaration),
    /// A test declaration.
    Test(TestDeclaration),
}

impl Declaration {
    /// The source span of this declaration.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        match self {
            Self::Import(value) => value.span,
            Self::Deftype(value) => value.span,
            Self::Defint(value) => value.span,
            Self::Deffect(value) => value.span,
            Self::Def(value) => value.span,
            Self::Defn(value) => value.span,
            Self::Test(value) => value.span,
        }
    }
}

/// An import declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportDeclaration {
    alias: Name,
    target: Name,
    span: ByteSpan,
}

impl ImportDeclaration {
    /// The local module alias.
    #[must_use]
    pub const fn alias(&self) -> &Name {
        &self.alias
    }

    /// The imported atom entity.
    #[must_use]
    pub const fn target(&self) -> &Name {
        &self.target
    }

    /// The source span.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

/// A nominal type declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeftypeDeclaration {
    name: Name,
    body: DeftypeBody,
    attributes: TypeAttributes,
    members: Vec<TypeMember>,
    span: ByteSpan,
}

impl DeftypeDeclaration {
    /// The unqualified declared name.
    #[must_use]
    pub const fn name(&self) -> &Name {
        &self.name
    }

    /// The context-specific nominal body.
    #[must_use]
    pub const fn body(&self) -> &DeftypeBody {
        &self.body
    }

    /// Type attributes in source order.
    #[must_use]
    pub const fn attributes(&self) -> &TypeAttributes {
        &self.attributes
    }

    /// Methods and implementations owned by the type.
    #[must_use]
    pub fn members(&self) -> &[TypeMember] {
        &self.members
    }

    /// The source span.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

/// An interface declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefintDeclaration {
    name: Name,
    attributes: TypeAttributes,
    members: Vec<TypeMember>,
    span: ByteSpan,
}

impl DefintDeclaration {
    /// The unqualified interface name.
    #[must_use]
    pub const fn name(&self) -> &Name {
        &self.name
    }

    /// Type/declaration attributes in source order.
    #[must_use]
    pub const fn attributes(&self) -> &TypeAttributes {
        &self.attributes
    }

    /// Interface methods and nested implementations.
    #[must_use]
    pub fn members(&self) -> &[TypeMember] {
        &self.members
    }

    /// The source span.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

/// A nominal effect declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeffectDeclaration {
    name: Name,
    attributes: DeclarationAttributes,
    members: Vec<FunctionDeclaration>,
    span: ByteSpan,
}

impl DeffectDeclaration {
    /// The unqualified effect root name.
    #[must_use]
    pub const fn name(&self) -> &Name {
        &self.name
    }

    /// Declaration attributes in source order.
    #[must_use]
    pub const fn attributes(&self) -> &DeclarationAttributes {
        &self.attributes
    }

    /// Effect operations in source order.
    #[must_use]
    pub fn members(&self) -> &[FunctionDeclaration] {
        &self.members
    }

    /// The source span.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

/// An immutable module value declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefDeclaration {
    name: Name,
    value_type: TypeExpr,
    value: RawNode,
    expression: Expression,
    attributes: DeclarationAttributes,
    span: ByteSpan,
}

impl DefDeclaration {
    /// The declared value name.
    #[must_use]
    pub const fn name(&self) -> &Name {
        &self.name
    }

    /// The declared type.
    #[must_use]
    pub const fn value_type(&self) -> &TypeExpr {
        &self.value_type
    }

    /// The unparsed expression value.
    #[must_use]
    pub const fn value(&self) -> &RawNode {
        &self.value
    }

    /// The parsed value expression.
    #[must_use]
    pub const fn expression(&self) -> &Expression {
        &self.expression
    }

    /// Declaration attributes in source order.
    #[must_use]
    pub const fn attributes(&self) -> &DeclarationAttributes {
        &self.attributes
    }

    /// The source span.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

/// A named function, method, or operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionDeclaration {
    name: Name,
    parameters: Vec<Parameter>,
    result: TypeExpr,
    attributes: FunctionAttributes,
    body: Vec<RawNode>,
    expressions: Vec<Expression>,
    span: ByteSpan,
}

impl FunctionDeclaration {
    /// The local function name.
    #[must_use]
    pub const fn name(&self) -> &Name {
        &self.name
    }

    /// Flat positional pattern/type pairs.
    #[must_use]
    pub fn parameters(&self) -> &[Parameter] {
        &self.parameters
    }

    /// The result type.
    #[must_use]
    pub const fn result(&self) -> &TypeExpr {
        &self.result
    }

    /// Function attributes in source order.
    #[must_use]
    pub const fn attributes(&self) -> &FunctionAttributes {
        &self.attributes
    }

    /// Raw body subtrees retained alongside the contextual expressions.
    #[must_use]
    pub fn body(&self) -> &[RawNode] {
        &self.body
    }

    /// Parsed body expressions in source order.
    #[must_use]
    pub fn expressions(&self) -> &[Expression] {
        &self.expressions
    }

    /// The source span.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

/// A test declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestDeclaration {
    name: Literal,
    effects: Option<EffectRow>,
    body: Vec<RawNode>,
    expressions: Vec<Expression>,
    span: ByteSpan,
}

impl TestDeclaration {
    /// The literal test name.
    #[must_use]
    pub const fn name(&self) -> &Literal {
        &self.name
    }

    /// An explicit effect ceiling, if written.
    #[must_use]
    pub const fn effects(&self) -> Option<&EffectRow> {
        self.effects.as_ref()
    }

    /// Raw test body subtrees retained alongside the contextual expressions.
    #[must_use]
    pub fn body(&self) -> &[RawNode] {
        &self.body
    }

    /// Parsed body expressions in source order.
    #[must_use]
    pub fn expressions(&self) -> &[Expression] {
        &self.expressions
    }

    /// The source span.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

/// A method or implementation directly owned by a type/interface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeMember {
    /// A nested method or interface member.
    Method(FunctionDeclaration),
    /// A native implementation block.
    Implementation(ImplDeclaration),
}

/// A native `impl` block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImplDeclaration {
    target: TypeExpr,
    members: Vec<FunctionDeclaration>,
    span: ByteSpan,
}

impl ImplDeclaration {
    /// The positional target type.
    #[must_use]
    pub const fn target(&self) -> &TypeExpr {
        &self.target
    }

    /// Native implementation members.
    #[must_use]
    pub fn members(&self) -> &[FunctionDeclaration] {
        &self.members
    }

    /// The source span.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

/// A flat positional parameter retaining both raw and contextual pattern views.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parameter {
    pattern: RawNode,
    parsed_pattern: Pattern,
    value_type: TypeExpr,
    span: ByteSpan,
}

impl Parameter {
    /// The retained pattern subtree.
    #[must_use]
    pub const fn pattern(&self) -> &RawNode {
        &self.pattern
    }

    /// The contextual pattern parsed from the retained source node.
    #[must_use]
    pub const fn parsed_pattern(&self) -> &Pattern {
        &self.parsed_pattern
    }

    /// The parameter type.
    #[must_use]
    pub const fn value_type(&self) -> &TypeExpr {
        &self.value_type
    }

    /// The source span.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

/// A labelled parameter and its literal default.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LabelledParameter {
    name: Name,
    value_type: TypeExpr,
    default: Literal,
    span: ByteSpan,
}

impl LabelledParameter {
    /// The unqualified parameter name.
    #[must_use]
    pub const fn name(&self) -> &Name {
        &self.name
    }

    /// The parameter type.
    #[must_use]
    pub const fn value_type(&self) -> &TypeExpr {
        &self.value_type
    }

    /// The default literal.
    #[must_use]
    pub const fn default(&self) -> &Literal {
        &self.default
    }

    /// The source span.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

/// A single variadic binding and its array/map tail type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariadicParameter {
    name: Name,
    value_type: VariadicType,
    span: ByteSpan,
}

impl VariadicParameter {
    /// The binding name or discard.
    #[must_use]
    pub const fn name(&self) -> &Name {
        &self.name
    }

    /// The variadic tail type.
    #[must_use]
    pub const fn value_type(&self) -> &VariadicType {
        &self.value_type
    }

    /// The source span.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

/// A type expression accepted in ordinary type positions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeExpr {
    /// A primitive, nominal, interface, or generic name.
    Name(Name),
    /// An applied nominal or generic type.
    Applied {
        /// The applied nominal or generic head.
        head: Name,
        /// Type arguments supplied to the head.
        arguments: Vec<TypeExpr>,
    },
    /// A tuple constructor.
    Tuple(Vec<TypeExpr>),
    /// An array constructor.
    Array(Box<TypeExpr>),
    /// A map constructor.
    Map(Box<TypeExpr>, Box<TypeExpr>),
    /// A function type.
    Function(FunctionType),
    /// The primitive `void` type.
    Void,
}

/// A function type's complete signature surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionType {
    parameters: Vec<TypeExpr>,
    result: Box<TypeExpr>,
    labelled: Vec<TypeSlot>,
    variadic: Option<VariadicType>,
    effects: Vec<Name>,
}

impl FunctionType {
    /// Required positional types.
    #[must_use]
    pub fn parameters(&self) -> &[TypeExpr] {
        &self.parameters
    }

    /// Result type.
    #[must_use]
    pub const fn result(&self) -> &TypeExpr {
        &self.result
    }

    /// Labelled type slots.
    #[must_use]
    pub fn labelled(&self) -> &[TypeSlot] {
        &self.labelled
    }

    /// Optional variadic tail type.
    #[must_use]
    pub const fn variadic(&self) -> Option<&VariadicType> {
        self.variadic.as_ref()
    }

    /// Effect references in source order.
    #[must_use]
    pub fn effects(&self) -> &[Name] {
        &self.effects
    }
}

/// A labelled type slot in a function type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeSlot {
    name: Name,
    value_type: TypeExpr,
}

impl TypeSlot {
    /// The slot's local name.
    #[must_use]
    pub const fn name(&self) -> &Name {
        &self.name
    }

    /// The slot type.
    #[must_use]
    pub const fn value_type(&self) -> &TypeExpr {
        &self.value_type
    }
}

/// An array or map variadic type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VariadicType {
    /// An array tail.
    Array(Box<TypeExpr>),
    /// A map tail.
    Map(Box<TypeExpr>, Box<TypeExpr>),
}

/// A field in a nominal record or enum body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeField {
    name: Name,
    value_type: TypeExpr,
    span: ByteSpan,
}

impl TypeField {
    /// The unqualified field or variant name.
    #[must_use]
    pub const fn name(&self) -> &Name {
        &self.name
    }

    /// The field or variant payload type.
    #[must_use]
    pub const fn ty(&self) -> &TypeExpr {
        &self.value_type
    }

    /// The source span.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

/// The body forms allowed only after a `deftype` header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeftypeBody {
    /// A normal type expression.
    Type(TypeExpr),
    /// A flat record body.
    Record(Vec<TypeField>),
    /// A flat enum body.
    Enum(Vec<TypeField>),
    /// A union with at least two member types.
    Union(Vec<TypeExpr>),
    /// A one-representation newtype.
    Newtype(Box<TypeExpr>),
}

/// A generic name/bound pair.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenericBinding {
    name: Name,
    bound: Name,
    span: ByteSpan,
}

impl GenericBinding {
    /// The generic name.
    #[must_use]
    pub const fn name(&self) -> &Name {
        &self.name
    }

    /// The interface bound name.
    #[must_use]
    pub const fn bound(&self) -> &Name {
        &self.bound
    }

    /// The source span.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

/// A source effect row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectRow {
    references: Vec<Name>,
    span: ByteSpan,
}

impl EffectRow {
    /// Effect symbols in source order.
    #[must_use]
    pub fn references(&self) -> &[Name] {
        &self.references
    }

    /// The source span.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

/// One parsed declaration attribute.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Attribute {
    /// A generic name/bound clause.
    Where(Vec<GenericBinding>),
    /// Labelled parameters and literal defaults.
    Labelled(Vec<LabelledParameter>),
    /// One variadic parameter.
    Variadic(VariadicParameter),
    /// Visibility atom.
    Visibility(Name),
    /// An effect ceiling.
    Effects(EffectRow),
    /// An external provider atom.
    External(Name),
    /// An external registry symbol literal.
    Symbol(Literal),
    /// Documentation text.
    Doc(Literal),
}

impl Attribute {
    /// The canonical order used when declaration attributes are formatted.
    #[must_use]
    pub const fn canonical_order(&self) -> usize {
        match self {
            Self::Where(_) => 0,
            Self::Labelled(_) => 1,
            Self::Variadic(_) => 2,
            Self::Visibility(_) => 3,
            Self::Effects(_) => 4,
            Self::External(_) => 5,
            Self::Symbol(_) => 6,
            Self::Doc(_) => 7,
        }
    }

    /// The canonical label without its trailing colon.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Where(_) => "where",
            Self::Labelled(_) => "labelled",
            Self::Variadic(_) => "variadic",
            Self::Visibility(_) => "visibility",
            Self::Effects(_) => "effects",
            Self::External(_) => "external",
            Self::Symbol(_) => "symbol",
            Self::Doc(_) => "doc",
        }
    }
}

/// Attributes available on a type declaration.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TypeAttributes {
    items: Vec<Attribute>,
}

impl TypeAttributes {
    /// Attributes in source order.
    #[must_use]
    pub fn items(&self) -> &[Attribute] {
        &self.items
    }
}

/// Attributes available on a declaration.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeclarationAttributes {
    items: Vec<Attribute>,
}

impl DeclarationAttributes {
    /// Attributes in source order.
    #[must_use]
    pub fn items(&self) -> &[Attribute] {
        &self.items
    }
}

/// Attributes available on a function or method.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FunctionAttributes {
    items: Vec<Attribute>,
}

impl FunctionAttributes {
    /// Attributes in source order.
    #[must_use]
    pub fn items(&self) -> &[Attribute] {
        &self.items
    }
}

/// Decode the source root into native declarations and ordinary types.
#[must_use]
pub fn decode_source_root(root: &CstNode) -> SourceDecode {
    let forms = meaningful_children(root);
    let mut parser = AstParser {
        diagnostics: Vec::new(),
        recognized: false,
        context_depth: 0,
        context_depth_exceeded: false,
    };
    let declarations = forms
        .iter()
        .filter_map(|form| parser.parse_top_form(form))
        .collect::<Vec<_>>();
    let ast = parser.recognized.then(|| SourceAst {
        declarations,
        span: root.span(),
    });
    SourceDecode {
        ast,
        diagnostics: parser.diagnostics,
        recognized: parser.recognized,
    }
}

/// Whether a root contains a form whose head belongs to the declaration
/// grammar. The reader uses this to keep syntax-only compatibility for
/// arbitrary expression fixtures while a contextual AST is opt-in for those
/// documents.
#[must_use]
pub fn contains_declaration_head(root: &CstNode) -> bool {
    meaningful_children(root).iter().any(|form| {
        head_text(form).is_some_and(|head| {
            matches!(
                head,
                "import"
                    | "deftype"
                    | "defint"
                    | "deffect"
                    | "def"
                    | "defn"
                    | "test"
                    | "impl"
            )
        })
    })
}

struct AstParser {
    diagnostics: Vec<Diagnostic>,
    recognized: bool,
    context_depth: usize,
    context_depth_exceeded: bool,
}

#[derive(Clone, Copy)]
enum FunctionOwner {
    Module,
    Nested,
    Interface,
    Effect,
    Implementation,
}

struct ParsedAttributes {
    items: Vec<Attribute>,
    next: usize,
}

#[derive(Clone, Copy)]
enum AttributeContext {
    Type,
    Declaration,
    Function,
    Lambda,
}

// Arity checks immediately precede the indexed accesses in this parser. The
// structural indexing mirrors the grammar and malformed input exits through
// those checks first.
#[allow(clippy::indexing_slicing)]
impl AstParser {
    fn error(&mut self, code: DiagnosticCode, span: ByteSpan, message: &'static str) {
        self.diagnostics.push(Diagnostic::new(code, span, message));
    }

    fn invalid_form(&mut self, node: &CstNode, message: &'static str) {
        self.error(DiagnosticCode::SyntaxInvalidForm, node.span(), message);
    }

    fn enter_context(&mut self, node: &CstNode) -> bool {
        if self.context_depth >= MAX_CONTEXTUAL_DEPTH {
            if !self.context_depth_exceeded {
                self.invalid_form(
                    node,
                    "contextual AST nesting exceeds the safe depth",
                );
                self.context_depth_exceeded = true;
            }
            return false;
        }
        self.context_depth += 1;
        true
    }

    fn leave_context(&mut self) {
        self.context_depth = self.context_depth.saturating_sub(1);
    }

    fn parse_top_form(&mut self, node: &CstNode) -> Option<Declaration> {
        let Some(head) = head_text(node) else {
            self.invalid_form(node, "a top-level declaration must be a named list");
            return None;
        };
        if matches!(
            head,
            "import"
                | "deftype"
                | "defint"
                | "deffect"
                | "def"
                | "defn"
                | "test"
                | "impl"
        ) {
            self.recognized = true;
        }
        match head {
            "import" => self.parse_import(node),
            "deftype" => self.parse_deftype(node),
            "defint" => self.parse_defint(node),
            "deffect" => self.parse_deffect(node),
            "def" => self.parse_def(node),
            "defn" => self.parse_function(node, FunctionOwner::Module, &[]),
            "test" => self.parse_test(node),
            _ => {
                self.invalid_form(node, "unknown source top form");
                None
            }
        }
    }

    fn parse_import(&mut self, node: &CstNode) -> Option<Declaration> {
        let forms = meaningful_children(node);
        if forms.len() != 3 {
            self.invalid_form(node, "import requires an alias and an atom target");
            return None;
        }
        let alias = self.local_name(forms[1], "an import alias must be one symbol")?;
        let target = self.atom_name(forms[2], "an import target must be an atom")?;
        Some(Declaration::Import(ImportDeclaration {
            alias,
            target,
            span: node.span(),
        }))
    }

    fn parse_deftype(&mut self, node: &CstNode) -> Option<Declaration> {
        let forms = meaningful_children(node);
        if forms.len() < 3 {
            self.invalid_form(node, "deftype requires a name and body");
            return None;
        }
        let name = self.declaration_name(forms[1])?;
        let body = self.parse_deftype_body(forms[2])?;
        let parsed = self.parse_attributes(&forms[3..], AttributeContext::Type, &[]);
        let attributes = TypeAttributes {
            items: parsed.items,
        };
        let mut members = Vec::new();
        let mut member_names = deftype_member_names(&body);
        for member in &forms[3 + parsed.next..] {
            match head_text(member) {
                Some("defn") => {
                    if let Some(Declaration::Defn(method)) = self.parse_function(
                        member,
                        FunctionOwner::Nested,
                        &generic_names(&attributes.items),
                    ) {
                        if !member_names.insert(method.name.value().to_owned()) {
                            self.error(
                                DiagnosticCode::NameMemberCollision,
                                method.span,
                                "a type member repeats a field or method name",
                            );
                        }
                        members.push(TypeMember::Method(method));
                    }
                }
                Some("impl") => {
                    if let Some(implementation) =
                        self.parse_impl(member, &generic_names(&attributes.items))
                    {
                        members.push(TypeMember::Implementation(implementation));
                    }
                }
                _ => self.invalid_form(
                    member,
                    "deftype members must be methods or impl blocks",
                ),
            }
        }
        Some(Declaration::Deftype(DeftypeDeclaration {
            name,
            body,
            attributes,
            members,
            span: node.span(),
        }))
    }

    fn parse_defint(&mut self, node: &CstNode) -> Option<Declaration> {
        let forms = meaningful_children(node);
        if forms.len() < 2 {
            self.invalid_form(node, "defint requires a name");
            return None;
        }
        let name = self.declaration_name(forms[1])?;
        let parsed = self.parse_attributes(&forms[2..], AttributeContext::Type, &[]);
        let attributes = TypeAttributes {
            items: parsed.items,
        };
        let mut members = Vec::new();
        let mut member_names = BTreeSet::new();
        for member in &forms[2 + parsed.next..] {
            match head_text(member) {
                Some("defn") => {
                    if let Some(Declaration::Defn(method)) = self.parse_function(
                        member,
                        FunctionOwner::Interface,
                        &generic_names(&attributes.items),
                    ) {
                        if !member_names.insert(method.name.value().to_owned()) {
                            self.error(
                                DiagnosticCode::NameMemberCollision,
                                method.span,
                                "an interface repeats a method name",
                            );
                        }
                        members.push(TypeMember::Method(method));
                    }
                }
                Some("impl") => {
                    if let Some(implementation) =
                        self.parse_impl(member, &generic_names(&attributes.items))
                    {
                        members.push(TypeMember::Implementation(implementation));
                    }
                }
                _ => self.invalid_form(
                    member,
                    "defint members must be methods or impl blocks",
                ),
            }
        }
        Some(Declaration::Defint(DefintDeclaration {
            name,
            attributes,
            members,
            span: node.span(),
        }))
    }

    fn parse_deffect(&mut self, node: &CstNode) -> Option<Declaration> {
        let forms = meaningful_children(node);
        if forms.len() < 2 {
            self.invalid_form(node, "deffect requires a name");
            return None;
        }
        let name = self.declaration_name(forms[1])?;
        let parsed =
            self.parse_attributes(&forms[2..], AttributeContext::Declaration, &[]);
        let attributes = DeclarationAttributes {
            items: parsed.items,
        };
        let mut members = Vec::new();
        let mut member_names = BTreeSet::new();
        for member in &forms[2 + parsed.next..] {
            if head_text(member) != Some("defn") {
                self.invalid_form(member, "deffect members must be defn forms");
                continue;
            }
            if let Some(Declaration::Defn(operation)) =
                self.parse_function(member, FunctionOwner::Effect, &[])
            {
                if !member_names.insert(operation.name.value().to_owned()) {
                    self.error(
                        DiagnosticCode::NameMemberCollision,
                        operation.span,
                        "an effect repeats an operation name",
                    );
                }
                members.push(operation);
            }
        }
        Some(Declaration::Deffect(DeffectDeclaration {
            name,
            attributes,
            members,
            span: node.span(),
        }))
    }

    fn parse_def(&mut self, node: &CstNode) -> Option<Declaration> {
        let forms = meaningful_children(node);
        if forms.len() < 4 {
            self.invalid_form(node, "def requires a name, type, and value");
            return None;
        }
        let name = self.value_declaration_name(forms[1])?;
        let value_type = self.parse_type_expr(forms[2])?;
        let expression = self.parse_expression(forms[3])?;
        let parsed =
            self.parse_attributes(&forms[4..], AttributeContext::Declaration, &[]);
        if forms.len() > 4 + parsed.next {
            self.invalid_form(
                forms[4 + parsed.next],
                "def attributes must follow its value",
            );
        }
        Some(Declaration::Def(DefDeclaration {
            name,
            value_type,
            value: RawNode::from_cst(forms[3]),
            expression,
            attributes: DeclarationAttributes {
                items: parsed.items,
            },
            span: node.span(),
        }))
    }

    fn parse_test(&mut self, node: &CstNode) -> Option<Declaration> {
        let forms = meaningful_children(node);
        if forms.len() < 3 {
            self.invalid_form(node, "test requires a string name and body");
            return None;
        }
        let name = self.literal(forms[1], "test names must be literals")?;
        if !matches!(name, Literal::String(_)) {
            self.invalid_form(forms[1], "test names must be strings");
            return None;
        }
        let mut index = 2;
        let effects = if is_label(forms[index], "effects") {
            let Some(value) = forms.get(index + 1) else {
                self.invalid_form(forms[index], "effects requires a row");
                return None;
            };
            index += 2;
            Some(self.parse_effect_row(value)?)
        } else {
            None
        };
        if index == forms.len() {
            self.invalid_form(node, "test requires at least one body expression");
            return None;
        }
        if let Some(attribute) = forms[index..]
            .iter()
            .find(|form| is_declaration_attribute_label(form))
        {
            self.error(
                DiagnosticCode::SyntaxInvalidAttribute,
                attribute.span(),
                "test attributes must precede the body",
            );
        }
        let expressions = forms[index..]
            .iter()
            .map(|form| {
                if is_declaration_attribute_label(form) {
                    None
                } else {
                    self.parse_expression(form)
                }
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Declaration::Test(TestDeclaration {
            name,
            effects,
            body: forms[index..]
                .iter()
                .map(|form| RawNode::from_cst(form))
                .collect(),
            expressions,
            span: node.span(),
        }))
    }

    fn parse_function(
        &mut self,
        node: &CstNode,
        owner: FunctionOwner,
        inherited_generics: &[String],
    ) -> Option<Declaration> {
        let forms = meaningful_children(node);
        if forms.len() < 4 {
            self.invalid_form(
                node,
                "defn requires a name, parameters, and result type",
            );
            return None;
        }
        let name =
            self.local_name(forms[1], "defn names must be unqualified symbols")?;
        if matches!(owner, FunctionOwner::Module)
            && RESERVED_VALUE_SPELLINGS.contains(&name.value())
        {
            self.error(
                DiagnosticCode::NameReservedValueSpelling,
                forms[1].span(),
                "a module-level value uses a reserved collection spelling",
            );
        }
        let parameters = self.parse_parameters(forms[2])?;
        let result = self.parse_type_expr(forms[3])?;
        let context = AttributeContext::Function;
        let parsed = self.parse_attributes(&forms[4..], context, inherited_generics);
        let body_start = 4 + parsed.next;
        let trailing_attribute = forms[body_start..]
            .iter()
            .find(|form| is_declaration_attribute_label(form));
        if let Some(attribute) = trailing_attribute {
            self.error(
                DiagnosticCode::SyntaxInvalidAttribute,
                attribute.span(),
                "function attributes must precede the body",
            );
        }
        let body = forms[body_start..]
            .iter()
            .map(|form| RawNode::from_cst(form))
            .collect::<Vec<_>>();
        let expressions = forms[body_start..]
            .iter()
            .map(|form| {
                if is_declaration_attribute_label(form) {
                    None
                } else {
                    self.parse_expression(form)
                }
            })
            .collect::<Option<Vec<_>>>()?;
        let has_external = parsed
            .items
            .iter()
            .any(|item| matches!(item, Attribute::External(_)));
        let has_symbol = parsed
            .items
            .iter()
            .any(|item| matches!(item, Attribute::Symbol(_)));
        if has_external != has_symbol {
            self.error(
                DiagnosticCode::SyntaxInvalidAttribute,
                node.span(),
                "external and symbol attributes must occur together",
            );
        }
        if has_external && let Some(first_body) = body.first() {
            self.error(
                DiagnosticCode::SyntaxInvalidForm,
                first_body.span(),
                "an external declaration cannot have a body",
            );
        }
        if matches!(owner, FunctionOwner::Interface) && has_external {
            self.error(
                DiagnosticCode::SyntaxInvalidAttribute,
                node.span(),
                "an interface member cannot be external",
            );
        }
        if matches!(owner, FunctionOwner::Effect) && has_external {
            let provider_is_host = parsed.items.iter().any(|item| {
                matches!(item, Attribute::External(name) if name.value() == "host")
            });
            if !provider_is_host {
                self.error(
                    DiagnosticCode::SyntaxInvalidAttribute,
                    node.span(),
                    "an effect member external must use the host provider",
                );
            }
        }
        if has_external {
            let provider = parsed
                .items
                .iter()
                .find(|item| matches!(item, Attribute::External(_)));
            let provider_name = provider.and_then(|item| match item {
                Attribute::External(name) => Some(name.value()),
                _ => None,
            });
            if provider_name == Some("host") && !matches!(owner, FunctionOwner::Effect)
            {
                self.error(
                    DiagnosticCode::SyntaxInvalidAttribute,
                    node.span(),
                    "the host provider is restricted to effect members",
                );
            }
            if matches!(provider_name, Some("host" | "compiler")) {
                let has_additive_effects = parsed.items.iter().any(|item| {
                    matches!(item, Attribute::Effects(row) if !row.references.is_empty())
                });
                if has_additive_effects {
                    self.error(
                        DiagnosticCode::SyntaxInvalidAttribute,
                        node.span(),
                        "external declarations cannot add effects",
                    );
                }
            }
        }
        Some(Declaration::Defn(FunctionDeclaration {
            name,
            parameters,
            result,
            attributes: FunctionAttributes {
                items: parsed.items,
            },
            body,
            expressions,
            span: node.span(),
        }))
    }

    fn parse_impl(
        &mut self,
        node: &CstNode,
        inherited_generics: &[String],
    ) -> Option<ImplDeclaration> {
        let forms = meaningful_children(node);
        if forms.len() < 3 {
            self.invalid_form(node, "impl requires a target and at least one member");
            return None;
        }
        let target = self.parse_type_expr(forms[1])?;
        let mut members = Vec::new();
        for member in &forms[2..] {
            if head_text(member) != Some("defn") {
                self.invalid_form(member, "impl members must be defn forms");
                continue;
            }
            if let Some(Declaration::Defn(method)) = self.parse_function(
                member,
                FunctionOwner::Implementation,
                inherited_generics,
            ) {
                members.push(method);
            }
        }
        Some(ImplDeclaration {
            target,
            members,
            span: node.span(),
        })
    }

    fn parse_parameters(&mut self, node: &CstNode) -> Option<Vec<Parameter>> {
        let Some(forms) = list_items(node) else {
            self.invalid_form(node, "parameters must be a list of pattern/type pairs");
            return None;
        };
        if !forms.len().is_multiple_of(2) {
            self.invalid_form(node, "parameters require flat pattern/type pairs");
            return None;
        }
        let mut parameters = Vec::with_capacity(forms.len() / 2);
        for pair in forms.chunks_exact(2) {
            let parsed_pattern = self.parse_pattern(pair[0])?;
            let value_type = self.parse_type_expr(pair[1])?;
            parameters.push(Parameter {
                pattern: RawNode::from_cst(pair[0]),
                parsed_pattern,
                value_type,
                span: ByteSpan::new(pair[0].span().start(), pair[1].span().end()),
            });
        }
        Some(parameters)
    }

    fn parse_expression(&mut self, node: &CstNode) -> Option<Expression> {
        if !self.enter_context(node) {
            return None;
        }
        let result = self.parse_expression_inner(node);
        self.leave_context();
        result
    }

    fn parse_expression_inner(&mut self, node: &CstNode) -> Option<Expression> {
        if node.kind() != SyntaxKind::List {
            if let Some(classification) = node.literal() {
                match classification {
                    LiteralClassification::Literal(literal) => {
                        return Some(Expression {
                            kind: ExpressionKind::Literal(literal),
                            span: node.span(),
                        });
                    }
                    LiteralClassification::Invalid(_) => return None,
                    LiteralClassification::Opaque => {}
                }
            }
            return match node.name() {
                Some(NameClassification::Name(name))
                    if matches!(name.kind(), NameKind::Symbol | NameKind::Atom) =>
                {
                    Some(Expression {
                        kind: ExpressionKind::Name(name.clone()),
                        span: node.span(),
                    })
                }
                Some(NameClassification::Name(_)) => {
                    self.invalid_form(node, "labels and discards are not expressions");
                    None
                }
                Some(NameClassification::Invalid) => None,
                None => {
                    self.invalid_form(node, "expression must be a literal or name");
                    None
                }
            };
        }

        let forms = meaningful_children(node);
        let Some(head_node) = forms.first() else {
            self.invalid_form(node, "an executable application cannot be empty");
            return None;
        };
        match head_node.leaf_text() {
            Some("lambda") => self.parse_lambda(node, &forms),
            Some("do") => self.parse_do(node, &forms),
            Some("let") => self.parse_let(node, &forms),
            Some("if") => self.parse_if(node, &forms),
            Some("match") => self.parse_match(node, &forms),
            Some("as") => self.parse_expression_as(node, &forms),
            Some("try") => self.parse_try(node, &forms),
            Some(head) if RETIRED_EXPRESSION_HEADS.contains(&head) => {
                self.error(
                    DiagnosticCode::SyntaxRetiredForm,
                    head_node.span(),
                    "this expression form was retired from v1",
                );
                None
            }
            Some(
                "tuple" | "array" | "map" | "record" | "enum" | "union" | "newtype"
                | "fn",
            ) => {
                self.invalid_form(
                    node,
                    "a reserved type or pattern head is not an expression",
                );
                None
            }
            _ => self.parse_application(node, &forms),
        }
    }

    fn parse_application(
        &mut self,
        node: &CstNode,
        forms: &[&CstNode],
    ) -> Option<Expression> {
        let callee = self.parse_expression(forms[0])?;
        let mut arguments = Vec::new();
        let mut type_arguments = None;
        let mut type_arguments_span = None;
        let mut type_arguments_after_operands = false;
        let mut index = 1;
        while index < forms.len() {
            if let Some(label) = self.label_name(forms[index]) {
                let Some(value) = forms.get(index + 1) else {
                    self.invalid_form(
                        forms[index],
                        "an application label requires an operand",
                    );
                    return None;
                };
                if label.value() == "types" {
                    if type_arguments.is_some() {
                        self.error(
                            DiagnosticCode::SyntaxDuplicateAttribute,
                            forms[index].span(),
                            "an application repeats its types argument",
                        );
                        return None;
                    }
                    type_arguments_after_operands = !arguments.is_empty();
                    type_arguments = Some(self.parse_type_argument_list(value)?);
                    type_arguments_span = Some(ByteSpan::new(
                        forms[index].span().start(),
                        value.span().end(),
                    ));
                } else {
                    let expression = self.parse_expression(value)?;
                    arguments.push(CallArgument {
                        label: Some(label),
                        value: expression,
                        span: ByteSpan::new(
                            forms[index].span().start(),
                            value.span().end(),
                        ),
                    });
                }
                index += 2;
            } else {
                let expression = self.parse_expression(forms[index])?;
                arguments.push(CallArgument {
                    label: None,
                    span: expression.span(),
                    value: expression,
                });
                index += 1;
            }
        }
        Some(Expression {
            kind: ExpressionKind::Application(Application {
                callee: Box::new(callee),
                type_arguments,
                type_arguments_span,
                type_arguments_after_operands,
                arguments,
                span: node.span(),
            }),
            span: node.span(),
        })
    }

    fn parse_type_argument_list(&mut self, node: &CstNode) -> Option<Vec<TypeExpr>> {
        let Some(forms) = list_items(node) else {
            self.invalid_form(node, "types requires a list of type arguments");
            return None;
        };
        forms
            .iter()
            .map(|form| self.parse_type_expr(form))
            .collect()
    }

    fn parse_lambda(
        &mut self,
        node: &CstNode,
        forms: &[&CstNode],
    ) -> Option<Expression> {
        if forms.len() < 3 {
            self.invalid_form(node, "lambda requires parameters and a result type");
            return None;
        }
        let parameters = self.parse_parameters(forms[1])?;
        let result = self.parse_type_expr(forms[2])?;
        let parsed = self.parse_attributes(&forms[3..], AttributeContext::Lambda, &[]);
        let body_start = 3 + parsed.next;
        if let Some(attribute) = forms[body_start..]
            .iter()
            .find(|form| is_lambda_attribute_label(form))
        {
            self.error(
                DiagnosticCode::SyntaxInvalidAttribute,
                attribute.span(),
                "lambda attributes must precede the body",
            );
        }
        let body = forms[body_start..]
            .iter()
            .map(|form| {
                if is_lambda_attribute_label(form) {
                    None
                } else {
                    self.parse_expression(form)
                }
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Expression {
            kind: ExpressionKind::Lambda(LambdaExpression {
                parameters,
                result,
                attributes: FunctionAttributes {
                    items: parsed.items,
                },
                body,
                span: node.span(),
            }),
            span: node.span(),
        })
    }

    fn parse_do(&mut self, node: &CstNode, forms: &[&CstNode]) -> Option<Expression> {
        Some(Expression {
            kind: ExpressionKind::Do(
                forms[1..]
                    .iter()
                    .map(|form| self.parse_expression(form))
                    .collect::<Option<Vec<_>>>()?,
            ),
            span: node.span(),
        })
    }

    fn parse_let(&mut self, node: &CstNode, forms: &[&CstNode]) -> Option<Expression> {
        if forms.len() < 3 {
            self.invalid_form(node, "let requires a pattern, value, and body");
            return None;
        }
        let pattern = self.parse_pattern(forms[1])?;
        let value = self.parse_expression(forms[2])?;
        let body = forms[3..]
            .iter()
            .map(|form| self.parse_expression(form))
            .collect::<Option<Vec<_>>>()?;
        Some(Expression {
            kind: ExpressionKind::Let {
                pattern,
                value: Box::new(value),
                body,
            },
            span: node.span(),
        })
    }

    fn parse_if(&mut self, node: &CstNode, forms: &[&CstNode]) -> Option<Expression> {
        if forms.len() != 4 {
            self.invalid_form(
                node,
                "if requires condition, then, and else expressions",
            );
            return None;
        }
        let condition = self.parse_expression(forms[1])?;
        let then_branch = self.parse_expression(forms[2])?;
        let else_branch = self.parse_expression(forms[3])?;
        Some(Expression {
            kind: ExpressionKind::If {
                condition: Box::new(condition),
                then_branch: Box::new(then_branch),
                else_branch: Box::new(else_branch),
            },
            span: node.span(),
        })
    }

    fn parse_match(
        &mut self,
        node: &CstNode,
        forms: &[&CstNode],
    ) -> Option<Expression> {
        if forms.len() < 4 || !(forms.len() - 2).is_multiple_of(2) {
            self.invalid_form(
                node,
                "match requires a scrutinee and pattern/result pairs",
            );
            return None;
        }
        let scrutinee = self.parse_expression(forms[1])?;
        let mut arms = Vec::with_capacity((forms.len() - 2) / 2);
        for pair in forms[2..].chunks_exact(2) {
            let pattern = self.parse_pattern(pair[0])?;
            let result = self.parse_expression(pair[1])?;
            arms.push(MatchArm {
                pattern,
                result,
                span: ByteSpan::new(pair[0].span().start(), pair[1].span().end()),
            });
        }
        Some(Expression {
            kind: ExpressionKind::Match {
                scrutinee: Box::new(scrutinee),
                arms,
            },
            span: node.span(),
        })
    }

    fn parse_expression_as(
        &mut self,
        node: &CstNode,
        forms: &[&CstNode],
    ) -> Option<Expression> {
        if forms.len() != 3 {
            self.invalid_form(node, "as requires one type and one expression");
            return None;
        }
        let value_type = self.parse_type_expr(forms[1])?;
        let operand = self.parse_expression(forms[2])?;
        Some(Expression {
            kind: ExpressionKind::As {
                value_type,
                operand: Box::new(operand),
            },
            span: node.span(),
        })
    }

    fn parse_try(&mut self, node: &CstNode, forms: &[&CstNode]) -> Option<Expression> {
        if forms.len() != 2 {
            self.invalid_form(node, "try requires exactly one expression");
            return None;
        }
        Some(Expression {
            kind: ExpressionKind::Try(Box::new(self.parse_expression(forms[1])?)),
            span: node.span(),
        })
    }

    fn parse_pattern(&mut self, node: &CstNode) -> Option<Pattern> {
        if !self.enter_context(node) {
            return None;
        }
        let result = self.parse_pattern_inner(node);
        self.leave_context();
        result
    }

    fn parse_pattern_inner(&mut self, node: &CstNode) -> Option<Pattern> {
        if node.kind() != SyntaxKind::List {
            if let Some(classification) = node.literal() {
                match classification {
                    LiteralClassification::Literal(literal) => {
                        return Some(Pattern {
                            kind: PatternKind::Literal(literal),
                            span: node.span(),
                        });
                    }
                    LiteralClassification::Invalid(_) => return None,
                    LiteralClassification::Opaque => {}
                }
            }
            return match node.name() {
                Some(NameClassification::Name(name)) => match name.kind() {
                    NameKind::Symbol if name.segments().len() == 1 => Some(Pattern {
                        kind: PatternKind::Binding(name.clone()),
                        span: node.span(),
                    }),
                    NameKind::Symbol => Some(Pattern {
                        kind: PatternKind::Constructor {
                            head: name.clone(),
                            arguments: Vec::new(),
                        },
                        span: node.span(),
                    }),
                    NameKind::Atom => {
                        self.invalid_form(node, "atoms are not patterns");
                        None
                    }
                    NameKind::Discard => Some(Pattern {
                        kind: PatternKind::Binding(name.clone()),
                        span: node.span(),
                    }),
                    NameKind::Label => {
                        self.invalid_form(
                            node,
                            "a pattern label requires a constructor value",
                        );
                        None
                    }
                },
                Some(NameClassification::Invalid) => None,
                None => {
                    self.invalid_form(
                        node,
                        "pattern must be a literal, binding, or list",
                    );
                    None
                }
            };
        }

        let forms = meaningful_children(node);
        let Some(head) = forms.first() else {
            self.invalid_form(node, "a pattern list cannot be empty");
            return None;
        };
        match head.leaf_text() {
            Some("tuple") => Some(Pattern {
                kind: PatternKind::Tuple(
                    forms[1..]
                        .iter()
                        .map(|form| self.parse_pattern(form))
                        .collect::<Option<Vec<_>>>()?,
                ),
                span: node.span(),
            }),
            Some("array") => Some(Pattern {
                kind: PatternKind::Array(
                    forms[1..]
                        .iter()
                        .map(|form| self.parse_pattern(form))
                        .collect::<Option<Vec<_>>>()?,
                ),
                span: node.span(),
            }),
            Some("as") => {
                if forms.len() != 3 {
                    self.invalid_form(
                        node,
                        "as pattern requires one type and one pattern",
                    );
                    return None;
                }
                let value_type = self.parse_type_expr(forms[1])?;
                let pattern = self.parse_pattern(forms[2])?;
                Some(Pattern {
                    kind: PatternKind::As {
                        value_type,
                        pattern: Box::new(pattern),
                    },
                    span: node.span(),
                })
            }
            Some(head) if RETIRED_EXPRESSION_HEADS.contains(&head) => {
                self.error(
                    DiagnosticCode::SyntaxRetiredForm,
                    forms[0].span(),
                    "this pattern form was retired from v1",
                );
                None
            }
            _ => {
                let head = self.type_name(head)?;
                let arguments = self.parse_pattern_arguments(&forms[1..])?;
                Some(Pattern {
                    kind: PatternKind::Constructor { head, arguments },
                    span: node.span(),
                })
            }
        }
    }

    fn parse_pattern_arguments(
        &mut self,
        forms: &[&CstNode],
    ) -> Option<Vec<PatternArgument>> {
        let mut arguments = Vec::new();
        let mut index = 0;
        while index < forms.len() {
            if let Some(label) = self.label_name(forms[index]) {
                let Some(value) = forms.get(index + 1) else {
                    self.invalid_form(
                        forms[index],
                        "a pattern label requires a pattern",
                    );
                    return None;
                };
                let pattern = self.parse_pattern(value)?;
                arguments.push(PatternArgument {
                    label: Some(label),
                    span: ByteSpan::new(
                        forms[index].span().start(),
                        value.span().end(),
                    ),
                    pattern,
                });
                index += 2;
            } else {
                let pattern = self.parse_pattern(forms[index])?;
                arguments.push(PatternArgument {
                    label: None,
                    span: pattern.span(),
                    pattern,
                });
                index += 1;
            }
        }
        Some(arguments)
    }

    fn label_name(&self, node: &CstNode) -> Option<Name> {
        match node.name() {
            Some(NameClassification::Name(name)) if name.kind() == NameKind::Label => {
                Some(name.clone())
            }
            _ => None,
        }
    }

    fn parse_deftype_body(&mut self, node: &CstNode) -> Option<DeftypeBody> {
        if node.kind() != SyntaxKind::List {
            return self.parse_type_expr(node).map(DeftypeBody::Type);
        }
        let forms = meaningful_children(node);
        let Some(head) = forms.first().and_then(|form| form.leaf_text()) else {
            self.invalid_form(node, "deftype body requires a type form");
            return None;
        };
        match head {
            "record" => self
                .parse_flat_fields(&forms, true)
                .map(DeftypeBody::Record),
            "enum" => self.parse_flat_fields(&forms, false).map(DeftypeBody::Enum),
            "union" => {
                let members = forms[1..]
                    .iter()
                    .filter_map(|form| self.parse_type_expr(form))
                    .collect::<Vec<_>>();
                if members.len() < 2 || forms.len() < 3 {
                    self.error(
                        DiagnosticCode::TypeUnionTooFewMembers,
                        node.span(),
                        "a union requires at least two member types",
                    );
                    None
                } else {
                    Some(DeftypeBody::Union(members))
                }
            }
            "newtype" => {
                if forms.len() != 2 {
                    self.invalid_form(
                        node,
                        "newtype requires exactly one representation type",
                    );
                    None
                } else {
                    self.parse_type_expr(forms[1])
                        .map(|value| DeftypeBody::Newtype(Box::new(value)))
                }
            }
            _ => self.parse_type_expr(node).map(DeftypeBody::Type),
        }
    }

    fn parse_flat_fields(
        &mut self,
        forms: &[&CstNode],
        record: bool,
    ) -> Option<Vec<TypeField>> {
        if forms.len() < 3 || !(forms.len() - 1).is_multiple_of(2) {
            if let Some(first) = forms.first().copied() {
                self.invalid_form(
                    first,
                    if record {
                        "record bodies require one or more name/type pairs"
                    } else {
                        "enum bodies require one or more name/type pairs"
                    },
                );
            }
            return None;
        }
        let mut fields = Vec::with_capacity((forms.len() - 1) / 2);
        let mut seen = BTreeSet::new();
        for pair in forms[1..].chunks_exact(2) {
            let name =
                self.local_name(pair[0], "type body members must be local names")?;
            if !seen.insert(name.value().to_owned()) {
                self.error(
                    DiagnosticCode::NameMemberCollision,
                    pair[0].span(),
                    "a type body repeats a member name",
                );
                return None;
            }
            let value_type = self.parse_type_expr(pair[1])?;
            fields.push(TypeField {
                name,
                value_type,
                span: ByteSpan::new(pair[0].span().start(), pair[1].span().end()),
            });
        }
        Some(fields)
    }

    fn parse_type_expr(&mut self, node: &CstNode) -> Option<TypeExpr> {
        if !self.enter_context(node) {
            return None;
        }
        let result = self.parse_type_expr_inner(node);
        self.leave_context();
        result
    }

    fn parse_type_expr_inner(&mut self, node: &CstNode) -> Option<TypeExpr> {
        if node.kind() == SyntaxKind::List {
            let forms = meaningful_children(node);
            let Some(head_node) = forms.first() else {
                self.invalid_form(node, "a type expression cannot be empty");
                return None;
            };
            let Some(head) = head_node.leaf_text() else {
                self.invalid_form(head_node, "a type head must be a symbol");
                return None;
            };
            if matches!(head, "record" | "enum" | "union" | "newtype") {
                self.error(
                    DiagnosticCode::TypeAnonymousTypeBody,
                    node.span(),
                    "a nominal type body is only valid in a deftype body",
                );
                return None;
            }
            match head {
                "tuple" => {
                    let values = forms[1..]
                        .iter()
                        .filter_map(|form| self.parse_type_expr(form))
                        .collect::<Vec<_>>();
                    if values.len() + 1 != forms.len() {
                        return None;
                    }
                    Some(TypeExpr::Tuple(values))
                }
                "array" => {
                    if forms.len() != 2 {
                        self.invalid_form(
                            node,
                            "array type requires exactly one element type",
                        );
                        None
                    } else {
                        self.parse_type_expr(forms[1])
                            .map(|value| TypeExpr::Array(Box::new(value)))
                    }
                }
                "map" => {
                    if forms.len() != 3 {
                        self.invalid_form(
                            node,
                            "map type requires key and value types",
                        );
                        None
                    } else {
                        let key = self.parse_type_expr(forms[1])?;
                        let value = self.parse_type_expr(forms[2])?;
                        Some(TypeExpr::Map(Box::new(key), Box::new(value)))
                    }
                }
                "fn" => self.parse_function_type(node, &forms),
                _ => {
                    let head_name = self.type_name(head_node)?;
                    if RESERVED_TYPE_HEADS.contains(&head_name.value()) {
                        self.error(
                            DiagnosticCode::TypeAnonymousTypeBody,
                            head_node.span(),
                            "a reserved type head is not an applied type name",
                        );
                        return None;
                    }
                    if forms.len() < 2 {
                        self.invalid_form(
                            node,
                            "an applied type requires at least one argument",
                        );
                        None
                    } else {
                        let arguments = forms[1..]
                            .iter()
                            .filter_map(|form| self.parse_type_expr(form))
                            .collect::<Vec<_>>();
                        (arguments.len() + 1 == forms.len()).then_some(
                            TypeExpr::Applied {
                                head: head_name,
                                arguments,
                            },
                        )
                    }
                }
            }
        } else {
            let Some(classification) = node.name() else {
                if matches!(
                    node.literal(),
                    Some(LiteralClassification::Literal(Literal::Void(_)))
                ) {
                    return Some(TypeExpr::Void);
                }
                self.invalid_form(
                    node,
                    "a type expression must be a name, list, or void",
                );
                return None;
            };
            match classification {
                NameClassification::Name(name)
                    if name.kind() == NameKind::Symbol && !name.is_discard() =>
                {
                    Some(TypeExpr::Name(name))
                }
                NameClassification::Name(_) => {
                    self.invalid_form(node, "a type name must be a symbol");
                    None
                }
                NameClassification::Invalid => None,
            }
        }
    }

    fn parse_function_type(
        &mut self,
        node: &CstNode,
        forms: &[&CstNode],
    ) -> Option<TypeExpr> {
        if forms.len() < 3 {
            self.invalid_form(node, "fn type requires parameters and a result");
            return None;
        }
        let Some(parameter_forms) = list_items(forms[1]) else {
            self.invalid_form(
                forms[1],
                "function type parameters must be a list of types",
            );
            return None;
        };
        let mut parameters = Vec::with_capacity(parameter_forms.len());
        for parameter in parameter_forms {
            parameters.push(self.parse_type_expr(parameter)?);
        }
        let result = self.parse_type_expr(forms[2])?;
        let mut labelled = Vec::new();
        let mut variadic = None;
        let mut effects = Vec::new();
        let mut seen = BTreeSet::new();
        let mut index = 3;
        while index < forms.len() {
            let label = forms[index];
            let Some(label_name) = label
                .name()
                .and_then(|classification| classification.as_name().cloned())
            else {
                self.invalid_form(label, "function type attributes must be labels");
                return None;
            };
            let Some(value) = forms.get(index + 1) else {
                self.invalid_form(
                    label,
                    "function type attribute is missing its value",
                );
                return None;
            };
            if !seen.insert(label_name.value().to_owned()) {
                self.error(
                    DiagnosticCode::SyntaxDuplicateAttribute,
                    label.span(),
                    "a function type repeats an attribute",
                );
                return None;
            }
            match label_name.value() {
                "labelled" => labelled = self.parse_type_slots(value)?,
                "variadic" => variadic = Some(self.parse_variadic_type(value)?),
                "effects" => effects = self.parse_effect_row(value)?.references,
                _ => {
                    self.error(
                        DiagnosticCode::SyntaxUnknownAttribute,
                        label.span(),
                        "function type uses an unknown attribute",
                    );
                    return None;
                }
            }
            index += 2;
        }
        if index != forms.len() {
            self.invalid_form(
                node,
                "function type attributes require label/value pairs",
            );
            return None;
        }
        Some(TypeExpr::Function(FunctionType {
            parameters,
            result: Box::new(result),
            labelled,
            variadic,
            effects,
        }))
    }

    fn parse_type_slots(&mut self, node: &CstNode) -> Option<Vec<TypeSlot>> {
        let Some(forms) = list_items(node) else {
            self.invalid_form(node, "labelled function types require a list");
            return None;
        };
        if !forms.len().is_multiple_of(2) {
            self.invalid_form(node, "labelled function types require name/type pairs");
            return None;
        }
        let mut slots = Vec::with_capacity(forms.len() / 2);
        for pair in forms.chunks_exact(2) {
            let name =
                self.local_name(pair[0], "labelled type slots require local names")?;
            let value_type = self.parse_type_expr(pair[1])?;
            slots.push(TypeSlot { name, value_type });
        }
        Some(slots)
    }

    fn parse_variadic_type(&mut self, node: &CstNode) -> Option<VariadicType> {
        let Some(forms) = list_items(node) else {
            self.invalid_form(node, "variadic type must be an array or map list");
            return None;
        };
        let Some(head) = forms.first().and_then(|form| form.leaf_text()) else {
            self.invalid_form(node, "variadic type requires array or map");
            return None;
        };
        match head {
            "array" if forms.len() == 2 => self
                .parse_type_expr(forms[1])
                .map(|value| VariadicType::Array(Box::new(value))),
            "map" if forms.len() == 3 => {
                let key = self.parse_type_expr(forms[1])?;
                let value = self.parse_type_expr(forms[2])?;
                Some(VariadicType::Map(Box::new(key), Box::new(value)))
            }
            "array" | "map" => {
                self.invalid_form(node, "variadic array/map type has the wrong arity");
                None
            }
            _ => {
                self.invalid_form(node, "variadic type must be array or map");
                None
            }
        }
    }

    fn parse_attributes(
        &mut self,
        forms: &[&CstNode],
        context: AttributeContext,
        inherited_generics: &[String],
    ) -> ParsedAttributes {
        let allowed = match context {
            AttributeContext::Type => &["where", "visibility", "doc"][..],
            AttributeContext::Declaration => &["visibility", "doc"][..],
            AttributeContext::Function => &[
                "where",
                "labelled",
                "variadic",
                "visibility",
                "effects",
                "external",
                "symbol",
                "doc",
            ][..],
            AttributeContext::Lambda => &["labelled", "variadic", "effects"][..],
        };
        let mut items = Vec::new();
        let mut seen = BTreeSet::new();
        let mut index = 0;
        while index < forms.len() {
            let Some(label_name) = forms[index]
                .name()
                .and_then(|classification| classification.as_name().cloned())
            else {
                break;
            };
            if label_name.kind() != NameKind::Label {
                break;
            }
            let label = label_name.value();
            if label == "types" {
                self.error(
                    DiagnosticCode::NameReservedLabel,
                    forms[index].span(),
                    "types is reserved for call-site type arguments",
                );
            }
            if !allowed.contains(&label) {
                self.error(
                    DiagnosticCode::SyntaxUnknownAttribute,
                    forms[index].span(),
                    "attribute is not allowed in this declaration context",
                );
                if forms.get(index + 1).is_some() {
                    index += 2;
                } else {
                    index += 1;
                }
                continue;
            }
            if !seen.insert(label.to_owned()) {
                self.error(
                    DiagnosticCode::SyntaxDuplicateAttribute,
                    forms[index].span(),
                    "declaration repeats an attribute",
                );
                index = index.saturating_add(2);
                continue;
            }
            let Some(value) = forms.get(index + 1) else {
                self.error(
                    DiagnosticCode::SyntaxInvalidAttribute,
                    forms[index].span(),
                    "attribute is missing its value",
                );
                index += 1;
                continue;
            };
            if let Some(attribute) =
                self.parse_attribute(label, value, inherited_generics)
            {
                if matches!(attribute, Attribute::Where(_)) {
                    // The next nested method may inherit these names. The
                    // containing declaration collects them from the result.
                }
                items.push(attribute);
            }
            index += 2;
        }
        ParsedAttributes { items, next: index }
    }

    fn parse_attribute(
        &mut self,
        label: &str,
        value: &CstNode,
        inherited_generics: &[String],
    ) -> Option<Attribute> {
        match label {
            "where" => self
                .parse_where(value, inherited_generics)
                .map(Attribute::Where),
            "labelled" => self.parse_labelled(value).map(Attribute::Labelled),
            "variadic" => self.parse_variadic(value).map(Attribute::Variadic),
            "visibility" => self
                .atom_name(value, "visibility requires an atom")
                .map(Attribute::Visibility),
            "effects" => self.parse_effect_row(value).map(Attribute::Effects),
            "external" => self
                .atom_name(value, "external requires a provider atom")
                .map(Attribute::External),
            "symbol" => self
                .parse_string_literal(value, "symbol requires a string")
                .map(Attribute::Symbol),
            "doc" => self
                .parse_string_literal(value, "doc requires a string")
                .map(Attribute::Doc),
            _ => None,
        }
    }

    fn parse_where(
        &mut self,
        node: &CstNode,
        inherited_generics: &[String],
    ) -> Option<Vec<GenericBinding>> {
        let Some(forms) = list_items(node) else {
            self.invalid_form(node, "where requires a generic binding list");
            return None;
        };
        if !forms.len().is_multiple_of(2) {
            self.invalid_form(node, "where requires generic-name/bound pairs");
            return None;
        }
        let mut bindings = Vec::with_capacity(forms.len() / 2);
        let mut seen = BTreeSet::new();
        for pair in forms.chunks_exact(2) {
            let name =
                self.local_name(pair[0], "generic names must be unqualified symbols")?;
            if RESERVED_TYPE_HEADS.contains(&name.value()) {
                self.error(
                    DiagnosticCode::NameReservedDeclaration,
                    pair[0].span(),
                    "a generic name uses a reserved type head",
                );
            }
            if inherited_generics
                .iter()
                .any(|generic| generic == name.value())
            {
                self.error(
                    DiagnosticCode::NameGenericRedeclaration,
                    pair[0].span(),
                    "a nested declaration redeclares an inherited generic",
                );
            }
            if !seen.insert(name.value().to_owned()) {
                self.error(
                    DiagnosticCode::SyntaxDuplicateAttribute,
                    pair[0].span(),
                    "a where clause repeats a generic name",
                );
                return None;
            }
            let bound = self.local_name(pair[1], "generic bounds must be symbols")?;
            bindings.push(GenericBinding {
                name,
                bound,
                span: ByteSpan::new(pair[0].span().start(), pair[1].span().end()),
            });
        }
        Some(bindings)
    }

    fn parse_labelled(&mut self, node: &CstNode) -> Option<Vec<LabelledParameter>> {
        let Some(forms) = list_items(node) else {
            self.invalid_form(node, "labelled requires a parameter list");
            return None;
        };
        if !forms.len().is_multiple_of(3) {
            self.invalid_form(node, "labelled requires flat name/type/literal triples");
            return None;
        }
        let mut parameters = Vec::with_capacity(forms.len() / 3);
        let mut seen = BTreeSet::new();
        for triple in forms.chunks_exact(3) {
            let name =
                self.local_name(triple[0], "labelled parameters require local names")?;
            if name.is_discard() {
                self.invalid_form(
                    triple[0],
                    "labelled parameters cannot discard their names",
                );
                return None;
            }
            if !seen.insert(name.value().to_owned()) {
                self.error(
                    DiagnosticCode::SyntaxInvalidForm,
                    triple[0].span(),
                    "labelled parameters repeat a name",
                );
                return None;
            }
            let value_type = self.parse_type_expr(triple[1])?;
            let default =
                self.literal(triple[2], "labelled defaults must be literals")?;
            parameters.push(LabelledParameter {
                name,
                value_type,
                default,
                span: ByteSpan::new(triple[0].span().start(), triple[2].span().end()),
            });
        }
        Some(parameters)
    }

    fn parse_variadic(&mut self, node: &CstNode) -> Option<VariadicParameter> {
        let Some(forms) = list_items(node) else {
            self.invalid_form(node, "variadic requires a binding/type list");
            return None;
        };
        if forms.len() != 2 {
            self.invalid_form(
                node,
                "variadic requires exactly one binding and one type",
            );
            return None;
        }
        let name = self.binding_name(forms[0])?;
        let value_type = self.parse_variadic_type(forms[1])?;
        Some(VariadicParameter {
            name,
            value_type,
            span: node.span(),
        })
    }

    fn parse_effect_row(&mut self, node: &CstNode) -> Option<EffectRow> {
        let Some(forms) = list_items(node) else {
            self.invalid_form(node, "effects requires a list of symbols");
            return None;
        };
        let mut references = Vec::with_capacity(forms.len());
        for form in forms {
            match form.name() {
                Some(NameClassification::Name(name))
                    if name.kind() == NameKind::Symbol =>
                {
                    references.push(name)
                }
                Some(NameClassification::Name(_)) => self.error(
                    DiagnosticCode::EffectInvalidReference,
                    form.span(),
                    "effect rows contain symbols, not atoms or labels",
                ),
                Some(NameClassification::Invalid) => self.error(
                    DiagnosticCode::EffectInvalidReference,
                    form.span(),
                    "effect rows contain symbol references",
                ),
                None => {
                    self.invalid_form(form, "effect rows contain symbol references")
                }
            }
        }
        Some(EffectRow {
            references,
            span: node.span(),
        })
    }

    fn declaration_name(&mut self, node: &CstNode) -> Option<Name> {
        let name =
            self.local_name(node, "declaration names must be unqualified symbols")?;
        if RESERVED_TYPE_HEADS.contains(&name.value()) {
            self.error(
                DiagnosticCode::NameReservedDeclaration,
                node.span(),
                "a declaration name uses a reserved type head",
            );
        }
        Some(name)
    }

    fn value_declaration_name(&mut self, node: &CstNode) -> Option<Name> {
        let name = self.local_name(node, "value names must be unqualified symbols")?;
        if RESERVED_VALUE_SPELLINGS.contains(&name.value()) {
            self.error(
                DiagnosticCode::NameReservedValueSpelling,
                node.span(),
                "a module-level value uses a reserved collection spelling",
            );
        }
        Some(name)
    }

    fn type_name(&mut self, node: &CstNode) -> Option<Name> {
        match node.name() {
            Some(NameClassification::Name(name)) if name.kind() == NameKind::Symbol => {
                Some(name)
            }
            Some(NameClassification::Name(_)) => {
                self.invalid_form(node, "type heads must be symbols");
                None
            }
            Some(NameClassification::Invalid) => None,
            None => {
                self.invalid_form(node, "type heads must be symbols");
                None
            }
        }
    }

    fn local_name(&mut self, node: &CstNode, message: &'static str) -> Option<Name> {
        match node.name() {
            Some(NameClassification::Name(name))
                if name.kind() == NameKind::Symbol && name.segments().len() == 1 =>
            {
                Some(name)
            }
            Some(NameClassification::Name(_)) => {
                self.invalid_form(node, message);
                None
            }
            Some(NameClassification::Invalid) => None,
            None => {
                self.invalid_form(node, message);
                None
            }
        }
    }

    fn binding_name(&mut self, node: &CstNode) -> Option<Name> {
        match node.name() {
            Some(NameClassification::Name(name))
                if (name.kind() == NameKind::Symbol && name.segments().len() == 1)
                    || name.kind() == NameKind::Discard =>
            {
                Some(name)
            }
            Some(NameClassification::Name(_)) => {
                self.invalid_form(node, "a binding must be a local name or discard");
                None
            }
            Some(NameClassification::Invalid) => None,
            None => {
                self.invalid_form(node, "a binding must be a local name or discard");
                None
            }
        }
    }

    fn atom_name(&mut self, node: &CstNode, message: &'static str) -> Option<Name> {
        match node.name() {
            Some(NameClassification::Name(name)) if name.kind() == NameKind::Atom => {
                Some(name)
            }
            Some(NameClassification::Invalid) => None,
            _ => {
                self.invalid_form(node, message);
                None
            }
        }
    }

    fn literal(&mut self, node: &CstNode, message: &'static str) -> Option<Literal> {
        match node.literal() {
            Some(LiteralClassification::Literal(literal)) => Some(literal),
            Some(LiteralClassification::Invalid(_)) | None => {
                self.invalid_form(node, message);
                None
            }
            Some(LiteralClassification::Opaque) => {
                self.invalid_form(node, message);
                None
            }
        }
    }

    fn parse_string_literal(
        &mut self,
        node: &CstNode,
        message: &'static str,
    ) -> Option<Literal> {
        let literal = self.literal(node, message)?;
        if matches!(literal, Literal::String(_)) {
            Some(literal)
        } else {
            self.invalid_form(node, message);
            None
        }
    }
}

fn meaningful_children(node: &CstNode) -> Vec<&CstNode> {
    node.children()
        .iter()
        .filter(|child| matches!(child.kind(), SyntaxKind::Atom | SyntaxKind::List))
        .collect()
}

fn list_items(node: &CstNode) -> Option<Vec<&CstNode>> {
    if node.kind() != SyntaxKind::List {
        return None;
    }
    Some(meaningful_children(node))
}

fn head_text(node: &CstNode) -> Option<&str> {
    meaningful_children(node)
        .first()
        .and_then(|head| head.leaf_text())
}

fn is_label(node: &CstNode, expected: &str) -> bool {
    node.name()
        .and_then(|classification| classification.as_name().cloned())
        .is_some_and(|name| name.kind() == NameKind::Label && name.value() == expected)
}

fn is_declaration_attribute_label(node: &CstNode) -> bool {
    [
        "where",
        "labelled",
        "variadic",
        "visibility",
        "effects",
        "external",
        "symbol",
        "doc",
    ]
    .iter()
    .any(|label| is_label(node, label))
}

fn is_lambda_attribute_label(node: &CstNode) -> bool {
    ["labelled", "variadic", "effects"]
        .iter()
        .any(|label| is_label(node, label))
}

fn generic_names(attributes: &[Attribute]) -> Vec<String> {
    attributes
        .iter()
        .filter_map(|attribute| match attribute {
            Attribute::Where(bindings) => Some(
                bindings
                    .iter()
                    .map(|binding| binding.name.value().to_owned()),
            ),
            _ => None,
        })
        .flatten()
        .collect()
}

fn deftype_member_names(body: &DeftypeBody) -> BTreeSet<String> {
    match body {
        DeftypeBody::Record(fields) | DeftypeBody::Enum(fields) => fields
            .iter()
            .map(|field| field.name.value().to_owned())
            .collect(),
        DeftypeBody::Type(_) | DeftypeBody::Union(_) | DeftypeBody::Newtype(_) => {
            BTreeSet::new()
        }
    }
}

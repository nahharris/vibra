//! The canonical Vibra formatter.
//!
//! `docs/spec/01-source-language.md` defines exactly one canonical
//! representation. Formatting is idempotent and semantics-preserving: it may
//! normalize recoverable presentation but must never guess through a syntax,
//! binding, or type ambiguity.
//!
//! # Position in the architecture
//!
//! Depends on [`vibra_syntax`] and [`vibra_diagnostics`]. Nothing in the
//! language semantics may depend on this crate.
//!
//! # Status
//!
//! Milestone 1 steps 4–9 supply the syntax-only formatter. It canonicalizes
//! whitespace, delimiters, comments, line endings, list layout, and valid
//! character spellings while leaving literal and valid or invalid name
//! spellings untouched. Valid source declarations use the contextual AST to
//! order complete attribute/value groups before bodies; comments continue
//! through the lossless CST path. Project-specific VIBON schemas arrive in
//! later steps; see
//! `docs/roadmap/milestone-1/README.md`.
//!
//! A recovered document is returned byte-for-byte unchanged because applying
//! canonical whitespace to incomplete or opaque leaf text would be a guess.

use std::collections::{BTreeSet, HashMap};
use std::fmt;
use std::path::Path;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_syntax::{
    Application, ApplicationBinding, Attribute, BindingError, BindingFacts,
    CallArgument, CstNode, Declaration, DeftypeBody, Document, DocumentMode,
    DocumentModeError, Expression, ExpressionKind, FunctionDeclaration, FunctionType,
    ImplDeclaration, LambdaExpression, Literal, LiteralClassification, Parameter,
    Pattern, PatternArgument, PatternKind, SourceAst, SyntaxKind, TypeExpr, TypeField,
    TypeMember, TypeSlot, VariadicType, canonical_character_spelling, canonical_data,
    classify, parse_document,
};

/// An error selecting or formatting a document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormatError {
    /// The filename does not select `.vib` or `.vibon`.
    UnsupportedExtension(DocumentModeError),
    /// Supplied application binding facts contradict the written operands.
    Binding(BindingError),
}

impl fmt::Display for FormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedExtension(error) => error.fmt(formatter),
            Self::Binding(error) => {
                write!(formatter, "invalid application binding facts: {error}")
            }
        }
    }
}

impl std::error::Error for FormatError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::UnsupportedExtension(error) => Some(error),
            Self::Binding(error) => Some(error),
        }
    }
}

impl From<DocumentModeError> for FormatError {
    fn from(error: DocumentModeError) -> Self {
        Self::UnsupportedExtension(error)
    }
}

impl From<BindingError> for FormatError {
    fn from(error: BindingError) -> Self {
        Self::Binding(error)
    }
}

/// Formatter output plus diagnostics produced by an authoritative binding
/// normalization pass.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundFormat {
    text: String,
    diagnostics: Vec<Diagnostic>,
}

impl BoundFormat {
    /// Canonical formatted source bytes.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Style diagnostics produced while applying supplied binding facts.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

struct RenderContext<'a> {
    bindings: &'a [ApplicationBinding],
    diagnostics: Vec<Diagnostic>,
    error: Option<BindingError>,
}

impl<'a> RenderContext<'a> {
    fn new(bindings: &'a [ApplicationBinding]) -> Self {
        Self {
            bindings,
            diagnostics: Vec::new(),
            error: None,
        }
    }

    fn arguments<'expression>(
        &mut self,
        application: &'expression Application,
    ) -> Vec<&'expression CallArgument> {
        let mut changed = application.type_arguments_after_operands();
        let Some(binding) = self
            .bindings
            .iter()
            .find(|binding| binding.span() == application.span())
        else {
            if changed {
                self.argument_order_diagnostic(application);
            }
            return application.arguments().iter().collect();
        };
        match application.ordered_arguments(binding.facts()) {
            Ok(arguments) => {
                changed |= arguments.len() == application.arguments().len()
                    && arguments
                        .iter()
                        .zip(application.arguments())
                        .any(|(left, right)| !std::ptr::eq(*left, right));
                if changed {
                    self.argument_order_diagnostic(application);
                }
                arguments
            }
            Err(error) => {
                if self.error.is_none() {
                    self.error = Some(error);
                }
                application.arguments().iter().collect()
            }
        }
    }

    fn argument_order_diagnostic(&mut self, application: &Application) {
        self.argument_order_diagnostic_span(application.span());
    }

    fn argument_order_diagnostic_span(&mut self, span: ByteSpan) {
        self.diagnostics.push(Diagnostic::new(
            DiagnosticCode::StyleArgumentOrder,
            span,
            "application groups were normalized using canonical or supplied binding order",
        ));
    }

    fn binding_facts(&self, span: ByteSpan) -> Option<BindingFacts> {
        self.bindings
            .iter()
            .find(|binding| binding.span() == span)
            .map(|binding| binding.facts().clone())
    }

    fn record_error(&mut self, error: BindingError) {
        if self.error.is_none() {
            self.error = Some(error);
        }
    }
}

/// The syntax-only canonical formatter.
#[derive(Clone, Copy, Debug, Default)]
pub struct Formatter;

impl Formatter {
    /// Formats an already parsed document.
    #[must_use]
    pub fn format(self, document: &Document) -> String {
        format_document(document)
    }

    /// Selects the mode from `path`, parses, and formats the document.
    pub fn format_source(
        self,
        path: impl AsRef<Path>,
        source: &str,
    ) -> Result<String, FormatError> {
        format_source(path, source)
    }

    /// Selects the mode from `path`, parses, and formats with binding facts.
    pub fn format_source_with_bindings(
        self,
        path: impl AsRef<Path>,
        source: &str,
        bindings: &[ApplicationBinding],
    ) -> Result<BoundFormat, FormatError> {
        format_source_with_bindings(path, source, bindings)
    }
}

/// Formats a document that has already been parsed by the shared reader.
#[must_use]
pub fn format_document(document: &Document) -> String {
    match format_document_with_bindings(document, &[]) {
        Ok(formatted) => formatted.text,
        Err(_) => document.source().to_owned(),
    }
}

/// Formats a document with optional authoritative application binding facts.
///
/// Without an entry for an application, written operand order is preserved.
/// The formatter never infers a signature from a callee spelling.
pub fn format_document_with_bindings(
    document: &Document,
    bindings: &[ApplicationBinding],
) -> Result<BoundFormat, FormatError> {
    let mut context = RenderContext::new(bindings);
    let text = format_document_inner(document, &mut context)?;
    if let Some(error) = context.error {
        return Err(error.into());
    }
    Ok(BoundFormat {
        text,
        diagnostics: context.diagnostics,
    })
}

fn format_document_inner(
    document: &Document,
    context: &mut RenderContext<'_>,
) -> Result<String, FormatError> {
    if document.recovered() {
        // A recovered tree contains an explicit error marker. In that state a
        // syntax-only formatter cannot safely choose a semantic layout, and
        // must not rewrite opaque or incomplete leaf text.
        return Ok(document.source().to_owned());
    }

    if document.mode() == DocumentMode::Data {
        // The generic data AST intentionally excludes trivia. Keep the
        // lossless CST formatter for commented data so comments remain tied
        // to their neighbouring entries instead of being silently dropped.
        if document.source().contains(';') {
            return Ok(format_syntax_document(document));
        }
        return Ok(document.data().map_or_else(
            || document.source().to_owned(),
            |data| format!("{}\n", canonical_data(data)),
        ));
    }

    if let Some(ast) = document.ast() {
        if contains_line_comment(document.root()) {
            return Ok(format_source_with_comments(document, context));
        }
        return format_source_ast(document.path(), ast, context);
    }

    Ok(format_syntax_document(document))
}

fn format_source_ast(
    path: &Path,
    ast: &SourceAst,
    context: &mut RenderContext<'_>,
) -> Result<String, FormatError> {
    let mut output = String::new();
    for (index, declaration) in ast.declarations().iter().enumerate() {
        if index != 0 {
            output.push_str("\n\n");
        }
        render_declaration(declaration, &mut output, context);
    }
    output.push('\n');
    if let Some(error) = context.error.take() {
        return Err(error.into());
    }
    match parse_document(path, &output) {
        Ok(document) if !document.recovered() => Ok(format_syntax_document(&document)),
        _ => Ok(output),
    }
}

fn render_declaration(
    declaration: &Declaration,
    output: &mut String,
    context: &mut RenderContext<'_>,
) {
    match declaration {
        Declaration::Import(value) => {
            output.push_str("(import ");
            output.push_str(value.alias().raw());
            output.push(' ');
            output.push_str(value.target().raw());
            output.push(')');
        }
        Declaration::Deftype(value) => {
            output.push_str("(deftype ");
            output.push_str(value.name().raw());
            output.push(' ');
            render_deftype_body(value.body(), output);
            render_attributes(value.attributes().items(), output);
            render_type_members(value.members(), output, context);
            output.push(')');
        }
        Declaration::Defint(value) => {
            output.push_str("(defint ");
            output.push_str(value.name().raw());
            render_attributes(value.attributes().items(), output);
            render_type_members(value.members(), output, context);
            output.push(')');
        }
        Declaration::Deffect(value) => {
            output.push_str("(deffect ");
            output.push_str(value.name().raw());
            render_attributes(value.attributes().items(), output);
            for member in value.members() {
                output.push(' ');
                render_function("defn", member, output, context);
            }
            output.push(')');
        }
        Declaration::Def(value) => {
            output.push_str("(def ");
            output.push_str(value.name().raw());
            output.push(' ');
            render_type(value.value_type(), output);
            output.push(' ');
            render_expression(value.expression(), output, context);
            render_attributes(value.attributes().items(), output);
            output.push(')');
        }
        Declaration::Defn(value) => render_function("defn", value, output, context),
        Declaration::Test(value) => {
            output.push_str("(test ");
            output.push_str(&format_leaf(value.name().raw()));
            if let Some(effects) = value.effects() {
                output.push_str(" effects: ");
                render_effect_row(effects.references(), output);
            }
            for body in value.expressions() {
                output.push(' ');
                render_expression(body, output, context);
            }
            output.push(')');
        }
    }
}

fn render_type_member(
    member: &TypeMember,
    output: &mut String,
    context: &mut RenderContext<'_>,
) {
    match member {
        TypeMember::Method(method) => render_function("defn", method, output, context),
        TypeMember::Implementation(implementation) => {
            render_impl(implementation, output, context)
        }
    }
}

fn render_type_members(
    members: &[TypeMember],
    output: &mut String,
    context: &mut RenderContext<'_>,
) {
    for member in members
        .iter()
        .filter(|member| matches!(member, TypeMember::Method(_)))
    {
        output.push(' ');
        render_type_member(member, output, context);
    }
    for member in members
        .iter()
        .filter(|member| matches!(member, TypeMember::Implementation(_)))
    {
        output.push(' ');
        render_type_member(member, output, context);
    }
}

fn render_impl(
    implementation: &ImplDeclaration,
    output: &mut String,
    context: &mut RenderContext<'_>,
) {
    output.push_str("(impl ");
    render_type(implementation.target(), output);
    for member in implementation.members() {
        output.push(' ');
        render_function("defn", member, output, context);
    }
    output.push(')');
}

fn render_function(
    head: &str,
    function: &FunctionDeclaration,
    output: &mut String,
    context: &mut RenderContext<'_>,
) {
    output.push('(');
    output.push_str(head);
    output.push(' ');
    output.push_str(function.name().raw());
    output.push(' ');
    render_parameters(function.parameters(), output, context);
    output.push(' ');
    render_type(function.result(), output);
    render_attributes(function.attributes().items(), output);
    for body in function.expressions() {
        output.push(' ');
        render_expression(body, output, context);
    }
    output.push(')');
}

fn render_parameters(
    parameters: &[Parameter],
    output: &mut String,
    context: &mut RenderContext<'_>,
) {
    output.push('(');
    for (index, parameter) in parameters.iter().enumerate() {
        if index != 0 {
            output.push(' ');
        }
        render_pattern(parameter.parsed_pattern(), output, context);
        output.push(' ');
        render_type(parameter.value_type(), output);
    }
    output.push(')');
}

fn render_expression(
    expression: &Expression,
    output: &mut String,
    context: &mut RenderContext<'_>,
) {
    match expression.kind() {
        ExpressionKind::Literal(literal) => {
            output.push_str(&format_leaf(literal.raw()))
        }
        ExpressionKind::Name(name) => output.push_str(name.raw()),
        ExpressionKind::Application(application) => {
            output.push('(');
            render_expression(application.callee(), output, context);
            if let Some(type_arguments) = application.type_arguments() {
                output.push_str(" types: (");
                for (index, value) in type_arguments.iter().enumerate() {
                    if index != 0 {
                        output.push(' ');
                    }
                    render_type(value, output);
                }
                output.push(')');
            }
            for argument in context.arguments(application) {
                output.push(' ');
                if let Some(label) = argument.label() {
                    output.push_str(label.raw());
                    output.push(' ');
                }
                render_expression(argument.value(), output, context);
            }
            output.push(')');
        }
        ExpressionKind::Lambda(lambda) => render_lambda(lambda, output, context),
        ExpressionKind::Do(body) => {
            output.push_str("(do");
            for expression in body {
                output.push(' ');
                render_expression(expression, output, context);
            }
            output.push(')');
        }
        ExpressionKind::Let {
            pattern,
            value,
            body,
        } => {
            output.push_str("(let ");
            render_pattern(pattern, output, context);
            output.push(' ');
            render_expression(value, output, context);
            for expression in body {
                output.push(' ');
                render_expression(expression, output, context);
            }
            output.push(')');
        }
        ExpressionKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            output.push_str("(if ");
            render_expression(condition, output, context);
            output.push(' ');
            render_expression(then_branch, output, context);
            output.push(' ');
            render_expression(else_branch, output, context);
            output.push(')');
        }
        ExpressionKind::Match { scrutinee, arms } => {
            output.push_str("(match ");
            render_expression(scrutinee, output, context);
            for arm in arms {
                output.push(' ');
                render_pattern(arm.pattern(), output, context);
                output.push(' ');
                render_expression(arm.result(), output, context);
            }
            output.push(')');
        }
        ExpressionKind::As {
            value_type,
            operand,
        } => {
            output.push_str("(as ");
            render_type(value_type, output);
            output.push(' ');
            render_expression(operand, output, context);
            output.push(')');
        }
        ExpressionKind::Try(operand) => {
            output.push_str("(try ");
            render_expression(operand, output, context);
            output.push(')');
        }
    }
}

fn render_lambda(
    lambda: &LambdaExpression,
    output: &mut String,
    context: &mut RenderContext<'_>,
) {
    output.push_str("(lambda ");
    render_parameters(lambda.parameters(), output, context);
    output.push(' ');
    render_type(lambda.result(), output);
    render_attributes(lambda.attributes().items(), output);
    for expression in lambda.body() {
        output.push(' ');
        render_expression(expression, output, context);
    }
    output.push(')');
}

fn render_pattern(
    pattern: &Pattern,
    output: &mut String,
    context: &mut RenderContext<'_>,
) {
    match pattern.kind() {
        PatternKind::Binding(name) | PatternKind::Atom(name) => {
            output.push_str(name.raw())
        }
        PatternKind::Literal(literal) => output.push_str(&format_leaf(literal.raw())),
        PatternKind::Constructor { head, arguments } => {
            output.push('(');
            output.push_str(head.raw());
            for argument in arguments {
                output.push(' ');
                render_pattern_argument(argument, output, context);
            }
            output.push(')');
        }
        PatternKind::Tuple(values) => {
            output.push_str("(tuple");
            for value in values {
                output.push(' ');
                render_pattern(value, output, context);
            }
            output.push(')');
        }
        PatternKind::Array(values) => {
            output.push_str("(array");
            for value in values {
                output.push(' ');
                render_pattern(value, output, context);
            }
            output.push(')');
        }
        PatternKind::As {
            value_type,
            pattern,
        } => {
            output.push_str("(as ");
            render_type(value_type, output);
            output.push(' ');
            render_pattern(pattern, output, context);
            output.push(')');
        }
    }
}

fn render_pattern_argument(
    argument: &PatternArgument,
    output: &mut String,
    context: &mut RenderContext<'_>,
) {
    if let Some(label) = argument.label() {
        output.push_str(label.raw());
        output.push(' ');
    }
    render_pattern(argument.pattern(), output, context);
}

fn render_deftype_body(body: &DeftypeBody, output: &mut String) {
    match body {
        DeftypeBody::Type(value) => render_type(value, output),
        DeftypeBody::Record(fields) => render_fields("record", fields, output),
        DeftypeBody::Enum(fields) => render_fields("enum", fields, output),
        DeftypeBody::Union(members) => {
            output.push_str("(union");
            for member in members {
                output.push(' ');
                render_type(member, output);
            }
            output.push(')');
        }
        DeftypeBody::Newtype(value) => {
            output.push_str("(newtype ");
            render_type(value, output);
            output.push(')');
        }
    }
}

fn render_fields(head: &str, fields: &[TypeField], output: &mut String) {
    output.push('(');
    output.push_str(head);
    for field in fields {
        output.push(' ');
        output.push_str(field.name().raw());
        output.push(' ');
        render_type(field.ty(), output);
    }
    output.push(')');
}

fn render_type(value: &TypeExpr, output: &mut String) {
    match value {
        TypeExpr::Name(name) => output.push_str(name.raw()),
        TypeExpr::Applied { head, arguments } => {
            output.push('(');
            output.push_str(head.raw());
            for argument in arguments {
                output.push(' ');
                render_type(argument, output);
            }
            output.push(')');
        }
        TypeExpr::Tuple(values) => {
            output.push_str("(tuple");
            for value in values {
                output.push(' ');
                render_type(value, output);
            }
            output.push(')');
        }
        TypeExpr::Array(value) => {
            output.push_str("(array ");
            render_type(value, output);
            output.push(')');
        }
        TypeExpr::Map(key, value) => {
            output.push_str("(map ");
            render_type(key, output);
            output.push(' ');
            render_type(value, output);
            output.push(')');
        }
        TypeExpr::Function(value) => render_function_type(value, output),
        TypeExpr::Void => output.push_str("void"),
    }
}

fn render_function_type(value: &FunctionType, output: &mut String) {
    output.push_str("(fn (");
    for (index, parameter) in value.parameters().iter().enumerate() {
        if index != 0 {
            output.push(' ');
        }
        render_type(parameter, output);
    }
    output.push_str(") ");
    render_type(value.result(), output);
    if !value.labelled().is_empty() {
        output.push_str(" labelled: (");
        for (index, slot) in value.labelled().iter().enumerate() {
            if index != 0 {
                output.push(' ');
            }
            render_type_slot(slot, output);
        }
        output.push(')');
    }
    if let Some(variadic) = value.variadic() {
        output.push_str(" variadic: ");
        render_variadic_type(variadic, output);
    }
    if !value.effects().is_empty() {
        output.push_str(" effects: ");
        render_effect_row(value.effects(), output);
    }
    output.push(')');
}

fn render_type_slot(slot: &TypeSlot, output: &mut String) {
    output.push_str(slot.name().raw());
    output.push(' ');
    render_type(slot.value_type(), output);
}

fn render_variadic_type(value: &VariadicType, output: &mut String) {
    match value {
        VariadicType::Array(value) => {
            output.push_str("(array ");
            render_type(value, output);
            output.push(')');
        }
        VariadicType::Map(key, value) => {
            output.push_str("(map ");
            render_type(key, output);
            output.push(' ');
            render_type(value, output);
            output.push(')');
        }
    }
}

fn render_attributes(attributes: &[Attribute], output: &mut String) {
    let mut ordered = attributes.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|attribute| attribute.canonical_order());
    for attribute in ordered {
        output.push(' ');
        output.push_str(attribute.label());
        output.push_str(": ");
        match attribute {
            Attribute::Where(bindings) => {
                output.push('(');
                for (index, binding) in bindings.iter().enumerate() {
                    if index != 0 {
                        output.push(' ');
                    }
                    output.push_str(binding.name().raw());
                    output.push(' ');
                    output.push_str(binding.bound().raw());
                }
                output.push(')');
            }
            Attribute::Labelled(parameters) => {
                output.push('(');
                for (index, parameter) in parameters.iter().enumerate() {
                    if index != 0 {
                        output.push(' ');
                    }
                    output.push_str(parameter.name().raw());
                    output.push(' ');
                    render_type(parameter.value_type(), output);
                    output.push(' ');
                    output.push_str(&format_leaf(parameter.default().raw()));
                }
                output.push(')');
            }
            Attribute::Variadic(parameter) => {
                output.push('(');
                output.push_str(parameter.name().raw());
                output.push(' ');
                render_variadic_type(parameter.value_type(), output);
                output.push(')');
            }
            Attribute::Visibility(name) | Attribute::External(name) => {
                output.push_str(name.raw());
            }
            Attribute::Effects(row) => render_effect_row(row.references(), output),
            Attribute::Symbol(value) | Attribute::Doc(value) => {
                output.push_str(&format_leaf(value.raw()));
            }
        }
    }
}

fn render_effect_row(references: &[vibra_syntax::Name], output: &mut String) {
    output.push('(');
    for (index, reference) in references.iter().enumerate() {
        if index != 0 {
            output.push(' ');
        }
        output.push_str(reference.raw());
    }
    output.push(')');
}

struct CstItem<'source> {
    node: &'source CstNode,
    leading: Vec<&'source CstNode>,
    trailing: Vec<&'source CstNode>,
}

fn format_source_with_comments(
    document: &Document,
    context: &mut RenderContext<'_>,
) -> String {
    let Some(groups) = root_groups(document.root()) else {
        return normalize_recovery(&document.root().to_source());
    };
    if groups.is_empty() {
        return "\n".to_owned();
    }
    let layouts = build_layouts(document.root());
    let mut output = String::new();
    for (group_index, group) in groups.iter().enumerate() {
        if group_index != 0 {
            output.push_str("\n\n");
        }
        for (comment_index, comment) in group.comments.iter().enumerate() {
            if comment_index != 0 {
                output.push('\n');
            }
            output.push_str(&comment_text(comment));
        }
        if let Some(form) = group.form {
            if !group.comments.is_empty() {
                output.push('\n');
            }
            if is_native_declaration(form) && contains_line_comment(form) {
                render_declaration_with_comments(
                    form,
                    document.source(),
                    0,
                    &layouts,
                    &mut output,
                    context,
                );
            } else {
                render_node(
                    form,
                    0,
                    &layouts,
                    &mut output,
                    document.source(),
                    Some(context),
                );
            }
        }
    }
    output.push('\n');
    output
}

fn render_declaration_with_comments(
    node: &CstNode,
    source: &str,
    indent: usize,
    layouts: &HashMap<*const CstNode, NodeLayout>,
    output: &mut String,
    context: &mut RenderContext<'_>,
) {
    let items = cst_items(node, source);
    let Some(head) = items.first().and_then(|item| item.node.leaf_text()) else {
        render_node(node, indent, layouts, output, source, Some(context));
        return;
    };
    let header_count = declaration_header_count(head);
    if items.len() < header_count {
        render_node(node, indent, layouts, output, source, Some(context));
        return;
    }

    let mut attribute_groups = Vec::new();
    let mut cursor = header_count;
    while let (Some(label), Some(value)) = (items.get(cursor), items.get(cursor + 1)) {
        if !is_attribute_label(label.node) {
            break;
        }
        let order = attribute_order(label.node.leaf_text().unwrap_or_default());
        attribute_groups.push((order, label, value));
        cursor += 2;
    }
    attribute_groups.sort_by_key(|(order, _, _)| *order);

    output.push('(');
    for item in items.iter().take(header_count) {
        render_cst_item(
            item,
            source,
            indent.saturating_add(2),
            layouts,
            output,
            false,
            context,
        );
    }
    for (_, label, value) in attribute_groups {
        render_cst_item(
            label,
            source,
            indent.saturating_add(2),
            layouts,
            output,
            false,
            context,
        );
        render_cst_item(
            value,
            source,
            indent.saturating_add(2),
            layouts,
            output,
            false,
            context,
        );
    }
    let body = items.iter().skip(cursor);
    if matches!(head, "deftype" | "defint") {
        for member_head in ["defn", "impl"] {
            for item in body.clone() {
                if meaningful_head(item.node) == Some(member_head) {
                    render_cst_item(
                        item,
                        source,
                        indent.saturating_add(2),
                        layouts,
                        output,
                        true,
                        context,
                    );
                }
            }
        }
        for item in body {
            if !matches!(meaningful_head(item.node), Some("defn" | "impl")) {
                render_cst_item(
                    item,
                    source,
                    indent.saturating_add(2),
                    layouts,
                    output,
                    is_native_declaration(item.node),
                    context,
                );
            }
        }
    } else {
        for item in body {
            render_cst_item(
                item,
                source,
                indent.saturating_add(2),
                layouts,
                output,
                is_native_declaration(item.node),
                context,
            );
        }
    }
    output.push('\n');
    output.push_str(&" ".repeat(indent));
    output.push(')');
}

fn render_cst_item(
    item: &CstItem<'_>,
    source: &str,
    indent: usize,
    layouts: &HashMap<*const CstNode, NodeLayout>,
    output: &mut String,
    recurse_declaration: bool,
    context: &mut RenderContext<'_>,
) {
    for comment in &item.leading {
        output.push('\n');
        output.push_str(&" ".repeat(indent));
        output.push_str(&comment_text(comment));
    }
    output.push('\n');
    output.push_str(&" ".repeat(indent));
    if recurse_declaration {
        render_declaration_with_comments(
            item.node, source, indent, layouts, output, context,
        );
    } else {
        render_node(item.node, indent, layouts, output, source, Some(context));
    }
    for comment in &item.trailing {
        output.push('\n');
        output.push_str(&" ".repeat(indent));
        output.push_str(&comment_text(comment));
    }
}

fn cst_items<'source>(node: &'source CstNode, source: &str) -> Vec<CstItem<'source>> {
    let mut items = Vec::new();
    let mut leading = Vec::new();
    for child in node.children() {
        match child.kind() {
            SyntaxKind::Whitespace | SyntaxKind::OpenParen | SyntaxKind::CloseParen => {
            }
            SyntaxKind::Atom | SyntaxKind::List => {
                items.push(CstItem {
                    node: child,
                    leading: std::mem::take(&mut leading),
                    trailing: Vec::new(),
                });
            }
            SyntaxKind::LineComment => {
                let attach_to_previous = items.last().is_some_and(|item| {
                    !has_line_break(
                        source,
                        item.node.span().end(),
                        child.span().start(),
                    )
                });
                if attach_to_previous {
                    if let Some(item) = items.last_mut() {
                        item.trailing.push(child);
                    }
                } else {
                    leading.push(child);
                }
            }
            SyntaxKind::Root | SyntaxKind::Error => {}
        }
    }
    if let Some(item) = items.last_mut() {
        item.trailing.extend(leading);
    }
    items
}

fn has_line_break(source: &str, start: usize, end: usize) -> bool {
    source
        .get(start..end)
        .is_some_and(|text| text.contains(['\r', '\n']))
}

fn contains_line_comment(node: &CstNode) -> bool {
    let mut nodes = vec![node];
    while let Some(current) = nodes.pop() {
        if current.kind() == SyntaxKind::LineComment {
            return true;
        }
        nodes.extend(current.children().iter());
    }
    false
}

fn is_native_declaration(node: &CstNode) -> bool {
    matches!(
        meaningful_head(node),
        Some(
            "import"
                | "deftype"
                | "defint"
                | "deffect"
                | "def"
                | "defn"
                | "test"
                | "impl"
        )
    )
}

fn meaningful_head(node: &CstNode) -> Option<&str> {
    node.children()
        .iter()
        .find(|child| matches!(child.kind(), SyntaxKind::Atom | SyntaxKind::List))
        .and_then(CstNode::leaf_text)
}

fn declaration_header_count(head: &str) -> usize {
    match head {
        "import" => 3,
        "deftype" => 3,
        "defint" | "deffect" | "impl" => 2,
        "def" => 4,
        "defn" => 4,
        "test" => 2,
        _ => 0,
    }
}

fn is_attribute_label(node: &CstNode) -> bool {
    node.leaf_text().is_some_and(|text| {
        matches!(
            text,
            "where:"
                | "labelled:"
                | "variadic:"
                | "visibility:"
                | "effects:"
                | "external:"
                | "symbol:"
                | "doc:"
        )
    })
}

fn attribute_order(label: &str) -> usize {
    match label {
        "where:" => 0,
        "labelled:" => 1,
        "variadic:" => 2,
        "visibility:" => 3,
        "effects:" => 4,
        "external:" => 5,
        "symbol:" => 6,
        "doc:" => 7,
        _ => usize::MAX,
    }
}

fn format_syntax_document(document: &Document) -> String {
    let Some(groups) = root_groups(document.root()) else {
        return normalize_recovery(&document.root().to_source());
    };
    if groups.is_empty() {
        return "\n".to_owned();
    }
    let layouts = build_layouts(document.root());
    let mut output = String::new();
    for (group_index, group) in groups.iter().enumerate() {
        if group_index != 0 {
            output.push_str("\n\n");
        }
        for (comment_index, comment) in group.comments.iter().enumerate() {
            if comment_index != 0 {
                output.push('\n');
            }
            output.push_str(&comment_text(comment));
        }
        if let Some(form) = group.form {
            if !group.comments.is_empty() {
                output.push('\n');
            }
            render_node(form, 0, &layouts, &mut output, document.source(), None);
        }
    }
    output.push('\n');
    output
}

/// Selects a document mode from `path`, parses, and returns canonical text.
pub fn format_source(
    path: impl AsRef<Path>,
    source: &str,
) -> Result<String, FormatError> {
    Ok(format_source_with_bindings(path, source, &[])?.text)
}

/// Selects a document mode from `path`, parses, and formats with authoritative
/// application binding facts.
pub fn format_source_with_bindings(
    path: impl AsRef<Path>,
    source: &str,
    bindings: &[ApplicationBinding],
) -> Result<BoundFormat, FormatError> {
    let document = parse_document(path, source)?;
    format_document_with_bindings(&document, bindings)
}

/// Alias for [`format_source`] for callers that use the shorter operation name.
pub fn format(path: impl AsRef<Path>, source: &str) -> Result<String, FormatError> {
    format_source(path, source)
}

struct RootGroup<'source> {
    comments: Vec<&'source CstNode>,
    form: Option<&'source CstNode>,
}

fn root_groups(root: &CstNode) -> Option<Vec<RootGroup<'_>>> {
    let mut groups = Vec::new();
    let mut comments = Vec::new();
    for child in root.children() {
        match child.kind() {
            SyntaxKind::Whitespace => {}
            SyntaxKind::LineComment => comments.push(child),
            SyntaxKind::Atom | SyntaxKind::List => {
                groups.push(RootGroup {
                    comments: std::mem::take(&mut comments),
                    form: Some(child),
                });
            }
            SyntaxKind::Root
            | SyntaxKind::OpenParen
            | SyntaxKind::CloseParen
            | SyntaxKind::Error => {
                // `format_document` handles recovery trees above. Keeping
                // this branch makes the formatter conservative if a future
                // reader exposes a new root child kind.
                return None;
            }
        }
    }
    if !comments.is_empty() {
        groups.push(RootGroup {
            comments,
            form: None,
        });
    }
    Some(groups)
}

#[derive(Clone, Copy)]
struct NodeLayout {
    inline: bool,
    inline_width: usize,
}

fn node_key(node: &CstNode) -> *const CstNode {
    std::ptr::from_ref(node)
}

enum LayoutTask<'source> {
    Visit(&'source CstNode, usize, bool),
}

fn build_layouts(root: &CstNode) -> HashMap<*const CstNode, NodeLayout> {
    let mut layouts = HashMap::new();
    let mut tasks = Vec::new();
    for child in root.children().iter().rev() {
        if matches!(child.kind(), SyntaxKind::Atom | SyntaxKind::List) {
            tasks.push(LayoutTask::Visit(child, 0, false));
        }
    }

    while let Some(LayoutTask::Visit(node, indent, expanded)) = tasks.pop() {
        if !expanded {
            tasks.push(LayoutTask::Visit(node, indent, true));
            if node.kind() == SyntaxKind::List {
                for child in node.children().iter().rev() {
                    if matches!(child.kind(), SyntaxKind::Atom | SyntaxKind::List) {
                        tasks.push(LayoutTask::Visit(
                            child,
                            indent.saturating_add(2),
                            false,
                        ));
                    }
                }
            }
            continue;
        }

        let layout = match node.kind() {
            SyntaxKind::Atom => leaf_layout(node.leaf_text().unwrap_or_default()),
            SyntaxKind::List => list_layout(node, indent, &layouts),
            _ => NodeLayout {
                inline: false,
                inline_width: 0,
            },
        };
        layouts.insert(node_key(node), layout);
    }
    layouts
}

fn leaf_layout(text: &str) -> NodeLayout {
    let formatted = format_leaf(text);
    let (has_newline, inline_width) = normalized_leaf_shape(&formatted);
    NodeLayout {
        inline: !has_newline,
        inline_width,
    }
}

fn normalized_leaf_shape(text: &str) -> (bool, usize) {
    let mut has_newline = false;
    let mut inline_width: usize = 0;
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\r' {
            has_newline = true;
            if characters.peek() == Some(&'\n') {
                characters.next();
            }
        } else if character == '\n' {
            has_newline = true;
        }
        inline_width = inline_width.saturating_add(1);
    }
    (has_newline, inline_width)
}

fn list_layout(
    node: &CstNode,
    indent: usize,
    layouts: &HashMap<*const CstNode, NodeLayout>,
) -> NodeLayout {
    let mut has_comment = false;
    let mut all_inline = true;
    let mut item_count = 0;
    let mut inline_width: usize = 2;
    for child in node.children() {
        match child.kind() {
            SyntaxKind::Whitespace | SyntaxKind::OpenParen | SyntaxKind::CloseParen => {
            }
            SyntaxKind::LineComment => has_comment = true,
            SyntaxKind::Atom | SyntaxKind::List => {
                let child_layout =
                    layouts
                        .get(&node_key(child))
                        .copied()
                        .unwrap_or(NodeLayout {
                            inline: false,
                            inline_width: 0,
                        });
                if item_count != 0 {
                    inline_width = inline_width.saturating_add(1);
                }
                inline_width = inline_width.saturating_add(child_layout.inline_width);
                item_count += 1;
                all_inline &= child_layout.inline;
            }
            SyntaxKind::Root | SyntaxKind::Error => {}
        }
    }
    NodeLayout {
        inline: !has_comment && all_inline && indent.saturating_add(inline_width) <= 88,
        inline_width,
    }
}

enum LineComponent<'source> {
    Node(&'source CstNode),
    Comment(&'source CstNode),
}

struct MatchArmLayout<'source> {
    leading: Vec<&'source CstNode>,
    pattern: &'source CstNode,
    between: Vec<&'source CstNode>,
    result: &'source CstNode,
    trailing: Vec<&'source CstNode>,
}

struct MatchLayout<'source> {
    head: &'source CstNode,
    scrutinee: &'source CstNode,
    arms: Vec<MatchArmLayout<'source>>,
}

struct CstOperand<'source> {
    label: Option<&'source CstNode>,
    value: &'source CstNode,
}

enum RenderTask<'source> {
    Node(&'source CstNode, usize),
    Raw(&'static str),
    LineNode(&'source CstNode, usize),
    LineComment(&'source CstNode, usize),
    LineGroup {
        items: Vec<&'source CstNode>,
        indent: usize,
    },
    MatchArm {
        leading: Vec<&'source CstNode>,
        pattern: &'source CstNode,
        between: Vec<&'source CstNode>,
        result: &'source CstNode,
        trailing: Vec<&'source CstNode>,
        indent: usize,
    },
    MatchAfterPattern {
        between: Vec<&'source CstNode>,
        result: &'source CstNode,
        trailing: Vec<&'source CstNode>,
        indent: usize,
    },
    MatchAfterResult {
        trailing: Vec<&'source CstNode>,
        indent: usize,
    },
    CloseList(usize),
}

fn render_node(
    node: &CstNode,
    indent: usize,
    layouts: &HashMap<*const CstNode, NodeLayout>,
    output: &mut String,
    source: &str,
    context: Option<&mut RenderContext<'_>>,
) {
    let mut context = context;
    let mut tasks = vec![RenderTask::Node(node, indent)];
    while let Some(task) = tasks.pop() {
        match task {
            RenderTask::Node(node, indent) => match node.kind() {
                SyntaxKind::Atom => {
                    output.push_str(&format_leaf(node.leaf_text().unwrap_or_default()));
                }
                SyntaxKind::List => {
                    let layout =
                        layouts.get(&node_key(node)).copied().unwrap_or(NodeLayout {
                            inline: false,
                            inline_width: 0,
                        });
                    if !contains_line_comment(node)
                        && let Some(facts) = context
                            .as_deref_mut()
                            .and_then(|context| context.binding_facts(node.span()))
                    {
                        match bound_application_groups(node, &facts) {
                            Ok((mut groups, changed)) => {
                                if changed && let Some(context) = context.as_deref_mut()
                                {
                                    context.argument_order_diagnostic_span(node.span());
                                }
                                output.push('(');
                                if layout.inline {
                                    tasks.push(RenderTask::Raw(")"));
                                    let items = groups
                                        .into_iter()
                                        .flatten()
                                        .collect::<Vec<_>>();
                                    for (index, item) in
                                        items.into_iter().enumerate().rev()
                                    {
                                        tasks.push(RenderTask::Node(
                                            item,
                                            indent.saturating_add(2),
                                        ));
                                        if index != 0 {
                                            tasks.push(RenderTask::Raw(" "));
                                        }
                                    }
                                } else {
                                    let head_group = groups.remove(0);
                                    let Some(head) = head_group.into_iter().next()
                                    else {
                                        continue;
                                    };
                                    let last = groups
                                        .last()
                                        .and_then(|group| group.last())
                                        .copied();
                                    let last_is_node = last.is_some_and(|last| {
                                        layouts.get(&node_key(last)).is_some_and(
                                            |child_layout| {
                                                if child_layout.inline {
                                                    indent
                                                        .saturating_add(2)
                                                        .saturating_add(
                                                            child_layout.inline_width,
                                                        )
                                                        .saturating_add(1)
                                                        <= 88
                                                } else {
                                                    true
                                                }
                                            },
                                        )
                                    });
                                    if last_is_node {
                                        tasks.push(RenderTask::Raw(")"));
                                    } else {
                                        tasks.push(RenderTask::CloseList(indent));
                                    }
                                    for group in groups.into_iter().rev() {
                                        tasks.push(RenderTask::LineGroup {
                                            items: group,
                                            indent: indent.saturating_add(2),
                                        });
                                    }
                                    tasks.push(RenderTask::Node(head, indent));
                                }
                                continue;
                            }
                            Err(error) => {
                                if let Some(context) = context.as_deref_mut() {
                                    context.record_error(error);
                                }
                            }
                        }
                    }
                    if layout.inline {
                        output.push('(');
                        let items = node
                            .children()
                            .iter()
                            .filter(|child| {
                                matches!(
                                    child.kind(),
                                    SyntaxKind::Atom | SyntaxKind::List
                                )
                            })
                            .collect::<Vec<_>>();
                        tasks.push(RenderTask::Raw(")"));
                        for (index, item) in items.into_iter().enumerate().rev() {
                            tasks
                                .push(RenderTask::Node(item, indent.saturating_add(2)));
                            if index != 0 {
                                tasks.push(RenderTask::Raw(" "));
                            }
                        }
                    } else {
                        if let Some(match_layout) = match_layout(node, source) {
                            output.push('(');
                            let last_arm = match_layout.arms.last();
                            let last_is_node = last_arm.is_some_and(|arm| {
                                if !arm.trailing.is_empty() {
                                    return false;
                                }
                                layouts.get(&node_key(arm.result)).is_some_and(
                                    |child_layout| {
                                        if child_layout.inline {
                                            indent
                                                .saturating_add(2)
                                                .saturating_add(
                                                    child_layout.inline_width,
                                                )
                                                .saturating_add(1)
                                                <= 88
                                        } else {
                                            true
                                        }
                                    },
                                )
                            });
                            if last_is_node {
                                tasks.push(RenderTask::Raw(")"));
                            } else {
                                tasks.push(RenderTask::CloseList(indent));
                            }
                            for arm in match_layout.arms.into_iter().rev() {
                                tasks.push(RenderTask::MatchArm {
                                    leading: arm.leading,
                                    pattern: arm.pattern,
                                    between: arm.between,
                                    result: arm.result,
                                    trailing: arm.trailing,
                                    indent: indent.saturating_add(2),
                                });
                            }
                            tasks.push(RenderTask::LineNode(
                                match_layout.scrutinee,
                                indent.saturating_add(2),
                            ));
                            tasks.push(RenderTask::Node(match_layout.head, indent));
                            continue;
                        }
                        output.push('(');
                        let components = multiline_components(node);
                        // Delimiter placement is a property of every
                        // multiline list, not only of a commented one. A
                        // hanging `(` or an orphaned `)` is not canonical
                        // Vibra, so a delimiter stands alone only when its
                        // neighbour is a comment, is itself multiline, or
                        // does not fit beside it.
                        let first_is_node = components.first().is_some_and(
                            |component| match component {
                                LineComponent::Node(child) => layouts
                                    .get(&node_key(child))
                                    .is_some_and(|child_layout| {
                                        child_layout.inline
                                            && indent.saturating_add(1).saturating_add(
                                                child_layout.inline_width,
                                            ) <= 88
                                    }),
                                LineComponent::Comment(_) => false,
                            },
                        );
                        let last_is_node =
                            components.last().is_some_and(
                                |component| match component {
                                    LineComponent::Node(child) => layouts
                                        .get(&node_key(child))
                                        .is_some_and(|child_layout| {
                                            if child_layout.inline {
                                                // The delimiter has to fit
                                                // beside the form it joins.
                                                indent
                                                    .saturating_add(2)
                                                    .saturating_add(
                                                        child_layout.inline_width,
                                                    )
                                                    .saturating_add(1)
                                                    <= 88
                                            } else {
                                                // A multiline last form
                                                // already ends on its own
                                                // closing delimiter, so this
                                                // one stacks onto that line
                                                // instead of orphaning itself
                                                // below it.
                                                true
                                            }
                                        }),
                                    LineComponent::Comment(_) => false,
                                },
                            );
                        if last_is_node {
                            tasks.push(RenderTask::Raw(")"));
                        } else {
                            tasks.push(RenderTask::CloseList(indent));
                        }
                        for (index, component) in
                            components.into_iter().enumerate().rev()
                        {
                            match component {
                                LineComponent::Node(child) => {
                                    if index == 0 && first_is_node {
                                        tasks.push(RenderTask::Node(
                                            child,
                                            indent.saturating_add(2),
                                        ));
                                    } else {
                                        tasks.push(RenderTask::LineNode(
                                            child,
                                            indent.saturating_add(2),
                                        ));
                                    }
                                }
                                LineComponent::Comment(child) => {
                                    tasks.push(RenderTask::LineComment(
                                        child,
                                        indent.saturating_add(2),
                                    ))
                                }
                            }
                        }
                    }
                }
                SyntaxKind::LineComment => output.push_str(&comment_text(node)),
                SyntaxKind::Whitespace
                | SyntaxKind::Root
                | SyntaxKind::OpenParen
                | SyntaxKind::CloseParen
                | SyntaxKind::Error => {
                    output.push_str(&normalize_leaf(&node.to_source()))
                }
            },
            RenderTask::Raw(text) => output.push_str(text),
            RenderTask::LineNode(node, indent) => {
                output.push('\n');
                output.push_str(&" ".repeat(indent));
                tasks.push(RenderTask::Node(node, indent));
            }
            RenderTask::LineComment(node, indent) => {
                output.push('\n');
                output.push_str(&" ".repeat(indent));
                output.push_str(&comment_text(node));
            }
            RenderTask::LineGroup { items, indent } => {
                output.push('\n');
                output.push_str(&" ".repeat(indent));
                for (index, item) in items.into_iter().enumerate().rev() {
                    tasks.push(RenderTask::Node(item, indent));
                    if index != 0 {
                        tasks.push(RenderTask::Raw(" "));
                    }
                }
            }
            RenderTask::MatchArm {
                leading,
                pattern,
                between,
                result,
                trailing,
                indent,
            } => {
                for comment in leading {
                    output.push('\n');
                    output.push_str(&" ".repeat(indent));
                    output.push_str(&comment_text(comment));
                }
                output.push('\n');
                output.push_str(&" ".repeat(indent));
                tasks.push(RenderTask::MatchAfterPattern {
                    between,
                    result,
                    trailing,
                    indent,
                });
                tasks.push(RenderTask::Node(pattern, indent));
            }
            RenderTask::MatchAfterPattern {
                between,
                result,
                trailing,
                indent,
            } => {
                if between.is_empty() {
                    tasks.push(RenderTask::MatchAfterResult { trailing, indent });
                    tasks.push(RenderTask::Node(result, indent));
                    tasks.push(RenderTask::Raw(" "));
                } else {
                    for comment in between {
                        output.push('\n');
                        output.push_str(&" ".repeat(indent));
                        output.push_str(&comment_text(comment));
                    }
                    output.push('\n');
                    output.push_str(&" ".repeat(indent));
                    tasks.push(RenderTask::MatchAfterResult { trailing, indent });
                    tasks.push(RenderTask::Node(result, indent));
                }
            }
            RenderTask::MatchAfterResult { trailing, indent } => {
                for comment in trailing {
                    output.push('\n');
                    output.push_str(&" ".repeat(indent));
                    output.push_str(&comment_text(comment));
                }
            }
            RenderTask::CloseList(indent) => {
                output.push('\n');
                output.push_str(&" ".repeat(indent));
                output.push(')');
            }
        }
    }
}

fn multiline_components(node: &CstNode) -> Vec<LineComponent<'_>> {
    let mut components = Vec::new();
    let mut comments = Vec::new();
    for child in node.children() {
        match child.kind() {
            SyntaxKind::Whitespace | SyntaxKind::OpenParen | SyntaxKind::CloseParen => {
            }
            SyntaxKind::LineComment => comments.push(child),
            SyntaxKind::Atom | SyntaxKind::List => {
                components.extend(comments.drain(..).map(LineComponent::Comment));
                components.push(LineComponent::Node(child));
            }
            SyntaxKind::Root | SyntaxKind::Error => {}
        }
    }
    components.extend(comments.into_iter().map(LineComponent::Comment));
    components
}

fn bound_application_groups<'source>(
    node: &'source CstNode,
    facts: &BindingFacts,
) -> Result<(Vec<Vec<&'source CstNode>>, bool), BindingError> {
    let forms = node
        .children()
        .iter()
        .filter(|child| matches!(child.kind(), SyntaxKind::Atom | SyntaxKind::List))
        .collect::<Vec<_>>();
    let head = forms
        .first()
        .copied()
        .ok_or(BindingError::MissingPositional {
            expected: 1,
            actual: 0,
        })?;
    let mut original_groups = vec![vec![head]];
    let mut ordinary = Vec::new();
    let mut type_group = None;
    let mut index = 1;
    while index < forms.len() {
        let Some(current) = forms.get(index).copied() else {
            break;
        };
        if let Some(label) = cst_label(current) {
            let Some(value) = forms.get(index + 1).copied() else {
                return Err(BindingError::UnknownLabel(label.to_owned()));
            };
            let group = vec![current, value];
            original_groups.push(group.clone());
            if label == "types" {
                if type_group.replace(group).is_some() {
                    return Err(BindingError::DuplicateLabel(label.to_owned()));
                }
            } else {
                ordinary.push(CstOperand {
                    label: Some(current),
                    value,
                });
            }
            index += 2;
        } else {
            let value = current;
            original_groups.push(vec![value]);
            ordinary.push(CstOperand { label: None, value });
            index += 1;
        }
    }

    let mut positional = Vec::new();
    let mut labelled = Vec::new();
    let mut variadic = Vec::new();
    let mut seen_labels = BTreeSet::new();
    for operand in ordinary {
        if let Some(label_node) = operand.label {
            let label = cst_label(label_node).unwrap_or_default();
            if !seen_labels.insert(label.to_owned()) {
                return Err(BindingError::DuplicateLabel(label.to_owned()));
            }
            let Some(order) = facts
                .labelled()
                .iter()
                .position(|declared| declared == label)
            else {
                return Err(BindingError::UnknownLabel(label.to_owned()));
            };
            labelled.push((order, operand));
        } else {
            positional.push(operand);
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
        Some(vibra_syntax::VariadicBinding::Array) => {}
        Some(vibra_syntax::VariadicBinding::Map)
            if !variadic.len().is_multiple_of(2) =>
        {
            return Err(BindingError::OddMapVariadic(variadic.len()));
        }
        Some(vibra_syntax::VariadicBinding::Map) => {}
        None if !variadic.is_empty() => {
            return Err(BindingError::UnexpectedPositional(variadic.len()));
        }
        None => {}
    }

    let mut groups = vec![vec![head]];
    if let Some(type_group) = type_group {
        groups.push(type_group);
    }
    groups.extend(fixed.into_iter().map(|operand| {
        operand
            .label
            .map_or_else(|| vec![operand.value], |label| vec![label, operand.value])
    }));
    labelled.sort_by_key(|(order, _)| *order);
    groups.extend(labelled.into_iter().map(|(_, operand)| {
        operand
            .label
            .map_or_else(|| vec![operand.value], |label| vec![label, operand.value])
    }));
    groups.extend(variadic.into_iter().map(|operand| vec![operand.value]));

    let original = original_groups.iter().flatten().copied();
    let ordered = groups.iter().flatten().copied();
    let changed = original
        .zip(ordered)
        .any(|(left, right)| !std::ptr::eq(left, right));
    Ok((groups, changed))
}

fn cst_label(node: &CstNode) -> Option<&str> {
    node.leaf_text().and_then(|text| text.strip_suffix(':'))
}

fn match_layout<'source>(
    node: &'source CstNode,
    source: &str,
) -> Option<MatchLayout<'source>> {
    let components = multiline_components(node);
    let LineComponent::Node(head) = components.first()? else {
        return None;
    };
    if head.leaf_text() != Some("match") {
        return None;
    }
    let LineComponent::Node(scrutinee) = components.get(1)? else {
        return None;
    };

    let mut index = 2;
    let mut pending = Vec::new();
    let mut arms = Vec::new();
    while index < components.len() {
        let mut leading = std::mem::take(&mut pending);
        while let Some(component) = components.get(index) {
            match component {
                LineComponent::Comment(comment) => {
                    leading.push(*comment);
                    index += 1;
                }
                LineComponent::Node(_) => break,
            }
        }
        let pattern = match components.get(index)? {
            LineComponent::Node(pattern) => {
                index += 1;
                *pattern
            }
            LineComponent::Comment(_) => return None,
        };
        let mut between = Vec::new();
        while let Some(component) = components.get(index) {
            match component {
                LineComponent::Comment(comment) => {
                    between.push(*comment);
                    index += 1;
                }
                LineComponent::Node(_) => break,
            }
        }
        let result = match components.get(index)? {
            LineComponent::Node(result) => {
                index += 1;
                *result
            }
            LineComponent::Comment(_) => return None,
        };
        let mut after_result = Vec::new();
        while let Some(component) = components.get(index) {
            match component {
                LineComponent::Comment(comment) => {
                    after_result.push(*comment);
                    index += 1;
                }
                LineComponent::Node(_) => break,
            }
        }
        let has_next_arm = components.get(index).is_some();
        let mut trailing = Vec::new();
        let mut next_leading = Vec::new();
        if has_next_arm {
            for comment in after_result {
                if has_line_break(source, result.span().end(), comment.span().start()) {
                    next_leading.push(comment);
                } else {
                    trailing.push(comment);
                }
            }
        } else {
            trailing = after_result;
        }
        arms.push(MatchArmLayout {
            leading,
            pattern,
            between,
            result,
            trailing,
        });
        pending = next_leading;
    }
    if arms.is_empty() {
        return None;
    }
    if let Some(last) = arms.last_mut() {
        last.trailing = pending;
    }
    Some(MatchLayout {
        head,
        scrutinee,
        arms,
    })
}

fn comment_text(node: &CstNode) -> String {
    normalize_leaf(node.leaf_text().unwrap_or_default())
        .trim_end()
        .to_owned()
}

fn normalize_leaf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn format_leaf(text: &str) -> String {
    match classify(text) {
        LiteralClassification::Literal(Literal::Character(character)) => {
            canonical_character_spelling(character.value())
        }
        LiteralClassification::Literal(_)
        | LiteralClassification::Invalid(_)
        | LiteralClassification::Opaque => text.to_owned(),
    }
}

fn normalize_recovery(source: &str) -> String {
    let normalized = source.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines = normalized
        .split('\n')
        .map(str::trim_end)
        .collect::<Vec<_>>();
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    if lines.is_empty() {
        return "\n".to_owned();
    }
    let mut result = lines.join("\n");
    result.push('\n');
    result
}

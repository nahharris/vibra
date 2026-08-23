//! Staged S-expression-native formatting and diagnostics.
//!
//! These APIs do not change the current CLI defaults. They provide a typed,
//! serde-free path for editor and CLI cutover work.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::ast::{
    self, Annotation, AnnotationKind, Expr, ExprKind, ImplItem, MacroExpr, MacroExprKind,
    MethodBinding, Module, Name, Pattern, PatternKind, TestMeta, TopLevel, TypeExpr, TypeExprKind,
    WasmArgument,
};
use crate::diagnostics::{
    file_uri, Category, Diagnostic, Fix, Position, RelatedDiagnostic, Severity, Span, TextEdit,
};
use crate::syntax::{self, Atom, Document, LineIndex, Node, NodeKind};

/// Parse and type-check S-expression source before canonical printing.
pub fn staged_format_sexpr(path: &Path, source: &str) -> Result<String, Diagnostic> {
    let document = syntax::parse(source).map_err(|error| syntax_diagnostic(path, source, error))?;
    let module = ast::lower_document_with_id(&document, ast::DocumentId::from_path(path))
        .map_err(|error| ast_diagnostic(path, source, error))?;
    let signatures = local_call_signatures(&module);
    if let Some(diagnostic) = call_order_diagnostics(path, source, &document, &signatures)
        .into_iter()
        .next()
    {
        return Err(diagnostic);
    }
    let formatted = syntax::print(&document);
    // The formatter is deliberately a layout-only pass. Reparse the result
    // and assert that its CST token tree is unchanged; this catches accidental
    // operand movement or comment loss while retaining the canonical spelling
    // normalization performed by the printer.
    assert_eq!(
        syntax_node_shape(&document),
        syntax_node_shape(&syntax::parse(&formatted).expect("canonical printer output parses"))
    );
    Ok(formatted)
}

fn syntax_node_shape(document: &Document) -> Vec<String> {
    fn visit(node: &Node, output: &mut Vec<String>) {
        match &node.kind {
            NodeKind::Atom(atom) => output.push(format!("atom:{atom:?}")),
            NodeKind::Comment(comment) => output.push(format!("comment:{comment}")),
            NodeKind::List(children) => {
                output.push("[".to_string());
                for child in children {
                    visit(child, output);
                }
                output.push("]".to_string());
            }
        }
    }

    let mut output = Vec::new();
    for node in &document.nodes {
        visit(node, &mut output);
    }
    output
}

/// Return reader or typed-surface diagnostics for an in-memory S-expression.
pub fn staged_sexpr_diagnostics(path: &Path, source: &str) -> Vec<Diagnostic> {
    let document = match syntax::parse(source) {
        Ok(document) => document,
        Err(error) => return vec![syntax_diagnostic(path, source, error)],
    };
    match ast::lower_document_with_id(&document, ast::DocumentId::from_path(path)) {
        Ok(module) => {
            let signatures = local_call_signatures(&module);
            call_order_diagnostics(path, source, &document, &signatures)
        }
        Err(error) => vec![ast_diagnostic(path, source, error)],
    }
}

/// Run typed kebab-case lint after reader and AST validation.
pub fn staged_lint_sexpr(path: &Path, source: &str) -> Vec<Diagnostic> {
    let document = match syntax::parse(source) {
        Ok(document) => document,
        Err(error) => return vec![syntax_diagnostic(path, source, error)],
    };
    let module = match ast::lower_document_with_id(&document, ast::DocumentId::from_path(path)) {
        Ok(module) => module,
        Err(error) => return vec![ast_diagnostic(path, source, error)],
    };
    let signatures = local_call_signatures(&module);
    let mut diagnostics = Vec::new();
    diagnostics.extend(call_order_diagnostics(path, source, &document, &signatures));
    lint_module_names(path, source, &module, &mut diagnostics);
    lint_labels(path, source, &document, &mut diagnostics);
    lint_redundant_do(path, source, &document, &mut diagnostics);
    diagnostics.sort_by_key(|diagnostic| {
        (
            diagnostic.span.start.offset.unwrap_or_default(),
            diagnostic.code.clone(),
        )
    });
    diagnostics
}

#[derive(Debug, Clone)]
struct CallSignature {
    parameter_names: Vec<String>,
    fixed_count: usize,
    variadic: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CallArgumentKind {
    Fixed,
    Labelled,
    Variadic,
}

#[derive(Debug, Clone)]
enum CallArgumentNodes {
    Positional { node: Node },
    Labelled { label: Node },
}

fn local_call_signatures(module: &Module) -> BTreeMap<String, CallSignature> {
    fn signature(function: &ast::Function) -> CallSignature {
        let variadic = function
            .parameters
            .last()
            .is_some_and(|parameter| parameter.variadic);
        CallSignature {
            parameter_names: function
                .parameters
                .iter()
                .map(|parameter| parameter.name.value.clone())
                .collect(),
            fixed_count: function
                .parameters
                .len()
                .saturating_sub(usize::from(variadic)),
            variadic,
        }
    }

    fn insert(
        signatures: &mut BTreeMap<String, CallSignature>,
        name: String,
        function: &ast::Function,
    ) {
        signatures.insert(name, signature(function));
    }

    let mut signatures = BTreeMap::new();
    for form in &module.forms {
        match form {
            TopLevel::Function(function) => {
                insert(&mut signatures, function.name.value.clone(), function);
            }
            TopLevel::Deffect(deffect) => {
                for operation in &deffect.operations {
                    insert(
                        &mut signatures,
                        format!("{}.{}", deffect.name.value, operation.function.name.value),
                        &operation.function,
                    );
                }
            }
            TopLevel::Definition(definition) => {
                for annotation in &definition.annotations {
                    if let AnnotationKind::Definitions(functions) = &annotation.value {
                        for function in functions {
                            insert(
                                &mut signatures,
                                format!("{}.{}", definition.name.value, function.name.value),
                                function,
                            );
                        }
                    }
                }
            }
            TopLevel::Macro(_)
            | TopLevel::Import(_)
            | TopLevel::Constant(_)
            | TopLevel::Test(_)
            | TopLevel::TestScenario(_) => {}
        }
    }
    signatures
}

fn parse_call_argument_nodes(children: &[Node]) -> Option<Vec<CallArgumentNodes>> {
    let semantic = children
        .iter()
        .filter(|node| !matches!(node.kind, NodeKind::Comment(_)))
        .collect::<Vec<_>>();
    if semantic.len() < 2 || symbol(semantic[0]).is_none() {
        return None;
    }
    let mut arguments = Vec::new();
    let mut index = 1;
    while index < semantic.len() {
        if matches!(semantic[index].kind, NodeKind::Atom(Atom::Label(_))) {
            semantic.get(index + 1)?;
            arguments.push(CallArgumentNodes::Labelled {
                label: semantic[index].clone(),
            });
            index += 2;
        } else {
            arguments.push(CallArgumentNodes::Positional {
                node: semantic[index].clone(),
            });
            index += 1;
        }
    }
    Some(arguments)
}

fn classify_call_arguments(
    arguments: &[CallArgumentNodes],
    signature: Option<&CallSignature>,
) -> Vec<CallArgumentKind> {
    let Some(signature) = signature else {
        let mut saw_label = false;
        return arguments
            .iter()
            .map(|argument| match argument {
                CallArgumentNodes::Labelled { .. } => {
                    saw_label = true;
                    CallArgumentKind::Labelled
                }
                CallArgumentNodes::Positional { .. } if saw_label => CallArgumentKind::Variadic,
                CallArgumentNodes::Positional { .. } => CallArgumentKind::Fixed,
            })
            .collect();
    };

    let mut reserved = vec![false; signature.fixed_count];
    for argument in arguments {
        if let CallArgumentNodes::Labelled { label, .. } = argument {
            if let Some(name) = label_name(label) {
                if let Some(index) = signature.parameter_names[..signature.fixed_count]
                    .iter()
                    .position(|parameter| parameter == name)
                {
                    reserved[index] = true;
                }
            }
        }
    }

    let mut next_fixed = 0;
    arguments
        .iter()
        .map(|argument| match argument {
            CallArgumentNodes::Labelled { .. } => CallArgumentKind::Labelled,
            CallArgumentNodes::Positional { .. } => {
                while next_fixed < signature.fixed_count && reserved[next_fixed] {
                    next_fixed += 1;
                }
                if next_fixed < signature.fixed_count {
                    next_fixed += 1;
                    CallArgumentKind::Fixed
                } else if signature.variadic {
                    CallArgumentKind::Variadic
                } else {
                    CallArgumentKind::Fixed
                }
            }
        })
        .collect()
}

fn call_order_diagnostics(
    path: &Path,
    source: &str,
    document: &Document,
    signatures: &BTreeMap<String, CallSignature>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    fn walk(
        path: &Path,
        source: &str,
        node: &Node,
        signatures: &BTreeMap<String, CallSignature>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let NodeKind::List(children) = &node.kind else {
            return;
        };
        for child in children {
            walk(path, source, child, signatures, diagnostics);
        }
        let Some(head) = children
            .iter()
            .find(|child| !matches!(child.kind, NodeKind::Comment(_)))
            .and_then(symbol)
        else {
            return;
        };
        if is_structural_form_head(head) {
            return;
        }
        let Some(arguments) = parse_call_argument_nodes(children) else {
            return;
        };
        let kinds = classify_call_arguments(&arguments, signatures.get(head));
        let mut saw_variadic = false;
        let mut saw_label = false;
        for (argument, kind) in arguments.iter().zip(kinds) {
            match (argument, kind) {
                (CallArgumentNodes::Labelled { label, .. }, CallArgumentKind::Labelled)
                    if saw_variadic =>
                {
                    let Some(name) = label_name(label) else {
                        continue;
                    };
                    diagnostics.push(diagnostic(
                        path,
                        source,
                        "E-SYN-013",
                        format!(
                            "labelled arguments should precede variadic arguments; move `{name}:` before the variadic values"
                        ),
                        Severity::Error,
                        Category::Syntax,
                        label.span,
                        None,
                    ));
                }
                (CallArgumentNodes::Positional { node }, CallArgumentKind::Fixed) if saw_label => {
                    diagnostics.push(diagnostic(
                        path,
                        source,
                        "E-SYN-013",
                        "fixed positional arguments must precede labelled or variadic arguments"
                            .to_string(),
                        Severity::Error,
                        Category::Syntax,
                        node.span,
                        None,
                    ));
                }
                (CallArgumentNodes::Labelled { .. }, CallArgumentKind::Labelled) => {
                    saw_label = true;
                }
                (_, CallArgumentKind::Variadic) => saw_variadic = true,
                _ => {}
            }
        }
    }

    for node in &document.nodes {
        walk(path, source, node, signatures, &mut diagnostics);
    }
    diagnostics
}

fn label_name(node: &Node) -> Option<&str> {
    match &node.kind {
        NodeKind::Atom(Atom::Label(value)) => Some(value),
        _ => None,
    }
}

fn symbol(node: &Node) -> Option<&str> {
    match &node.kind {
        NodeKind::Atom(Atom::Symbol(value)) => Some(value),
        _ => None,
    }
}

fn is_structural_form_head(head: &str) -> bool {
    matches!(
        head,
        "def"
            | "defn"
            | "deffect"
            | "dependency"
            | "do"
            | "embed"
            | "enum"
            | "fn-type"
            | "for"
            | "if"
            | "import"
            | "interface"
            | "join"
            | "let"
            | "let-as"
            | "map"
            | "macro"
            | "match"
            | "newtype"
            | "package"
            | "plugin-interface"
            | "project"
            | "range"
            | "record"
            | "ref"
            | "return"
            | "set"
            | "spawn"
            | "task"
            | "template"
            | "test"
            | "test.case"
            | "test.scenario"
            | "tuple"
            | "types"
            | "wasm"
            | "while"
    )
}

fn syntax_diagnostic(path: &Path, source: &str, error: syntax::SyntaxError) -> Diagnostic {
    diagnostic(
        path,
        source,
        error.code,
        error.message,
        Severity::Error,
        Category::Syntax,
        error.span,
        None,
    )
}

fn ast_diagnostic(path: &Path, source: &str, error: ast::AstError) -> Diagnostic {
    let related = (!error.related.is_empty()).then(|| {
        error
            .related
            .into_iter()
            .map(|(message, span)| RelatedDiagnostic {
                message,
                span: tooling_span(path, source, span),
            })
            .collect()
    });
    diagnostic(
        path,
        source,
        error.code,
        error.message,
        Severity::Error,
        Category::Syntax,
        error.span,
        related,
    )
}

fn diagnostic(
    path: &Path,
    source: &str,
    code: &str,
    message: String,
    severity: Severity,
    category: Category,
    span: syntax::Span,
    related: Option<Vec<RelatedDiagnostic>>,
) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        message,
        severity,
        span: tooling_span(path, source, span),
        related,
        fix: None,
        category,
    }
}

pub(crate) fn tooling_span(path: &Path, source: &str, span: syntax::Span) -> Span {
    let index = LineIndex::new(source);
    let start = index.position(source, span.start);
    let end = index.position(source, span.end);
    Span {
        uri: file_uri(path),
        start: Position {
            line: start.line - 1,
            column: utf16_column(source, start.offset),
            offset: Some(start.offset),
        },
        end: Position {
            line: end.line - 1,
            column: utf16_column(source, end.offset),
            offset: Some(end.offset),
        },
    }
}

fn utf16_column(source: &str, offset: usize) -> usize {
    let boundary = offset.min(source.len());
    let line_start = source[..boundary]
        .rfind('\n')
        .map_or(0, |newline| newline + 1);
    source[line_start..boundary].encode_utf16().count()
}

fn lint_module_names(
    path: &Path,
    source: &str,
    module: &Module,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for form in &module.forms {
        match form {
            TopLevel::Import(value) => {
                lint_name(path, source, &value.alias, "import alias", diagnostics)
            }
            TopLevel::Definition(value) => {
                lint_name(path, source, &value.name, "definition", diagnostics);
                lint_type(path, source, &value.body, diagnostics);
                lint_annotations(path, source, &value.annotations, diagnostics);
            }
            TopLevel::Constant(value) => {
                lint_name(path, source, &value.name, "constant", diagnostics);
                lint_type(path, source, &value.ty, diagnostics);
                lint_expr(path, source, &value.value, diagnostics);
                lint_annotations(path, source, &value.annotations, diagnostics);
            }
            TopLevel::Function(value) => lint_function(path, source, value, diagnostics),
            TopLevel::Deffect(value) => {
                lint_name(path, source, &value.name, "deffect root", diagnostics);
                for operation in &value.operations {
                    lint_function(path, source, &operation.function, diagnostics);
                }
            }
            TopLevel::Macro(value) => {
                lint_name(path, source, &value.name, "macro", diagnostics);
                for parameter in &value.parameters {
                    lint_name(
                        path,
                        source,
                        &parameter.name,
                        "macro parameter",
                        diagnostics,
                    );
                }
                for expression in &value.body {
                    lint_macro_expr(path, source, expression, diagnostics);
                }
                lint_annotations(path, source, &value.annotations, diagnostics);
            }
            TopLevel::Test(value) => {
                lint_name(path, source, &value.profile, "test profile", diagnostics);
                for metadata in &value.metadata {
                    match metadata {
                        TestMeta::Tags(names) => {
                            for name in names {
                                lint_name(path, source, name, "test tag", diagnostics);
                            }
                        }
                        TestMeta::ExpectError(_) => {}
                        TestMeta::Workspace(name) => {
                            lint_name(path, source, name, "workspace mode", diagnostics)
                        }
                        TestMeta::Clock { .. }
                        | TestMeta::TimeoutMillis(_)
                        | TestMeta::RandomSeed(_)
                        | TestMeta::Skip(_) => {}
                    }
                }
                for expression in &value.body {
                    lint_expr(path, source, expression, diagnostics);
                }
            }
            TopLevel::TestScenario(scenario) => {
                for value in &scenario.cases {
                    lint_name(path, source, &value.profile, "test profile", diagnostics);
                    for metadata in &value.metadata {
                        match metadata {
                            TestMeta::Tags(names) => {
                                for name in names {
                                    lint_name(path, source, name, "test tag", diagnostics);
                                }
                            }
                            TestMeta::ExpectError(_) => {}
                            TestMeta::Workspace(name) => {
                                lint_name(path, source, name, "workspace mode", diagnostics)
                            }
                            TestMeta::Clock { .. }
                            | TestMeta::TimeoutMillis(_)
                            | TestMeta::RandomSeed(_)
                            | TestMeta::Skip(_) => {}
                        }
                    }
                    for expression in &value.body {
                        lint_expr(path, source, expression, diagnostics);
                    }
                }
            }
        }
    }
}

fn lint_function(
    path: &Path,
    source: &str,
    function: &ast::Function,
    diagnostics: &mut Vec<Diagnostic>,
) {
    lint_name(path, source, &function.name, "function", diagnostics);
    for parameter in &function.parameters {
        lint_name(path, source, &parameter.name, "parameter", diagnostics);
        lint_type(path, source, &parameter.ty, diagnostics);
    }
    lint_type(path, source, &function.return_type, diagnostics);
    for expression in &function.body {
        lint_expr(path, source, expression, diagnostics);
    }
    lint_lifecycle_body(
        path,
        source,
        &function.parameters,
        &function.body,
        diagnostics,
    );
    lint_annotations(path, source, &function.annotations, diagnostics);
}

const HANDLE_USE_AFTER_CLOSE_CODE: &str = "W-HANDLE-001";
const HANDLE_DOUBLE_CLOSE_CODE: &str = "W-HANDLE-002";

type BindingId = (usize, usize);

fn binding_id(name: &Name) -> BindingId {
    (name.span.start, name.span.end)
}

#[derive(Debug, Clone, Default)]
struct LifecycleState {
    closed: BTreeMap<BindingId, syntax::Span>,
    borrowed: BTreeSet<BindingId>,
}

#[derive(Debug, Clone, Default)]
struct LifecycleScopes {
    scopes: Vec<BTreeMap<String, BindingId>>,
}

impl LifecycleScopes {
    fn push(&mut self) {
        self.scopes.push(BTreeMap::new());
    }

    fn pop(&mut self) {
        self.scopes.pop();
    }

    fn bind(&mut self, name: &Name) -> BindingId {
        let id = binding_id(name);
        if name.value != "_" {
            self.scopes
                .last_mut()
                .expect("lifecycle scope stack has a root")
                .insert(name.value.clone(), id);
        }
        id
    }

    fn resolve(&self, name: &str) -> Option<BindingId> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }
}

fn is_borrowed_standard_stream(expression: &Expr) -> bool {
    matches!(
        &expression.value,
        ExprKind::Call {
            callee,
            arguments,
            ..
        } if arguments.is_empty()
            && matches!(
                callee.value.as_str(),
                "io.stdin.open" | "io.stdout.open" | "io.stderr.open"
            )
    )
}

/// Check only direct binding uses in one function/control-flow body. This is
/// deliberately not an alias, aggregate, generic, task, or interprocedural
/// analysis.
fn lint_lifecycle_body(
    path: &Path,
    source: &str,
    parameters: &[ast::Parameter],
    body: &[Expr],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut scopes = LifecycleScopes::default();
    scopes.push();
    for parameter in parameters {
        scopes.bind(&parameter.name);
    }
    let _ = lint_lifecycle_exprs(
        path,
        source,
        body,
        LifecycleState::default(),
        &mut scopes,
        diagnostics,
    );
}

fn lint_lifecycle_exprs(
    path: &Path,
    source: &str,
    expressions: &[Expr],
    mut state: LifecycleState,
    scopes: &mut LifecycleScopes,
    diagnostics: &mut Vec<Diagnostic>,
) -> LifecycleState {
    for expression in expressions {
        state = lint_lifecycle_expr(path, source, expression, state, scopes, diagnostics);
    }
    state
}

fn lint_lifecycle_expr(
    path: &Path,
    source: &str,
    expression: &Expr,
    mut state: LifecycleState,
    scopes: &mut LifecycleScopes,
    diagnostics: &mut Vec<Diagnostic>,
) -> LifecycleState {
    match &expression.value {
        ExprKind::Literal(_) | ExprKind::Break | ExprKind::Continue => {}
        ExprKind::Reference(binding) => {
            if let Some(binding_id) = scopes.resolve(binding) {
                if let Some(close_span) = state.closed.get(&binding_id).copied() {
                    lifecycle_warning(
                        path,
                        source,
                        HANDLE_USE_AFTER_CLOSE_CODE,
                        format!("use of closed handle binding `{binding}`"),
                        expression.span,
                        binding,
                        close_span,
                        diagnostics,
                    );
                }
            }
        }
        ExprKind::Call {
            callee, arguments, ..
        } => {
            if callee.value == "stream.manage.close" {
                if let Some(argument) = arguments.first().map(|argument| argument.value()) {
                    if let ExprKind::Reference(binding) = &argument.value {
                        if let Some(binding_id) = scopes.resolve(binding) {
                            record_close(
                                path,
                                source,
                                expression.span,
                                binding_id,
                                binding,
                                &mut state,
                                diagnostics,
                            );
                        }
                        for argument in arguments.iter().skip(1) {
                            state = lint_lifecycle_expr(
                                path,
                                source,
                                argument.value(),
                                state,
                                scopes,
                                diagnostics,
                            );
                        }
                        return state;
                    }
                }
            }
            for argument in arguments {
                state =
                    lint_lifecycle_expr(path, source, argument.value(), state, scopes, diagnostics);
            }
        }
        ExprKind::AnonymousFunction { .. } => {}
        ExprKind::Do(values) => {
            state = lint_lifecycle_exprs(path, source, values, state, scopes, diagnostics);
        }
        ExprKind::Let { name, value, .. } => {
            state = lint_lifecycle_expr(path, source, value, state, scopes, diagnostics);
            let binding_id = scopes.bind(name);
            if is_borrowed_standard_stream(value) {
                state.borrowed.insert(binding_id);
            }
        }
        ExprKind::Set { name, value } => {
            state = lint_lifecycle_expr(path, source, value, state, scopes, diagnostics);
            if let Some(binding_id) = scopes.resolve(&name.value) {
                state.closed.remove(&binding_id);
                state.borrowed.remove(&binding_id);
            }
        }
        ExprKind::Return(value) => {
            if let Some(value) = value {
                state = lint_lifecycle_expr(path, source, value, state, scopes, diagnostics);
            }
        }
        ExprKind::Try(value) => {
            state = lint_lifecycle_expr(path, source, value, state, scopes, diagnostics);
        }
        ExprKind::If {
            condition,
            then_body,
            else_body,
        } => {
            let after_condition =
                lint_lifecycle_expr(path, source, condition, state, scopes, diagnostics);
            scopes.push();
            let then_state = lint_lifecycle_exprs(
                path,
                source,
                then_body,
                after_condition.clone(),
                scopes,
                diagnostics,
            );
            scopes.pop();
            scopes.push();
            let else_state = lint_lifecycle_exprs(
                path,
                source,
                else_body,
                after_condition,
                scopes,
                diagnostics,
            );
            scopes.pop();
            state = merge_lifecycle_states(then_state, else_state);
        }
        ExprKind::While { condition, body } => {
            state = lint_lifecycle_expr(path, source, condition, state, scopes, diagnostics);
            scopes.push();
            let _ = lint_lifecycle_exprs(path, source, body, state.clone(), scopes, diagnostics);
            scopes.pop();
        }
        ExprKind::For {
            binding,
            source: value,
            body,
        } => {
            state = lint_lifecycle_expr(path, source, value, state, scopes, diagnostics);
            scopes.push();
            scopes.bind(binding);
            let _ = lint_lifecycle_exprs(path, source, body, state.clone(), scopes, diagnostics);
            scopes.pop();
        }
        ExprKind::Match { target, cases } => {
            let after_target =
                lint_lifecycle_expr(path, source, target, state, scopes, diagnostics);
            let mut merged = None;
            for case in cases {
                scopes.push();
                bind_lifecycle_pattern(&case.pattern, scopes);
                let arm_state = lint_lifecycle_exprs(
                    path,
                    source,
                    &case.body,
                    after_target.clone(),
                    scopes,
                    diagnostics,
                );
                scopes.pop();
                merged = Some(match merged {
                    None => arm_state,
                    Some(previous) => merge_lifecycle_states(previous, arm_state),
                });
            }
            state = merged.unwrap_or(after_target);
        }
        ExprKind::Record(fields) => {
            for field in fields {
                state = lint_lifecycle_expr(path, source, &field.value, state, scopes, diagnostics);
            }
        }
        ExprKind::Tuple(values) | ExprKind::Array(values) => {
            for value in values {
                state = lint_lifecycle_expr(path, source, value, state, scopes, diagnostics);
            }
        }
        ExprKind::Map(entries) => {
            for (key, value) in entries {
                state = lint_lifecycle_expr(path, source, key, state, scopes, diagnostics);
                state = lint_lifecycle_expr(path, source, value, state, scopes, diagnostics);
            }
        }
        ExprKind::Mutable(value) | ExprKind::ReferenceOf(value) => {
            state = lint_lifecycle_expr(path, source, value, state, scopes, diagnostics);
        }
        ExprKind::Range(start, end, step) => {
            state = lint_lifecycle_expr(path, source, start, state, scopes, diagnostics);
            state = lint_lifecycle_expr(path, source, end, state, scopes, diagnostics);
            state = lint_lifecycle_expr(path, source, step, state, scopes, diagnostics);
        }
        ExprKind::Convert { value, .. } | ExprKind::Cast { value, .. } => {
            state = lint_lifecycle_expr(path, source, value, state, scopes, diagnostics);
        }
        ExprKind::Embed { .. } => {}
        ExprKind::Template { bindings, .. } => {
            for binding in bindings {
                state =
                    lint_lifecycle_expr(path, source, &binding.value, state, scopes, diagnostics);
            }
        }
        ExprKind::Intrinsic { name, arguments } => {
            if name.value == "fd-close" {
                if let Some(argument) = arguments.first() {
                    if let ExprKind::Reference(binding) = &argument.value {
                        if let Some(binding_id) = scopes.resolve(binding) {
                            record_close(
                                path,
                                source,
                                expression.span,
                                binding_id,
                                binding,
                                &mut state,
                                diagnostics,
                            );
                        }
                        for argument in arguments.iter().skip(1) {
                            state = lint_lifecycle_expr(
                                path,
                                source,
                                argument,
                                state,
                                scopes,
                                diagnostics,
                            );
                        }
                        return state;
                    }
                }
            }
            for argument in arguments {
                state = lint_lifecycle_expr(path, source, argument, state, scopes, diagnostics);
            }
        }
        ExprKind::Wasm { arguments, .. } => {
            for argument in arguments {
                if let WasmArgument::Expression(value) = argument {
                    state = lint_lifecycle_expr(path, source, value, state, scopes, diagnostics);
                }
            }
        }
        // A task or function is a separate computation. Do not carry lifecycle
        // state into it or infer anything about values it captures.
        ExprKind::Task { .. } | ExprKind::Spawn { .. } => {}
        ExprKind::Join { .. } => {}
    }
    state
}

fn bind_lifecycle_pattern(pattern: &Pattern, scopes: &mut LifecycleScopes) {
    match &pattern.value {
        PatternKind::Bind(name) => {
            scopes.bind(name);
        }
        PatternKind::Constructor { arguments, .. }
        | PatternKind::Tuple(arguments)
        | PatternKind::Array(arguments) => {
            for pattern in arguments {
                bind_lifecycle_pattern(pattern, scopes);
            }
        }
        PatternKind::Record(fields) => {
            for field in fields {
                bind_lifecycle_pattern(&field.pattern, scopes);
            }
        }
        PatternKind::Map(entries) => {
            for (key, value) in entries {
                bind_lifecycle_pattern(key, scopes);
                bind_lifecycle_pattern(value, scopes);
            }
        }
        PatternKind::Newtype { pattern, .. } | PatternKind::Interface { pattern, .. } => {
            bind_lifecycle_pattern(pattern, scopes);
        }
        PatternKind::Literal(_) | PatternKind::Wildcard => {}
    }
}

fn record_close(
    path: &Path,
    source: &str,
    close_span: syntax::Span,
    binding_id: BindingId,
    binding: &str,
    state: &mut LifecycleState,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if state.borrowed.contains(&binding_id) {
        return;
    }
    if let Some(previous_close_span) = state.closed.get(&binding_id).copied() {
        lifecycle_warning(
            path,
            source,
            HANDLE_DOUBLE_CLOSE_CODE,
            format!("handle binding `{binding}` is closed more than once"),
            close_span,
            binding,
            previous_close_span,
            diagnostics,
        );
    } else {
        state.closed.insert(binding_id, close_span);
    }
}

fn merge_lifecycle_states(left: LifecycleState, right: LifecycleState) -> LifecycleState {
    let closed = left
        .closed
        .into_iter()
        .filter(|(binding, _)| right.closed.contains_key(binding))
        .collect();
    let borrowed = left
        .borrowed
        .into_iter()
        .filter(|binding| right.borrowed.contains(binding))
        .collect();
    LifecycleState { closed, borrowed }
}

fn lifecycle_warning(
    path: &Path,
    source: &str,
    code: &str,
    message: String,
    span: syntax::Span,
    binding: &str,
    close_span: syntax::Span,
    diagnostics: &mut Vec<Diagnostic>,
) {
    diagnostics.push(diagnostic(
        path,
        source,
        code,
        message,
        Severity::Warning,
        Category::Style,
        span,
        Some(vec![RelatedDiagnostic {
            message: format!("handle binding `{binding}` was closed here"),
            span: tooling_span(path, source, close_span),
        }]),
    ));
}

fn lint_annotations(
    path: &Path,
    source: &str,
    annotations: &[Annotation],
    diagnostics: &mut Vec<Diagnostic>,
) {
    for annotation in annotations {
        match &annotation.value {
            AnnotationKind::Doc(_) => {}
            AnnotationKind::Effects(row) => {
                for label in &row.labels {
                    lint_type(path, source, label, diagnostics);
                }
            }
            AnnotationKind::Where(parameters) => {
                for parameter in parameters {
                    lint_name(path, source, &parameter.name, "type parameter", diagnostics);
                    for bound in &parameter.bounds {
                        lint_type(path, source, bound, diagnostics);
                    }
                }
            }
            AnnotationKind::Definitions(functions) => {
                for function in functions {
                    lint_function(path, source, function, diagnostics);
                }
            }
            AnnotationKind::Implementation { interface, items } => {
                lint_type(path, source, interface, diagnostics);
                for item in items {
                    match item {
                        ImplItem::Types(types) => {
                            for ty in types {
                                lint_type(path, source, ty, diagnostics);
                            }
                        }
                        ImplItem::Method { name, binding, .. } => {
                            lint_name(path, source, name, "implementation method", diagnostics);
                            match binding {
                                MethodBinding::Reference(name) => {
                                    lint_name(path, source, name, "method reference", diagnostics)
                                }
                                MethodBinding::Function(function) => {
                                    lint_function(path, source, function, diagnostics)
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn lint_type(path: &Path, source: &str, ty: &TypeExpr, diagnostics: &mut Vec<Diagnostic>) {
    match &ty.value {
        TypeExprKind::Named(name) => {
            lint_spanned_value(path, source, name, ty.span, "type reference", diagnostics)
        }
        TypeExprKind::Application {
            constructor,
            arguments,
        } => {
            lint_name(path, source, constructor, "type constructor", diagnostics);
            for argument in arguments {
                lint_type(path, source, argument, diagnostics);
            }
        }
        TypeExprKind::Record(members)
        | TypeExprKind::Enum(members)
        | TypeExprKind::Interface(members) => {
            for member in members {
                lint_name(path, source, &member.name, "type member", diagnostics);
                lint_type(path, source, &member.ty, diagnostics);
            }
        }
        TypeExprKind::Tuple(types)
        | TypeExprKind::Union(types)
        | TypeExprKind::Intersect(types) => {
            for ty in types {
                lint_type(path, source, ty, diagnostics);
            }
        }
        TypeExprKind::Array(ty)
        | TypeExprKind::Newtype(ty)
        | TypeExprKind::Mutable(ty)
        | TypeExprKind::Reference(ty)
        | TypeExprKind::MutableReference(ty) => lint_type(path, source, ty, diagnostics),
        TypeExprKind::Map(key, value) => {
            lint_type(path, source, key, diagnostics);
            lint_type(path, source, value, diagnostics);
        }
        TypeExprKind::Function {
            parameters, result, ..
        } => {
            for parameter in parameters {
                lint_name(
                    path,
                    source,
                    &parameter.name,
                    "fn-type parameter",
                    diagnostics,
                );
                lint_type(path, source, &parameter.ty, diagnostics);
            }
            lint_type(path, source, result, diagnostics);
        }
        TypeExprKind::WasmValue(name) => {
            lint_name(path, source, name, "wasm value type", diagnostics)
        }
        TypeExprKind::Handle(_) | TypeExprKind::Literal(_) => {}
    }
}

fn lint_expr(path: &Path, source: &str, expr: &Expr, diagnostics: &mut Vec<Diagnostic>) {
    match &expr.value {
        ExprKind::Literal(_) | ExprKind::Break | ExprKind::Continue => {}
        ExprKind::Reference(name) => {
            lint_spanned_value(path, source, name, expr.span, "reference", diagnostics)
        }
        ExprKind::Call {
            callee, arguments, ..
        } => {
            lint_name(path, source, callee, "callee", diagnostics);
            for argument in arguments {
                lint_expr(path, source, argument.value(), diagnostics);
            }
        }
        ExprKind::Intrinsic { arguments, .. } => {
            lint_exprs(path, source, arguments, diagnostics);
        }
        ExprKind::AnonymousFunction {
            parameters,
            return_type,
            body,
        } => {
            for parameter in parameters {
                lint_name(
                    path,
                    source,
                    &parameter.name,
                    "function parameter",
                    diagnostics,
                );
                lint_type(path, source, &parameter.ty, diagnostics);
            }
            lint_type(path, source, return_type, diagnostics);
            lint_exprs(path, source, body, diagnostics);
        }
        ExprKind::Do(values) | ExprKind::Tuple(values) | ExprKind::Array(values) => {
            lint_exprs(path, source, values, diagnostics)
        }
        ExprKind::Let { name, ty, value } => {
            lint_name(path, source, name, "binding", diagnostics);
            if let Some(ty) = ty {
                lint_type(path, source, ty, diagnostics);
            }
            lint_expr(path, source, value, diagnostics);
        }
        ExprKind::Set { name, value } => {
            lint_name(path, source, name, "binding", diagnostics);
            lint_expr(path, source, value, diagnostics);
        }
        ExprKind::Return(value) => {
            if let Some(value) = value {
                lint_expr(path, source, value, diagnostics);
            }
        }
        ExprKind::Try(value) => lint_expr(path, source, value, diagnostics),
        ExprKind::If {
            condition,
            then_body,
            else_body,
        } => {
            lint_expr(path, source, condition, diagnostics);
            lint_exprs(path, source, then_body, diagnostics);
            lint_exprs(path, source, else_body, diagnostics);
        }
        ExprKind::While { condition, body } => {
            lint_expr(path, source, condition, diagnostics);
            lint_exprs(path, source, body, diagnostics);
        }
        ExprKind::For {
            binding,
            source: value,
            body,
        } => {
            lint_name(path, source, binding, "loop binding", diagnostics);
            lint_expr(path, source, value, diagnostics);
            lint_exprs(path, source, body, diagnostics);
        }
        ExprKind::Match { target, cases } => {
            lint_expr(path, source, target, diagnostics);
            for case in cases {
                lint_pattern(path, source, &case.pattern, diagnostics);
                lint_exprs(path, source, &case.body, diagnostics);
            }
        }
        ExprKind::Record(fields) => {
            for field in fields {
                lint_name(path, source, &field.name, "record field", diagnostics);
                lint_expr(path, source, &field.value, diagnostics);
            }
        }
        ExprKind::Map(entries) => {
            for (key, value) in entries {
                lint_expr(path, source, key, diagnostics);
                lint_expr(path, source, value, diagnostics);
            }
        }
        ExprKind::Mutable(value) | ExprKind::ReferenceOf(value) => {
            lint_expr(path, source, value, diagnostics)
        }
        ExprKind::Range(start, end, step) => {
            lint_expr(path, source, start, diagnostics);
            lint_expr(path, source, end, diagnostics);
            lint_expr(path, source, step, diagnostics);
        }
        ExprKind::Convert { value, into, .. } | ExprKind::Cast { value, into } => {
            lint_expr(path, source, value, diagnostics);
            lint_type(path, source, into, diagnostics);
        }
        ExprKind::Embed { .. } | ExprKind::Wasm { .. } => {}
        ExprKind::Template { bindings, .. } => {
            for binding in bindings {
                lint_name(path, source, &binding.name, "template binding", diagnostics);
                lint_expr(path, source, &binding.value, diagnostics);
            }
        }
        ExprKind::Task { captures, body } => {
            lint_names(path, source, captures, "task capture", diagnostics);
            lint_exprs(path, source, body, diagnostics);
        }
        ExprKind::Spawn {
            handle,
            captures,
            value,
        } => {
            lint_name(path, source, handle, "task handle", diagnostics);
            lint_names(path, source, captures, "task capture", diagnostics);
            lint_expr(path, source, value, diagnostics);
        }
        ExprKind::Join { handle, binding } => {
            lint_name(path, source, handle, "task handle", diagnostics);
            lint_name(path, source, binding, "task binding", diagnostics);
        }
    }
}

fn lint_exprs(path: &Path, source: &str, values: &[Expr], diagnostics: &mut Vec<Diagnostic>) {
    for value in values {
        lint_expr(path, source, value, diagnostics);
    }
}

fn lint_pattern(path: &Path, source: &str, pattern: &Pattern, diagnostics: &mut Vec<Diagnostic>) {
    match &pattern.value {
        PatternKind::Literal(_) | PatternKind::Wildcard => {}
        PatternKind::Bind(name) => lint_name(path, source, name, "pattern binding", diagnostics),
        PatternKind::Constructor {
            constructor,
            arguments,
        } => {
            lint_name(
                path,
                source,
                constructor,
                "pattern constructor",
                diagnostics,
            );
            for argument in arguments {
                lint_pattern(path, source, argument, diagnostics);
            }
        }
        PatternKind::Record(fields) => {
            for field in fields {
                lint_name(path, source, &field.name, "pattern field", diagnostics);
                lint_pattern(path, source, &field.pattern, diagnostics);
            }
        }
        PatternKind::Tuple(patterns) | PatternKind::Array(patterns) => {
            for pattern in patterns {
                lint_pattern(path, source, pattern, diagnostics);
            }
        }
        PatternKind::Map(entries) => {
            for (key, value) in entries {
                lint_pattern(path, source, key, diagnostics);
                lint_pattern(path, source, value, diagnostics);
            }
        }
        PatternKind::Newtype { ty, pattern } | PatternKind::Interface { ty, pattern } => {
            lint_type(path, source, ty, diagnostics);
            lint_pattern(path, source, pattern, diagnostics);
        }
    }
}

fn lint_macro_expr(
    path: &Path,
    source: &str,
    expression: &MacroExpr,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match &expression.value {
        MacroExprKind::Atom(_) | MacroExprKind::Quote { .. } => {}
        MacroExprKind::Let { name, value } => {
            lint_name(path, source, name, "macro binding", diagnostics);
            lint_macro_expr(path, source, value, diagnostics);
        }
        MacroExprKind::If {
            condition,
            then_body,
            else_body,
        } => {
            lint_macro_expr(path, source, condition, diagnostics);
            for value in then_body.iter().chain(else_body) {
                lint_macro_expr(path, source, value, diagnostics);
            }
        }
        MacroExprKind::Unquote(name)
        | MacroExprKind::Splice(name)
        | MacroExprKind::Capture(name) => {
            lint_name(path, source, name, "macro binding", diagnostics)
        }
    }
}

fn lint_names(
    path: &Path,
    source: &str,
    names: &[Name],
    role: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for name in names {
        lint_name(path, source, name, role, diagnostics);
    }
}

fn lint_name(
    path: &Path,
    source: &str,
    name: &Name,
    role: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if (name.value == "_"
        && matches!(
            role,
            "binding" | "loop binding" | "pattern binding" | "task binding"
        ))
        || is_kebab_qualified(&name.value)
    {
        return;
    }
    diagnostics.push(diagnostic(
        path,
        source,
        "W-STYLE-001",
        format!(
            "non-kebab-case {role}: `{}` (recommended: kebab-case)",
            name.value
        ),
        Severity::Warning,
        Category::Style,
        name.span,
        None,
    ));
}

fn lint_spanned_value(
    path: &Path,
    source: &str,
    value: &str,
    span: syntax::Span,
    role: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if is_kebab_qualified(value) {
        return;
    }
    diagnostics.push(diagnostic(
        path,
        source,
        "W-STYLE-001",
        format!("non-kebab-case {role}: `{value}` (recommended: kebab-case)"),
        Severity::Warning,
        Category::Style,
        span,
        None,
    ));
}

fn lint_labels(path: &Path, source: &str, document: &Document, diagnostics: &mut Vec<Diagnostic>) {
    fn walk(path: &Path, source: &str, nodes: &[Node], diagnostics: &mut Vec<Diagnostic>) {
        for node in nodes {
            match &node.kind {
                NodeKind::List(children) => walk(path, source, children, diagnostics),
                NodeKind::Atom(Atom::Label(label)) if !is_kebab(label) => {
                    diagnostics.push(diagnostic(
                        path,
                        source,
                        "W-STYLE-001",
                        format!("non-kebab-case label: `{label}:` (recommended: kebab-case)"),
                        Severity::Warning,
                        Category::Style,
                        node.span,
                        None,
                    ))
                }
                _ => {}
            }
        }
    }
    walk(path, source, &document.nodes, diagnostics);
}

/// Find sequencing blocks that add no semantics at their position.  Body
/// owners accept a single outer `(do ...)` as a convenience, but the wrapper
/// is redundant once the body is represented as a list of expressions.  In
/// ordinary expression positions only a one-expression do is redundant;
/// empty and multi-expression blocks remain meaningful there.
fn lint_redundant_do(
    path: &Path,
    source: &str,
    document: &Document,
    diagnostics: &mut Vec<Diagnostic>,
) {
    fn semantic<'a>(children: &'a [Node]) -> Vec<&'a Node> {
        children
            .iter()
            .filter(|node| !matches!(node.kind, NodeKind::Comment(_)))
            .collect()
    }

    fn head(node: &Node) -> Option<&str> {
        let NodeKind::List(children) = &node.kind else {
            return None;
        };
        semantic(children).first().and_then(|node| symbol(node))
    }

    fn body_owned_child(parent: &Node, candidate: &Node) -> bool {
        let NodeKind::List(children) = &parent.kind else {
            return false;
        };
        let values = semantic(children);
        let Some(form) = values.first().and_then(|node| symbol(node)) else {
            return false;
        };
        let fixed = match form {
            "defn" | "macro" | "fn" => 3,
            "test.case" | "while" => 1,
            "for" => 2,
            "task" => 0,
            _ => return false,
        };
        // `fn` has two fixed operands (parameters and return type), unlike
        // the declaration forms.  The match arm above keeps the policy table
        // together while this adjustment mirrors the AST parser exactly.
        let fixed = if form == "fn" { 2 } else { fixed };
        let mut index = 1 + fixed;
        while index + 1 < values.len()
            && matches!(values[index].kind, NodeKind::Atom(Atom::Label(_)))
        {
            index += 2;
        }
        let Some([body]) = values.get(index..).map(|remainder| remainder) else {
            return false;
        };
        body.span == candidate.span && head(body) == Some("do")
    }

    fn fix_for_do(path: &Path, source: &str, node: &Node) -> Fix {
        let NodeKind::List(children) = &node.kind else {
            unreachable!("redundant do must be a list")
        };
        let head = children
            .iter()
            .find(|child| !matches!(child.kind, NodeKind::Comment(_)))
            .expect("redundant do has a semantic head");
        let opening = syntax::Span::new(node.span.start, node.span.start.saturating_add(1));
        let closing_start = node.span.end.saturating_sub(1);
        Fix::safe(
            "Remove redundant do block",
            vec![
                TextEdit {
                    span: tooling_span(path, source, opening),
                    replacement: String::new(),
                },
                TextEdit {
                    span: tooling_span(path, source, head.span),
                    replacement: String::new(),
                },
                TextEdit {
                    span: tooling_span(
                        path,
                        source,
                        syntax::Span::new(closing_start, node.span.end),
                    ),
                    replacement: String::new(),
                },
            ],
        )
    }

    fn walk(
        path: &Path,
        source: &str,
        node: &Node,
        parent: Option<&Node>,
        diagnostics: &mut Vec<Diagnostic>,
        seen: &mut BTreeSet<(usize, usize)>,
    ) {
        let NodeKind::List(children) = &node.kind else {
            return;
        };
        let values = semantic(children);
        let is_do = values.first().and_then(|node| symbol(node)) == Some("do");
        let semantic_count = values.len().saturating_sub(1);
        let redundant = is_do
            && (semantic_count == 1 || parent.is_some_and(|parent| body_owned_child(parent, node)));
        if redundant && seen.insert((node.span.start, node.span.end)) {
            diagnostics.push(Diagnostic {
                code: "W-STYLE-003".to_string(),
                message: "redundant `do` sequencing block; remove the wrapper".to_string(),
                severity: Severity::Warning,
                span: tooling_span(path, source, node.span),
                related: None,
                fix: Some(vec![fix_for_do(path, source, node)]),
                category: Category::Style,
            });
        }
        for child in children {
            walk(path, source, child, Some(node), diagnostics, seen);
        }
    }

    let mut seen = BTreeSet::new();
    for node in &document.nodes {
        walk(path, source, node, None, diagnostics, &mut seen);
    }
}

fn is_kebab_qualified(name: &str) -> bool {
    name.split('.').all(is_kebab)
}

fn is_kebab(name: &str) -> bool {
    if name.is_empty() || name.starts_with('-') || name.ends_with('-') || name.contains("--") {
        return false;
    }
    name.chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_rejects_labels_after_variadic_arguments() {
        let source =
            "(defn target (first int64 second int64 label-1 int64 rest... int64) void (do unit))\n\
(defn caller () void (do (target 1 2 3 label-1: 4 label-2: 5)))\n";
        let diagnostics = staged_sexpr_diagnostics(Path::new("call.vib"), source);
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "E-SYN-013"));
    }

    #[test]
    fn formatter_preserves_already_canonical_argument_order() {
        let source = "(defn target (first int64 second int64 label-1 int64 label-2 int64 rest... int64) void (do unit))\n\
(defn caller () void (do (target 1 2 label-1: 4 label-2: 5 3)))\n";
        let formatted = staged_format_sexpr(Path::new("call.vib"), source).unwrap();
        assert!(formatted.contains("(target 1 2 label-1: 4 label-2: 5 3)"));
        assert_eq!(
            formatted,
            staged_format_sexpr(Path::new("call.vib"), &formatted).unwrap()
        );
    }

    #[test]
    fn linter_reports_redundant_single_item_do_with_a_fix() {
        let source = "(defn main () void (if true (do (return unit)) (do unit)))\n";
        let diagnostics = staged_lint_sexpr(Path::new("do.vib"), source);
        let redundant = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == "W-STYLE-003")
            .expect("single-item do should be diagnosed");
        assert!(
            redundant.fix.is_some(),
            "diagnostic should carry a safe edit"
        );
    }

    #[test]
    fn body_owned_do_is_redundant_even_when_empty_or_multi_item() {
        for source in [
            "(defn empty () void (do))\n",
            "(defn multi () void (do (let _ true) (return unit)))\n",
        ] {
            let diagnostics = staged_lint_sexpr(Path::new("do.vib"), source);
            assert!(diagnostics.iter().any(|diagnostic| {
                diagnostic.code == "W-STYLE-003" && diagnostic.fix.is_some()
            }));
        }
        for source in ["(if true (do) (do (unit) (unit)))\n"] {
            assert!(staged_lint_sexpr(Path::new("do.vib"), source)
                .iter()
                .all(|diagnostic| diagnostic.code != "W-STYLE-003"));
        }
    }

    #[test]
    fn linter_reports_when_labelled_argument_follows_variadic_argument() {
        let source =
            "(defn target (first int64 second int64 label-1 int64 rest... int64) void (do unit))\n\
(defn caller () void (do (target 1 2 3 label-1: 4)))\n";
        let diagnostics = staged_lint_sexpr(Path::new("call.vib"), source);
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "E-SYN-013" && diagnostic.message.contains("labelled arguments")
        }));
    }

    #[test]
    fn call_order_diagnostics_ignore_reader_comments_without_reordering() {
        let source = "(defn target (first int64 label-1 int64 rest... int64) void (do unit))\n\
(defn caller () void (do (target 1 2 ; keep this note\n label-1: 3)))\n";
        let diagnostics = staged_sexpr_diagnostics(Path::new("call.vib"), source);
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "E-SYN-013"
                && diagnostic.span.start.offset == Some(source.find("label-1:").unwrap())
        }));
    }

    #[test]
    fn formatting_is_typed_idempotent_and_preserves_comments_and_labels() {
        let path = Path::new("src/main.vib");
        let source = "; lead\n(defn hello-world (name str) str doc: \"Greets\"\n(do ; body\n(return name)))\n";
        let once = staged_format_sexpr(path, source).unwrap();
        let twice = staged_format_sexpr(path, &once).unwrap();
        assert_eq!(once, twice);
        assert!(once.contains("; lead"));
        assert!(once.contains("; body"));
        assert!(once.contains("doc: \"Greets\""));
    }

    #[test]
    fn unicode_byte_spans_map_to_zero_based_lsp_columns() {
        let source = "; λ\n(defn valid () void (do unit))\n)\n";
        let diagnostics = staged_sexpr_diagnostics(Path::new("unicode.vib"), source);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "E-SYN-005");
        assert_eq!(diagnostics[0].span.start.line, 2);
        assert_eq!(diagnostics[0].span.start.column, 0);
        assert_eq!(
            diagnostics[0].span.start.offset,
            Some(source.rfind(')').unwrap())
        );
    }

    #[test]
    fn ast_errors_use_precise_unicode_ranges() {
        let source = "; λ\n(defn bad () void mystery: true (do unit))\n";
        let diagnostics = staged_sexpr_diagnostics(Path::new("unicode.vib"), source);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "E-SYN-011");
        assert_eq!(diagnostics[0].span.start.line, 1);
        assert!(diagnostics[0].span.start.column > 0);
        assert_eq!(
            diagnostics[0].span.start.offset,
            Some(source.find("mystery:").unwrap())
        );
    }

    #[test]
    fn diagnostic_columns_use_utf16_code_units() {
        let source = "(defn valid () void doc: \"😀\" mystery: true (do unit))\n";
        let diagnostics = staged_sexpr_diagnostics(Path::new("emoji.vib"), source);
        assert_eq!(diagnostics.len(), 1);
        let offset = source.find("mystery:").unwrap();
        assert_eq!(diagnostics[0].span.start.offset, Some(offset));
        assert_eq!(
            diagnostics[0].span.start.column,
            source[..offset].encode_utf16().count()
        );
        assert_eq!(
            diagnostics[0].span.start.column,
            source[..offset].chars().count() + 1
        );
    }

    #[test]
    fn typed_lint_visits_nested_names() {
        let source = "(defn Bad_Name (BadArg int64) int64 (let Local_Value BadArg) Local_Value)\n";
        let diagnostics = staged_lint_sexpr(Path::new("lint.vib"), source);
        assert!(diagnostics
            .iter()
            .any(|value| value.message.contains("Bad_Name")));
        assert!(diagnostics
            .iter()
            .any(|value| value.message.contains("BadArg")));
        assert!(diagnostics
            .iter()
            .any(|value| value.message.contains("Local_Value")));
        assert!(diagnostics.iter().all(|value| value.code == "W-STYLE-001"));
    }

    #[test]
    fn typed_lint_visits_mutable_reference_inner_types() {
        let source = "(defn borrow (value (mut-ref Bad_Type)) void (do unit))\n";
        let diagnostics = staged_lint_sexpr(Path::new("lint.vib"), source);
        assert!(diagnostics
            .iter()
            .any(|value| value.message.contains("Bad_Type")));
    }

    #[test]
    fn formatter_rejects_typed_invalid_source() {
        let error =
            staged_format_sexpr(Path::new("bad.vib"), "(defn bad () void nope: 1 (do unit))")
                .unwrap_err();
        assert_eq!(error.code, "E-SYN-011");
    }

    #[test]
    fn expect_error_codes_are_not_kebab_linted() {
        let source = "(test.scenario \"fails\" (test.case \"fails\" expect-error: (@compile E-OP-002 \"overflow\") unit))\n";
        assert!(staged_lint_sexpr(Path::new("test.vib"), source).is_empty());
    }

    #[test]
    fn linter_warns_for_same_binding_use_after_close() {
        let source = "(defn close-and-use (resource (handle @any)) void\n\
                      (do (stream.manage.close resource)\n\
                          (stream.write.string resource \"late\")))\n";

        let diagnostics = staged_lint_sexpr(Path::new("handle.vib"), source);

        let warning = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == "W-HANDLE-001")
            .expect("same-binding use after close should warn");
        assert!(warning.message.contains("resource"));
        assert_eq!(warning.severity, Severity::Warning);
        let related = warning
            .related
            .as_ref()
            .expect("warning should identify the closing site");
        assert_eq!(related.len(), 1);
        assert!(related[0].message.contains("closed here"));
        assert_eq!(
            related[0].span.start.offset,
            Some(source.find("(stream.manage.close").unwrap())
        );
    }

    #[test]
    fn linter_warns_for_same_binding_double_close() {
        let source = "(defn double-close (resource (handle @any)) void\n\
                      (do (stream.manage.close resource)\n\
                          (stream.manage.close resource)))\n";

        let diagnostics = staged_lint_sexpr(Path::new("handle.vib"), source);

        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "W-HANDLE-002"));
    }

    #[test]
    fn lifecycle_lint_is_path_local_and_does_not_follow_aliases_or_tasks() {
        let source = "(defn path-local (resource (handle @any)) void\n\
                      (do\n\
                        (let alias resource)\n\
                        (if true\n\
                            (do (stream.manage.close resource)\n\
                                (stream.write.string resource \"late\"))\n\
                            (do (stream.write.string resource \"live\")))\n\
                        (stream.write.string resource \"after\")\n\
                        (task\n\
                          captures: (resource)\n\
                          (do (stream.manage.close resource)\n\
                              (stream.write.string resource \"task\")))\n\
                        (stream.write.string alias \"alias\")))\n";

        let diagnostics = staged_lint_sexpr(Path::new("handle.vib"), source);

        assert_eq!(
            diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == "W-HANDLE-001")
                .count(),
            1,
            "only the use on the closing branch should warn: {diagnostics:?}"
        );
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "W-HANDLE-002"));
    }

    #[test]
    fn lifecycle_lint_resolves_sibling_match_bindings_independently() {
        let source = "(defn sibling-match (input any) void\n\
                      (do\n\
                        (match input\n\
                          (left (bind resource)) (do (stream.manage.close resource))\n\
                          (right (bind resource)) (do (stream.manage.close resource)))\n\
                        (match input\n\
                          (left (bind resource)) (do (stream.write.string resource \"live\"))\n\
                          (right (bind resource)) (do (stream.write.string resource \"live\")))))\n";

        let diagnostics = staged_lint_sexpr(Path::new("handle.vib"), source);

        assert!(
            diagnostics.iter().all(|diagnostic| !matches!(
                diagnostic.code.as_str(),
                "W-HANDLE-001" | "W-HANDLE-002"
            )),
            "sibling match bindings must not share lifecycle state: {diagnostics:?}"
        );
    }

    #[test]
    fn lifecycle_lint_clears_closed_state_on_reassignment() {
        let source = "(defn reassign (resource (mut (handle @any))) void\n\
                      (do\n\
                        (stream.manage.close resource)\n\
                        (set resource (fresh-handle))\n\
                        (stream.write.string resource \"fresh\")))\n";

        let diagnostics = staged_lint_sexpr(Path::new("handle.vib"), source);

        assert!(
            diagnostics.iter().all(|diagnostic| !matches!(
                diagnostic.code.as_str(),
                "W-HANDLE-001" | "W-HANDLE-002"
            )),
            "a fresh assignment must reopen the direct binding: {diagnostics:?}"
        );
    }

    #[test]
    fn lifecycle_lint_does_not_revoke_borrowed_standard_streams() {
        let source = "(defn borrowed-stream () void\n\
                      (do\n\
                        (let resource (io.stdout.open))\n\
                        (stream.manage.close resource)\n\
                        (stream.manage.close resource)\n\
                        (stream.write.string resource \"still live\")))\n";

        let diagnostics = staged_lint_sexpr(Path::new("handle.vib"), source);

        assert!(
            diagnostics.iter().all(|diagnostic| !matches!(
                diagnostic.code.as_str(),
                "W-HANDLE-001" | "W-HANDLE-002"
            )),
            "borrowed standard streams are not revocable: {diagnostics:?}"
        );
    }

    #[test]
    fn lifecycle_lint_does_not_follow_aggregates_generics_or_calls() {
        let source = "(defn closes (resource (handle @any)) void\n\
                      (do (stream.manage.close resource)))\n\
                    (defn
                      generic-closes
                      (resource (handle @any) marker t)
                      void\n\
                      where:
                      (t any)
                      (do (stream.manage.close resource)))\n\
                    (defn caller (resource (handle @any)) void\n\
                      (do\n\
                        (let boxed (record (handle resource)))\n\
                        (let tupled (tuple resource))\n\
                        (stream.manage.close boxed)\n\
                        (closes resource)\n\
                        (generic-closes resource 1)\n\
                        (task\n\
                          captures: (resource)\n\
                          (do (stream.manage.close resource)\n\
                              (stream.write.string resource \"task\")))\n\
                        (stream.write.string resource \"live\")))\n";

        let diagnostics = staged_lint_sexpr(Path::new("handle.vib"), source);

        assert!(
            diagnostics.iter().all(|diagnostic| !matches!(
                diagnostic.code.as_str(),
                "W-HANDLE-001" | "W-HANDLE-002"
            )),
            "lifecycle lint must remain local to direct bindings: {diagnostics:?}"
        );
    }

    #[test]
    fn lifecycle_warning_codes_are_registered_as_warnings() {
        let schema: serde_json::Value =
            serde_json::from_str(include_str!("../schemas/linter-codes.json")).unwrap();
        let codes = schema["codes"].as_array().unwrap();

        for code in ["W-HANDLE-001", "W-HANDLE-002"] {
            let entry = codes
                .iter()
                .find(|entry| entry["code"] == code)
                .unwrap_or_else(|| panic!("{code} must be registered"));
            assert_eq!(entry["severity"], "warning");
        }
    }
}

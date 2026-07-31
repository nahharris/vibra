//! Staged S-expression-native formatting and diagnostics.
//!
//! These APIs do not change the current CLI defaults. They provide a typed,
//! serde-free path for editor and CLI cutover work.

use std::path::Path;

use crate::ast::{
    self, Annotation, AnnotationKind, Expr, ExprKind, ImplItem, MacroExpr, MacroExprKind,
    MethodBinding, Module, Name, Pattern, PatternKind, TestMeta, TopLevel, TypeExpr, TypeExprKind,
};
use crate::diagnostics::{
    file_uri, Category, Diagnostic, Position, RelatedDiagnostic, Severity, Span,
};
use crate::syntax::{self, Atom, Document, LineIndex, Node, NodeKind};

/// Parse and type-check S-expression source before canonical printing.
pub fn staged_format_sexpr(path: &Path, source: &str) -> Result<String, Diagnostic> {
    let document = syntax::parse(source).map_err(|error| syntax_diagnostic(path, source, error))?;
    ast::lower_document_with_id(&document, ast::DocumentId::from_path(path))
        .map_err(|error| ast_diagnostic(path, source, error))?;
    Ok(syntax::print(&document))
}

/// Return reader or typed-surface diagnostics for an in-memory S-expression.
pub fn staged_sexpr_diagnostics(path: &Path, source: &str) -> Vec<Diagnostic> {
    let document = match syntax::parse(source) {
        Ok(document) => document,
        Err(error) => return vec![syntax_diagnostic(path, source, error)],
    };
    match ast::lower_document_with_id(&document, ast::DocumentId::from_path(path)) {
        Ok(_) => Vec::new(),
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
    let mut diagnostics = Vec::new();
    lint_module_names(path, source, &module, &mut diagnostics);
    lint_labels(path, source, &document, &mut diagnostics);
    diagnostics.sort_by_key(|diagnostic| {
        (
            diagnostic.span.start.offset.unwrap_or_default(),
            diagnostic.code.clone(),
        )
    });
    diagnostics
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

fn tooling_span(path: &Path, source: &str, span: syntax::Span) -> Span {
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
    lint_annotations(path, source, &function.annotations, diagnostics);
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
        TypeExprKind::Function { parameters, result } => {
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
        TypeExprKind::Handle(_) => {}
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
    if is_kebab_qualified(&name.value) {
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
    fn formatting_is_typed_idempotent_and_preserves_comments_and_labels() {
        let path = Path::new("src/main.vibra");
        let source = "; lead\n(defn hello-world (name str) str\n(do ; body\n(return name)) doc: \"Greets\")\n";
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
        let diagnostics = staged_sexpr_diagnostics(Path::new("unicode.vibra"), source);
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
        let source = "; λ\n(defn bad () void (do unit) mystery: true)\n";
        let diagnostics = staged_sexpr_diagnostics(Path::new("unicode.vibra"), source);
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
        let source = "(defn valid () void (do unit) doc: \"😀\" mystery: true)\n";
        let diagnostics = staged_sexpr_diagnostics(Path::new("emoji.vibra"), source);
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
        let source =
            "(defn Bad_Name (BadArg int64) int64 (do (let Local_Value BadArg) Local_Value))\n";
        let diagnostics = staged_lint_sexpr(Path::new("lint.vibra"), source);
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
        let diagnostics = staged_lint_sexpr(Path::new("lint.vibra"), source);
        assert!(diagnostics
            .iter()
            .any(|value| value.message.contains("Bad_Type")));
    }

    #[test]
    fn formatter_rejects_typed_invalid_source() {
        let error = staged_format_sexpr(
            Path::new("bad.vibra"),
            "(defn bad () void (do unit) nope: 1)",
        )
        .unwrap_err();
        assert_eq!(error.code, "E-SYN-011");
    }

    #[test]
    fn expect_error_codes_are_not_kebab_linted() {
        let source = "(test.scenario \"fails\" (test.case \"fails\" unit\n\
                      expect-error: (compile E-OP-002 \"overflow\")))\n";
        assert!(staged_lint_sexpr(Path::new("test.vibra"), source).is_empty());
    }
}

//! Lexical binding validation for the post-expansion typed surface.
//!
//! This pass deliberately runs over the typed AST after macro expansion. The
//! binding names it compares are therefore the resolved names produced by the
//! hygienic expander, rather than raw source spelling.

use std::collections::BTreeMap;
use std::fmt;

use super::{
    AnnotationKind, Expr, ExprKind, Function, ImplItem, MatchCase, Module, Name, Pattern,
    PatternKind, SourceLocation, TopLevel, WasmArgument,
};
use crate::syntax::Span;

pub const LEXICAL_SCOPE_SHADOWING_CODE: &str = "E-SCOPE-001";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeError {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
    pub document: super::DocumentId,
    pub related: Vec<(String, SourceLocation)>,
}

impl ScopeError {
    fn shadowing(
        name: &Name,
        fallback_document: super::DocumentId,
        original: SourceLocation,
    ) -> Self {
        let document = name.origin.document_id().unwrap_or(fallback_document);
        Self {
            code: LEXICAL_SCOPE_SHADOWING_CODE,
            message: format!(
                "binding `{}` shadows an enclosing lexical binding; choose a different name",
                name.value
            ),
            span: name.origin.primary_span(),
            document,
            related: vec![("original binding is here".to_string(), original)],
        }
    }
}

impl fmt::Display for ScopeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} at bytes {}..{}: {}",
            self.code, self.span.start, self.span.end, self.message
        )
    }
}

impl std::error::Error for ScopeError {}

#[derive(Debug, Default)]
struct ScopeStack {
    scopes: Vec<BTreeMap<String, SourceLocation>>,
}

impl ScopeStack {
    fn new() -> Self {
        Self {
            scopes: vec![BTreeMap::new()],
        }
    }

    fn push(&mut self) {
        self.scopes.push(BTreeMap::new());
    }

    fn pop(&mut self) {
        self.scopes.pop();
    }

    fn visible(&self, name: &str) -> Option<SourceLocation> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }

    fn bind(
        &mut self,
        name: &Name,
        fallback_document: super::DocumentId,
    ) -> Result<(), ScopeError> {
        if name.value == "_" {
            return Ok(());
        }
        if let Some(original) = self.visible(&name.value) {
            return Err(ScopeError::shadowing(name, fallback_document, original));
        }
        let location = source_location(name, fallback_document);
        self.scopes
            .last_mut()
            .expect("scope stack always has a root")
            .insert(name.value.clone(), location);
        Ok(())
    }

    fn capture(&mut self, name: &Name) {
        if name.value == "_" {
            return;
        }
        if let Some(location) = self.visible(&name.value) {
            self.scopes
                .last_mut()
                .expect("scope stack always has a root")
                .insert(name.value.clone(), location);
        }
    }
}

fn source_location(name: &Name, fallback_document: super::DocumentId) -> SourceLocation {
    SourceLocation {
        document: name.origin.document_id().unwrap_or(fallback_document),
        span: name.origin.primary_span(),
    }
}

/// Reject lexical shadowing in one fully expanded module.
pub fn validate_lexical_scopes(module: &Module) -> Result<(), ScopeError> {
    let mut validator = ScopeValidator {
        document: module.document_id,
        scopes: ScopeStack::new(),
    };
    for form in &module.forms {
        validator.visit_top_level(form)?;
    }
    Ok(())
}

struct ScopeValidator {
    document: super::DocumentId,
    scopes: ScopeStack,
}

impl ScopeValidator {
    fn visit_top_level(&mut self, form: &TopLevel) -> Result<(), ScopeError> {
        match form {
            TopLevel::Constant(constant) => {
                self.scopes.push();
                let result = self.visit_expr(&constant.value);
                self.scopes.pop();
                result
            }
            TopLevel::Function(function) => self.visit_function(function),
            TopLevel::Deffect(deffect) => deffect
                .operations
                .iter()
                .try_for_each(|operation| self.visit_function(&operation.function)),
            TopLevel::Definition(definition) => {
                for annotation in &definition.annotations {
                    if let AnnotationKind::Definitions(functions) = &annotation.value {
                        for function in functions {
                            self.visit_function(function)?;
                        }
                    } else if let AnnotationKind::Implementation { items, .. } = &annotation.value {
                        for item in items {
                            if let ImplItem::Method {
                                binding: super::MethodBinding::Function(function),
                                ..
                            } = item
                            {
                                self.visit_function(function)?;
                            }
                        }
                    }
                }
                Ok(())
            }
            TopLevel::Test(test) => self.visit_test_body(&test.body),
            TopLevel::TestScenario(scenario) => scenario
                .cases
                .iter()
                .try_for_each(|test| self.visit_test_body(&test.body)),
            TopLevel::Import(_) | TopLevel::Macro(_) => Ok(()),
        }
    }

    fn visit_function(&mut self, function: &Function) -> Result<(), ScopeError> {
        self.scopes.push();
        for parameter in &function.parameters {
            self.scopes.bind(&parameter.name, self.document)?;
        }
        let result = self.visit_exprs(&function.body);
        self.scopes.pop();
        result
    }

    fn visit_test_body(&mut self, body: &[Expr]) -> Result<(), ScopeError> {
        self.scopes.push();
        let result = self.visit_exprs(body);
        self.scopes.pop();
        result
    }

    fn visit_exprs(&mut self, expressions: &[Expr]) -> Result<(), ScopeError> {
        expressions
            .iter()
            .try_for_each(|expression| self.visit_expr(expression))
    }

    fn visit_expr(&mut self, expression: &Expr) -> Result<(), ScopeError> {
        match &expression.value {
            ExprKind::Call { arguments, .. } => arguments
                .iter()
                .try_for_each(|argument| self.visit_expr(argument.value())),
            ExprKind::AnonymousFunction {
                parameters, body, ..
            } => {
                self.scopes.push();
                for parameter in parameters {
                    self.scopes.bind(&parameter.name, self.document)?;
                }
                let result = self.visit_exprs(body);
                self.scopes.pop();
                result
            }
            ExprKind::Do(values) | ExprKind::Tuple(values) | ExprKind::Array(values) => {
                self.visit_exprs(values)
            }
            ExprKind::Intrinsic { arguments, .. } => self.visit_exprs(arguments),
            ExprKind::Let { name, value, .. } => {
                self.visit_expr(value)?;
                self.scopes.bind(name, self.document)
            }
            ExprKind::Set { value, .. }
            | ExprKind::Mutable(value)
            | ExprKind::ReferenceOf(value)
            | ExprKind::Return(Some(value))
            | ExprKind::Convert { value, .. }
            | ExprKind::Cast { value, .. } => self.visit_expr(value),
            ExprKind::Return(None)
            | ExprKind::Literal(_)
            | ExprKind::Reference(_)
            | ExprKind::Break
            | ExprKind::Continue
            | ExprKind::Embed { .. }
            | ExprKind::Join { .. } => Ok(()),
            ExprKind::If {
                condition,
                then_body,
                else_body,
            } => {
                self.visit_expr(condition)?;
                self.visit_branch(then_body)?;
                self.visit_branch(else_body)
            }
            ExprKind::While { condition, body } => {
                self.visit_expr(condition)?;
                self.visit_branch(body)
            }
            ExprKind::For {
                binding,
                source,
                body,
            } => {
                self.visit_expr(source)?;
                self.scopes.push();
                let result = self
                    .scopes
                    .bind(binding, self.document)
                    .and_then(|()| self.visit_exprs(body));
                self.scopes.pop();
                result
            }
            ExprKind::Match { target, cases } => {
                self.visit_expr(target)?;
                for case in cases {
                    self.visit_match_case(case)?;
                }
                Ok(())
            }
            ExprKind::Record(fields)
            | ExprKind::Template {
                bindings: fields, ..
            } => fields
                .iter()
                .try_for_each(|field| self.visit_expr(&field.value)),
            ExprKind::Map(entries) => entries.iter().try_for_each(|(key, value)| {
                self.visit_expr(key)?;
                self.visit_expr(value)
            }),
            ExprKind::Range(start, end, step) => {
                self.visit_expr(start)?;
                self.visit_expr(end)?;
                self.visit_expr(step)
            }
            ExprKind::Task { captures, body } => self.visit_captured_body(captures, body),
            ExprKind::Spawn {
                captures, value, ..
            } => self.visit_captured_body(captures, std::slice::from_ref(value)),
            ExprKind::Wasm { arguments, .. } => arguments.iter().try_for_each(|argument| {
                if let WasmArgument::Expression(value) = argument {
                    self.visit_expr(value)?;
                }
                Ok(())
            }),
        }
    }

    fn visit_branch(&mut self, body: &[Expr]) -> Result<(), ScopeError> {
        self.scopes.push();
        let result = self.visit_exprs(body);
        self.scopes.pop();
        result
    }

    fn visit_match_case(&mut self, case: &MatchCase) -> Result<(), ScopeError> {
        self.scopes.push();
        let result = self
            .visit_pattern(&case.pattern)
            .and_then(|()| self.visit_exprs(&case.body));
        self.scopes.pop();
        result
    }

    fn visit_pattern(&mut self, pattern: &Pattern) -> Result<(), ScopeError> {
        match &pattern.value {
            PatternKind::Bind(name) => self.scopes.bind(name, self.document),
            PatternKind::Constructor { arguments, .. }
            | PatternKind::Tuple(arguments)
            | PatternKind::Array(arguments) => arguments
                .iter()
                .try_for_each(|pattern| self.visit_pattern(pattern)),
            PatternKind::Record(fields) => fields
                .iter()
                .try_for_each(|field| self.visit_pattern(&field.pattern)),
            PatternKind::Map(entries) => entries.iter().try_for_each(|(key, value)| {
                self.visit_pattern(key)?;
                self.visit_pattern(value)
            }),
            PatternKind::Newtype { pattern, .. } | PatternKind::Interface { pattern, .. } => {
                self.visit_pattern(pattern)
            }
            PatternKind::Literal(_) | PatternKind::Wildcard => Ok(()),
        }
    }

    fn visit_captured_body(&mut self, captures: &[Name], body: &[Expr]) -> Result<(), ScopeError> {
        self.scopes.push();
        for capture in captures {
            self.scopes.capture(capture);
        }
        let result = self.visit_exprs(body);
        self.scopes.pop();
        result
    }
}

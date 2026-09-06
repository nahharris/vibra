//! Primitive type checking and lowering for the M2 Step 5 profile.
//!
//! The checker consumes the syntax AST and returns an immutable checked IR.
//! It deliberately admits only module-level functions with primitive
//! signatures and literal/sequence bodies.  A parsed AST that contains a
//! later-step form never crosses into `vibra-ir` or `vibra-interp`.

#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used
    )
)]

use std::path::Path;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::{
    CheckedFunction, CheckedProgram, Expr, FunctionSignature, PrimitiveType,
    SourceOrigin, Value,
};
use vibra_syntax::{
    Attribute, Declaration, Expression, ExpressionKind, FloatSuffix, IntegerSuffix,
    Literal, NameKind, SourceAst, TypeExpr,
};

/// The result of checking one source document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckResult {
    program: Option<CheckedProgram>,
    diagnostics: Vec<Diagnostic>,
}

impl CheckResult {
    /// Creates a semantic result.  Callers normally use [`check_source`].
    #[must_use]
    pub fn new(program: Option<CheckedProgram>, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            program,
            diagnostics,
        }
    }

    /// The checked program, only when every required phase succeeded.
    #[must_use]
    pub const fn program(&self) -> Option<&CheckedProgram> {
        self.program.as_ref()
    }

    /// Diagnostics in deterministic source order.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
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
/// Step 5 subset to controlled IR.
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
        return CheckResult::new(None, diagnostics);
    };
    let program = check_ast(source_id, ast, &mut diagnostics);
    if !diagnostics
        .iter()
        .all(|diagnostic| diagnostic.level() != vibra_diagnostics::Level::Error)
    {
        return CheckResult::new(None, diagnostics);
    }
    CheckResult::new(program, diagnostics)
}

/// Checks an already decoded source AST.  This is useful to workspace
/// adapters that have already performed the reader phase.
pub fn check_ast(
    source_id: impl AsRef<str>,
    ast: &SourceAst,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<CheckedProgram> {
    let source_id = source_id.as_ref();
    let mut functions = Vec::new();

    for declaration in ast.declarations() {
        let Declaration::Defn(function) = declaration else {
            // This source-only checker has no resolved graph input, so an
            // import cannot safely contribute a checked function.  Every
            // other declaration family likewise needs its owning semantic
            // step before it can enter checked IR.
            let message = if matches!(declaration, Declaration::Import(_)) {
                "imports require the resolved multi-module checker in a later M2 step"
            } else {
                "this declaration family is outside the Step 5 primitive profile"
            };
            unavailable(diagnostics, source_id, declaration.span(), message);
            continue;
        };

        let Some(signature) = check_signature(source_id, function, diagnostics) else {
            continue;
        };
        if !function.parameters().is_empty() {
            unavailable(
                diagnostics,
                source_id,
                function.span(),
                "non-nullary function execution is available in a later M2 step",
            );
            continue;
        }
        if has_deferred_attributes(function.attributes().items()) {
            unavailable(
                diagnostics,
                source_id,
                function.span(),
                "function attributes outside the empty-effect primitive profile are unavailable",
            );
            continue;
        }

        let body = check_sequence(
            source_id,
            function.expressions(),
            signature.result(),
            diagnostics,
        );
        let Some(body) = body else {
            continue;
        };
        let origin = SourceOrigin::new(source_id, function.span());
        match CheckedFunction::new(function.name().value(), signature, body, origin) {
            Ok(function) => functions.push(function),
            Err(error) => unavailable(
                diagnostics,
                source_id,
                function.span(),
                format!("checked IR construction failed: {error}"),
            ),
        }
    }

    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.level() == vibra_diagnostics::Level::Error)
        || functions.is_empty()
    {
        return None;
    }
    match CheckedProgram::try_new(functions, 0) {
        Ok(program) => Some(program),
        Err(error) => {
            unavailable(
                diagnostics,
                source_id,
                ast.span(),
                format!("checked IR construction failed: {error}"),
            );
            None
        }
    }
}

fn check_signature(
    source_id: &str,
    function: &vibra_syntax::FunctionDeclaration,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<FunctionSignature> {
    let mut parameters = Vec::with_capacity(function.parameters().len());
    let mut valid = true;
    for parameter in function.parameters() {
        match primitive_type(parameter.value_type()) {
            Some(value_type) => parameters.push(value_type),
            None => {
                valid = false;
                unavailable(
                    diagnostics,
                    source_id,
                    parameter.span(),
                    "only primitive parameter types are available in Step 5",
                );
            }
        }
    }
    let result = match primitive_type(function.result()) {
        Some(value_type) => value_type,
        None => {
            valid = false;
            unavailable(
                diagnostics,
                source_id,
                function.span(),
                "only primitive result types are available in Step 5",
            );
            PrimitiveType::Void
        }
    };
    valid.then(|| FunctionSignature::new(parameters, result))
}

fn primitive_type(value: &TypeExpr) -> Option<PrimitiveType> {
    match value {
        TypeExpr::Void => Some(PrimitiveType::Void),
        TypeExpr::Name(name) => match name.value() {
            "bool" => Some(PrimitiveType::Bool),
            "char" => Some(PrimitiveType::Char),
            "str" => Some(PrimitiveType::Str),
            "bytes" => Some(PrimitiveType::Bytes),
            "atom" => Some(PrimitiveType::Atom),
            "i8" => Some(PrimitiveType::I8),
            "i16" => Some(PrimitiveType::I16),
            "i32" => Some(PrimitiveType::I32),
            "i64" => Some(PrimitiveType::I64),
            "u8" => Some(PrimitiveType::U8),
            "u16" => Some(PrimitiveType::U16),
            "u32" => Some(PrimitiveType::U32),
            "u64" => Some(PrimitiveType::U64),
            "f32" => Some(PrimitiveType::F32),
            "f64" => Some(PrimitiveType::F64),
            _ => None,
        },
        TypeExpr::Applied { .. }
        | TypeExpr::Tuple(_)
        | TypeExpr::Array(_)
        | TypeExpr::Map(_, _)
        | TypeExpr::Function(_) => None,
    }
}

fn has_deferred_attributes(attributes: &[Attribute]) -> bool {
    attributes.iter().any(|attribute| match attribute {
        Attribute::Where(_)
        | Attribute::Labelled(_)
        | Attribute::Variadic(_)
        | Attribute::External(_)
        | Attribute::Symbol(_) => true,
        Attribute::Effects(row) => !row.references().is_empty(),
        Attribute::Visibility(_) | Attribute::Doc(_) => false,
    })
}

fn check_sequence(
    source_id: &str,
    expressions: &[Expression],
    expected: PrimitiveType,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Expr> {
    if expressions.is_empty() {
        if expected != PrimitiveType::Void {
            mismatch(
                diagnostics,
                source_id,
                ByteSpan::empty_at(0),
                expected,
                PrimitiveType::Void,
                "an empty function body returns void",
            );
            return None;
        }
        return Some(Expr::literal(
            Value::Void,
            SourceOrigin::new(source_id, ByteSpan::empty_at(0)),
        ));
    }

    let mut checked = Vec::with_capacity(expressions.len());
    let mut valid = true;
    for (index, expression) in expressions.iter().enumerate() {
        let expression_expected = (index + 1 == expressions.len()).then_some(expected);
        match check_expression(source_id, expression, expression_expected, diagnostics)
        {
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
            SourceOrigin::new(source_id, first.span().join(last.span()))
        })
        .unwrap_or_else(|| SourceOrigin::new(source_id, ByteSpan::empty_at(0)));
    Some(Expr::sequence(checked, origin))
}

fn check_expression(
    source_id: &str,
    expression: &Expression,
    expected: Option<PrimitiveType>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Expr> {
    match expression.kind() {
        ExpressionKind::Literal(literal) => {
            check_literal(source_id, expression.span(), literal, expected, diagnostics)
                .map(|value| {
                    Expr::literal(
                        value,
                        SourceOrigin::new(source_id, expression.span()),
                    )
                })
        }
        ExpressionKind::Name(name) if name.kind() == NameKind::Atom => {
            let actual = PrimitiveType::Atom;
            if expected.is_some_and(|expected| expected != actual) {
                mismatch(
                    diagnostics,
                    source_id,
                    expression.span(),
                    expected.unwrap_or(actual),
                    actual,
                    "an atom literal does not match the written result type",
                );
                return None;
            }
            Some(Expr::literal(
                Value::Atom(name.value().to_owned()),
                SourceOrigin::new(source_id, expression.span()),
            ))
        }
        ExpressionKind::Name(_) => {
            unavailable(
                diagnostics,
                source_id,
                expression.span(),
                "name resolution and value bindings are available in a later M2 step",
            );
            None
        }
        ExpressionKind::Do(_) => {
            unavailable(
                diagnostics,
                source_id,
                expression.span(),
                "explicit do expressions are available in Step 6",
            );
            None
        }
        _ => {
            unavailable(
                diagnostics,
                source_id,
                expression.span(),
                "this expression form is outside the Step 5 literal profile",
            );
            None
        }
    }
}

fn check_literal(
    source_id: &str,
    span: ByteSpan,
    literal: &Literal,
    expected: Option<PrimitiveType>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Value> {
    match literal {
        Literal::String(value) => expect_fixed(
            source_id,
            span,
            expected,
            PrimitiveType::Str,
            Value::Str(value.value().to_owned()),
            diagnostics,
        ),
        Literal::Character(value) => expect_fixed(
            source_id,
            span,
            expected,
            PrimitiveType::Char,
            Value::Char(value.value()),
            diagnostics,
        ),
        Literal::Boolean(value) => expect_fixed(
            source_id,
            span,
            expected,
            PrimitiveType::Bool,
            Value::Bool(value.value()),
            diagnostics,
        ),
        Literal::Void(_) => expect_fixed(
            source_id,
            span,
            expected,
            PrimitiveType::Void,
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
    expected: Option<PrimitiveType>,
    actual: PrimitiveType,
    value: Value,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Value> {
    if expected.is_some_and(|expected| expected != actual) {
        mismatch(
            diagnostics,
            source_id,
            span,
            expected.unwrap_or(actual),
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
    expected: Option<PrimitiveType>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Value> {
    let target = literal
        .suffix()
        .map(integer_suffix_type)
        .or_else(|| expected.filter(|value| value.is_integer()));
    let Some(target) = target else {
        mismatch(
            diagnostics,
            source_id,
            span,
            expected.unwrap_or(PrimitiveType::I64),
            PrimitiveType::I64,
            "an unsuffixed integer needs one expected integer type",
        );
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
    let Some(value) = integer_value(target, literal.is_negative(), magnitude) else {
        out_of_range(
            diagnostics,
            source_id,
            span,
            "integer is outside its exact primitive range",
        );
        return None;
    };
    if expected.is_some_and(|expected| expected != target) {
        mismatch(
            diagnostics,
            source_id,
            span,
            expected.unwrap_or(target),
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
    expected: Option<PrimitiveType>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Value> {
    let target = literal
        .suffix()
        .map(float_suffix_type)
        .or_else(|| expected.filter(|value| value.is_float()));
    let Some(target) = target else {
        mismatch(
            diagnostics,
            source_id,
            span,
            expected.unwrap_or(PrimitiveType::F64),
            PrimitiveType::F64,
            "an unsuffixed float needs one expected floating-point type",
        );
        return None;
    };
    let value = match target {
        PrimitiveType::F32 => literal
            .body()
            .parse::<f32>()
            .ok()
            .filter(|value| value.is_finite())
            .and_then(Value::f32),
        PrimitiveType::F64 => literal
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
    if expected.is_some_and(|expected| expected != target) {
        mismatch(
            diagnostics,
            source_id,
            span,
            expected.unwrap_or(target),
            target,
            "numeric suffixes never request an implicit conversion",
        );
        return None;
    }
    Some(value)
}

fn integer_suffix_type(suffix: IntegerSuffix) -> PrimitiveType {
    match suffix {
        IntegerSuffix::I8 => PrimitiveType::I8,
        IntegerSuffix::I16 => PrimitiveType::I16,
        IntegerSuffix::I32 => PrimitiveType::I32,
        IntegerSuffix::I64 => PrimitiveType::I64,
        IntegerSuffix::U8 => PrimitiveType::U8,
        IntegerSuffix::U16 => PrimitiveType::U16,
        IntegerSuffix::U32 => PrimitiveType::U32,
        IntegerSuffix::U64 => PrimitiveType::U64,
    }
}

fn float_suffix_type(suffix: FloatSuffix) -> PrimitiveType {
    match suffix {
        FloatSuffix::F32 => PrimitiveType::F32,
        FloatSuffix::F64 => PrimitiveType::F64,
    }
}

fn integer_value(
    target: PrimitiveType,
    negative: bool,
    magnitude: u128,
) -> Option<Value> {
    match target {
        PrimitiveType::I8 => {
            signed_value(negative, magnitude, i8::MIN as i128, i8::MAX as i128)
                .map(|value| Value::I8(value as i8))
        }
        PrimitiveType::I16 => {
            signed_value(negative, magnitude, i16::MIN as i128, i16::MAX as i128)
                .map(|value| Value::I16(value as i16))
        }
        PrimitiveType::I32 => {
            signed_value(negative, magnitude, i32::MIN as i128, i32::MAX as i128)
                .map(|value| Value::I32(value as i32))
        }
        PrimitiveType::I64 => {
            signed_value(negative, magnitude, i64::MIN as i128, i64::MAX as i128)
                .map(|value| Value::I64(value as i64))
        }
        PrimitiveType::U8 => (!negative && magnitude <= u8::MAX as u128)
            .then_some(Value::U8(magnitude as u8)),
        PrimitiveType::U16 => (!negative && magnitude <= u16::MAX as u128)
            .then_some(Value::U16(magnitude as u16)),
        PrimitiveType::U32 => (!negative && magnitude <= u32::MAX as u128)
            .then_some(Value::U32(magnitude as u32)),
        PrimitiveType::U64 => (!negative && magnitude <= u64::MAX as u128)
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

fn mismatch(
    diagnostics: &mut Vec<Diagnostic>,
    source_id: &str,
    span: ByteSpan,
    expected: PrimitiveType,
    actual: PrimitiveType,
    message: impl Into<String>,
) {
    diagnostics.push(
        Diagnostic::new(
            DiagnosticCode::TypeArgumentMismatch,
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

fn unavailable(
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

#[cfg(test)]
mod tests {
    use super::check_source;
    use vibra_diagnostics::DiagnosticCode;

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
            vibra_ir::PrimitiveType::I32
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
        assert!(result.diagnostics().iter().any(
            |diagnostic| diagnostic.code() == DiagnosticCode::TypeArgumentMismatch
        ));
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
    fn valid_later_step_declarations_do_not_enter_partial_ir() {
        let result =
            check_source("deferred.vib", "(def value i32 1)\n(defn answer () i32 2)");
        assert!(!result.accepted());
        assert!(result.program().is_none());
        assert!(result.diagnostics().iter().any(|diagnostic| {
            diagnostic.code() == DiagnosticCode::ToolUnavailable
        }));
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
}

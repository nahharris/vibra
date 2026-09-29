//! Constructor, projection, and anonymous-value checking (M3 Step 2).
//!
//! `docs/spec/02-type-system.md` fixes the applicable categories: a declared
//! record is built from its closed labelled fields, an enum through one of its
//! variants, a wrapper type from its representation, and a record value applied to
//! one atom selector projects a field. `recordof` builds an anonymous record
//! and `enumof` an anonymous enum checked against a written expected type.

use std::collections::BTreeSet;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::{Expr, SourceOrigin, Type, TypeBody};
use vibra_syntax::{
    Application, BindingFacts, CallArgument, Expression, ExpressionKind, NameKind,
};

use crate::infer::Instantiation;
use crate::nominal::{ConstructorTarget, declared_self_type};
use crate::{
    CheckEnvironment, ambiguous_generic, call_contract_error, check_expression,
    check_inferred_operand, ensure_expected, start_instantiation, types_match,
};

/// Checks an application whose callee names a constructor.
///
/// A generic declaration infers its complete argument list from the field,
/// payload, or representation operands, the written result type, and
/// `types:`, exactly as a generic function application does.
pub(crate) fn check_constructor(
    environment: &mut CheckEnvironment<'_>,
    application: &Application,
    target: &ConstructorTarget,
    type_arguments: Option<&[Type]>,
    expected: Option<Type>,
) -> Option<Expr> {
    let (index, variant) = match target {
        ConstructorTarget::Type(index) => (*index, None),
        ConstructorTarget::Variant(index, variant) => (*index, Some(variant.as_str())),
    };
    let declared = environment.types.get(index)?.clone();
    // A declaration whose body failed to lower already reported why.
    let body = declared.body.clone()?;
    let pattern = declared_self_type(&declared.id, &declared.parameters);
    let mut instantiation = start_instantiation(
        environment,
        application,
        &declared.parameters,
        &pattern,
        type_arguments,
        expected.as_ref(),
    )?;
    let types_written = type_arguments.is_some();
    let origin = SourceOrigin::new(environment.source_id, application.span());
    let mut binding = ConstructorBinding::positional(0);
    let expression = match (&body, variant) {
        (TypeBody::Record(fields), None) => {
            let names: Vec<String> =
                fields.iter().map(|(name, _)| name.clone()).collect();
            let (arguments, labelled) =
                bind_labelled(environment, application, &names)?;
            binding = labelled;
            let mut checked = Vec::with_capacity(fields.len());
            for ((name, field_type), argument) in fields.iter().zip(arguments) {
                let field_type = instantiation.open(field_type);
                let value = check_inferred_operand(
                    environment,
                    &mut instantiation,
                    argument.value(),
                    &field_type,
                    types_written,
                )?;
                checked.push((name.clone(), value));
            }
            Expr::Record {
                value_type: instantiated(
                    environment,
                    application,
                    &instantiation,
                    &pattern,
                )?,
                fields: checked,
                origin,
            }
        }
        (TypeBody::Tuple(components), None) => {
            let operands = application.arguments();
            if operands.len() != components.len()
                || operands.iter().any(|operand| operand.label().is_some())
            {
                call_contract_error(
                    environment,
                    application.span(),
                    format!(
                        "a `{}` constructor takes exactly {} unlabelled operands, one per component",
                        declared.name,
                        components.len()
                    ),
                );
                return None;
            }
            binding = ConstructorBinding::positional(components.len());
            let mut checked = Vec::with_capacity(components.len());
            for (component, operand) in components.iter().zip(operands) {
                let component = instantiation.open(component);
                checked.push(check_inferred_operand(
                    environment,
                    &mut instantiation,
                    operand.value(),
                    &component,
                    types_written,
                )?);
            }
            Expr::Tuple {
                value_type: instantiated(
                    environment,
                    application,
                    &instantiation,
                    &pattern,
                )?,
                components: checked,
                origin,
            }
        }
        (TypeBody::Wrapper(representation), None) => {
            let operand =
                single_positional(environment, application, "a wrapper constructor")?;
            binding = ConstructorBinding::positional(1);
            let representation = instantiation.open(representation);
            let value = check_inferred_operand(
                environment,
                &mut instantiation,
                operand,
                &representation,
                types_written,
            )?;
            Expr::Wrap {
                value_type: instantiated(
                    environment,
                    application,
                    &instantiation,
                    &pattern,
                )?,
                value: Box::new(value),
                origin,
            }
        }
        (TypeBody::Enum(variants), Some(variant)) => {
            let Some((_, payload_type)) =
                variants.iter().find(|(name, _)| name == variant)
            else {
                environment.diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::NameUnknownSymbol,
                        application.callee().span(),
                        format!("`{}` has no variant `{variant}`", declared.name),
                    )
                    .with_source_id(environment.source_id),
                );
                return None;
            };
            // A payload is nullary when it is `void`, written or instantiated.
            // Zero operands against an open generic payload fix it to `void`.
            let payload_type = instantiation.open(payload_type);
            let nullary = match instantiation.resolved(&payload_type) {
                Some(resolved) => resolved == Type::Void,
                None => {
                    application.arguments().is_empty()
                        && instantiation.unify(&payload_type, &Type::Void)
                }
            };
            let payload = if nullary {
                if !application.arguments().is_empty() {
                    call_contract_error(
                        environment,
                        application.span(),
                        format!(
                            "variant `{variant}` has a `void` payload and takes no operand"
                        ),
                    );
                    return None;
                }
                None
            } else {
                let operand =
                    single_positional(environment, application, "an enum variant")?;
                binding = ConstructorBinding::positional(1);
                Some(Box::new(check_inferred_operand(
                    environment,
                    &mut instantiation,
                    operand,
                    &payload_type,
                    types_written,
                )?))
            };
            Expr::Variant {
                value_type: instantiated(
                    environment,
                    application,
                    &instantiation,
                    &pattern,
                )?,
                variant: variant.to_owned(),
                payload,
                origin,
            }
        }
        (TypeBody::Enum(_), None) => {
            wrong_kind(
                environment,
                application.callee().span(),
                format!(
                    "`{}` is an enum type; apply one of its variants",
                    declared.name
                ),
            );
            return None;
        }
        (
            TypeBody::Record(_) | TypeBody::Wrapper(_) | TypeBody::Tuple(_),
            Some(member),
        ) => {
            environment.diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::NameUnknownSymbol,
                    application.callee().span(),
                    format!("`{}` has no variant `{member}`", declared.name),
                )
                .with_source_id(environment.source_id),
            );
            return None;
        }
    };
    binding.record(environment, application);
    let value_type = expression.result_type();
    finish(
        environment,
        application.span(),
        expected,
        value_type,
        expression,
    )
}

/// Checks a record value applied to one atom field selector.
pub(crate) fn check_projection(
    environment: &mut CheckEnvironment<'_>,
    application: &Application,
    record: Expr,
    expected: Option<Type>,
) -> Option<Expr> {
    let record_type = record.result_type();
    let fields = record_fields(environment, &record_type)?;
    let [operand] = application.arguments() else {
        call_contract_error(
            environment,
            application.span(),
            "a record projection takes exactly one atom field selector".to_owned(),
        );
        return None;
    };
    let selector = match operand.value().kind() {
        ExpressionKind::Name(name)
            if name.kind() == NameKind::Atom && operand.label().is_none() =>
        {
            name.value().to_owned()
        }
        _ => {
            call_contract_error(
                environment,
                operand.span(),
                "a record projection takes exactly one atom field selector".to_owned(),
            );
            return None;
        }
    };
    let Some((_, field_type)) = fields.iter().find(|(name, _)| *name == selector)
    else {
        environment.diagnostics.push(
            Diagnostic::new(
                DiagnosticCode::TypeUnknownRecordField,
                operand.span(),
                format!("{record_type} has no field `{selector}`"),
            )
            .with_source_id(environment.source_id),
        );
        return None;
    };
    let value_type = field_type.clone();
    let expression = Expr::Project {
        record: Box::new(record),
        field: selector,
        value_type: value_type.clone(),
        origin: SourceOrigin::new(environment.source_id, application.span()),
    };
    finish(
        environment,
        application.span(),
        expected,
        value_type,
        expression,
    )
}

/// The fields of a declared or anonymous record type, or `None` when the type
/// is not a record.
pub(crate) fn record_fields(
    environment: &CheckEnvironment<'_>,
    value_type: &Type,
) -> Option<Vec<(String, Type)>> {
    match value_type {
        Type::Record(fields) => Some(fields.clone()),
        Type::Declared(id) | Type::Applied(id, _) => {
            let index = environment.types.index_of(id)?;
            let declared = environment.types.get(index)?;
            let Some(TypeBody::Record(fields)) = &declared.body else {
                return None;
            };
            let arguments = match value_type {
                Type::Applied(_, arguments) => arguments.as_slice(),
                _ => &[],
            };
            let substitution: std::collections::BTreeMap<String, Type> = declared
                .parameters
                .iter()
                .cloned()
                .zip(arguments.iter().cloned())
                .collect();
            Some(
                fields
                    .iter()
                    .map(|(name, field)| {
                        (name.clone(), field.substitute(&substitution))
                    })
                    .collect(),
            )
        }
        _ => None,
    }
}

/// Checks `(recordof a: e …)`: fields evaluate in written order, and the type
/// is the anonymous record of the field types, in canonical order.
pub(crate) fn check_recordof(
    environment: &mut CheckEnvironment<'_>,
    expression: &Expression,
    fields: &[CallArgument],
    expected: Option<Type>,
) -> Option<Expr> {
    let expected_fields = match &expected {
        Some(Type::Record(fields)) => Some(fields.clone()),
        _ => None,
    };
    let mut seen = BTreeSet::new();
    let mut checked = Vec::with_capacity(fields.len());
    let mut members = Vec::with_capacity(fields.len());
    for field in fields {
        let label = field.label()?.value().to_owned();
        if !seen.insert(label.clone()) {
            call_contract_error(
                environment,
                field.span(),
                format!("recordof repeats field `{label}`"),
            );
            return None;
        }
        let field_expected = expected_fields.as_ref().and_then(|fields| {
            fields
                .iter()
                .find(|(name, _)| *name == label)
                .map(|(_, value)| value.clone())
        });
        let value = check_expression(environment, field.value(), field_expected)?;
        members.push((label.clone(), value.result_type()));
        checked.push((label, value));
    }
    let value_type = Type::Record(vibra_ir::canonical_members(members));
    let expression_ir = Expr::Record {
        value_type: value_type.clone(),
        fields: checked,
        origin: SourceOrigin::new(environment.source_id, expression.span()),
    };
    finish(
        environment,
        expression.span(),
        expected,
        value_type,
        expression_ir,
    )
}

/// Checks `(enumof a: e)` against its written expected anonymous enum.
pub(crate) fn check_enumof(
    environment: &mut CheckEnvironment<'_>,
    expression: &Expression,
    variant: &CallArgument,
    expected: Option<Type>,
) -> Option<Expr> {
    let label = variant.label()?.value().to_owned();
    let Some(Type::Enum(variants)) = expected.clone() else {
        environment.diagnostics.push(
            Diagnostic::new(
                DiagnosticCode::TypeAmbiguousInference,
                expression.span(),
                "enumof needs a written anonymous enum type that declares its variant",
            )
            .with_source_id(environment.source_id),
        );
        return None;
    };
    let Some((_, payload_type)) = variants.iter().find(|(name, _)| *name == label)
    else {
        environment.diagnostics.push(
            Diagnostic::new(
                DiagnosticCode::TypeMismatch,
                expression.span(),
                format!(
                    "the expected {} has no variant `{label}`",
                    Type::Enum(variants.clone())
                ),
            )
            .with_source_id(environment.source_id),
        );
        return None;
    };
    let payload =
        check_expression(environment, variant.value(), Some(payload_type.clone()))?;
    let payload = (*payload_type != Type::Void).then(|| Box::new(payload));
    Some(Expr::Variant {
        value_type: Type::Enum(variants),
        variant: label,
        payload,
        origin: SourceOrigin::new(environment.source_id, expression.span()),
    })
}

/// Binds labelled constructor fields in declaration order, rejecting missing,
/// unknown, duplicate, and positional operands. Records the binding facts for
/// the formatter and the canonical-order style check.
fn bind_labelled<'a>(
    environment: &mut CheckEnvironment<'_>,
    application: &'a Application,
    names: &[String],
) -> Option<(Vec<&'a CallArgument>, ConstructorBinding)> {
    let facts = BindingFacts::new(0, names.to_vec(), None);
    let ordered = match application.ordered_arguments(&facts) {
        Ok(ordered) => ordered,
        Err(error) => {
            call_contract_error(environment, application.span(), error.to_string());
            return None;
        }
    };
    let supplied: BTreeSet<&str> = ordered
        .iter()
        .filter_map(|argument| argument.label().map(|label| label.value()))
        .collect();
    if let Some(missing) = names.iter().find(|name| !supplied.contains(name.as_str())) {
        call_contract_error(
            environment,
            application.span(),
            format!("record constructor is missing field `{missing}`"),
        );
        return None;
    }
    let reordered = ordered
        .iter()
        .zip(application.arguments())
        .any(|(left, right)| !std::ptr::eq(*left, right));
    Some((ordered, ConstructorBinding { facts, reordered }))
}

fn single_positional<'a>(
    environment: &mut CheckEnvironment<'_>,
    application: &'a Application,
    what: &str,
) -> Option<&'a Expression> {
    match application.arguments() {
        [argument] if argument.label().is_none() => Some(argument.value()),
        _ => {
            call_contract_error(
                environment,
                application.span(),
                format!("{what} takes exactly one unlabelled operand"),
            );
            None
        }
    }
}

fn wrong_kind(environment: &mut CheckEnvironment<'_>, span: ByteSpan, message: String) {
    environment.diagnostics.push(
        Diagnostic::new(DiagnosticCode::NameWrongEntityKind, span, message)
            .with_source_id(environment.source_id),
    );
}

fn finish(
    environment: &mut CheckEnvironment<'_>,
    span: ByteSpan,
    expected: Option<Type>,
    actual: Type,
    expression: Expr,
) -> Option<Expr> {
    ensure_expected(environment, span, expected.clone(), actual.clone());
    expected
        .as_ref()
        .is_none_or(|expected| types_match(expected, &actual))
        .then_some(expression)
}

/// The constructed type once every generic argument is fixed, or
/// `@type.ambiguous-inference`.
fn instantiated(
    environment: &mut CheckEnvironment<'_>,
    application: &Application,
    instantiation: &Instantiation,
    pattern: &Type,
) -> Option<Type> {
    let opened = instantiation.open(pattern);
    let resolved = instantiation.resolved(&opened);
    if resolved.is_none() {
        ambiguous_generic(environment, application.span(), &instantiation.unbound());
    }
    resolved
}

/// The binding facts of one constructor application, recorded only once the
/// construction checks, so the formatter and the style warning never act on
/// an incomplete binding.
struct ConstructorBinding {
    facts: BindingFacts,
    reordered: bool,
}

impl ConstructorBinding {
    fn positional(count: usize) -> Self {
        Self {
            facts: BindingFacts::new(count, Vec::new(), None),
            reordered: false,
        }
    }

    fn record(self, environment: &mut CheckEnvironment<'_>, application: &Application) {
        if self.reordered || application.type_arguments_after_operands() {
            environment.diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::StyleArgumentOrder,
                    application.span(),
                    "constructor operands are not in canonical declaration order",
                )
                .with_source_id(environment.source_id),
            );
        }
        environment
            .bindings
            .push(vibra_syntax::ApplicationBinding::new(
                application.span(),
                self.facts,
            ));
    }
}

//! Pattern checking and the exhaustiveness engine (M3 Step 5).
//!
//! `docs/spec/02-type-system.md`, "Control flow and failure": one usefulness
//! engine answers both whether a `match` arm set covers its scrutinee type and
//! whether a single binding pattern is irrefutable. It is Maranget's
//! usefulness algorithm over constructor spaces: `bool`, enums (by variant),
//! and `void` are finite; records, tuples, and wrappers have one constructor;
//! arrays (by length), `atom`, `str`, `bytes`, `char`, and the numbers are
//! infinite, so only a binder or discard covers them.

use std::collections::BTreeSet;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::{Pattern, Type, TypeBody, Value};
use vibra_syntax::{Literal, PatternArgument, PatternKind};

use crate::nominal::{ConstructorTarget, TypeNames, declared_self_type};
use crate::{CheckEnvironment, call_contract_error, check_literal, mismatch};

/// Lowers a written pattern against its expected type, binding every named
/// binder in `environment`. Returns `None` after reporting a diagnostic.
pub(crate) fn check_pattern(
    environment: &mut CheckEnvironment<'_>,
    pattern: &vibra_syntax::Pattern,
    expected: &Type,
) -> Option<Pattern> {
    let span = pattern.span();
    match pattern.kind() {
        PatternKind::Binding(name) if name.is_discard() => Some(Pattern::Wildcard),
        PatternKind::Binding(name) => {
            environment.add_binding_type(name.value(), expected.clone(), span);
            Some(Pattern::Bind {
                slot: environment.next_slot.saturating_sub(1),
                value_type: expected.clone(),
            })
        }
        PatternKind::Atom(name) => {
            let closed_by_it =
                matches!(expected, Type::AtomSingleton(atom) if atom == name.value());
            if *expected != Type::Atom && !closed_by_it {
                mismatch(
                    environment.diagnostics,
                    environment.source_id,
                    span,
                    expected.clone(),
                    Type::AtomSingleton(name.value().to_owned()),
                    "an atom pattern does not match the expected type",
                );
                return None;
            }
            Some(Pattern::Literal(Value::Atom(name.value().to_owned())))
        }
        PatternKind::Literal(literal) => {
            check_literal_pattern(environment, span, literal, expected)
        }
        PatternKind::Constructor { head, arguments } => {
            let Some(target) =
                environment.types.constructor(environment.source_id, head)
            else {
                environment.diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::NameUnknownSymbol,
                        span,
                        format!("`{}` does not name a constructor", head.value()),
                    )
                    .with_source_id(environment.source_id),
                );
                return None;
            };
            check_constructor_pattern(environment, span, &target, arguments, expected)
        }
        PatternKind::Tuple(items) => {
            let Type::Tuple(components) = expected else {
                return shape_mismatch(
                    environment,
                    span,
                    expected,
                    "a `tupleof` pattern",
                );
            };
            if items.len() != components.len() {
                call_contract_error(
                    environment,
                    span,
                    format!(
                        "a `tupleof` pattern for {expected} takes exactly {} components",
                        components.len()
                    ),
                );
                return None;
            }
            let components = components.clone();
            let mut checked = Vec::with_capacity(items.len());
            for (item, component) in items.iter().zip(&components) {
                checked.push(check_pattern(environment, item, component)?);
            }
            Some(Pattern::Tuple(checked))
        }
        PatternKind::RecordOf(fields) => {
            let Type::Record(members) = expected else {
                return shape_mismatch(
                    environment,
                    span,
                    expected,
                    "a `recordof` pattern",
                );
            };
            let members = members.clone();
            check_record_fields(environment, span, fields, &members)
                .map(Pattern::Record)
        }
        PatternKind::EnumOf(variant) => {
            let Type::Enum(variants) = expected else {
                return shape_mismatch(
                    environment,
                    span,
                    expected,
                    "an `enumof` pattern",
                );
            };
            let variants = variants.clone();
            let Some(label) = variant.label() else {
                call_contract_error(
                    environment,
                    span,
                    "an `enumof` pattern takes one labelled operand".to_owned(),
                );
                return None;
            };
            let Some((name, payload_type)) =
                variants.iter().find(|(name, _)| name == label.value())
            else {
                environment.diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::NameUnknownSymbol,
                        variant.span(),
                        format!("{expected} has no variant `{}`", label.value()),
                    )
                    .with_source_id(environment.source_id),
                );
                return None;
            };
            let payload = check_pattern(environment, variant.pattern(), payload_type)?;
            // A `void` payload has no value to inspect, but a binder written
            // for it still binds `void`.
            let binds = !matches!(payload, Pattern::Wildcard);
            Some(Pattern::Variant {
                variant: name.clone(),
                payload: (*payload_type != Type::Void || binds)
                    .then(|| Box::new(payload)),
            })
        }
        PatternKind::Array(items) => {
            let Type::Array(element) = expected else {
                return shape_mismatch(
                    environment,
                    span,
                    expected,
                    "an `array` pattern",
                );
            };
            let element = (**element).clone();
            let mut checked = Vec::with_capacity(items.len());
            for item in items {
                checked.push(check_pattern(environment, item, &element)?);
            }
            Some(Pattern::Array(checked))
        }
        PatternKind::As {
            value_type,
            pattern: payload,
        } => {
            let Some(members) = crate::union::members(environment.types, expected)
            else {
                environment.diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::TypeNarrowingNonUnion,
                        span,
                        format!("an `as` pattern narrows a union, not {expected}"),
                    )
                    .with_source_id(environment.source_id),
                );
                return None;
            };
            let member = environment.types.lower_or_report(
                environment.source_id,
                crate::nominal::Scope::new(
                    environment.self_type.as_ref(),
                    &environment.generics,
                ),
                value_type,
                span,
                environment.diagnostics,
            )?;
            let Some(index) = members
                .iter()
                .position(|candidate| candidate.same_shape(&member))
            else {
                environment.diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::TypeNotAUnionMember,
                        span,
                        format!("{member} is not a member of {expected}"),
                    )
                    .with_source_id(environment.source_id),
                );
                return None;
            };
            let inner = check_pattern(environment, payload, &member)?;
            Some(Pattern::Member {
                index,
                member,
                pattern: Box::new(inner),
            })
        }
    }
}

fn check_literal_pattern(
    environment: &mut CheckEnvironment<'_>,
    span: ByteSpan,
    literal: &Literal,
    expected: &Type,
) -> Option<Pattern> {
    if matches!(literal, Literal::Float(_) | Literal::Void(_)) {
        let actual = match literal {
            Literal::Float(_) if *expected == Type::F32 => Type::F32,
            Literal::Float(_) => Type::F64,
            _ => Type::Void,
        };
        mismatch(
            environment.diagnostics,
            environment.source_id,
            span,
            expected.clone(),
            actual,
            "float and `void` literals are not patterns",
        );
        return None;
    }
    check_literal(
        environment.source_id,
        span,
        literal,
        Some(expected.clone()),
        environment.diagnostics,
    )
    .map(Pattern::Literal)
}

fn shape_mismatch(
    environment: &mut CheckEnvironment<'_>,
    span: ByteSpan,
    expected: &Type,
    what: &str,
) -> Option<Pattern> {
    environment.diagnostics.push(
        Diagnostic::new(
            DiagnosticCode::TypeMismatch,
            span,
            format!("{what} cannot match a value of type {expected}"),
        )
        .with_source_id(environment.source_id),
    );
    None
}

fn check_constructor_pattern(
    environment: &mut CheckEnvironment<'_>,
    span: ByteSpan,
    target: &ConstructorTarget,
    arguments: &[PatternArgument],
    expected: &Type,
) -> Option<Pattern> {
    let (index, variant) = match target {
        ConstructorTarget::Type(index) => (*index, None),
        ConstructorTarget::Variant(index, variant) => (*index, Some(variant.as_str())),
    };
    let declared = environment.types.get(index)?.clone();
    let representation = environment.types.representation(index);
    let matches_expected = match expected {
        Type::Declared(id) | Type::Applied(id, _) => *id == declared.id,
        _ => representation.as_ref() == Some(expected),
    };
    if !matches_expected {
        mismatch(
            environment.diagnostics,
            environment.source_id,
            span,
            expected.clone(),
            declared_self_type(&declared.id, &declared.parameters),
            "the constructor pattern does not match the expected type",
        );
        return None;
    }
    // A declaration whose body failed to lower already reported why.
    let body = instantiated_body(environment.types, expected)?;
    match (body, variant) {
        (TypeBody::Record(members), None) => {
            check_record_fields(environment, span, arguments, &members)
                .map(Pattern::Record)
        }
        (TypeBody::Tuple(components), None) => {
            if arguments.len() != components.len()
                || arguments.iter().any(|argument| argument.label().is_some())
            {
                call_contract_error(
                    environment,
                    span,
                    format!(
                        "a `{}` pattern takes exactly {} unlabelled operands, one per component",
                        declared.name,
                        components.len()
                    ),
                );
                return None;
            }
            let mut checked = Vec::with_capacity(components.len());
            for (argument, component) in arguments.iter().zip(&components) {
                checked.push(check_pattern(
                    environment,
                    argument.pattern(),
                    component,
                )?);
            }
            Some(Pattern::Tuple(checked))
        }
        (TypeBody::Wrapper(representation), None) => {
            let argument =
                single_positional(environment, span, arguments, "a wrapper pattern")?;
            let inner = check_pattern(environment, argument, &representation)?;
            Some(Pattern::Wrap(Box::new(inner)))
        }
        (TypeBody::Enum(variants), Some(variant)) => {
            let Some((_, payload_type)) =
                variants.iter().find(|(name, _)| name == variant)
            else {
                environment.diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::NameUnknownSymbol,
                        span,
                        format!("`{}` has no variant `{variant}`", declared.name),
                    )
                    .with_source_id(environment.source_id),
                );
                return None;
            };
            let payload = if *payload_type == Type::Void {
                if !arguments.is_empty() {
                    call_contract_error(
                        environment,
                        span,
                        format!(
                            "variant `{variant}` has a `void` payload and its pattern takes no operand"
                        ),
                    );
                    return None;
                }
                None
            } else {
                let argument = single_positional(
                    environment,
                    span,
                    arguments,
                    "an enum variant pattern",
                )?;
                Some(Box::new(check_pattern(
                    environment,
                    argument,
                    payload_type,
                )?))
            };
            // A `bool` variant is the literal of its representation.
            if *expected == Type::Bool && payload.is_none() {
                return Some(Pattern::Literal(Value::Bool(variant == "true")));
            }
            Some(Pattern::Variant {
                variant: variant.to_owned(),
                payload,
            })
        }
        (TypeBody::Union(_), _) => {
            environment.diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::NameWrongEntityKind,
                    span,
                    format!(
                        "`{}` is a union type; narrow it with an `as` pattern",
                        declared.name
                    ),
                )
                .with_source_id(environment.source_id),
            );
            None
        }
        (TypeBody::Enum(_), None) => {
            environment.diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::NameWrongEntityKind,
                    span,
                    format!(
                        "`{}` is an enum type; match one of its variants",
                        declared.name
                    ),
                )
                .with_source_id(environment.source_id),
            );
            None
        }
        (
            TypeBody::Record(_) | TypeBody::Wrapper(_) | TypeBody::Tuple(_),
            Some(member),
        ) => {
            environment.diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::NameUnknownSymbol,
                    span,
                    format!("`{}` has no variant `{member}`", declared.name),
                )
                .with_source_id(environment.source_id),
            );
            None
        }
    }
}

/// Checks labelled record-pattern fields; omitted fields match anything.
/// The result lists the written fields in the type's field order.
fn check_record_fields(
    environment: &mut CheckEnvironment<'_>,
    span: ByteSpan,
    arguments: &[PatternArgument],
    members: &[(String, Type)],
) -> Option<Vec<(String, Pattern)>> {
    let mut seen = BTreeSet::new();
    let mut written = Vec::with_capacity(arguments.len());
    for argument in arguments {
        let Some(label) = argument.label() else {
            call_contract_error(
                environment,
                span,
                "record pattern fields are labelled".to_owned(),
            );
            return None;
        };
        let Some((name, field_type)) =
            members.iter().find(|(name, _)| name == label.value())
        else {
            environment.diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::TypeUnknownRecordField,
                    argument.span(),
                    format!("the record has no field `{}`", label.value()),
                )
                .with_source_id(environment.source_id),
            );
            return None;
        };
        if !seen.insert(name.clone()) {
            call_contract_error(
                environment,
                argument.span(),
                format!("record pattern field `{name}` is written twice"),
            );
            return None;
        }
        written.push((
            name.clone(),
            check_pattern(environment, argument.pattern(), field_type)?,
        ));
    }
    Some(
        members
            .iter()
            .filter_map(|(name, _)| {
                written
                    .iter()
                    .position(|(field, _)| field == name)
                    .map(|index| written.swap_remove(index))
            })
            .collect(),
    )
}

fn single_positional<'a>(
    environment: &mut CheckEnvironment<'_>,
    span: ByteSpan,
    arguments: &'a [PatternArgument],
    what: &str,
) -> Option<&'a vibra_syntax::Pattern> {
    match arguments {
        [argument] if argument.label().is_none() => Some(argument.pattern()),
        _ => {
            call_contract_error(
                environment,
                span,
                format!("{what} takes exactly one unlabelled operand"),
            );
            None
        }
    }
}

/// The body of a declared or applied type with its arguments substituted,
/// or the declared body of a role type the compiler represents directly.
pub(crate) fn instantiated_body(
    types: &TypeNames,
    value_type: &Type,
) -> Option<TypeBody> {
    let (id, arguments) = match value_type {
        Type::Declared(id) => (id, &[][..]),
        Type::Applied(id, arguments) => (id, arguments.as_slice()),
        _ => return types.representation_body(value_type),
    };
    let declared = types.get(types.index_of(id)?)?;
    let body = declared.body.clone()?;
    vibra_ir::TypeDefinition::new(declared.id.clone(), body)
        .with_parameters(declared.parameters.clone())
        .instantiate(arguments)
}

// ---------------------------------------------------------------------------
// Usefulness
// ---------------------------------------------------------------------------

/// One constructor of a type's value space.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Constructor {
    /// The one constructor of a record, tuple, wrapper, or `void` value.
    Single,
    /// An enum variant.
    Variant(String),
    /// A literal of `bool` or of an infinite primitive space.
    Literal(Value),
    /// An array of exactly this length.
    Array(usize),
    /// The union member with this discriminant.
    Member(usize),
}

/// The shape of a type's value space for the usefulness engine.
enum Space {
    /// Values of one record type, fields in type order.
    Record(Vec<(String, Type)>),
    /// Values of one tuple type.
    Tuple(Vec<Type>),
    /// Values of one wrapper type.
    Wrapper(Type),
    /// The finite variants of an enum, in declaration order.
    Enum(Vec<(String, Type)>),
    /// `bool`.
    Bool,
    /// `void`.
    Void,
    /// Arrays of one element type, by length.
    Array(Type),
    /// The members of a union, in discriminant order.
    Union(Vec<Type>),
    /// The singleton type of one atom.
    Singleton(String),
    /// Any other type: only a binder or discard covers it.
    Infinite,
}

fn space(types: &TypeNames, value_type: &Type) -> Space {
    match value_type {
        Type::Bool => Space::Bool,
        Type::Void => Space::Void,
        Type::Record(members) => Space::Record(members.clone()),
        Type::Tuple(components) => Space::Tuple(components.clone()),
        Type::Enum(variants) => Space::Enum(variants.clone()),
        Type::Array(element) => Space::Array((**element).clone()),
        Type::Union(members) => Space::Union(members.clone()),
        // A string or byte sequence is one wrapper over its scalars or bytes,
        // whose literals are an infinite subset.
        Type::Str | Type::Bytes => types
            .representation_body(value_type)
            .and_then(|body| match body {
                TypeBody::Wrapper(representation) => {
                    Some(Space::Wrapper(representation))
                }
                _ => None,
            })
            .unwrap_or(Space::Infinite),
        Type::AtomSingleton(atom) => Space::Singleton(atom.clone()),
        Type::Declared(_) | Type::Applied(_, _) => {
            match instantiated_body(types, value_type) {
                Some(TypeBody::Record(members)) => Space::Record(members),
                Some(TypeBody::Tuple(components)) => Space::Tuple(components),
                Some(TypeBody::Wrapper(representation)) => {
                    Space::Wrapper(representation)
                }
                Some(TypeBody::Enum(variants)) => Space::Enum(variants),
                Some(TypeBody::Union(members)) => Space::Union(members),
                None => Space::Infinite,
            }
        }
        _ => Space::Infinite,
    }
}

/// Every constructor of a finite space, in declaration order; `None` for an
/// infinite one.
fn all_constructors(space: &Space) -> Option<Vec<Constructor>> {
    match space {
        Space::Record(_) | Space::Tuple(_) | Space::Wrapper(_) | Space::Void => {
            Some(vec![Constructor::Single])
        }
        Space::Enum(variants) => Some(
            variants
                .iter()
                .map(|(name, _)| Constructor::Variant(name.clone()))
                .collect(),
        ),
        Space::Union(members) => {
            Some((0..members.len()).map(Constructor::Member).collect())
        }
        Space::Singleton(atom) => {
            Some(vec![Constructor::Literal(Value::Atom(atom.clone()))])
        }
        Space::Bool => Some(vec![
            Constructor::Literal(Value::Bool(false)),
            Constructor::Literal(Value::Bool(true)),
        ]),
        Space::Array(_) | Space::Infinite => None,
    }
}

/// The component types a constructor exposes to the next columns.
fn arity_types(space: &Space, constructor: &Constructor) -> Vec<Type> {
    match (space, constructor) {
        (Space::Record(members), Constructor::Single) => {
            members.iter().map(|(_, member)| member.clone()).collect()
        }
        (Space::Tuple(components), Constructor::Single) => components.clone(),
        (Space::Wrapper(representation), Constructor::Single) => {
            vec![representation.clone()]
        }
        (Space::Enum(variants), Constructor::Variant(variant)) => variants
            .iter()
            .find(|(name, _)| name == variant)
            .filter(|(_, payload)| *payload != Type::Void)
            .map(|(_, payload)| vec![payload.clone()])
            .unwrap_or_default(),
        (Space::Array(element), Constructor::Array(length)) => {
            vec![element.clone(); *length]
        }
        (Space::Union(members), Constructor::Member(index)) => {
            members.get(*index).cloned().into_iter().collect()
        }
        _ => Vec::new(),
    }
}

fn head_constructor(pattern: &Pattern) -> Option<Constructor> {
    match pattern {
        Pattern::Wildcard | Pattern::Bind { .. } => None,
        Pattern::Literal(value) => Some(Constructor::Literal(value.clone())),
        Pattern::Variant { variant, .. } => Some(Constructor::Variant(variant.clone())),
        Pattern::Record(_) | Pattern::Tuple(_) | Pattern::Wrap(_) => {
            Some(Constructor::Single)
        }
        Pattern::Array(items) => Some(Constructor::Array(items.len())),
        Pattern::Member { index, .. } => Some(Constructor::Member(*index)),
    }
}

/// The subpatterns of `pattern` under `constructor`, or `None` when it
/// cannot match that constructor.
fn specialize_head(
    space: &Space,
    pattern: &Pattern,
    constructor: &Constructor,
) -> Option<Vec<Pattern>> {
    let arity = arity_types(space, constructor).len();
    match pattern {
        Pattern::Wildcard | Pattern::Bind { .. } => {
            Some(vec![Pattern::Wildcard; arity])
        }
        // A string or byte wrapper whose operand binds covers every literal.
        Pattern::Wrap(inner)
            if matches!(constructor, Constructor::Literal(_))
                && matches!(**inner, Pattern::Wildcard | Pattern::Bind { .. }) =>
        {
            Some(Vec::new())
        }
        _ if head_constructor(pattern).as_ref() != Some(constructor) => None,
        Pattern::Literal(_) => Some(Vec::new()),
        Pattern::Variant { payload, .. } => Some(payload.as_deref().map_or_else(
            || vec![Pattern::Wildcard; arity],
            |payload| vec![payload.clone()],
        )),
        Pattern::Record(fields) => {
            let Space::Record(members) = space else {
                return None;
            };
            Some(
                members
                    .iter()
                    .map(|(name, _)| {
                        fields
                            .iter()
                            .find(|(field, _)| field == name)
                            .map_or(Pattern::Wildcard, |(_, field)| field.clone())
                    })
                    .collect(),
            )
        }
        Pattern::Tuple(items) | Pattern::Array(items) => Some(items.clone()),
        Pattern::Wrap(inner) => Some(vec![(**inner).clone()]),
        Pattern::Member { pattern, .. } => Some(vec![(**pattern).clone()]),
    }
}

fn specialize_row(
    space: &Space,
    row: &[Pattern],
    constructor: &Constructor,
) -> Option<Vec<Pattern>> {
    let (head, rest) = row.split_first()?;
    let mut specialized = specialize_head(space, head, constructor)?;
    specialized.extend_from_slice(rest);
    Some(specialized)
}

/// Rebuilds a witness row: the first `arity` patterns become the operands of
/// `constructor`.
fn rebuild(
    space: &Space,
    constructor: &Constructor,
    witness: Vec<Pattern>,
) -> Vec<Pattern> {
    let arity = arity_types(space, constructor).len();
    let mut witness = witness.into_iter();
    let operands: Vec<Pattern> = witness.by_ref().take(arity).collect();
    let head = match (space, constructor) {
        (Space::Record(members), Constructor::Single) => Pattern::Record(
            members
                .iter()
                .map(|(name, _)| name.clone())
                .zip(operands)
                .collect(),
        ),
        (Space::Tuple(_), Constructor::Single) => Pattern::Tuple(operands),
        (Space::Wrapper(_), Constructor::Single) => Pattern::Wrap(Box::new(
            operands.into_iter().next().unwrap_or(Pattern::Wildcard),
        )),
        (_, Constructor::Variant(variant)) => Pattern::Variant {
            variant: variant.clone(),
            payload: operands.into_iter().next().map(Box::new),
        },
        (_, Constructor::Literal(value)) => Pattern::Literal(value.clone()),
        (_, Constructor::Array(_)) => Pattern::Array(operands),
        (Space::Union(members), Constructor::Member(index)) => Pattern::Member {
            index: *index,
            member: members.get(*index).cloned().unwrap_or(Type::Void),
            pattern: Box::new(operands.into_iter().next().unwrap_or(Pattern::Wildcard)),
        },
        (_, Constructor::Member(_)) => Pattern::Wildcard,
        (_, Constructor::Single) => Pattern::Wildcard,
    };
    std::iter::once(head).chain(witness).collect()
}

/// Maranget's usefulness: whether `query` matches a value no row of
/// `matrix` matches. Returns one witness row, choosing the first uncovered
/// constructor in declaration order.
fn useful(
    types: &TypeNames,
    matrix: &[Vec<Pattern>],
    columns: &[Type],
    query: &[Pattern],
) -> Option<Vec<Pattern>> {
    let Some((column, rest_columns)) = columns.split_first() else {
        return matrix.is_empty().then(Vec::new);
    };
    let space = space(types, column);
    let head = query.first()?;
    let specialize_by = |constructor: &Constructor| {
        let rows: Vec<Vec<Pattern>> = matrix
            .iter()
            .filter_map(|row| specialize_row(&space, row, constructor))
            .collect();
        let mut next = arity_types(&space, constructor);
        next.extend_from_slice(rest_columns);
        let query = specialize_row(&space, query, constructor)?;
        useful(types, &rows, &next, &query)
            .map(|witness| rebuild(&space, constructor, witness))
    };
    if let Some(constructor) = head_constructor(head) {
        return specialize_by(&constructor);
    }
    let mut used: Vec<Constructor> = Vec::new();
    for row in matrix {
        if let Some(constructor) = row.first().and_then(head_constructor)
            && !used.contains(&constructor)
        {
            used.push(constructor);
        }
    }
    let all = all_constructors(&space);
    if let Some(all) = &all
        && all.iter().all(|constructor| used.contains(constructor))
    {
        return all.iter().find_map(specialize_by);
    }
    // Some constructor is missing: a value built with it is useful exactly
    // when the rest of the query is useful against the default matrix.
    let default: Vec<Vec<Pattern>> = matrix
        .iter()
        .filter(|row| {
            row.first()
                .is_some_and(|head| head_constructor(head).is_none())
        })
        .map(|row| row.get(1..).unwrap_or_default().to_vec())
        .collect();
    let witness = useful(
        types,
        &default,
        rest_columns,
        query.get(1..).unwrap_or_default(),
    )?;
    let missing = match (&all, &space) {
        (Some(all), _) => all
            .iter()
            .find(|constructor| !used.contains(constructor))
            .map(|constructor| {
                let arity = arity_types(&space, constructor).len();
                rebuild(&space, constructor, vec![Pattern::Wildcard; arity])
                    .into_iter()
                    .next()
                    .unwrap_or(Pattern::Wildcard)
            }),
        (None, Space::Array(_)) if !used.is_empty() => (0..)
            .find(|length| !used.contains(&Constructor::Array(*length)))
            .map(|length| Pattern::Array(vec![Pattern::Wildcard; length])),
        _ => None,
    };
    Some(
        std::iter::once(missing.unwrap_or(Pattern::Wildcard))
            .chain(witness)
            .collect(),
    )
}

/// The first value shape no pattern of `arms` covers, or `None` when the
/// arms are exhaustive for `value_type`.
pub(crate) fn uncovered(
    types: &TypeNames,
    arms: &[Pattern],
    value_type: &Type,
) -> Option<Pattern> {
    let matrix: Vec<Vec<Pattern>> = arms.iter().map(|arm| vec![arm.clone()]).collect();
    useful(
        types,
        &matrix,
        std::slice::from_ref(value_type),
        &[Pattern::Wildcard],
    )
    .and_then(|witness| witness.into_iter().next())
}

/// Whether `pattern` matches some value no pattern of `earlier` matches.
pub(crate) fn reachable(
    types: &TypeNames,
    earlier: &[Pattern],
    pattern: &Pattern,
    value_type: &Type,
) -> bool {
    let matrix: Vec<Vec<Pattern>> =
        earlier.iter().map(|arm| vec![arm.clone()]).collect();
    useful(
        types,
        &matrix,
        std::slice::from_ref(value_type),
        std::slice::from_ref(pattern),
    )
    .is_some()
}

/// Spells a witness in canonical pattern syntax: discards are `-`, and a
/// record, tuple, or wrapper whose operands are all discards is `-`.
pub(crate) fn spell(types: &TypeNames, pattern: &Pattern, value_type: &Type) -> String {
    let declared_name = || match value_type {
        Type::Declared(id) | Type::Applied(id, _) => types
            .index_of(id)
            .and_then(|index| types.get(index))
            .map(|declared| declared.name.clone()),
        _ => None,
    };
    let space = space(types, value_type);
    let all_wild =
        |items: &[&Pattern]| items.iter().all(|item| matches!(item, Pattern::Wildcard));
    match (pattern, &space) {
        (Pattern::Wildcard | Pattern::Bind { .. }, _) => "-".to_owned(),
        (Pattern::Literal(value), _) => literal_spelling(value),
        (Pattern::Variant { variant, payload }, Space::Enum(variants)) => {
            let payload_type = variants
                .iter()
                .find(|(name, _)| name == variant)
                .map(|(_, payload)| payload.clone())
                .unwrap_or(Type::Void);
            let payload = payload
                .as_deref()
                .map(|payload| format!(" {}", spell(types, payload, &payload_type)))
                .unwrap_or_default();
            match declared_name() {
                Some(name) => format!("({name}.{variant}{payload})"),
                None if payload.is_empty() => format!("(enumof {variant}: -)"),
                None => format!("(enumof {variant}:{payload})"),
            }
        }
        (Pattern::Record(fields), Space::Record(members)) => {
            if all_wild(&fields.iter().map(|(_, field)| field).collect::<Vec<_>>()) {
                return "-".to_owned();
            }
            let written = fields
                .iter()
                .filter(|(_, field)| !matches!(field, Pattern::Wildcard))
                .map(|(name, field)| {
                    let field_type = members
                        .iter()
                        .find(|(member, _)| member == name)
                        .map(|(_, member)| member.clone())
                        .unwrap_or(Type::Void);
                    format!(" {name}: {}", spell(types, field, &field_type))
                })
                .collect::<String>();
            let head = declared_name().unwrap_or_else(|| "recordof".to_owned());
            format!("({head}{written})")
        }
        (Pattern::Tuple(items), Space::Tuple(components)) => {
            if all_wild(&items.iter().collect::<Vec<_>>()) {
                return "-".to_owned();
            }
            let head = declared_name().unwrap_or_else(|| "tupleof".to_owned());
            let operands = items
                .iter()
                .zip(components)
                .map(|(item, component)| format!(" {}", spell(types, item, component)))
                .collect::<String>();
            format!("({head}{operands})")
        }
        (Pattern::Wrap(inner), Space::Wrapper(representation)) => {
            if matches!(**inner, Pattern::Wildcard) {
                return "-".to_owned();
            }
            let head = declared_name().unwrap_or_default();
            format!("({head} {})", spell(types, inner, representation))
        }
        (
            Pattern::Member {
                member, pattern, ..
            },
            _,
        ) => {
            format!("(as {member} {})", spell(types, pattern, member))
        }
        (Pattern::Array(items), Space::Array(element)) => format!(
            "(array{})",
            items
                .iter()
                .map(|item| format!(" {}", spell(types, item, element)))
                .collect::<String>()
        ),
        _ => "-".to_owned(),
    }
}

fn literal_spelling(value: &Value) -> String {
    value.canonical_vibon()
}

/// Every named binder a written pattern introduces.
pub(crate) fn binder_names(pattern: &vibra_syntax::Pattern) -> Vec<String> {
    let mut names = Vec::new();
    collect_binder_names(pattern, &mut names);
    names
}

fn collect_binder_names(pattern: &vibra_syntax::Pattern, names: &mut Vec<String>) {
    match pattern.kind() {
        PatternKind::Binding(name) if !name.is_discard() => {
            names.push(name.value().to_owned())
        }
        PatternKind::Constructor { arguments, .. }
        | PatternKind::RecordOf(arguments) => {
            for argument in arguments {
                collect_binder_names(argument.pattern(), names);
            }
        }
        PatternKind::Tuple(items) | PatternKind::Array(items) => {
            for item in items {
                collect_binder_names(item, names);
            }
        }
        PatternKind::EnumOf(variant) => collect_binder_names(variant.pattern(), names),
        PatternKind::As { pattern, .. } => collect_binder_names(pattern, names),
        PatternKind::Binding(_) | PatternKind::Literal(_) | PatternKind::Atom(_) => {}
    }
}

//! Labelled defaults written as expressions.
//!
//! `docs/spec/02-type-system.md`, "Constant patterns": a labelled default is a
//! constant expression, the same notion a constant pattern uses, in any form
//! and with a value of any type. A default that is not a literal or an atom is
//! decided here, once, after the module values are declared and before any
//! signature is checked, by the helper constant patterns use
//! ([`crate::pattern::constant_of_expression`]), so there is one definition of
//! what is constant.

use std::collections::BTreeMap;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_syntax::{
    Attribute, Declaration, Expression, ExpressionKind, FunctionDeclaration,
    LabelledDefault, LabelledParameter, NameKind, SourceAst, TypeMember,
};

use crate::nominal::{DefaultConstant, Scope, TypeNames};
use crate::{
    CheckEnvironment, GlobalHeader, ResolvedReferenceTarget, walk_expressions,
};

/// How a default's names reach module values.
pub(crate) enum DefaultNames<'a> {
    /// A single source: the names of its own module values.
    Source(&'a BTreeMap<String, usize>),
    /// A workspace: the resolver's references, keyed by source identity and
    /// span, which already reported an unknown name.
    Resolved(&'a BTreeMap<(String, usize, usize), ResolvedReferenceTarget>),
}

/// What every labelled default written as an expression in `sources` denotes.
pub(crate) fn decide(
    types: &TypeNames,
    sources: &[(&str, &SourceAst)],
    globals: &[GlobalHeader],
    names: &DefaultNames<'_>,
    module_names: &BTreeMap<String, ByteSpan>,
    diagnostics: &mut Vec<Diagnostic>,
) -> BTreeMap<(String, usize, usize), DefaultConstant> {
    let empty = BTreeMap::new();
    let mut decided = BTreeMap::new();
    for (source_id, ast) in sources {
        let mut defaults = Vec::new();
        for declaration in ast.declarations() {
            collect_declaration(declaration, &mut defaults);
        }
        // A function of the module is a name that is not a constant, and the
        // module's functions are declared after its values.
        let functions = ast
            .declarations()
            .iter()
            .filter_map(|declaration| match declaration {
                Declaration::Defn(function) => Some(function.name().value().to_owned()),
                _ => None,
            })
            .collect::<std::collections::BTreeSet<_>>();
        for entry in defaults {
            let LabelledDefault::Expression { expression, .. } = entry.default() else {
                continue;
            };
            let expected = types.lower_or_report(
                source_id,
                Scope::NONE,
                entry.value_type(),
                entry.span(),
                &mut Vec::new(),
            );
            let mut bindings = Vec::new();
            let indices = match names {
                DefaultNames::Source(indices) => *indices,
                DefaultNames::Resolved(_) => &empty,
            };
            let mut environment = CheckEnvironment::new(
                source_id,
                diagnostics,
                indices,
                globals,
                &[],
                &empty,
                module_names,
                &mut bindings,
                types,
            );
            if let DefaultNames::Resolved(references) = names {
                environment.resolved_targets = Some(references);
            }
            let constant =
                decide_one(&mut environment, names, &functions, expression, expected);
            decided.insert(
                (
                    (*source_id).to_owned(),
                    entry.default_span().start(),
                    entry.default_span().end(),
                ),
                constant,
            );
        }
    }
    decided
}

/// Whether `expression` has the shape of a constant expression, and the
/// module value it names when it is a bare name.
enum Shape {
    /// Literals, atoms, module values, and constructions of those.
    Constructive,
    /// Anything else: a call, a `lambda`, a `match`, and the like.
    Other,
}

fn decide_one(
    environment: &mut CheckEnvironment<'_>,
    names: &DefaultNames<'_>,
    functions: &std::collections::BTreeSet<String>,
    expression: &Expression,
    expected: Option<vibra_ir::Type>,
) -> DefaultConstant {
    let named = global_of(environment, names, expression);
    // A name that is no module value is a function or an unknown name; the
    // resolver, or this module's own names, reported an unknown one.
    if let ExpressionKind::Name(name) = expression.kind()
        && name.kind() == NameKind::Symbol
        && named.is_none()
    {
        if matches!(names, DefaultNames::Source(_)) && !functions.contains(name.value())
        {
            environment.diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::NameUnknownSymbol,
                    expression.span(),
                    "symbol does not resolve to a declaration",
                )
                .with_source_id(environment.source_id),
            );
            return DefaultConstant::Failed;
        }
        return DefaultConstant::NotConstant(None);
    }
    if matches!(shape(environment, expression), Shape::Other) {
        return DefaultConstant::NotConstant(None);
    }
    let Some(checked) = crate::check_expression(environment, expression, expected)
    else {
        return DefaultConstant::Failed;
    };
    match crate::pattern::constant_of_expression(environment, &checked, 0) {
        Some(constant) => DefaultConstant::Value(constant),
        None => DefaultConstant::NotConstant(named.and_then(|index| {
            environment
                .globals
                .get(index)
                .map(|header| (header.source_id.clone(), header.span))
        })),
    }
}

/// The module value a bare name denotes, when it denotes one.
fn global_of(
    environment: &CheckEnvironment<'_>,
    names: &DefaultNames<'_>,
    expression: &Expression,
) -> Option<usize> {
    let ExpressionKind::Name(name) = expression.kind() else {
        return None;
    };
    if name.kind() != NameKind::Symbol {
        return None;
    }
    match names {
        DefaultNames::Source(indices) => (name.segments().len() == 1)
            .then(|| indices.get(name.value()).copied())
            .flatten(),
        DefaultNames::Resolved(references) => match references.get(&(
            environment.source_id.to_owned(),
            expression.span().start(),
            expression.span().end(),
        )) {
            Some(ResolvedReferenceTarget::Global(index)) => Some(*index),
            _ => None,
        },
    }
}

/// The shape of `expression`: constructive when every part is a literal, an
/// atom, a name, a constructor application, or a `tupleof`, `recordof`, or
/// `enumof` of such parts.
fn shape(environment: &CheckEnvironment<'_>, expression: &Expression) -> Shape {
    match expression.kind() {
        ExpressionKind::Literal(_) | ExpressionKind::Name(_) => Shape::Constructive,
        ExpressionKind::Application(application) => {
            let ExpressionKind::Name(callee) = application.callee().kind() else {
                return Shape::Other;
            };
            if environment
                .types
                .constructor(environment.source_id, callee)
                .is_none()
            {
                return Shape::Other;
            }
            all_constructive(
                environment,
                application
                    .arguments()
                    .iter()
                    .map(|argument| argument.value()),
            )
        }
        ExpressionKind::TupleOf(values) => all_constructive(environment, values.iter()),
        ExpressionKind::RecordOf(fields) => {
            all_constructive(environment, fields.iter().map(|field| field.value()))
        }
        ExpressionKind::EnumOf(variant) => {
            all_constructive(environment, std::iter::once(variant.value()))
        }
        _ => Shape::Other,
    }
}

fn all_constructive<'e>(
    environment: &CheckEnvironment<'_>,
    parts: impl Iterator<Item = &'e Expression>,
) -> Shape {
    for part in parts {
        if matches!(shape(environment, part), Shape::Other) {
            return Shape::Other;
        }
    }
    Shape::Constructive
}

fn collect_declaration<'a>(
    declaration: &'a Declaration,
    defaults: &mut Vec<&'a LabelledParameter>,
) {
    match declaration {
        Declaration::Defn(function) => collect_function(function, defaults),
        Declaration::Def(definition) => {
            collect_expression(definition.expression(), defaults);
        }
        Declaration::Test(test) => {
            for expression in test.expressions() {
                collect_expression(expression, defaults);
            }
        }
        Declaration::Deftype(declaration) => {
            for member in declaration.members() {
                collect_member(member, defaults);
            }
        }
        Declaration::Defint(declaration) => {
            for member in declaration.members() {
                collect_member(member, defaults);
            }
        }
        Declaration::Deffect(declaration) => {
            for function in declaration.members() {
                collect_function(function, defaults);
            }
        }
        Declaration::Import(_) => {}
    }
}

fn collect_member<'a>(
    member: &'a TypeMember,
    defaults: &mut Vec<&'a LabelledParameter>,
) {
    match member {
        TypeMember::Method(function) => collect_function(function, defaults),
        TypeMember::Implementation(block) => {
            for function in block.members() {
                collect_function(function, defaults);
            }
        }
    }
}

fn collect_function<'a>(
    function: &'a FunctionDeclaration,
    defaults: &mut Vec<&'a LabelledParameter>,
) {
    collect_attributes(function.attributes().items(), defaults);
    for expression in function.expressions() {
        collect_expression(expression, defaults);
    }
}

fn collect_expression<'a>(
    expression: &'a Expression,
    defaults: &mut Vec<&'a LabelledParameter>,
) {
    walk_expressions(expression, &mut |nested| {
        if let ExpressionKind::Lambda(lambda) = nested.kind() {
            collect_attributes(lambda.attributes().items(), defaults);
        }
    });
}

fn collect_attributes<'a>(
    attributes: &'a [Attribute],
    defaults: &mut Vec<&'a LabelledParameter>,
) {
    for attribute in attributes {
        if let Attribute::Labelled(entries) = attribute {
            defaults.extend(entries.iter());
        }
    }
}

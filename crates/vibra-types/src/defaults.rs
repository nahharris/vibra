//! Labelled defaults written as names.
//!
//! `docs/spec/02-type-system.md`, "Constant patterns": a labelled default is a
//! constant expression, the same notion a constant pattern uses. A default
//! written as a name denotes the constant module value it resolves to, and
//! the toolchain stores a default as a primitive value, so a name is decided
//! here, once, after the module values are declared and before any signature
//! is checked. The decision reuses [`crate::pattern::constant_pattern`], so
//! there is one definition of what is constant.

use std::collections::BTreeMap;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::Pattern;
use vibra_syntax::{
    Attribute, Declaration, Expression, ExpressionKind, FunctionDeclaration,
    LabelledDefault, SourceAst, TypeMember,
};

use crate::nominal::{DefaultConstant, TypeNames};
use crate::{
    CheckEnvironment, GlobalHeader, ResolvedReferenceTarget, walk_expressions,
};

/// How a default's name reaches a module value.
pub(crate) enum DefaultNames<'a> {
    /// A single source: the names of its own module values.
    Source(&'a BTreeMap<String, usize>),
    /// A workspace: the resolver's references, keyed by source identity and
    /// span, which already reported an unknown name.
    Resolved(&'a BTreeMap<(String, usize, usize), ResolvedReferenceTarget>),
}

/// What every labelled default written as a name in `sources` denotes.
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
        for (name, span) in defaults {
            let target = match names {
                DefaultNames::Source(indices) => {
                    let found = (name.segments().len() == 1)
                        .then(|| indices.get(name.value()).copied())
                        .flatten();
                    // A module function is a name, but not a constant.
                    let function = ast.declarations().iter().any(|declaration| {
                        matches!(declaration, Declaration::Defn(function)
                            if function.name().value() == name.value())
                    });
                    if found.is_none() && !function {
                        diagnostics.push(
                            Diagnostic::new(
                                DiagnosticCode::NameUnknownSymbol,
                                span,
                                "symbol does not resolve to a declaration",
                            )
                            .with_source_id(*source_id),
                        );
                    }
                    found
                }
                DefaultNames::Resolved(references) => {
                    match references.get(&(
                        (*source_id).to_owned(),
                        span.start(),
                        span.end(),
                    )) {
                        Some(ResolvedReferenceTarget::Global(index)) => Some(*index),
                        _ => None,
                    }
                }
            };
            let mut scratch = Vec::new();
            let mut bindings = Vec::new();
            let constant = target
                .and_then(|index| {
                    let indices = match names {
                        DefaultNames::Source(indices) => *indices,
                        DefaultNames::Resolved(_) => &empty,
                    };
                    let mut environment = CheckEnvironment::new(
                        source_id,
                        &mut scratch,
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
                    crate::pattern::constant_pattern(&environment, index, 0)
                })
                .map_or(DefaultConstant::NotConstant, |pattern| match pattern {
                    Pattern::Literal(value) => DefaultConstant::Value(value),
                    _ => DefaultConstant::NotConstant,
                });
            decided.insert(
                ((*source_id).to_owned(), span.start(), span.end()),
                constant,
            );
        }
    }
    decided
}

fn collect_declaration(
    declaration: &Declaration,
    defaults: &mut Vec<(vibra_syntax::Name, ByteSpan)>,
) {
    match declaration {
        Declaration::Defn(function) => collect_function(function, defaults),
        Declaration::Def(definition) => {
            collect_expression(definition.expression(), defaults)
        }
        Declaration::Test(test) => {
            for expression in test.expressions() {
                collect_expression(expression, defaults);
            }
        }
        Declaration::Deftype(declaration) => {
            for member in declaration.members() {
                match member {
                    TypeMember::Method(function) => {
                        collect_function(function, defaults)
                    }
                    TypeMember::Implementation(block) => {
                        for function in block.members() {
                            collect_function(function, defaults);
                        }
                    }
                }
            }
        }
        Declaration::Defint(declaration) => {
            for member in declaration.members() {
                match member {
                    TypeMember::Method(function) => {
                        collect_function(function, defaults)
                    }
                    TypeMember::Implementation(block) => {
                        for function in block.members() {
                            collect_function(function, defaults);
                        }
                    }
                }
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

fn collect_function(
    function: &FunctionDeclaration,
    defaults: &mut Vec<(vibra_syntax::Name, ByteSpan)>,
) {
    collect_attributes(function.attributes().items(), defaults);
    for expression in function.expressions() {
        collect_expression(expression, defaults);
    }
}

fn collect_expression(
    expression: &Expression,
    defaults: &mut Vec<(vibra_syntax::Name, ByteSpan)>,
) {
    walk_expressions(expression, &mut |nested| {
        if let ExpressionKind::Lambda(lambda) = nested.kind() {
            collect_attributes(lambda.attributes().items(), defaults);
        }
    });
}

fn collect_attributes(
    attributes: &[Attribute],
    defaults: &mut Vec<(vibra_syntax::Name, ByteSpan)>,
) {
    for attribute in attributes {
        if let Attribute::Labelled(entries) = attribute {
            for entry in entries {
                if let LabelledDefault::Constant(name) = entry.default() {
                    defaults.push((name.clone(), entry.default_span()));
                }
            }
        }
    }
}

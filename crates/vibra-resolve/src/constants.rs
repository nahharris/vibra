//! The syntactic constant-expression test, and which module values are
//! constant.
//!
//! `docs/spec/02-type-system.md`, "Constant expressions and constant
//! patterns": a constant expression is a literal, an atom, or `void`; a
//! construction, tuple, record, list, or dict whose operands are all
//! constant expressions and whose type has no `where … else`; or the name of
//! a module value whose initializer is a constant expression. A function call
//! is never one, so the property is decidable without evaluation. Whether a
//! constructed type has a `where … else` is the checker's: resolution records
//! every construction of constants as a candidate. A module value whose
//! initializer reaches itself is not constant; the cycle is the checker's
//! `@type.initializerCycle`.

use vibra_syntax::source::{Expr, ExprForm, Primary, Suffix};

use crate::graph::{DeclView, Graph};
use crate::identity::DeclarationKind;
use crate::paths::{Entity, Scope, Walker};

/// What is known of one module value while the table is built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    /// Not looked at yet.
    Open,
    /// Being decided: an initializer that reaches it again is a cycle.
    Deciding,
    /// Decided.
    Known(bool),
}

/// Whether each declaration, by index, is a module value whose initializer
/// is a constant expression.
pub(crate) fn table(graph: &Graph<'_>) -> Vec<bool> {
    let mut states = vec![State::Open; graph.decls.len()];
    for index in 0..graph.decls.len() {
        decide(graph, index, &mut states);
    }
    states
        .into_iter()
        .map(|state| state == State::Known(true))
        .collect()
}

/// Decides the declaration `index`, and every module value its initializer
/// names, first.
fn decide(graph: &Graph<'_>, index: usize, states: &mut [State]) -> bool {
    match states.get(index) {
        Some(State::Known(known)) => return *known,
        Some(State::Deciding) | None => return false,
        Some(State::Open) => {}
    }
    let Some(decl) = graph.decls.get(index) else {
        return false;
    };
    let DeclView::Let(view) = decl.view else {
        if let Some(state) = states.get_mut(index) {
            *state = State::Known(false);
        }
        return false;
    };
    if let Some(state) = states.get_mut(index) {
        *state = State::Deciding;
    }
    let constant = view
        .value()
        .is_some_and(|value| constant(graph, decl.module, &value, states));
    if let Some(state) = states.get_mut(index) {
        *state = State::Known(constant);
    }
    constant
}

/// Whether `expr`, written at module level in `module`, is a constant
/// expression.
fn constant(
    graph: &Graph<'_>,
    module: usize,
    expr: &Expr<'_>,
    states: &mut [State],
) -> bool {
    let ExprForm::Postfix(postfix) = expr.form() else {
        return false;
    };
    let suffixes = postfix.suffixes();
    match postfix.primary() {
        Some(Primary::Literal(_)) => suffixes.is_empty(),
        Some(Primary::Tuple(items) | Primary::List(items)) => {
            suffixes.is_empty()
                && items
                    .iter()
                    .all(|item| constant(graph, module, item, states))
        }
        Some(Primary::Record(fields)) => {
            suffixes.is_empty()
                && fields
                    .iter()
                    .all(|(_, value)| constant(graph, module, value, states))
        }
        Some(Primary::Dict(entries)) => {
            suffixes.is_empty()
                && entries.iter().all(|(key, value)| {
                    constant(graph, module, key, states)
                        && constant(graph, module, value, states)
                })
        }
        Some(Primary::Name(first)) => {
            let mut parts = vec![first];
            let mut rest = suffixes.into_iter();
            let mut call = None;
            for suffix in rest.by_ref() {
                match suffix {
                    Suffix::Field(name) => parts.push(name),
                    Suffix::Call(arguments) => {
                        call = Some(arguments);
                        break;
                    }
                }
            }
            if rest.next().is_some() {
                return false;
            }
            let Some((entity, consumed)) = silent_path(graph, module, &parts) else {
                return false;
            };
            if consumed != parts.len() {
                return false;
            }
            let Entity::Decl(index) = entity else {
                return false;
            };
            let kind = graph.decls.get(index).map(|decl| decl.kind());
            match call {
                None => {
                    kind == Some(DeclarationKind::Value) && decide(graph, index, states)
                }
                Some(arguments) => {
                    matches!(kind, Some(DeclarationKind::Type | DeclarationKind::Case))
                        && arguments.iter().all(|argument| {
                            constant(graph, module, &argument.value, states)
                        })
                }
            }
        }
        _ => false,
    }
}

/// What the dotted name `parts` denotes at module level in `module`, and how
/// many of its parts the path takes, without reporting anything.
fn silent_path(
    graph: &Graph<'_>,
    module: usize,
    parts: &[vibra_syntax::source::Ident<'_>],
) -> Option<(Entity, usize)> {
    let mut out = Vec::new();
    let mut references = Vec::new();
    let mut walker = Walker::new(graph, &mut out, &mut references);
    walker.module = module;
    walker.value_path(&Scope::default(), parts)
}

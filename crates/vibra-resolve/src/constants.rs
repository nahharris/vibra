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
///
/// A module value is constant when its initializer is constant in form and
/// every module value it names is constant. The values a chain of
/// initializers names are decided on an explicit stack, so a chain as long
/// as the declarations of a workspace needs no host stack in proportion to
/// it; only the walk of one initializer recurses, over its nesting, which
/// the source nesting limit bounds.
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

/// One value being decided: its index, the values its initializer names,
/// and how many of them, in order, are known to be constant.
struct Frame {
    index: usize,
    names: Vec<usize>,
    next: usize,
}

/// Decides the declaration `root`, and first every module value its
/// initializer names. A value whose initializer reaches a value still being
/// decided reaches itself, and is not constant.
fn decide(graph: &Graph<'_>, root: usize, states: &mut [State]) {
    let mut stack = Vec::new();
    open(graph, root, states, &mut stack);
    while let Some(frame) = stack.last_mut() {
        let Some(&name) = frame.names.get(frame.next) else {
            // Every value it names is constant.
            let index = frame.index;
            stack.pop();
            set(states, index, State::Known(true));
            continue;
        };
        match states.get(name).copied() {
            Some(State::Known(true)) => frame.next += 1,
            // Decided first; the frame looks at the name again once it is.
            Some(State::Open) => open(graph, name, states, &mut stack),
            Some(State::Known(false) | State::Deciding) | None => {
                let index = frame.index;
                stack.pop();
                set(states, index, State::Known(false));
            }
        }
    }
}

/// Starts deciding the open declaration `index`: a value constant in form
/// is pushed with the values it names; anything else is decided now.
fn open(graph: &Graph<'_>, index: usize, states: &mut [State], stack: &mut Vec<Frame>) {
    if states.get(index) != Some(&State::Open) {
        return;
    }
    let mut names = Vec::new();
    let form = graph.decls.get(index).is_some_and(|decl| match decl.view {
        DeclView::Let(view) => view
            .value()
            .is_some_and(|value| constant(graph, decl.module, &value, &mut names)),
        _ => false,
    });
    if form {
        set(states, index, State::Deciding);
        stack.push(Frame {
            index,
            names,
            next: 0,
        });
    } else {
        set(states, index, State::Known(false));
    }
}

fn set(states: &mut [State], index: usize, state: State) {
    if let Some(slot) = states.get_mut(index) {
        *slot = state;
    }
}

/// Whether `expr`, written at module level in `module`, is a constant
/// expression in form, given that every module value it names is constant;
/// those values are appended to `names`, in order.
fn constant(
    graph: &Graph<'_>,
    module: usize,
    expr: &Expr<'_>,
    names: &mut Vec<usize>,
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
                    .all(|item| constant(graph, module, item, names))
        }
        Some(Primary::Record(fields)) => {
            suffixes.is_empty()
                && fields
                    .iter()
                    .all(|(_, value)| constant(graph, module, value, names))
        }
        Some(Primary::Dict(entries)) => {
            suffixes.is_empty()
                && entries.iter().all(|(key, value)| {
                    constant(graph, module, key, names)
                        && constant(graph, module, value, names)
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
                    let value = kind == Some(DeclarationKind::Value);
                    if value {
                        names.push(index);
                    }
                    value
                }
                Some(arguments) => {
                    matches!(kind, Some(DeclarationKind::Type | DeclarationKind::Case))
                        && arguments.iter().all(|argument| {
                            constant(graph, module, &argument.value, names)
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

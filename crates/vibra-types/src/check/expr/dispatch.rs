//! Contract calls, bound evidence, and widening.
//!
//! `docs/spec/02-type-system.md`, "Dispatch", "Interface values", "Bounds",
//! and "Widening". A contract call is a call whose callee's type parameters
//! are, in order, the interface's parameters, the member's own, `Self`, and
//! the associated types. The receiver slot, or the written expected type,
//! fixes `Self` first; the view of step 5 then fixes the interface's
//! arguments and every choice, and the ordinary joint inference fixes the
//! rest. One search for evidence serves dispatch, bounds, and widening, in
//! the order the checked IR tries it: a bound in scope, `Self` in a default
//! member, the one implementation whose key matches, and the structural
//! `Equatable` and `Ordered` of atoms, tuples, and records.

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::{
    ContractCall, Expr as IrExpr, FnType, Library, NamedTypes, Node, Substitution, Type,
};

use super::{BodyChecker, CallState, Out, Reference, Target, unify};
use crate::check::declare::BoundData;
use crate::check::interfaces::same_head;

/// The most goals one search for evidence visits.
const GOAL_LIMIT: usize = 4096;

/// The view a contract call is checked against in step 5.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum View {
    /// Through an interface value in the receiver slot.
    Public,
    /// On a type parameter by its bound, or on `Self` in a default member.
    Evidence,
    /// By the implementation `index`, at the types its binders take.
    Selected(usize, Vec<Option<Type>>),
    /// By the structural `Equatable` or `Ordered` of an atom, a tuple, or a
    /// record.
    Structural,
}

/// The dispatch of one contract call.
#[derive(Debug)]
pub(super) struct Dispatch {
    pub(super) interface: usize,
    pub(super) member: String,
    /// The number of the interface's parameters.
    pub(super) params: usize,
    /// The number of the member's own parameters.
    pub(super) own: usize,
    /// The written argument in the receiver slot, when the member has one.
    pub(super) receiver_arg: Option<usize>,
    /// Where an unsatisfied receiver is reported: the receiver operand or
    /// the written expected type.
    pub(super) receiver_span: ByteSpan,
    pub(super) view: Option<View>,
    /// The parameters outside the receiver slot whose type mentions `Self`.
    pub(super) self_params: Vec<usize>,
    /// The expected type at the call.
    pub(super) expected: Option<Type>,
}

impl Dispatch {
    /// The variable of `Self`.
    pub(super) const fn self_var(&self) -> usize {
        self.params.saturating_add(self.own)
    }

    /// The variable of the associated type `at`, in declaration order.
    const fn assoc_var(&self, at: usize) -> usize {
        self.self_var().saturating_add(1).saturating_add(at)
    }
}

/// What a search for evidence found.
#[derive(Debug)]
pub(super) enum Found {
    None,
    Many,
    One(Evidence),
}

/// Evidence that a receiver implements an interface.
#[derive(Debug)]
pub(super) struct Evidence {
    /// The interface's positional arguments it fixes.
    arguments: Vec<Option<Type>>,
    /// The choice of each associated type, in declaration order; `None`
    /// where the evidence leaves it abstract.
    choices: Vec<(String, Option<Type>)>,
    view: View,
}

/// A further goal the evidence rests on.
struct Goal {
    receiver: Type,
    interface: usize,
    arguments: Vec<Option<Type>>,
    required: Vec<(String, Type)>,
}

/// Matches `pattern`, whose binders are the variables of `assignment`,
/// against `actual`, in which a binder is a type parameter in scope and
/// matches only itself.
fn match_open(pattern: &Type, actual: &Type, assignment: &mut [Option<Type>]) -> bool {
    let mut pending = vec![(pattern, actual)];
    while let Some((pattern, actual)) = pending.pop() {
        if let Type::Binder(index) = pattern
            && let Some(slot) = assignment.get_mut(*index)
        {
            match slot {
                Some(bound) if bound != actual => return false,
                Some(_) => {}
                None => *slot = Some(actual.clone()),
            }
            continue;
        }
        if !same_head(pattern, actual) {
            return false;
        }
        pending.extend(pattern.components().into_iter().zip(actual.components()));
    }
    true
}

/// `ty` with each binder replaced by its type in `assignment`, when every
/// binder it mentions has one.
pub(super) fn closed(ty: &Type, assignment: &[Option<Type>]) -> Option<Type> {
    let mut open = false;
    ty.visit(&mut |inner| {
        if let Type::Binder(at) = inner {
            open |= !matches!(assignment.get(*at), Some(Some(_)));
        }
    });
    if open {
        return None;
    }
    let binders = assignment
        .iter()
        .enumerate()
        .map(|(at, bound)| bound.clone().unwrap_or(Type::Binder(at)))
        .collect::<Vec<_>>();
    Some(ty.instantiate(&binders))
}

/// The one type `expected` holds at every position of `pattern` written as
/// each of the binders `0..params`, walking every position where the two
/// have the same form; `None` for a binder with no such position or with
/// two that disagree.
fn positional_parts(
    pattern: &Type,
    expected: &Type,
    params: usize,
) -> Vec<Option<Type>> {
    let mut found: Vec<Option<Type>> = vec![None; params];
    let mut conflict = vec![false; params];
    let mut pending = vec![(pattern, expected)];
    while let Some((pattern, expected)) = pending.pop() {
        if *expected == Type::Never {
            continue;
        }
        if let Type::Binder(at) = pattern {
            if let (Some(slot), Some(clash)) =
                (found.get_mut(*at), conflict.get_mut(*at))
            {
                match slot {
                    Some(seen) if seen != expected => *clash = true,
                    Some(_) => {}
                    None => *slot = Some(expected.clone()),
                }
            }
            continue;
        }
        if same_head(pattern, expected) {
            pending.extend(pattern.components().into_iter().zip(expected.components()));
        }
    }
    found
        .into_iter()
        .zip(conflict)
        .map(|(found, clash)| found.filter(|_| !clash))
        .collect()
}

/// Fixes the binders of `pattern` from `actual` wherever the two have the
/// same form, and leaves the rest as they are.
fn match_part(pattern: &Type, actual: &Type, assignment: &mut [Option<Type>]) {
    let mut pending = vec![(pattern, actual)];
    while let Some((pattern, actual)) = pending.pop() {
        if let Type::Binder(index) = pattern {
            if let Some(slot @ None) = assignment.get_mut(*index) {
                *slot = Some(actual.clone());
            }
            continue;
        }
        if same_head(pattern, actual) {
            pending.extend(pattern.components().into_iter().zip(actual.components()));
        }
    }
}

/// Whether an implementation whose key has the positional `arguments`, open
/// in `count` binders, may be the one chosen at activation for a call at
/// `actual`, which is open in the `open` type parameters of the body, in
/// `Self`, and in associated types.
fn may_select(arguments: &[Type], count: usize, actual: &[Type], open: usize) -> bool {
    let mut names = Vec::new();
    for ty in actual {
        ty.visit(&mut |inner| {
            if let Type::Associated(name) = inner
                && !names.contains(name)
            {
                names.push(name.clone());
            }
        });
    }
    let self_var = Type::Binder(open);
    let choices = NamedTypes::new(names.into_iter().enumerate().map(|(at, name)| {
        (
            name,
            Type::Binder(open.saturating_add(1).saturating_add(at)),
        )
    }));
    let Ok(choices) = choices else {
        return true;
    };
    let substitution = Substitution {
        binders: &[],
        self_type: Some(&self_var),
        associated: Some(&choices),
    };
    let actual = actual
        .iter()
        .map(|ty| ty.substitute(&substitution))
        .collect::<Vec<_>>();
    let variables = open
        .saturating_add(1)
        .saturating_add(choices.entries().len());
    crate::check::interfaces::unifiable(&actual, variables, arguments, count)
}

/// Whether `ty` mentions a type parameter.
fn mentions_binder(ty: &Type) -> bool {
    let mut found = false;
    ty.visit(&mut |inner| found |= matches!(inner, Type::Binder(_)));
    found
}

/// Whether the known types of `arguments` equal `fixed`.
fn agree(fixed: &[Type], arguments: &[Option<Type>]) -> bool {
    fixed.len() == arguments.len()
        && fixed.iter().zip(arguments).all(|(fixed, argument)| {
            argument.as_ref().is_none_or(|argument| argument == fixed)
        })
}

/// The one type found at every position of `pattern` that holds the
/// variable `var`, walking only the positions of the `Self` map of
/// `expected`; `None` when a position is missing or two disagree.
fn self_positions(
    checker: &BodyChecker<'_, '_>,
    pattern: &Type,
    expected: &Type,
    var: usize,
) -> Option<Type> {
    let mentions = |ty: &Type| {
        let mut found = false;
        ty.visit(&mut |inner| found |= *inner == Type::Binder(var));
        found
    };
    let mut found: Option<Type> = None;
    let mut pending = vec![(pattern, expected)];
    while let Some((pattern, expected)) = pending.pop() {
        if *pattern == Type::Binder(var) {
            match &found {
                Some(found) if found != expected => return None,
                Some(_) => {}
                None => found = Some(expected.clone()),
            }
            continue;
        }
        if !mentions(pattern) {
            continue;
        }
        let library = |ty: &Type| match ty {
            Type::Declared(id, _) => checker.program.library_of(id),
            _ => None,
        };
        let walks = match pattern {
            Type::Tuple(_) | Type::Record(_) | Type::List(_) => true,
            Type::Declared(..) => {
                matches!(library(pattern), Some(Library::Option | Library::Result))
            }
            _ => false,
        };
        if !walks || !same_head(pattern, expected) {
            return None;
        }
        pending.extend(pattern.components().into_iter().zip(expected.components()));
    }
    found
}

impl<'a> BodyChecker<'_, 'a> {
    /// The written type span of the expected type at the expression
    /// `span`, when it is written.
    pub(super) fn written_span(&self, span: ByteSpan) -> Option<ByteSpan> {
        self.destinations
            .iter()
            .rev()
            .find(|(at, _)| *at == span)
            .map(|(_, written)| *written)
    }

    /// Reports `code` at `span`, related to `related` of `module`.
    fn report_related(
        &mut self,
        code: DiagnosticCode,
        span: ByteSpan,
        message: String,
        module: usize,
        related: ByteSpan,
        note: &str,
    ) {
        let diagnostic = Diagnostic::new(code, span, message)
            .with_source_id(self.source_id.as_ref());
        let diagnostic = self.program.related(diagnostic, module, related, note);
        self.diagnostics.push(diagnostic);
    }

    /// Reports that `receiver` does not implement `interface`, at `span`,
    /// related to the interface.
    fn unsatisfied(&mut self, span: ByteSpan, receiver: &Type, interface: usize) {
        let Some(data) = self.program.interfaces.get(interface) else {
            return;
        };
        let related = data
            .view
            .name()
            .map_or(data.view.construct().span(), |name| name.span());
        let message = format!("`{receiver}` does not implement `{}`", data.name);
        self.report_related(
            DiagnosticCode::TypeUnsatisfiedBound,
            span,
            message,
            data.module,
            related,
            "the interface",
        );
    }

    /// Reports that `actual` does not satisfy `bound`, a bound of a callee
    /// of `module`, at `span`, related to the bound.
    fn unsatisfied_bound(
        &mut self,
        span: ByteSpan,
        actual: &Type,
        bound: &BoundData,
        module: usize,
        detail: &str,
    ) {
        let name = self
            .program
            .interfaces
            .get(bound.interface)
            .map(|data| data.name.clone())
            .unwrap_or_default();
        let message = format!("`{actual}` does not satisfy the bound `{name}`{detail}");
        self.report_related(
            DiagnosticCode::TypeUnsatisfiedBound,
            span,
            message,
            module,
            bound.span,
            "the bound",
        );
    }

    /// The evidence that `receiver` implements `interface` at `arguments`,
    /// of which `None` are still undetermined.
    pub(super) fn evidence(
        &self,
        receiver: &Type,
        interface: usize,
        arguments: &[Option<Type>],
    ) -> Found {
        let (found, mut goals) = self.select_one(receiver, interface, arguments);
        if !matches!(found, Found::One(_)) {
            return found;
        }
        let mut visited = 0_usize;
        while let Some(goal) = goals.pop() {
            visited = visited.saturating_add(1);
            if visited > GOAL_LIMIT {
                return Found::None;
            }
            let (sub, more) =
                self.select_one(&goal.receiver, goal.interface, &goal.arguments);
            let Found::One(sub) = sub else {
                return Found::None;
            };
            let covered = goal.required.iter().all(|(name, ty)| {
                sub.choices
                    .iter()
                    .find(|(choice, _)| choice == name)
                    .and_then(|(_, chosen)| chosen.as_ref())
                    == Some(ty)
            });
            if !covered {
                return Found::None;
            }
            goals.extend(more);
        }
        found
    }

    /// The evidence for one goal and the goals it rests on.
    fn select_one(
        &self,
        receiver: &Type,
        interface: usize,
        arguments: &[Option<Type>],
    ) -> (Found, Vec<Goal>) {
        let program = self.program;
        let none = (Found::None, Vec::new());
        if receiver.is_interface_value() || *receiver == Type::Never {
            return none;
        }
        let Some(data) = program.interfaces.get(interface) else {
            return none;
        };
        let names = data
            .associated
            .iter()
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        if let Type::Binder(at) = receiver {
            let Some(Some(bound)) = self.bounds.get(*at) else {
                return none;
            };
            if bound.interface != interface || !agree(&bound.arguments, arguments) {
                return none;
            }
            let choices = names
                .into_iter()
                .map(|name| {
                    let chosen = bound
                        .choices
                        .iter()
                        .find(|(label, _)| *label == name)
                        .map(|(_, ty)| ty.clone());
                    (name, chosen)
                })
                .collect();
            let evidence = Evidence {
                arguments: bound.arguments.iter().cloned().map(Some).collect(),
                choices,
                view: View::Evidence,
            };
            return (Found::One(evidence), Vec::new());
        }
        if *receiver == Type::SelfType {
            let parameters =
                (0..data.params.len()).map(Type::Binder).collect::<Vec<_>>();
            if self.self_interface != Some(interface) || !agree(&parameters, arguments)
            {
                return none;
            }
            let choices = names
                .into_iter()
                .map(|name| {
                    let chosen = Type::Associated(name.clone());
                    (name, Some(chosen))
                })
                .collect();
            let evidence = Evidence {
                arguments: parameters.into_iter().map(Some).collect(),
                choices,
                view: View::Evidence,
            };
            return (Found::One(evidence), Vec::new());
        }
        let mut candidates = Vec::new();
        for (index, implementation) in program.impls.iter().enumerate() {
            if implementation.interface != interface
                || !implementation.usable
                || implementation.arguments.len() != arguments.len()
            {
                continue;
            }
            let mut assignment = vec![None; implementation.binders.len()];
            let matched =
                match_open(&implementation.receiver, receiver, &mut assignment)
                    && implementation.arguments.iter().zip(arguments).all(
                        |(pattern, actual)| {
                            actual.as_ref().is_none_or(|actual| {
                                match_open(pattern, actual, &mut assignment)
                            })
                        },
                    );
            if matched {
                candidates.push((index, assignment));
            }
        }
        if candidates.len() > 1 {
            return (Found::Many, Vec::new());
        }
        if let Some((index, assignment)) = candidates.pop() {
            let Some(implementation) = program.impls.get(index) else {
                return none;
            };
            let fill = |ty: &Type| closed(ty, &assignment);
            let choices = names
                .into_iter()
                .map(|name| {
                    let chosen = implementation
                        .choices
                        .iter()
                        .find(|(choice, _)| *choice == name)
                        .and_then(|(_, ty)| fill(ty));
                    (name, chosen)
                })
                .collect();
            let mut goals = Vec::new();
            for (bound, actual) in implementation.bounds.iter().zip(&assignment) {
                if let (Some(bound), Some(actual)) = (bound, actual) {
                    goals.push(Goal {
                        receiver: actual.clone(),
                        interface: bound.interface,
                        arguments: bound.arguments.iter().map(fill).collect(),
                        required: bound
                            .choices
                            .iter()
                            .filter_map(|(name, ty)| Some((name.clone(), fill(ty)?)))
                            .collect(),
                    });
                }
            }
            let evidence = Evidence {
                arguments: implementation.arguments.iter().map(fill).collect(),
                choices,
                view: View::Selected(index, assignment.clone()),
            };
            return (Found::One(evidence), goals);
        }
        let structural = ["Equatable", "Ordered"]
            .into_iter()
            .any(|name| program.prelude_interface(name) == Some(interface));
        let shaped = matches!(
            receiver,
            Type::Atom | Type::AtomSingleton(_) | Type::Tuple(_) | Type::Record(_)
        );
        if structural && arguments.is_empty() && shaped {
            let goals = receiver
                .components()
                .into_iter()
                .map(|component| Goal {
                    receiver: component.clone(),
                    interface,
                    arguments: Vec::new(),
                    required: Vec::new(),
                })
                .collect();
            let evidence = Evidence {
                arguments: Vec::new(),
                choices: Vec::new(),
                view: View::Structural,
            };
            return (Found::One(evidence), goals);
        }
        none
    }

    /// The type parameter names, the signature, and the dispatch of a call
    /// of `member` of `interface`, its type parameters numbered as the
    /// module docs say.
    pub(super) fn contract_signature(
        &self,
        interface: usize,
        member: &str,
        application: ByteSpan,
    ) -> Option<(Vec<String>, FnType, Dispatch)> {
        let data = self.program.interfaces.get(interface)?;
        let declared = data.member(member)?;
        let params = data.params.len();
        let own = declared.binders.len().saturating_sub(params);
        let self_var = params.saturating_add(own);
        let binders = (0..self_var).map(Type::Binder).collect::<Vec<_>>();
        let choices = NamedTypes::new(data.associated.iter().enumerate().map(
            |(at, (name, _))| {
                (
                    name.clone(),
                    Type::Binder(self_var.saturating_add(1).saturating_add(at)),
                )
            },
        ))
        .ok()?;
        let substitution = Substitution {
            binders: &binders,
            self_type: Some(&Type::Binder(self_var)),
            associated: Some(&choices),
        };
        let substituted =
            Type::function(declared.signature.clone()).substitute(&substitution);
        let Type::Fn(signature) = &substituted else {
            return None;
        };
        let signature = signature.as_ref().clone();
        let mut names = declared.binders.clone();
        names.push("Self".to_owned());
        names.extend(data.associated.iter().map(|(name, _)| name.clone()));
        let self_params = declared
            .signature
            .parameters
            .iter()
            .enumerate()
            .filter(|(at, ty)| {
                Some(*at) != declared.receiver
                    && crate::check::interfaces::mentions_self(ty)
            })
            .map(|(at, _)| at)
            .collect();
        let dispatch = Dispatch {
            interface,
            member: member.to_owned(),
            params,
            own,
            receiver_arg: None,
            receiver_span: application,
            view: None,
            self_params,
            expected: None,
        };
        Some((names, signature, dispatch))
    }

    /// Starts the dispatch of a contract call once its arguments are bound:
    /// its receiver operand is checked first, or the written expected type
    /// fixes the receiver (destination dispatch).
    pub(super) fn contract_start(
        &mut self,
        state: &mut CallState<'a>,
        expected: Option<&Type>,
    ) {
        let Some(mut dispatch) = state.dispatch.take() else {
            return;
        };
        dispatch.expected = expected.cloned();
        let slot = self
            .program
            .interfaces
            .get(dispatch.interface)
            .and_then(|data| data.member(&dispatch.member))
            .and_then(|member| member.receiver);
        match slot {
            Some(slot) => {
                dispatch.receiver_arg =
                    state.binding.iter().position(|bound| *bound == Some(slot));
                match dispatch.receiver_arg {
                    Some(receiver) => {
                        state.order.retain(|at| *at != receiver);
                        state.order.insert(0, receiver);
                    }
                    None => state.failed = true,
                }
                state.dispatch = Some(dispatch);
            }
            None => {
                let application = state.application;
                let Some(expected) = expected else {
                    self.report(
                        DiagnosticCode::TypeAmbiguousDestination,
                        application,
                        "this member is dispatched on the written expected type, and none is written here",
                    );
                    state.failed = true;
                    state.dispatch = Some(dispatch);
                    return;
                };
                dispatch.receiver_span =
                    self.written_span(application).unwrap_or(application);
                let var = dispatch.self_var();
                let found = state
                    .result
                    .as_ref()
                    .and_then(|result| self_positions(self, result, expected, var));
                let Some(receiver) = found else {
                    self.report(
                        DiagnosticCode::TypeMismatch,
                        application,
                        format!(
                            "the expected type `{expected}` gives no single receiver at the positions of `Self` in this member's result"
                        ),
                    );
                    state.failed = true;
                    state.dispatch = Some(dispatch);
                    return;
                };
                if let Some(slot) = state.assignment.get_mut(var) {
                    *slot = Some(receiver.clone());
                }
                let span = dispatch.receiver_span;
                state.dispatch = Some(dispatch);
                self.decide_view(state, &receiver, span);
            }
        }
    }

    /// Takes the type of the receiver operand.
    pub(super) fn receiver_arrived(
        &mut self,
        state: &mut CallState<'a>,
        receiver: &Type,
        span: ByteSpan,
    ) {
        let Some(dispatch) = state.dispatch.as_mut() else {
            return;
        };
        dispatch.receiver_span = span;
        let var = dispatch.self_var();
        if let Some(slot) = state.assignment.get_mut(var) {
            *slot = Some(receiver.clone());
        }
        self.decide_view(state, receiver, span);
    }

    /// Decides the view once the receiver is known: the public view of an
    /// interface value, the evidence view of a type parameter or `Self`, or
    /// selection, now when the interface has no parameter, else once every
    /// operand was checked.
    fn decide_view(
        &mut self,
        state: &mut CallState<'a>,
        receiver: &Type,
        span: ByteSpan,
    ) {
        let Some(dispatch) = state.dispatch.as_ref() else {
            return;
        };
        let interface = dispatch.interface;
        let id = self
            .program
            .interfaces
            .get(interface)
            .and_then(|data| data.id.clone());
        if let Type::Interface(held, arguments, choices) = receiver {
            if id.as_ref() == Some(held) && dispatch.receiver_arg.is_some() {
                let evidence = Evidence {
                    arguments: arguments.iter().cloned().map(Some).collect(),
                    choices: self
                        .program
                        .interfaces
                        .get(interface)
                        .map(|data| {
                            data.associated
                                .iter()
                                .map(|(name, _)| {
                                    (name.clone(), choices.get(name).cloned())
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                    view: View::Public,
                };
                self.apply_evidence(state, evidence);
            } else {
                self.unsatisfied(span, receiver, interface);
                state.failed = true;
            }
            return;
        }
        let evidence_view = matches!(receiver, Type::Binder(_) | Type::SelfType);
        if evidence_view || dispatch.params == 0 {
            self.select(state);
        }
    }

    /// Selects the evidence for the receiver at the interface's arguments
    /// inferred so far, and applies it.
    pub(super) fn select(&mut self, state: &mut CallState<'a>) {
        let Some(dispatch) = state.dispatch.as_ref() else {
            return;
        };
        if dispatch.view.is_some() || state.failed {
            return;
        }
        let (interface, span, var, params) = (
            dispatch.interface,
            dispatch.receiver_span,
            dispatch.self_var(),
            dispatch.params,
        );
        let Some(Some(receiver)) = state.assignment.get(var).cloned() else {
            return;
        };
        // Step 2: the expected type constrains each parameter of the
        // interface written in the result type before selection; the
        // associated and member-local positions wait for step 5.
        if let (Some(result), Some(expected)) = (&state.result, &dispatch.expected) {
            for (at, found) in positional_parts(result, expected, params)
                .into_iter()
                .enumerate()
            {
                if let (Some(found), Some(slot @ None)) =
                    (found, state.assignment.get_mut(at))
                {
                    *slot = Some(found);
                }
            }
        }
        let arguments = (0..params)
            .map(|at| state.instance(&Type::Binder(at)))
            .collect::<Vec<_>>();
        match self.evidence(&receiver, interface, &arguments) {
            Found::One(evidence) => self.apply_evidence(state, evidence),
            Found::Many => {
                self.report(
                    DiagnosticCode::TypeAmbiguousImplementation,
                    state.application,
                    format!(
                        "more than one implementation for `{receiver}` fits, since an argument of the interface is not determined"
                    ),
                );
                state.failed = true;
            }
            Found::None => {
                self.unsatisfied(span, &receiver, interface);
                state.failed = true;
            }
        }
    }

    /// Fixes the interface's arguments and every choice from `evidence`,
    /// then constrains the rest by the expected type.
    fn apply_evidence(&mut self, state: &mut CallState<'a>, evidence: Evidence) {
        let Some(mut dispatch) = state.dispatch.take() else {
            return;
        };
        let application = state.application;
        for (at, argument) in evidence.arguments.iter().enumerate() {
            let Some(argument) = argument else { continue };
            let mut trial = state.assignment.clone();
            if unify(&Type::Binder(at), argument, &mut trial) {
                state.assignment = trial;
            } else {
                self.unsatisfied(dispatch.receiver_span, argument, dispatch.interface);
                state.failed = true;
            }
        }
        for (at, (name, choice)) in evidence.choices.into_iter().enumerate() {
            let value = choice.unwrap_or_else(|| {
                self.gaps.push((
                    application,
                    "a contract call on a type parameter whose bound leaves an associated type open"
                        .to_owned(),
                ));
                Type::Associated(name.clone())
            });
            match state.assignment.get_mut(dispatch.assoc_var(at)) {
                Some(Some(found)) if *found != value => {
                    let message = format!(
                        "`{name}` is `{value}` here, but the call needs `{found}`"
                    );
                    self.report(DiagnosticCode::TypeMismatch, application, message);
                    state.failed = true;
                }
                Some(slot) => *slot = Some(value),
                None => {}
            }
        }
        dispatch.view = Some(evidence.view);
        if let (Some(result), Some(expected)) = (&state.result, &dispatch.expected) {
            let mut trial = state.assignment.clone();
            if unify(result, expected, &mut trial) {
                state.assignment = trial;
            }
        }
        state.dispatch = Some(dispatch);
    }

    /// Whether the argument `at` of a call through an interface value
    /// supplies a parameter outside the receiver slot that mentions `Self`,
    /// reporting it.
    pub(super) fn self_outside_slot(
        &mut self,
        state: &CallState<'a>,
        at: usize,
        span: ByteSpan,
    ) -> bool {
        let Some(dispatch) = state.dispatch.as_ref() else {
            return false;
        };
        let parameter = state.binding.get(at).copied().flatten();
        if dispatch.view != Some(View::Public)
            || !parameter
                .is_some_and(|parameter| dispatch.self_params.contains(&parameter))
        {
            return false;
        }
        self.report(
            DiagnosticCode::TypeArgumentMismatch,
            span,
            "through an interface value no argument is known to hold the receiver's type, so a parameter of type `Self` outside the receiver slot cannot be supplied",
        );
        true
    }

    /// The checked contract call of a finished call, recording what it
    /// refers to and instantiates.
    pub(super) fn contract_call(
        &mut self,
        state: &CallState<'a>,
    ) -> Option<ContractCall> {
        let dispatch = state.dispatch.as_ref()?;
        let program = self.program;
        let data = program.interfaces.get(dispatch.interface)?;
        let interface = data.id.clone()?;
        let declared = data.member(&dispatch.member)?;
        let instance = |at: usize| state.instance(&Type::Binder(at));
        let receiver = instance(dispatch.self_var())?;
        let arguments = (0..dispatch.params)
            .map(instance)
            .collect::<Option<Vec<_>>>()?;
        let member_arguments = (dispatch.params..dispatch.self_var())
            .map(instance)
            .collect::<Option<Vec<_>>>()?;
        let choices = NamedTypes::new(
            data.associated
                .iter()
                .enumerate()
                .map(|(at, (name, _))| {
                    Some((name.clone(), instance(dispatch.assoc_var(at))?))
                })
                .collect::<Option<Vec<_>>>()?,
        )
        .ok()?;
        let application = state.application;
        let target = match &dispatch.view {
            Some(View::Selected(index, binders)) => {
                let implementation = program.impls.get(*index)?;
                let supplied = implementation
                    .members
                    .iter()
                    .find(|(name, _)| *name == dispatch.member)
                    .map(|(_, function)| *function);
                match supplied {
                    Some(function) => {
                        binders.iter().cloned().collect::<Option<Vec<_>>>().map(
                            |mut binders| {
                                binders.extend(member_arguments.iter().cloned());
                                (function, binders)
                            },
                        )
                    }
                    None => declared.default.map(|function| {
                        let mut binders = arguments.clone();
                        binders.extend(member_arguments.iter().cloned());
                        (function, binders)
                    }),
                }
            }
            _ => declared.default.map(|function| {
                let mut binders = arguments.clone();
                binders.extend(member_arguments.iter().cloned());
                (function, binders)
            }),
        };
        if let Some((function, binders)) = target {
            if program
                .functions
                .get(function)
                .is_none_or(|data| data.signature.is_none())
            {
                self.unavailable(application, "a member of this implementation");
                return None;
            }
            self.references.push(Reference {
                target: Target::Function(function),
                span: state.callee_span,
            });
            if !binders.is_empty()
                && matches!(
                    dispatch.view,
                    Some(View::Selected(..) | View::Evidence | View::Public)
                )
            {
                self.instantiations.push((function, binders, application));
            }
        }
        // Through scoped evidence or an interface value the implementation
        // is chosen at activation: the call instantiates the member of every
        // implementation that can be chosen there.
        if matches!(dispatch.view, Some(View::Evidence | View::Public)) {
            let open = self.binders.len();
            for implementation in &program.impls {
                if implementation.interface != dispatch.interface
                    || !implementation.usable
                    || !may_select(
                        &implementation.arguments,
                        implementation.binders.len(),
                        &arguments,
                        open,
                    )
                {
                    continue;
                }
                let Some(function) = implementation
                    .members
                    .iter()
                    .find(|(name, _)| *name == dispatch.member)
                    .map(|(_, function)| *function)
                else {
                    continue;
                };
                let mut binders = vec![None; implementation.binders.len()];
                for (pattern, actual) in implementation.arguments.iter().zip(&arguments)
                {
                    match_part(pattern, actual, &mut binders);
                }
                // A parameter of the implementation that no interface
                // argument gives is fixed by the receiver alone, which
                // carries no type parameter of this body to it.
                let mut binders = binders
                    .into_iter()
                    .map(|bound| bound.unwrap_or(Type::Never))
                    .collect::<Vec<_>>();
                binders.extend(member_arguments.iter().cloned());
                if !binders.is_empty() {
                    self.instantiations.push((function, binders, application));
                }
            }
        }
        Some(ContractCall {
            interface,
            member: dispatch.member.clone(),
            receiver,
            arguments,
            choices,
            member_arguments,
        })
    }

    /// Resolves the bounds of a callee's type parameters once ordinary
    /// inference fixed what it could: each resolvable bound selects its
    /// evidence, whose choices fix the parameters its labels name. Whether
    /// every resolvable bound holds.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn resolve_bounds(
        &mut self,
        bounds: &[Option<BoundData>],
        module: usize,
        assignment: &mut [Option<Type>],
        resolved: &mut [bool],
        sources: &[Option<ByteSpan>],
        application: ByteSpan,
        last: bool,
    ) -> bool {
        loop {
            let mut progress = false;
            for (at, bound) in bounds.iter().enumerate() {
                let Some(bound) = bound else { continue };
                if resolved.get(at).copied().unwrap_or(true) {
                    continue;
                }
                let Some(actual) = closed(&Type::Binder(at), assignment) else {
                    continue;
                };
                let Some(arguments) = bound
                    .arguments
                    .iter()
                    .map(|argument| closed(argument, assignment))
                    .collect::<Option<Vec<_>>>()
                else {
                    continue;
                };
                let span = sources.get(at).copied().flatten().unwrap_or(application);
                let known = arguments.iter().cloned().map(Some).collect::<Vec<_>>();
                let evidence = match self.evidence(&actual, bound.interface, &known) {
                    Found::One(evidence) => evidence,
                    Found::None | Found::Many => {
                        if let Some(done) = resolved.get_mut(at) {
                            *done = true;
                        }
                        self.unsatisfied_bound(span, &actual, bound, module, "");
                        return false;
                    }
                };
                let choice = |label: &str| {
                    evidence
                        .choices
                        .iter()
                        .find(|(name, _)| name == label)
                        .and_then(|(_, chosen)| chosen.clone())
                };
                // A choice the evidence leaves open equals no type the call
                // can name; while the parameter it is compared with may still
                // be fixed by an argument checked again, the bound waits.
                let waits = !last
                    && bound.choices.iter().any(|(label, pattern)| {
                        choice(label).is_none() && closed(pattern, assignment).is_none()
                    });
                if waits {
                    continue;
                }
                if let Some(done) = resolved.get_mut(at) {
                    *done = true;
                }
                progress = true;
                for (label, pattern) in &bound.choices {
                    let linked = mentions_binder(pattern);
                    match choice(label) {
                        Some(chosen) if linked => {
                            let mut trial = assignment.to_vec();
                            if unify(pattern, &chosen, &mut trial) {
                                assignment.clone_from_slice(&trial);
                            } else {
                                let wanted = closed(pattern, assignment)
                                    .map_or_else(String::new, |ty| {
                                        format!(", but the call needs `{ty}`")
                                    });
                                self.report(
                                    DiagnosticCode::TypeMismatch,
                                    span,
                                    format!(
                                        "the implementation for `{actual}` chooses `{chosen}` for `{label}`{wanted}"
                                    ),
                                );
                                return false;
                            }
                        }
                        Some(chosen) if chosen == *pattern => {}
                        Some(chosen) => {
                            let detail = format!(
                                ": its implementation chooses `{chosen}` for `{label}`"
                            );
                            self.unsatisfied_bound(
                                span, &actual, bound, module, &detail,
                            );
                            return false;
                        }
                        None => {
                            let detail = format!(
                                ": its evidence leaves `{label}` open, and an open choice equals no other type"
                            );
                            self.unsatisfied_bound(
                                span, &actual, bound, module, &detail,
                            );
                            return false;
                        }
                    }
                }
                if let View::Selected(index, binders) = &evidence.view
                    && let Some(binders) =
                        binders.iter().cloned().collect::<Option<Vec<_>>>()
                    && let Some(implementation) = self.program.impls.get(*index)
                {
                    for (_, function) in &implementation.members {
                        let fits =
                            self.program.functions.get(*function).is_some_and(|data| {
                                data.binders.len() == binders.len()
                            });
                        if fits && !binders.is_empty() {
                            self.instantiations.push((
                                *function,
                                binders.clone(),
                                application,
                            ));
                        }
                    }
                }
            }
            if !progress {
                return true;
            }
        }
    }

    /// Fixes the open type parameters of `pattern`, a written interface
    /// value type, from the evidence by which `actual` implements it, so
    /// that `actual` widens to it. Whether it did.
    pub(super) fn widening_inference(
        &self,
        pattern: &Type,
        actual: &Type,
        assignment: &mut Vec<Option<Type>>,
    ) -> bool {
        let Type::Interface(id, arguments, choices) = pattern else {
            return false;
        };
        if actual.is_interface_value() {
            return false;
        }
        let Some(interface) = self
            .program
            .interfaces
            .iter()
            .position(|data| data.id.as_ref() == Some(id))
        else {
            return false;
        };
        let known = arguments
            .iter()
            .map(|argument| closed(argument, assignment))
            .collect::<Vec<_>>();
        let Found::One(evidence) = self.evidence(actual, interface, &known) else {
            return false;
        };
        let Some(found_arguments) =
            evidence.arguments.into_iter().collect::<Option<Vec<_>>>()
        else {
            return false;
        };
        let Some(found_choices) = choices
            .entries()
            .iter()
            .map(|(name, _)| {
                evidence
                    .choices
                    .iter()
                    .find(|(choice, _)| choice == name)
                    .and_then(|(_, chosen)| chosen.clone())
                    .map(|chosen| (name.clone(), chosen))
            })
            .collect::<Option<Vec<_>>>()
        else {
            return false;
        };
        let Ok(found_choices) = NamedTypes::new(found_choices) else {
            return false;
        };
        let found = Type::Interface(id.clone(), found_arguments, found_choices);
        let mut trial = assignment.clone();
        if unify(pattern, &found, &mut trial) {
            *assignment = trial;
            true
        } else {
            false
        }
    }

    /// Widens `actual` to the written expected type `expected` where the
    /// chapter allows it, else checks it as [`Self::conform`] does.
    pub(super) fn widen(
        &mut self,
        expected: &Type,
        actual: &Type,
        node: Option<Node>,
        span: ByteSpan,
        code: DiagnosticCode,
    ) -> Option<Node> {
        if actual == expected || *actual == Type::Never {
            return node;
        }
        match (expected, actual) {
            (Type::Any, _) if !actual.is_interface_value() => node.map(|node| {
                self.node(
                    IrExpr::WidenToInterface {
                        value: Box::new(node),
                        value_type: Type::Any,
                    },
                    span,
                )
            }),
            (Type::Atom, Type::AtomSingleton(_)) => {
                node.map(|node| self.node(IrExpr::WidenAtom(Box::new(node)), span))
            }
            (Type::Interface(id, arguments, choices), _)
                if !actual.is_interface_value() =>
            {
                let interface = self
                    .program
                    .interfaces
                    .iter()
                    .position(|data| data.id.as_ref() == Some(id));
                let known = arguments.iter().cloned().map(Some).collect::<Vec<_>>();
                let found =
                    interface.map(|interface| self.evidence(actual, interface, &known));
                let Some(Found::One(evidence)) = found else {
                    return self.conform(expected, actual, node, span, code);
                };
                let same = evidence.choices.len() == choices.len()
                    && evidence
                        .choices
                        .iter()
                        .all(|(name, chosen)| chosen.as_ref() == choices.get(name));
                if !same {
                    self.report(
                        DiagnosticCode::TypeMismatch,
                        span,
                        format!(
                            "`{actual}` implements the interface with other choices than `{expected}` fixes"
                        ),
                    );
                    return None;
                }
                node.map(|node| {
                    self.node(
                        IrExpr::WidenToInterface {
                            value: Box::new(node),
                            value_type: expected.clone(),
                        },
                        span,
                    )
                })
            }
            _ => self.conform(expected, actual, node, span, code),
        }
    }

    /// A contract member named as a value: instantiated from the expected
    /// callable type, which fixes the receiver and selects the
    /// implementation whose member it denotes.
    pub(super) fn contract_value(
        &mut self,
        interface: usize,
        member: &str,
        span: ByteSpan,
        expected: Option<&Type>,
    ) -> Out {
        let Some((names, signature, dispatch)) =
            self.contract_signature(interface, member, span)
        else {
            return Out::error();
        };
        let pattern = Type::function(signature);
        let var = dispatch.self_var();
        let mut assignment = vec![None; names.len()];
        let fits =
            expected.is_some_and(|expected| unify(&pattern, expected, &mut assignment));
        let receiver = assignment.get(var).cloned().flatten();
        let Some(receiver) = receiver.filter(|_| fits) else {
            match expected {
                Some(expected) if !fits => self.report(
                    DiagnosticCode::TypeMismatch,
                    span,
                    format!("expected `{expected}`, found the member `{member}`, of type `{pattern}`"),
                ),
                _ => self.ambiguous(span, &["Self".to_owned()]),
            }
            return Out::error();
        };
        let arguments = (0..dispatch.params)
            .map(|at| closed(&Type::Binder(at), &assignment))
            .collect::<Vec<_>>();
        let evidence = match self.evidence(&receiver, interface, &arguments) {
            Found::One(evidence) => evidence,
            Found::Many => {
                self.report(
                    DiagnosticCode::TypeAmbiguousImplementation,
                    span,
                    format!("more than one implementation for `{receiver}` fits"),
                );
                return Out::error();
            }
            Found::None => {
                self.unsatisfied(span, &receiver, interface);
                return Out::error();
            }
        };
        let View::Selected(index, binders) = &evidence.view else {
            self.unavailable(
                span,
                "an interface member used as a value without an implementation",
            );
            return Out::error();
        };
        for (at, argument) in evidence.arguments.iter().enumerate() {
            if let (Some(argument), Some(slot)) = (argument, assignment.get_mut(at))
                && slot.is_none()
            {
                *slot = Some(argument.clone());
            }
        }
        for (at, (_, choice)) in evidence.choices.iter().enumerate() {
            if let (Some(choice), Some(slot)) =
                (choice, assignment.get_mut(dispatch.assoc_var(at)))
            {
                *slot = Some(choice.clone());
            }
        }
        let missing = names
            .iter()
            .zip(&assignment)
            .filter(|(_, bound)| bound.is_none())
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            self.ambiguous(span, &missing);
            return Out::error();
        }
        let Some(ty) = closed(&pattern, &assignment) else {
            return Out::error();
        };
        if expected.is_some_and(|expected| *expected != ty) {
            let expected = expected.map(ToString::to_string).unwrap_or_default();
            self.report(
                DiagnosticCode::TypeMismatch,
                span,
                format!("expected `{expected}`, found `{ty}`"),
            );
            return Out::error();
        }
        let supplied = self.program.impls.get(*index).and_then(|implementation| {
            implementation
                .members
                .iter()
                .find(|(name, _)| name == member)
                .map(|(_, function)| *function)
        });
        let Some(function) = supplied else {
            self.unavailable(span, "a default member used as a value");
            return Out::error();
        };
        let Some(mut type_arguments) =
            binders.iter().cloned().collect::<Option<Vec<_>>>()
        else {
            return Out::error();
        };
        type_arguments.extend(
            assignment
                .get(dispatch.params..var)
                .unwrap_or_default()
                .iter()
                .flatten()
                .cloned(),
        );
        self.references.push(Reference {
            target: Target::Function(function),
            span,
        });
        if !type_arguments.is_empty() {
            self.instantiations
                .push((function, type_arguments.clone(), span));
        }
        let node =
            self.layout
                .functions
                .get(function)
                .copied()
                .flatten()
                .map(|function| {
                    self.node(
                        IrExpr::Function {
                            function,
                            type_arguments,
                        },
                        span,
                    )
                });
        Out { ty: Some(ty), node }
    }
}

//! The discharge of bare-guard obligations.
//!
//! `docs/spec/02-type-system.md`, "Bare guards": a bare `requires` states an
//! obligation at every call, a bare `where` at every construction, and a
//! bare `ensures` at every `return` of the body and every `try` exit, one
//! obligation for its predicate. Each body is walked once with the facts
//! that hold at each point ([`super::facts`]): the conditions of the
//! enclosing `if`s, the case and literal facts of the enclosing `match`
//! arms, the body's own `requires`, the invariants of the types of the
//! values read, and the proven `ensures` of the callees whose `requires` was
//! discharged. An obligation whose operands are constant is decided by
//! evaluating its predicate (`GUARD-1`), the only rule that refutes; every
//! other obligation passes when the rules establish it and is `unknown`
//! otherwise. The `ensures` a callee's result may be read through are found
//! as a least fixed point, so that no `ensures` is proven by itself.
//!
//! A function converted to a callable value keeps no obligation, so a bare
//! `requires` its parameter types alone do not establish is
//! `@contract.obligationLost` at the conversion.
//!
//! Every walk is a work list, so no host stack grows with the depth of a
//! program.

use std::collections::{BTreeMap, BTreeSet};

use vibra_diagnostics::{ByteSpan, DiagnosticCode};
use vibra_ir::{
    Arguments, CallTarget, Constant, Construction, Expr, Library, MatchArm, Node,
    Pattern, Type, TypeId, Value,
};

use super::super::ctfe::{Evaluator, Outcome};
use super::super::declare::{Entity, TypeKind};
use super::super::obligation::{Obligation, ObligationKind, ObligationStatus};
use super::facts::{
    self, BRANCH, CONSTANTS, Callee, Connective, Data, Env, Evidence, FactSet,
    INVARIANT, RESULT, Readable, SUBSTITUTION, Term, Terms, Vocabulary,
};
use super::fragment::{Standing, operands};
use super::obligations::predicate_call as where_call;
use super::{CheckedGuard, Context, Owner, Predicate, Site};

/// The repairs an unproved obligation names.
pub(super) const REPAIRS: [&str; 3] = [
    "branch on the same predicate",
    "pass a value of a type whose invariant states it",
    "give the guard an `else`",
];

/// How a callable's `requires` is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Entry {
    None,
    Bare,
    Else,
}

/// What the discharge reads of one function, by checked index.
#[derive(Debug)]
struct Guarded<'v> {
    module: usize,
    identity: String,
    qualified: String,
    name: ByteSpan,
    /// The positional parameters, then the labels in canonical order.
    positional: usize,
    labels: Vec<String>,
    /// The name of each argument slot.
    names: Vec<String>,
    /// The type of each argument slot.
    types: Vec<Type>,
    entry: Entry,
    /// The `requires`, bare or with `else`: a fact of the body.
    precondition: Option<&'v CheckedGuard>,
    /// The bare `requires`.
    requires: Option<&'v CheckedGuard>,
    requires_certified: bool,
    ensures: Option<(usize, &'v CheckedGuard)>,
    ensures_certified: bool,
    result_name: String,
    result_type: Type,
    /// For a member of an implementation whose guards are its contract
    /// member's: the module of the interface and the contract member's name.
    contract: Option<(usize, ByteSpan)>,
}

/// What the discharge reads of one type with a bare or a fallible `where`.
#[derive(Debug)]
struct Invariant<'v> {
    module: usize,
    identity: String,
    name: String,
    name_span: ByteSpan,
    /// The predicate's checked entry.
    entry: Option<usize>,
    /// The predicate's body, its parameter at slot 0, and its parameter name.
    body: Option<(&'v Node, Option<String>)>,
    /// The identity of a named predicate.
    named: Option<String>,
    bare: bool,
    certified: bool,
    span: ByteSpan,
    representation: Option<Type>,
}

/// What a guarded site guards.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Guard {
    /// The function at this checked index.
    Function(usize),
    /// The type at this program index.
    Type(usize),
}

/// The verdict on one obligation at one site.
#[derive(Clone, Debug)]
enum Verdict {
    Passed(Evidence),
    Refuted {
        counterexample: Option<String>,
        related: ByteSpan,
    },
    Unknown,
    Limit,
    Unsupported,
}

/// One obligation found at one site.
#[derive(Debug)]
struct Found {
    kind: ObligationKind,
    owner: usize,
    span: ByteSpan,
    guard: Guard,
    verdict: Verdict,
}

/// One exit of a body with a bare `ensures`.
#[derive(Debug)]
struct Exit {
    span: ByteSpan,
    verdict: Verdict,
    through_try: bool,
}

/// A body to walk: its owner, the checked index of its function, and the
/// node.
#[derive(Clone, Copy, Debug)]
struct Body<'v> {
    owner: usize,
    function: Option<usize>,
    node: &'v Node,
    /// Whether the body is the function's own, whose returns are its exits.
    exits: bool,
}

/// A declaration that contains sites.
#[derive(Debug)]
struct Declaration {
    module: usize,
    identity: String,
}

/// What `GUARD-1` decided once, by owner, site, and kind.
#[derive(Clone, Debug)]
enum Decided {
    Truth(bool, Option<String>),
    Limit,
    Open,
}

/// What a walk found.
#[derive(Debug, Default)]
struct Walked {
    found: Vec<Found>,
    exits: Vec<Exit>,
    lost: Vec<(ByteSpan, usize)>,
}

/// The state of one discharge.
struct Prover<'v> {
    terms: Terms,
    vocabulary: Vocabulary<'v>,
    guarded: BTreeMap<usize, Guarded<'v>>,
    invariants: BTreeMap<TypeId, (usize, Invariant<'v>)>,
    declarations: Vec<Declaration>,
    owners: BTreeMap<Owner, usize>,
    /// The functions whose `ensures` is proven.
    proven: BTreeSet<usize>,
    /// The call terms whose `requires` was discharged.
    discharged: BTreeSet<Term>,
    activations: usize,
    decided: BTreeMap<(usize, ByteSpan, ObligationKind), Decided>,
}

/// A step of a walk.
enum Task<'n> {
    Visit(&'n Node),
    Enter,
    Leave,
    Assume(&'n Node, bool),
    Bind(Option<usize>, &'n Node),
    Arm(&'n Node, &'n [MatchArm], usize),
    Refuted(&'n Pattern, &'n Node),
    Call(&'n Node),
    Construct(&'n Node),
    Return(&'n Node, ByteSpan),
    Try(ByteSpan),
    Lost(ByteSpan, usize),
}

/// Discharges every bare-guard obligation of the local declarations.
pub(super) fn discharge(
    context: &mut Context<'_, '_>,
    certified: &super::Certified,
    evaluator: &mut Evaluator<'_>,
) {
    let inputs = context.inputs;
    let mut prover = Prover {
        terms: Terms::default(),
        vocabulary: vocabulary(context),
        guarded: guarded(context, certified),
        invariants: invariants(context),
        declarations: Vec::new(),
        owners: BTreeMap::new(),
        proven: BTreeSet::new(),
        discharged: BTreeSet::new(),
        activations: 0,
        decided: BTreeMap::new(),
    };
    let mut bodies = Vec::new();
    for (index, checked) in inputs.functions {
        let Some(data) = inputs.program.functions.get(*index) else {
            continue;
        };
        if !context.local(data.module) {
            continue;
        }
        let owner =
            prover.declare(Owner::Function(*index), data.module, data.identity.clone());
        let entry = inputs.layout.functions.get(*index).copied().flatten();
        if let Some(node) = &checked.node {
            bodies.push(Body {
                owner,
                function: entry,
                node,
                exits: true,
            });
        }
        let guards = [
            checked.guards.entry.as_ref(),
            checked.guards.exit.as_ref().map(|(_, guard)| guard),
        ];
        for guard in guards.into_iter().flatten() {
            for node in std::iter::once(&guard.predicate).chain(&guard.consequence) {
                bodies.push(Body {
                    owner,
                    function: None,
                    node,
                    exits: false,
                });
            }
        }
    }
    let others = inputs
        .values
        .iter()
        .map(|(index, checked)| (Owner::Value(*index), checked))
        .chain(
            inputs
                .tests
                .iter()
                .map(|(index, checked)| (Owner::Test(*index), checked)),
        );
    for (key, checked) in others {
        {
            let Some((module, identity)) = context.owner(key) else {
                continue;
            };
            if !context.local(module) {
                continue;
            }
            let owner = prover.declare(key, module, identity);
            if let Some(node) = &checked.node {
                bodies.push(Body {
                    owner,
                    function: None,
                    node,
                    exits: false,
                });
            }
        }
    }
    // The proven `ensures`, as a least fixed point.
    let ensured = prover
        .guarded
        .iter()
        .filter(|(_, data)| data.ensures.is_some() && data.ensures_certified)
        .map(|(entry, _)| *entry)
        .collect::<BTreeSet<_>>();
    for _ in 0..=ensured.len() {
        let mut proven = BTreeSet::new();
        for body in bodies.iter().filter(|body| body.exits) {
            let Some(function) = body.function.filter(|entry| ensured.contains(entry))
            else {
                continue;
            };
            let walked = prover.walk(*body, evaluator, false);
            if walked
                .exits
                .iter()
                .all(|exit| matches!(exit.verdict, Verdict::Passed(_)))
            {
                proven.insert(function);
            }
        }
        if proven == prover.proven {
            break;
        }
        prover.proven = proven;
    }
    let mut found = Vec::new();
    let mut lost = Vec::new();
    let mut exits: BTreeMap<usize, Vec<Exit>> = BTreeMap::new();
    for body in &bodies {
        let walked = prover.walk(*body, evaluator, true);
        found.extend(walked.found);
        lost.extend(walked.lost.into_iter().map(|lost| (body.owner, lost)));
        if body.exits
            && let Some(function) = body.function
        {
            exits.entry(function).or_default().extend(walked.exits);
        }
    }
    prover.unwalked(context, &mut found, evaluator);
    prover.report(context, found, exits, &lost);
}

impl<'v> Prover<'v> {
    fn declare(&mut self, owner: Owner, module: usize, identity: String) -> usize {
        self.declarations.push(Declaration { module, identity });
        let at = self.declarations.len() - 1;
        self.owners.insert(owner, at);
        at
    }

    /// The obligation of each site of a bare guard the walks did not reach,
    /// which no rule but `GUARD-1` decides.
    fn unwalked(
        &mut self,
        context: &Context<'_, '_>,
        found: &mut Vec<Found>,
        evaluator: &mut Evaluator<'_>,
    ) {
        let inputs = context.inputs;
        for (_, owner, site) in inputs.sites {
            let Some(declaration) = self.owners.get(owner).copied() else {
                continue;
            };
            let (kind, span, guard, verdict) = match site {
                Site::Requires { function, span } => {
                    let Some(entry) =
                        inputs.layout.functions.get(*function).copied().flatten()
                    else {
                        continue;
                    };
                    let Some(data) = self
                        .guarded
                        .get(&entry)
                        .filter(|data| data.requires.is_some())
                    else {
                        continue;
                    };
                    let verdict = if data.requires_certified {
                        Verdict::Unknown
                    } else {
                        Verdict::Unsupported
                    };
                    (
                        ObligationKind::Requires,
                        *span,
                        Guard::Function(entry),
                        verdict,
                    )
                }
                Site::Where { ty, span, node } => {
                    let Some((_, invariant)) = inputs
                        .program
                        .types
                        .get(*ty)
                        .and_then(|data| data.id.as_ref())
                        .and_then(|id| self.invariants.get(id))
                        .filter(|(_, invariant)| invariant.bare)
                    else {
                        continue;
                    };
                    let verdict = if invariant.certified {
                        let outcome = node
                            .as_ref()
                            .and_then(|node| where_call(context, *ty, node))
                            .map(|call| evaluator.evaluate(&call));
                        match outcome {
                            Some(Outcome::LimitReached) => Verdict::Limit,
                            Some(Outcome::Value(value)) => match facts::truth(&value) {
                                Some(true) => Verdict::Passed(Evidence::of(CONSTANTS)),
                                Some(false) => Verdict::Refuted {
                                    counterexample: None,
                                    related: *span,
                                },
                                None => Verdict::Unknown,
                            },
                            _ => Verdict::Unknown,
                        }
                    } else {
                        Verdict::Unsupported
                    };
                    (ObligationKind::Where, *span, Guard::Type(*ty), verdict)
                }
                Site::Pattern { .. } => continue,
            };
            if found.iter().any(|found| {
                found.owner == declaration && found.span == span && found.kind == kind
            }) {
                continue;
            }
            found.push(Found {
                kind,
                owner: declaration,
                span,
                guard,
                verdict,
            });
        }
    }

    fn activation(&mut self) -> usize {
        self.activations += 1;
        self.activations
    }

    /// Walks `body` and decides each obligation in it. `last` is the final
    /// walk, whose conversions are reported.
    fn walk(
        &mut self,
        body: Body<'v>,
        evaluator: &mut Evaluator<'_>,
        last: bool,
    ) -> Walked {
        let mut walked = Walked::default();
        let mut env = Env::new(self.activation(), true);
        let mut facts = FactSet::default();
        if let Some(data) = body.function.and_then(|entry| self.guarded.get(&entry))
            && let Some(guard) = data.precondition
        {
            let (term, _) = facts::build(
                &mut self.terms,
                &self.vocabulary,
                &mut env,
                &guard.predicate,
            );
            facts.assume(
                &mut self.terms,
                &self.vocabulary,
                term,
                true,
                Evidence::of(SUBSTITUTION),
            );
        }
        let mut marks = Vec::new();
        let mut closures: Vec<&'v Node> = Vec::new();
        let mut pending = vec![Task::Visit(body.node)];
        loop {
            let Some(task) = pending.pop() else {
                // Each anonymous callable is an activation of its own, with
                // no fact of the body it is written in.
                let Some(closure) = closures.pop() else {
                    break;
                };
                env = Env::new(self.activation(), true);
                facts = FactSet::default();
                pending.push(Task::Visit(closure));
                continue;
            };
            match task {
                Task::Visit(node) => visit(node, &mut pending, &mut closures),
                Task::Enter => marks.push(env.mark(&facts)),
                Task::Leave => {
                    if let Some(mark) = marks.pop() {
                        env.restore(&mut facts, mark);
                    }
                }
                Task::Assume(condition, holds) => {
                    let (term, substituted) = facts::build(
                        &mut self.terms,
                        &self.vocabulary,
                        &mut env,
                        condition,
                    );
                    let mut evidence = Evidence::default();
                    if substituted {
                        evidence = evidence.with(SUBSTITUTION);
                    }
                    facts.assume(
                        &mut self.terms,
                        &self.vocabulary,
                        term,
                        holds,
                        evidence.with(BRANCH),
                    );
                }
                Task::Bind(slot, value) => {
                    let (term, _) = facts::build(
                        &mut self.terms,
                        &self.vocabulary,
                        &mut env,
                        value,
                    );
                    if let Some(slot) = slot {
                        env.bind(slot, term, true);
                    }
                }
                Task::Arm(subject, arms, at) => {
                    self.arm(&mut env, &mut facts, subject, arms, at);
                }
                Task::Refuted(pattern, value) => {
                    let arm = MatchArm {
                        pattern: pattern.clone(),
                        body: value.clone(),
                    };
                    self.arm(
                        &mut env,
                        &mut facts,
                        value,
                        std::slice::from_ref(&arm),
                        0,
                    );
                }
                Task::Call(node) => {
                    if let Some(found) =
                        self.call(&mut env, &facts, body, node, evaluator)
                    {
                        walked.found.push(found);
                    }
                }
                Task::Construct(node) => {
                    if let Some(found) =
                        self.construct(&mut env, &facts, body, node, evaluator)
                    {
                        walked.found.push(found);
                    }
                }
                Task::Return(value, span) => {
                    if body.exits
                        && let Some(exit) = self.exit(
                            &mut env,
                            &facts,
                            body,
                            Some(value),
                            span,
                            evaluator,
                        )
                    {
                        walked.exits.push(exit);
                    }
                }
                Task::Try(span) => {
                    if body.exits
                        && let Some(exit) =
                            self.exit(&mut env, &facts, body, None, span, evaluator)
                    {
                        walked.exits.push(exit);
                    }
                }
                Task::Lost(span, function) => {
                    if last && !self.convertible(function) {
                        walked.lost.push((span, function));
                    }
                }
            }
        }
        walked
    }

    /// Binds and assumes what the arm `at` of a `match` of `subject` tells:
    /// its pattern's bindings, case facts, and literal equalities, and that
    /// no earlier arm's case or literal matched.
    fn arm(
        &mut self,
        env: &mut Env,
        facts: &mut FactSet,
        subject: &Node,
        arms: &[MatchArm],
        at: usize,
    ) {
        let (subject, substituted) =
            facts::build(&mut self.terms, &self.vocabulary, env, subject);
        let mut evidence = Evidence::default();
        if substituted {
            evidence = evidence.with(SUBSTITUTION);
        }
        let evidence = evidence.with(BRANCH);
        for earlier in arms.iter().take(at) {
            match &earlier.pattern {
                Pattern::Case { case, fields }
                    if fields.iter().all(|(_, inner)| {
                        matches!(inner, Pattern::Discard | Pattern::Bind(_))
                    }) =>
                {
                    self.case_fact(facts, subject, case, false, &evidence);
                }
                Pattern::Literal(constant) => {
                    let literal = self.terms.intern(Data::Constant(constant.clone()));
                    if let Some(equal) = self.vocabulary.equality(
                        &mut self.terms,
                        constant.value_type(),
                        subject,
                        literal,
                    ) {
                        facts.assume(
                            &mut self.terms,
                            &self.vocabulary,
                            equal,
                            false,
                            evidence.clone(),
                        );
                    }
                }
                _ => {}
            }
        }
        let Some(arm) = arms.get(at) else {
            return;
        };
        for (part, term) in facts::pattern_parts(&mut self.terms, &arm.pattern, subject)
        {
            match part {
                Pattern::Bind(slot) => env.bind(*slot, term, false),
                Pattern::Case { case, .. } => {
                    self.case_fact(facts, term, case, true, &evidence);
                }
                Pattern::Literal(constant) => {
                    let literal = self.terms.intern(Data::Constant(constant.clone()));
                    env.rewrite(term, literal);
                    if let Some(equal) = self.vocabulary.equality(
                        &mut self.terms,
                        constant.value_type(),
                        term,
                        literal,
                    ) {
                        facts.assume(
                            &mut self.terms,
                            &self.vocabulary,
                            equal,
                            true,
                            evidence.clone(),
                        );
                    }
                }
                _ => {}
            }
        }
    }

    /// Assumes that `term` is (`holds`) or is not of the case `case`; a
    /// `Bool` of the case `true` or `false` is that truth.
    fn case_fact(
        &mut self,
        facts: &mut FactSet,
        term: Term,
        case: &str,
        holds: bool,
        evidence: &Evidence,
    ) {
        let is = self.terms.intern(Data::IsCase(term, case.to_owned()));
        facts.assume(
            &mut self.terms,
            &self.vocabulary,
            is,
            holds,
            evidence.clone(),
        );
        let truth = match case {
            "true" => Some(holds),
            "false" => Some(!holds),
            _ => None,
        };
        if let Some(truth) = truth {
            facts.assume(
                &mut self.terms,
                &self.vocabulary,
                term,
                truth,
                evidence.clone(),
            );
        }
    }

    /// The obligation of a call of a function whose `requires` is bare.
    fn call(
        &mut self,
        env: &mut Env,
        facts: &FactSet,
        body: Body<'v>,
        node: &Node,
        evaluator: &mut Evaluator<'_>,
    ) -> Option<Found> {
        let (Expr::Call { target, arguments } | Expr::TailCall { target, arguments }) =
            &node.expr
        else {
            return None;
        };
        let CallTarget::Direct { function, .. } = target else {
            return None;
        };
        let data = self.guarded.get(function)?;
        let guard = data.requires?;
        let span = node.origin.span();
        let found = |verdict| Found {
            kind: ObligationKind::Requires,
            owner: body.owner,
            span,
            guard: Guard::Function(*function),
            verdict,
        };
        if !data.requires_certified {
            return Some(found(Verdict::Unsupported));
        }
        let (call, substituted) =
            facts::build(&mut self.terms, &self.vocabulary, env, node);
        let operands = match self.terms.get(call) {
            Some(Data::Call(_, operands)) => operands.clone(),
            _ => Vec::new(),
        };
        // The argument of each slot of the callee, and where it is written.
        let mut slots = BTreeMap::new();
        for (at, written) in arguments.positional.iter().enumerate() {
            if let Some(term) = operands.get(at) {
                slots.insert(at, (*term, written.origin.span()));
            }
        }
        for (at, (label, written)) in arguments.labelled.iter().enumerate() {
            let (Some(slot), Some(term)) = (
                data.labels.iter().position(|name| name == label),
                operands.get(arguments.positional.len() + at),
            ) else {
                continue;
            };
            slots.insert(data.positional + slot, (*term, written.origin.span()));
        }
        let names = data.names.clone();
        let types = data.types.clone();
        let predicate = &guard.predicate;
        let decided = self.constant(
            (body.owner, span, ObligationKind::Requires),
            predicate,
            &slots,
            &names,
            &types,
            evaluator,
        );
        let verdict = match decided {
            Decided::Truth(true, _) => {
                Verdict::Passed(prefixed(substituted, Evidence::of(CONSTANTS)))
            }
            Decided::Truth(false, counterexample) => Verdict::Refuted {
                counterexample,
                related: first_read(predicate, &slots).unwrap_or(span),
            },
            Decided::Limit => Verdict::Limit,
            Decided::Open => {
                let mut callee = Env::new(self.activation(), false);
                for (slot, (term, _)) in &slots {
                    callee.bind(*slot, *term, false);
                }
                let (goal, _) = facts::build(
                    &mut self.terms,
                    &self.vocabulary,
                    &mut callee,
                    predicate,
                );
                match self.establish(facts, &[goal]) {
                    Some(evidence) => Verdict::Passed(prefixed(substituted, evidence)),
                    None => Verdict::Unknown,
                }
            }
        };
        if matches!(verdict, Verdict::Passed(_)) {
            self.discharged.insert(call);
        }
        Some(found(verdict))
    }

    /// The obligation of a construction of a type whose `where` is bare.
    fn construct(
        &mut self,
        env: &mut Env,
        facts: &FactSet,
        body: Body<'v>,
        node: &Node,
        evaluator: &mut Evaluator<'_>,
    ) -> Option<Found> {
        let Expr::Construct {
            value_type: Type::Declared(id, _),
            construction,
        } = &node.expr
        else {
            return None;
        };
        let (ty, invariant) = self.invariants.get(id)?;
        let ty = *ty;
        if !invariant.bare {
            return None;
        }
        let span = node.origin.span();
        let found = |verdict| Found {
            kind: ObligationKind::Where,
            owner: body.owner,
            span,
            guard: Guard::Type(ty),
            verdict,
        };
        if !invariant.certified {
            return Some(found(Verdict::Unsupported));
        }
        let entry = invariant.entry;
        let unfolded = invariant.body.clone();
        let named = invariant.named.clone();
        let representation = invariant.representation.clone();
        let (term, substituted) =
            facts::build(&mut self.terms, &self.vocabulary, env, node);
        let repr = self.terms.intern(Data::Repr(term));
        let constant = match (construction, &representation) {
            (Construction::Wrapper(_), _) => self.terms.constant(repr).cloned(),
            (Construction::Record(fields), Some(ty)) => {
                self.operand_constants(term).and_then(|values| {
                    let value = Value::Record(
                        fields
                            .iter()
                            .map(|(name, _)| name.clone())
                            .zip(values)
                            .collect(),
                    );
                    Constant::new(ty.clone(), value).ok()
                })
            }
            (Construction::Tuple(_), Some(ty)) => {
                self.operand_constants(term).and_then(|values| {
                    Constant::new(ty.clone(), Value::Tuple(values)).ok()
                })
            }
            _ => None,
        };
        let mut verdict = None;
        if let (Some(constant), Some(entry)) = (&constant, entry) {
            let key = (body.owner, span, ObligationKind::Where);
            let decided = match self.decided.get(&key) {
                Some(decided) => decided.clone(),
                None => {
                    let call = predicate_call(entry, constant, node);
                    let decided = match evaluator.evaluate(&call) {
                        Outcome::Value(value) => match facts::truth(&value) {
                            Some(holds) => {
                                let name = unfolded
                                    .as_ref()
                                    .and_then(|(_, name)| name.clone());
                                let counterexample = (!holds)
                                    .then(|| {
                                        counterexample(
                                            evaluator,
                                            &[(name, constant.clone())],
                                        )
                                    })
                                    .flatten();
                                Decided::Truth(holds, counterexample)
                            }
                            None => Decided::Open,
                        },
                        Outcome::LimitReached => Decided::Limit,
                        Outcome::Unevaluated => Decided::Open,
                    };
                    self.decided.insert(key, decided.clone());
                    decided
                }
            };
            verdict = match decided {
                Decided::Truth(true, _) => Some(Verdict::Passed(prefixed(
                    substituted,
                    Evidence::of(CONSTANTS),
                ))),
                Decided::Truth(false, counterexample) => Some(Verdict::Refuted {
                    counterexample,
                    related: span,
                }),
                Decided::Limit => Some(Verdict::Limit),
                Decided::Open => None,
            };
        }
        let verdict = match verdict {
            Some(verdict) => verdict,
            None => {
                let mut goals = Vec::new();
                if let Some(entry) = entry {
                    goals.push(self.terms.intern(Data::Call(
                        Callee::Function(entry, Vec::new()),
                        vec![repr],
                    )));
                }
                let mut read = Evidence::default();
                if let Some((predicate, _)) = unfolded {
                    let mut callee = Env::new(self.activation(), false);
                    callee.bind(0, repr, false);
                    let (goal, _) = facts::build(
                        &mut self.terms,
                        &self.vocabulary,
                        &mut callee,
                        predicate,
                    );
                    goals.push(goal);
                    if let Some(named) = &named {
                        read = read.reading(named);
                    }
                }
                match self.establish(facts, &goals) {
                    Some(evidence) => {
                        Verdict::Passed(prefixed(substituted, evidence).and(&read))
                    }
                    None => Verdict::Unknown,
                }
            }
        };
        Some(found(verdict))
    }

    /// The constant operands of the construction `term`.
    fn operand_constants(&self, term: Term) -> Option<Vec<Value>> {
        let Some(Data::Construct(_, _, operands)) = self.terms.get(term) else {
            return None;
        };
        operands
            .iter()
            .map(|operand| {
                self.terms
                    .constant(*operand)
                    .map(|constant| constant.value().clone())
            })
            .collect()
    }

    /// The `ensures` obligation at one exit of a body: the `return` of
    /// `value`, or a `try` exit, whose value nothing is known of.
    fn exit(
        &mut self,
        env: &mut Env,
        facts: &FactSet,
        body: Body<'v>,
        value: Option<&Node>,
        span: ByteSpan,
        evaluator: &mut Evaluator<'_>,
    ) -> Option<Exit> {
        let data = self.guarded.get(&body.function?)?;
        let (slot, guard) = data.ensures?;
        if !data.ensures_certified {
            return None;
        }
        let names = data.names.clone();
        let mut types = data.types.clone();
        let result_name = data.result_name.clone();
        let result_type = data.result_type.clone();
        let (term, substituted) = match value {
            Some(value) => facts::build(&mut self.terms, &self.vocabulary, env, value),
            None => (self.terms.opaque(), false),
        };
        let mut slots = BTreeMap::new();
        slots.insert(slot, (term, span));
        let mut names = names;
        while names.len() <= slot {
            names.push(String::new());
            types.push(Type::Void);
        }
        if let Some(name) = names.get_mut(slot) {
            name.clone_from(&result_name);
        }
        if let Some(ty) = types.get_mut(slot) {
            *ty = result_type;
        }
        let decided = if value.is_some() {
            self.constant(
                (body.owner, span, ObligationKind::Ensures),
                &guard.predicate,
                &slots,
                &names,
                &types,
                evaluator,
            )
        } else {
            Decided::Open
        };
        let verdict = match decided {
            Decided::Truth(true, _) => {
                Verdict::Passed(prefixed(substituted, Evidence::of(CONSTANTS)))
            }
            Decided::Truth(false, counterexample) => Verdict::Refuted {
                counterexample,
                related: span,
            },
            Decided::Limit => Verdict::Limit,
            Decided::Open => {
                // The predicate reads the parameters as the body does.
                let mut own = Env::new(env.activation, true);
                own.bind(slot, term, false);
                let (goal, _) = facts::build(
                    &mut self.terms,
                    &self.vocabulary,
                    &mut own,
                    &guard.predicate,
                );
                match self.establish(facts, &[goal]) {
                    Some(evidence) => Verdict::Passed(prefixed(substituted, evidence)),
                    None => Verdict::Unknown,
                }
            }
        };
        Some(Exit {
            span,
            verdict,
            through_try: value.is_none(),
        })
    }

    /// Decides `predicate` by evaluation (`GUARD-1`) when every slot it
    /// reads is one of `slots` with a constant argument.
    fn constant(
        &mut self,
        key: (usize, ByteSpan, ObligationKind),
        predicate: &Node,
        slots: &BTreeMap<usize, (Term, ByteSpan)>,
        names: &[String],
        types: &[Type],
        evaluator: &mut Evaluator<'_>,
    ) -> Decided {
        if let Some(decided) = self.decided.get(&key) {
            return decided.clone();
        }
        let mut values = BTreeMap::new();
        for slot in read_slots(predicate) {
            let Some(constant) = slots
                .get(&slot)
                .and_then(|(term, _)| self.terms.constant(*term))
            else {
                return Decided::Open;
            };
            values.insert(slot, constant.clone());
        }
        let decided = match evaluator.evaluate(&with_constants(predicate, &values)) {
            Outcome::Value(value) => match facts::truth(&value) {
                Some(true) => Decided::Truth(true, None),
                Some(false) => {
                    let named = values
                        .iter()
                        .map(|(slot, constant)| {
                            let name = names.get(*slot).cloned();
                            let constant = types
                                .get(*slot)
                                .and_then(|ty| {
                                    Constant::new(ty.clone(), constant.value().clone())
                                        .ok()
                                })
                                .unwrap_or_else(|| constant.clone());
                            (name, constant)
                        })
                        .collect::<Vec<_>>();
                    Decided::Truth(false, counterexample(evaluator, &named))
                }
                None => Decided::Open,
            },
            Outcome::LimitReached => Decided::Limit,
            Outcome::Unevaluated => Decided::Open,
        };
        self.decided.insert(key, decided.clone());
        decided
    }

    /// The evidence that one of `goals` holds under `facts`, with the
    /// invariants (`GUARD-2`) and certified results (`GUARD-7`) of the terms
    /// the goal is built from.
    fn establish(&mut self, facts: &FactSet, goals: &[Term]) -> Option<Evidence> {
        let mut local = facts.clone();
        let mut seen = BTreeSet::new();
        for goal in goals {
            for term in self.terms.subterms(*goal) {
                if !seen.insert(term) {
                    continue;
                }
                self.invariant_facts(&mut local, term);
                self.result_facts(&mut local, term);
            }
        }
        goals.iter().find_map(|goal| {
            facts::prove(&mut self.terms, &self.vocabulary, &local, *goal)
        })
    }

    /// Assumes the invariant of the type of `term`, when it states one.
    fn invariant_facts(&mut self, facts: &mut FactSet, term: Term) {
        let Some(Type::Declared(id, _)) = self.terms.type_of(term) else {
            return;
        };
        let id = id.clone();
        let Some((_, invariant)) = self.invariants.get(&id) else {
            return;
        };
        let mut evidence = Evidence::of(INVARIANT).reading(&invariant.identity);
        if let Some(named) = &invariant.named {
            evidence = evidence.reading(named);
        }
        let entry = invariant.entry;
        let body = invariant.body.as_ref().map(|(body, _)| *body);
        let repr = self.terms.intern(Data::Repr(term));
        if let Some(entry) = entry {
            let call = self
                .terms
                .intern(Data::Call(Callee::Function(entry, Vec::new()), vec![repr]));
            facts.assume(
                &mut self.terms,
                &self.vocabulary,
                call,
                true,
                evidence.clone(),
            );
        }
        if let Some(body) = body {
            let mut env = Env::new(self.activation(), false);
            env.bind(0, repr, false);
            let (predicate, _) =
                facts::build(&mut self.terms, &self.vocabulary, &mut env, body);
            facts.assume(&mut self.terms, &self.vocabulary, predicate, true, evidence);
        }
    }

    /// Assumes the proven `ensures` of the callee of `term`, when its
    /// `requires` was discharged or is absent.
    fn result_facts(&mut self, facts: &mut FactSet, term: Term) {
        let Some(Data::Call(Callee::Function(function, _), operands)) =
            self.terms.get(term).cloned()
        else {
            return;
        };
        if !self.proven.contains(&function) {
            return;
        }
        let Some(data) = self.guarded.get(&function) else {
            return;
        };
        let established = match data.entry {
            Entry::None => true,
            Entry::Bare => self.discharged.contains(&term),
            Entry::Else => false,
        };
        let Some((slot, guard)) = data.ensures.filter(|_| established) else {
            return;
        };
        let evidence = Evidence::of(RESULT).reading(&data.identity);
        let mut env = Env::new(self.activation(), false);
        for (at, operand) in operands.iter().enumerate() {
            env.bind(at, *operand, false);
        }
        env.bind(slot, term, false);
        let (predicate, _) = facts::build(
            &mut self.terms,
            &self.vocabulary,
            &mut env,
            &guard.predicate,
        );
        facts.assume(&mut self.terms, &self.vocabulary, predicate, true, evidence);
    }

    /// Whether the function at checked index `function` converts to a
    /// callable value: its `requires` has an `else`, is absent, or the types
    /// of its parameters establish it (`GUARD-2`).
    fn convertible(&mut self, function: usize) -> bool {
        let Some(data) = self.guarded.get(&function) else {
            return true;
        };
        let Some(guard) = data.requires else {
            return true;
        };
        let mut env = Env::new(self.activation(), true);
        let (goal, _) = facts::build(
            &mut self.terms,
            &self.vocabulary,
            &mut env,
            &guard.predicate,
        );
        self.establish(&FactSet::default(), &[goal]).is_some()
    }

    /// Reports every obligation and writes its record.
    fn report(
        &mut self,
        context: &mut Context<'_, '_>,
        found: Vec<Found>,
        exits: BTreeMap<usize, Vec<Exit>>,
        lost: &[(usize, (ByteSpan, usize))],
    ) {
        for found in found {
            self.site(context, found);
        }
        for (function, exits) in exits {
            self.ensures(context, function, &exits);
        }
        for (owner, (span, function)) in lost {
            let (Some(declaration), Some(data)) =
                (self.declarations.get(*owner), self.guarded.get(function))
            else {
                continue;
            };
            let module = declaration.module;
            let mut diagnostic = context.diagnostic(
                DiagnosticCode::ContractObligationLost,
                module,
                *span,
                format!(
                    "`{}` becomes a callable value here, which keeps no obligation, \
                     and the types of its parameters do not establish its bare \
                     `requires`; give the guard an `else`, or take a parameter of a \
                     type whose invariant states it",
                    data.qualified
                ),
            );
            if let Some(guard) = data.requires {
                diagnostic = context.program().related(
                    diagnostic,
                    data.module,
                    guard.span,
                    "the bare `requires`",
                );
            }
            context.push(module, diagnostic);
        }
    }

    /// Reports the obligation of one site and records it.
    fn site(&self, context: &mut Context<'_, '_>, found: Found) {
        let Some(declaration) = self.declarations.get(found.owner) else {
            return;
        };
        let module = declaration.module;
        let (predicate, guard_module, guard_span, name_span, what) = match found.guard {
            Guard::Function(function) => {
                let Some(data) = self.guarded.get(&function) else {
                    return;
                };
                let Some(guard) = data.requires else {
                    return;
                };
                (
                    data.identity.clone(),
                    data.module,
                    guard.span,
                    data.name,
                    format!("the bare `requires` of `{}`", data.qualified),
                )
            }
            Guard::Type(ty) => {
                let Some(data) = context.program().types.get(ty) else {
                    return;
                };
                let Some((_, invariant)) =
                    data.id.as_ref().and_then(|id| self.invariants.get(id))
                else {
                    return;
                };
                (
                    invariant.identity.clone(),
                    invariant.module,
                    invariant.span,
                    invariant.name_span,
                    format!("the bare `where` of `{}`", invariant.name),
                )
            }
        };
        let at = match found.kind {
            ObligationKind::Where => "this construction",
            _ => "this call",
        };
        match &found.verdict {
            Verdict::Unknown => {
                let text = context.source_text(guard_module, guard_span);
                let diagnostic = context.diagnostic(
                    DiagnosticCode::ContractUnproved,
                    module,
                    found.span,
                    format!(
                        "the predicate `{text}` of {what} is not established at {at} \
                         by the discharge rules"
                    ),
                );
                let diagnostic = unproved_notes(context.program().related(
                    diagnostic,
                    guard_module,
                    name_span,
                    "the guarded declaration",
                ));
                context.push(module, diagnostic);
            }
            Verdict::Refuted { related, .. } => {
                let diagnostic = context.diagnostic(
                    DiagnosticCode::ContractRefuted,
                    guard_module,
                    guard_span,
                    format!("the predicate of {what} is false at {at}"),
                );
                let diagnostic = context.program().related(
                    diagnostic,
                    module,
                    *related,
                    "the refuting value",
                );
                context.push(guard_module, diagnostic);
            }
            Verdict::Limit => context.limit(guard_module, guard_span),
            Verdict::Passed(_) | Verdict::Unsupported => {}
        }
        let record = record(
            context,
            found.kind,
            declaration,
            predicate,
            found.span,
            &[&found.verdict],
        );
        context.reviewed.obligations.push(record);
    }

    /// Reports the `ensures` obligation of the function at checked index
    /// `function` from its exits, and records it.
    fn ensures(&self, context: &mut Context<'_, '_>, function: usize, exits: &[Exit]) {
        let Some(data) = self.guarded.get(&function) else {
            return;
        };
        let Some((_, guard)) = data.ensures else {
            return;
        };
        let Some(declaration) = self
            .declarations
            .iter()
            .find(|declaration| declaration.identity == data.identity)
        else {
            return;
        };
        let module = data.module;
        let verdicts = if data.ensures_certified {
            exits.iter().map(|exit| &exit.verdict).collect::<Vec<_>>()
        } else {
            vec![&Verdict::Unsupported]
        };
        let refuted = exits
            .iter()
            .find(|exit| matches!(exit.verdict, Verdict::Refuted { .. }));
        let limited = exits
            .iter()
            .any(|exit| matches!(exit.verdict, Verdict::Limit));
        let open = exits
            .iter()
            .filter(|exit| matches!(exit.verdict, Verdict::Unknown))
            .collect::<Vec<_>>();
        // A contract's `ensures` is reported at the implementation's
        // `return`s; a limit is reported at the implementation's name.
        let span = if data.contract.is_some() {
            data.name
        } else {
            guard.span
        };
        if !data.ensures_certified {
        } else if let (Some(exit), Some((contract, _))) = (refuted, data.contract) {
            let diagnostic = context.diagnostic(
                DiagnosticCode::ContractRefuted,
                module,
                exit.span,
                format!(
                    "the bare `ensures` of the contract member `{}` is false of the \
                     value this `return` returns",
                    data.qualified
                ),
            );
            let diagnostic = context.program().related(
                diagnostic,
                contract,
                guard.span,
                "the contract's `ensures`",
            );
            context.push(module, diagnostic);
        } else if let Some(exit) = refuted {
            let diagnostic = context.diagnostic(
                DiagnosticCode::ContractRefuted,
                module,
                guard.span,
                format!(
                    "the bare `ensures` of `{}` is false of the value this `return` \
                     returns",
                    data.qualified
                ),
            );
            let diagnostic = context.program().related(
                diagnostic,
                module,
                exit.span,
                "the refuting `return`",
            );
            context.push(module, diagnostic);
        } else if limited {
            context.limit(module, span);
        } else if let (Some(first), Some((contract, name))) =
            (open.first(), data.contract)
        {
            let text = context.source_text(contract, guard.span);
            let diagnostic = context.diagnostic(
                DiagnosticCode::ContractUnproved,
                module,
                first.span,
                format!(
                    "this exit does not establish the predicate `{text}` of the bare \
                     `ensures` of the contract member `{}` by the discharge rules",
                    data.qualified
                ),
            );
            let mut diagnostic = context.program().related(
                diagnostic,
                contract,
                name,
                "the contract member",
            );
            for exit in open.iter().skip(1) {
                diagnostic = context.program().related(
                    diagnostic,
                    module,
                    exit.span,
                    "another exit that does not establish it",
                );
            }
            context.push(module, unproved_notes(diagnostic));
        } else if !open.is_empty() {
            let text = context.source_text(module, guard.span);
            let diagnostic = context.diagnostic(
                DiagnosticCode::ContractUnproved,
                module,
                guard.span,
                format!(
                    "the predicate `{text}` of the bare `ensures` of `{}` is not \
                     established at every exit of the body by the discharge rules",
                    data.qualified
                ),
            );
            let mut diagnostic = context.program().related(
                diagnostic,
                module,
                data.name,
                "the guarded declaration",
            );
            for exit in open {
                let message = if exit.through_try {
                    "the `try` exit that does not establish it"
                } else {
                    "the `return` that does not establish it"
                };
                diagnostic = context
                    .program()
                    .related(diagnostic, module, exit.span, message);
            }
            context.push(module, unproved_notes(diagnostic));
        }
        let record = record(
            context,
            ObligationKind::Ensures,
            declaration,
            data.identity.clone(),
            guard.span,
            &verdicts,
        );
        // The site of an `ensures` is its predicate, which a contract
        // member writes in the interface's module.
        let record = match data
            .contract
            .and_then(|(contract, _)| context.program().modules.get(contract))
        {
            Some(interface) => Obligation {
                source_id: interface.source_id.to_string(),
                ..record
            },
            None => record,
        };
        context.reviewed.obligations.push(record);
    }
}

/// `diagnostic` with the three repairs as notes.
fn unproved_notes(
    diagnostic: vibra_diagnostics::Diagnostic,
) -> vibra_diagnostics::Diagnostic {
    REPAIRS.iter().fold(diagnostic, |diagnostic, repair| {
        diagnostic.with_note(*repair)
    })
}

/// The record of an obligation whose verdicts, one per exit for an
/// `ensures`, are `verdicts`.
fn record(
    context: &Context<'_, '_>,
    kind: ObligationKind,
    declaration: &Declaration,
    predicate: String,
    span: ByteSpan,
    verdicts: &[&Verdict],
) -> Obligation {
    let mut evidence = Evidence::default();
    let mut status = ObligationStatus::Passed;
    let mut counterexample = None;
    for verdict in verdicts {
        match verdict {
            Verdict::Passed(found) => evidence = evidence.and(found),
            Verdict::Refuted {
                counterexample: found,
                ..
            } => {
                if status != ObligationStatus::Refuted {
                    counterexample.clone_from(found);
                }
                status = ObligationStatus::Refuted;
            }
            Verdict::Unknown | Verdict::Limit => {
                if status == ObligationStatus::Passed {
                    status = ObligationStatus::Unknown;
                }
            }
            Verdict::Unsupported => {
                if status != ObligationStatus::Refuted {
                    status = ObligationStatus::Unsupported;
                }
            }
        }
    }
    let rules = if status == ObligationStatus::Passed {
        evidence.identifiers()
    } else if status == ObligationStatus::Refuted {
        vec!["GUARD-1".to_owned()]
    } else {
        Vec::new()
    };
    let mut dependencies = BTreeSet::new();
    dependencies.insert(predicate.clone());
    if status == ObligationStatus::Passed {
        dependencies.extend(evidence.reads.iter().cloned());
    }
    dependencies.remove(&declaration.identity);
    Obligation {
        kind,
        declaration: declaration.identity.clone(),
        predicate,
        source_id: context
            .program()
            .modules
            .get(declaration.module)
            .map(|module| module.source_id.to_string())
            .unwrap_or_default(),
        span,
        status,
        rules,
        dependencies: dependencies.into_iter().collect(),
        counterexample: counterexample.filter(|_| status == ObligationStatus::Refuted),
    }
}

/// `evidence`, after `GUARD-4` when the goal was formed by a substitution.
fn prefixed(substituted: bool, evidence: Evidence) -> Evidence {
    if substituted {
        Evidence::of(SUBSTITUTION).and(&evidence)
    } else {
        evidence
    }
}

/// The canonical VIBON text of the refuting values: a record of each named
/// parameter's value, or the one value of an unnamed one.
fn counterexample(
    evaluator: &mut Evaluator<'_>,
    values: &[(Option<String>, Constant)],
) -> Option<String> {
    match values {
        [(None, constant)] => evaluator.encode(constant.value(), constant.value_type()),
        _ => {
            let mut fields = values
                .iter()
                .map(|(name, constant)| {
                    (
                        name.clone().unwrap_or_default(),
                        constant.value_type().clone(),
                        constant.value().clone(),
                    )
                })
                .collect::<Vec<_>>();
            fields.sort_by(|left, right| left.0.cmp(&right.0));
            let ty = Type::record(
                fields
                    .iter()
                    .map(|(name, ty, _)| (name.clone(), ty.clone())),
            )
            .ok()?;
            let value = Value::Record(
                fields
                    .into_iter()
                    .map(|(name, _, value)| (name, value))
                    .collect(),
            );
            evaluator.encode(&value, &ty)
        }
    }
}

/// The span of the argument of the lowest slot `predicate` reads.
fn first_read(
    predicate: &Node,
    slots: &BTreeMap<usize, (Term, ByteSpan)>,
) -> Option<ByteSpan> {
    read_slots(predicate)
        .into_iter()
        .find_map(|slot| slots.get(&slot).map(|(_, span)| *span))
}

/// The call of the predicate at checked index `entry` over `constant`.
fn predicate_call(entry: usize, constant: &Constant, at: &Node) -> Node {
    Node::new(
        Expr::Call {
            target: CallTarget::Direct {
                function: entry,
                type_arguments: Vec::new(),
            },
            arguments: Arguments {
                positional: vec![Node::new(
                    Expr::Literal(constant.clone()),
                    at.origin.clone(),
                )],
                labelled: Vec::new(),
            },
        },
        at.origin.clone(),
    )
}

/// The slots `node` reads, closures aside.
fn read_slots(node: &Node) -> BTreeSet<usize> {
    let mut slots = BTreeSet::new();
    let mut pending = vec![node];
    while let Some(node) = pending.pop() {
        if let Expr::Variable { slot, .. } = node.expr {
            slots.insert(slot);
        }
        pending.extend(operands(node));
    }
    slots
}

/// `node` with each read of a slot of `values` replaced by its constant.
fn with_constants(node: &Node, values: &BTreeMap<usize, Constant>) -> Node {
    let mut node = node.clone();
    let mut pending = vec![&mut node];
    while let Some(node) = pending.pop() {
        if let Expr::Variable { slot, .. } = node.expr
            && let Some(constant) = values.get(&slot)
        {
            node.expr = Expr::Literal(constant.clone());
            continue;
        }
        match &mut node.expr {
            Expr::Intrinsic { arguments, .. } => pending.extend(arguments.iter_mut()),
            Expr::Sequence(nodes) => pending.extend(nodes.iter_mut()),
            Expr::Closure { captures, .. } => pending.extend(captures.iter_mut()),
            Expr::Let { value, body, .. } => {
                pending.push(value);
                pending.push(body);
            }
            Expr::LetElse {
                value,
                fallback,
                body,
                ..
            } => {
                pending.push(value);
                pending.push(fallback);
                pending.push(body);
            }
            Expr::Match { subject, arms, .. } => {
                pending.push(subject);
                pending.extend(arms.iter_mut().map(|arm| &mut arm.body));
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                pending.push(condition);
                pending.push(then_branch);
                if let Some(branch) = else_branch {
                    pending.push(branch);
                }
            }
            Expr::Construct { construction, .. } => match construction {
                Construction::Tuple(nodes) | Construction::List(nodes) => {
                    pending.extend(nodes.iter_mut());
                }
                Construction::Record(fields) | Construction::Case { fields, .. } => {
                    pending.extend(fields.iter_mut().map(|(_, node)| node));
                }
                Construction::Wrapper(inner) => pending.push(inner),
                Construction::Dict(entries) => {
                    for (key, value) in entries {
                        pending.push(key);
                        pending.push(value);
                    }
                }
            },
            Expr::Field { record, .. } => pending.push(record),
            Expr::Lookup { collection, key } => {
                pending.push(collection);
                pending.push(key);
            }
            Expr::Call { target, arguments } | Expr::TailCall { target, arguments } => {
                if let CallTarget::Value(callee) = target {
                    pending.push(callee);
                }
                pending.extend(arguments.positional.iter_mut());
                pending.extend(arguments.labelled.iter_mut().map(|(_, node)| node));
            }
            Expr::Return(inner)
            | Expr::Try(inner)
            | Expr::WidenAtom(inner)
            | Expr::WidenToInterface { value: inner, .. } => pending.push(inner),
            Expr::Literal(_)
            | Expr::Variable { .. }
            | Expr::Global(_)
            | Expr::Function { .. }
            | Expr::Captured { .. }
            | Expr::Default(_) => {}
        }
    }
    node
}

/// Pushes the steps of a walk of `node`.
fn visit<'n>(
    node: &'n Node,
    pending: &mut Vec<Task<'n>>,
    closures: &mut Vec<&'n Node>,
) {
    match &node.expr {
        Expr::Let { slot, value, body } => {
            pending.push(Task::Leave);
            pending.push(Task::Visit(body));
            pending.push(Task::Bind(*slot, value));
            pending.push(Task::Visit(value));
            pending.push(Task::Enter);
        }
        Expr::LetElse {
            pattern,
            value,
            fallback,
            body,
        } => {
            pending.push(Task::Leave);
            pending.push(Task::Visit(body));
            pending.push(Task::Refuted(pattern, value));
            pending.push(Task::Enter);
            pending.push(Task::Leave);
            pending.push(Task::Visit(fallback));
            pending.push(Task::Enter);
            pending.push(Task::Visit(value));
        }
        Expr::Match { subject, arms, .. } => {
            for (at, arm) in arms.iter().enumerate().rev() {
                pending.push(Task::Leave);
                pending.push(Task::Visit(&arm.body));
                pending.push(Task::Arm(subject, arms, at));
                pending.push(Task::Enter);
            }
            pending.push(Task::Visit(subject));
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            if let Some(branch) = else_branch {
                pending.push(Task::Leave);
                pending.push(Task::Visit(branch));
                pending.push(Task::Assume(condition, false));
                pending.push(Task::Enter);
            }
            pending.push(Task::Leave);
            pending.push(Task::Visit(then_branch));
            pending.push(Task::Assume(condition, true));
            pending.push(Task::Enter);
            pending.push(Task::Visit(condition));
        }
        Expr::Call { target, arguments } | Expr::TailCall { target, arguments } => {
            pending.push(Task::Call(node));
            let written = arguments
                .positional
                .iter()
                .chain(arguments.labelled.iter().map(|(_, node)| node))
                .collect::<Vec<_>>();
            pending.extend(written.into_iter().rev().map(Task::Visit));
            if let CallTarget::Value(callee) = target {
                pending.push(Task::Visit(callee));
            }
        }
        Expr::Construct { .. } => {
            pending.push(Task::Construct(node));
            pending.extend(operands(node).into_iter().rev().map(Task::Visit));
        }
        Expr::Return(inner) => {
            pending.push(Task::Return(inner, node.origin.span()));
            pending.push(Task::Visit(inner));
        }
        Expr::Try(inner) => {
            pending.push(Task::Try(node.origin.span()));
            pending.push(Task::Visit(inner));
        }
        Expr::Function { function, .. } => {
            pending.push(Task::Lost(node.origin.span(), *function));
        }
        Expr::Closure { closure, captures } => {
            closures.push(&closure.body);
            pending.extend(captures.iter().rev().map(Task::Visit));
        }
        _ => pending.extend(operands(node).into_iter().rev().map(Task::Visit)),
    }
}

/// The `Bool` operations, comparison interfaces, invariant types, constant
/// module values, labelled defaults, and certified bodies of `context`.
fn vocabulary<'v>(context: &Context<'v, '_>) -> Vocabulary<'v> {
    let inputs = context.inputs;
    let program = inputs.program;
    let layout = inputs.layout;
    let entry = |function: usize| layout.functions.get(function).copied().flatten();
    let mut vocabulary = Vocabulary::default();
    for (binding, entity) in &program.library {
        match (binding, entity) {
            (Library::Bool, Entity::Type(ty)) => {
                let Some(data) = program.types.get(*ty) else {
                    continue;
                };
                for (name, connective) in [
                    ("not", Connective::Not),
                    ("and", Connective::And),
                    ("or", Connective::Or),
                    ("xor", Connective::Xor),
                ] {
                    if let Some(at) = data.members.get(name).copied().and_then(entry) {
                        vocabulary.connectives.insert(at, connective);
                    }
                }
            }
            (Library::Ordered, Entity::Interface(index)) => {
                vocabulary.ordered =
                    program.interfaces.get(*index).and_then(|i| i.id.clone());
            }
            (Library::Equatable, Entity::Interface(index)) => {
                vocabulary.equatable =
                    program.interfaces.get(*index).and_then(|i| i.id.clone());
            }
            _ => {}
        }
    }
    for data in &program.types {
        if data.invariant
            && let Some(id) = &data.id
        {
            vocabulary.invariant_types.insert(id.clone());
        }
    }
    for (index, data) in program.values.iter().enumerate() {
        let (Some(global), Some(ty)) = (
            layout.globals.get(index).copied().flatten(),
            data.ty.as_ref(),
        ) else {
            continue;
        };
        if let Some(value) = layout.constants.get(&global)
            && let Ok(constant) = Constant::new(ty.clone(), value.clone())
        {
            vocabulary.globals.insert(global, constant);
        }
    }
    for (index, checked) in inputs.functions {
        let Some(at) = entry(*index) else { continue };
        if !checked.defaults.is_empty() {
            vocabulary.defaults.insert(at, checked.defaults.clone());
        }
        if context.standings.get(at) == Some(&Standing::Certified)
            && let (Some(body), Some(data)) =
                (checked.node.as_ref(), program.functions.get(*index))
        {
            vocabulary.certified.insert(
                at,
                Readable {
                    body,
                    identity: data.identity.clone(),
                },
            );
        }
    }
    vocabulary
}

/// What the discharge reads of each local or library function with a
/// guard, by checked index.
fn guarded<'v>(
    context: &Context<'v, '_>,
    certified: &super::Certified,
) -> BTreeMap<usize, Guarded<'v>> {
    let inputs = context.inputs;
    let program = inputs.program;
    let mut out = BTreeMap::new();
    for (index, checked) in inputs.functions {
        let (Some(at), Some(data)) = (
            inputs.layout.functions.get(*index).copied().flatten(),
            program.functions.get(*index),
        ) else {
            continue;
        };
        let Some(signature) = &data.signature else {
            continue;
        };
        let (entry_ok, exit_ok) = certified.get(index).copied().unwrap_or_default();
        let entry = match &checked.guards.entry {
            None => Entry::None,
            Some(guard) if guard.consequence.is_none() => Entry::Bare,
            Some(_) => Entry::Else,
        };
        let requires = checked
            .guards
            .entry
            .as_ref()
            .filter(|guard| guard.consequence.is_none());
        let ensures = checked
            .guards
            .exit
            .as_ref()
            .filter(|(_, guard)| guard.consequence.is_none())
            .map(|(slot, guard)| (*slot, guard));
        if entry == Entry::None && checked.guards.exit.is_none() {
            continue;
        }
        let text = program
            .modules
            .get(data.module)
            .and_then(|module| module.text);
        let mut names = data
            .view
            .takes()
            .into_iter()
            .map(|(pattern, _)| {
                let span = pattern.node().span();
                text.and_then(|text| text.get(span.start()..span.end()))
                    .unwrap_or_default()
                    .to_owned()
            })
            .collect::<Vec<_>>();
        let labels = signature
            .labels
            .entries()
            .iter()
            .map(|(label, _)| label.clone())
            .collect::<Vec<_>>();
        names.extend(labels.iter().cloned());
        let mut types = signature.parameters.clone();
        types.extend(signature.labels.entries().iter().map(|(_, ty)| ty.clone()));
        out.insert(
            at,
            Guarded {
                module: data.module,
                identity: data.identity.clone(),
                qualified: data.name.clone(),
                name: data
                    .view
                    .name()
                    .map_or_else(|| ByteSpan::empty_at(0), |name| name.span()),
                positional: signature.parameters.len(),
                labels,
                names,
                types,
                entry,
                precondition: checked.guards.entry.as_ref(),
                requires,
                requires_certified: entry_ok,
                ensures,
                ensures_certified: exit_ok,
                result_name: data
                    .view
                    .result()
                    .map(|(name, _)| name.text().to_owned())
                    .unwrap_or_default(),
                result_type: signature.result.clone(),
                contract: data.implements.and_then(|interface| {
                    let interface = program.interfaces.get(interface)?;
                    let member =
                        interface.view.members().into_iter().find(|member| {
                            member.name().is_some_and(|name| name.text() == data.name)
                        })?;
                    Some((interface.module, member.name()?.span()))
                }),
            },
        );
    }
    out
}

/// What the discharge reads of each type with an invariant, by identity.
fn invariants<'v>(
    context: &Context<'v, '_>,
) -> BTreeMap<TypeId, (usize, Invariant<'v>)> {
    let inputs = context.inputs;
    let program = inputs.program;
    let mut out = BTreeMap::new();
    for (ty, invariant) in inputs.invariants {
        let Some(data) = program.types.get(*ty) else {
            continue;
        };
        let Some(id) = data.id.clone() else { continue };
        let (body, named) = match &invariant.predicate {
            Predicate::Anonymous(closure) => (Some((&closure.body, None)), None),
            Predicate::Named(function) => {
                let body = inputs
                    .functions
                    .get(function)
                    .and_then(|checked| checked.node.as_ref());
                let function_data = program.functions.get(*function);
                let name = function_data.and_then(|data| {
                    let text = program.modules.get(data.module)?.text?;
                    let (pattern, _) = data.view.takes().into_iter().next()?;
                    let span = pattern.node().span();
                    text.get(span.start()..span.end()).map(str::to_owned)
                });
                (
                    body.map(|body| (body, name)),
                    function_data.map(|data| data.identity.clone()),
                )
            }
        };
        let representation = match &data.kind {
            TypeKind::Wrapper(representation) => Some(representation.clone()),
            TypeKind::Tuple(components) => Some(Type::Tuple(components.clone())),
            TypeKind::Record(fields) => Type::record(fields.clone()).ok(),
            _ => None,
        };
        out.insert(
            id,
            (
                *ty,
                Invariant {
                    module: data.module,
                    identity: data.identity.clone(),
                    name: data.name.clone(),
                    name_span: data
                        .view
                        .name()
                        .map_or_else(|| ByteSpan::empty_at(0), |name| name.span()),
                    entry: context.predicate_entry(*ty),
                    body,
                    named,
                    bare: invariant.consequence.is_none(),
                    certified: context.certified_predicate(*ty),
                    span: invariant.span,
                    representation,
                },
            ),
        );
    }
    out
}

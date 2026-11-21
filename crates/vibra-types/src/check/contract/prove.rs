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
//! An implementation member carries its contract member's guards, checked
//! under its substitution where it is lowered: a bare `requires` is a fact
//! of its body, and a bare `ensures` is reported at the `return` that does
//! not establish it, related to the contract member.
//!
//! Every walk is a work list, so no host stack grows with the depth of a
//! program.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use vibra_diagnostics::{ByteSpan, DiagnosticCode};
use vibra_ir::{
    Arguments, CallTarget, ClosureBody, Constant, Construction, Expr, Library,
    MatchArm, Node, Pattern, Type, TypeId, Value,
};

use super::super::callables::substitute;
use vibra_interp::Reads;

use super::super::ctfe::{Evaluator, Outcome, PROFILE};
use super::super::declare::{Entity, TypeKind};
use super::super::obligation::{Obligation, ObligationKind, ObligationStatus};
use super::facts::{
    self, BRANCH, Budget, CONSTANTS, Callee, Connective, Data, Env, Evidence, FactSet,
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
    /// Whether it is an anonymous callable, whose `identity` is the
    /// declaration it is written in.
    anonymous: bool,
    /// Whether applying it builds a plan: it is a `proc`, named or
    /// anonymous. Its `ensures` is a fact only of an `await` whose operand is
    /// directly its application, and only when it is named (`GUARD-7`).
    plan: bool,
    /// For a member of an implementation whose guards are its contract
    /// member's: the module of the interface and the contract member's name.
    contract: Option<(usize, ByteSpan)>,
}

impl Guarded<'_> {
    /// How a message names it.
    fn shown(&self) -> String {
        if self.anonymous {
            "the anonymous callable".to_owned()
        } else {
            format!("`{}`", self.qualified)
        }
    }
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
        /// The declarations the refuting evaluation read.
        reads: BTreeSet<String>,
    },
    Unknown,
    /// A compile-time evaluation reached the reduction limit.
    Limit,
    /// The proof budget stopped the rules before a verdict.
    Budget,
    Unsupported,
}

/// One obligation found at one site.
#[derive(Debug)]
struct Found {
    kind: ObligationKind,
    owner: usize,
    /// The module whose text `span` is in: the owner's, or for a contract
    /// guard copied into an implementation, the interface's.
    module: usize,
    /// Whether the site is in a contract guard copied into the owner.
    copied: bool,
    span: ByteSpan,
    guard: Guard,
    verdict: Verdict,
}

/// One exit of a body with a bare `ensures`.
#[derive(Debug)]
struct Exit {
    /// The callable it leaves, by checked index.
    function: usize,
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
    /// The module whose text the node's spans are in.
    module: usize,
    /// Whether the node is a contract guard copied into an implementation.
    copied: bool,
    /// Whether the body is the function's own, whose returns are its exits.
    exits: bool,
}

/// A declaration that contains sites.
#[derive(Debug)]
struct Declaration {
    module: usize,
    identity: String,
    /// The name of an implementation member, which a site of the contract
    /// guard copied into it relates.
    implementation: Option<ByteSpan>,
}

/// What the rules other than `GUARD-1` decided of one obligation.
#[derive(Clone, Debug)]
enum Proof {
    /// It holds, by this evidence.
    Proven(Evidence),
    /// The rules examined it and could not establish it.
    Unknown,
    /// The proof budget stopped the rules before a verdict.
    Limit,
}

/// What `GUARD-1` decided once, by owner, site, and kind.
#[derive(Clone, Debug)]
enum Decided {
    /// The truth, the counterexample of a false one, and the identities of
    /// the declarations the evaluation read.
    Truth(bool, Option<String>, BTreeSet<String>),
    Limit,
    Open,
}

/// What a walk found.
#[derive(Debug, Default)]
struct Walked {
    found: Vec<Found>,
    exits: Vec<Exit>,
    /// The callables with a guard whose activation the walk went through.
    started: Vec<usize>,
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
    /// The checked index of each member a program's own `impl` supplies at
    /// a closed receiver, by interface, interface arguments, receiver, and
    /// member name.
    members: &'v [(TypeId, Vec<Type>, Type, String, usize)],
    /// The entry a call of a contract member at any other receiver is
    /// decided from, by interface and member name: the default body, or
    /// the abstract member's own guards.
    contracts: BTreeMap<(TypeId, String), usize>,
    /// The identity of each function, by entry, and of each module value,
    /// by global index, a compile-time evaluation may read, outside the
    /// embedded standard library.
    readable: (BTreeMap<usize, String>, BTreeMap<usize, String>),
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
    /// A node visited where it does not convert an anonymous callable.
    Held(&'n Node),
    /// A read that may convert an anonymous callable with a guard.
    Read(&'n Node),
}

/// Discharges every bare-guard obligation of the local declarations.
pub(super) fn discharge(
    context: &mut Context<'_, '_>,
    (certified, closure_certified, contract_certified): (
        &super::Certified,
        &super::ClosureCertified,
        &super::ContractCertified,
    ),
    members: &[(TypeId, Vec<Type>, Type, String, usize)],
    evaluator: &mut Evaluator<'_>,
) {
    let inputs = context.inputs;
    let mut vocabulary = vocabulary(context);
    let mut guarded = guarded(context, certified);
    // Each anonymous callable with a guard is numbered after every checked
    // entry.
    let (anonymous, keys, defaults) =
        closures(context, closure_certified, context.standings.len());
    guarded.extend(anonymous);
    // Each abstract contract member with a bare `requires` is numbered
    // after them.
    let base = context.standings.len() + keys.len();
    let (abstract_members, contracts) = contracts(context, contract_certified, base);
    for (entry, data) in &abstract_members {
        if let Some(contract) = context
            .inputs
            .contracts
            .values()
            .find(|contract| contract.identity == data.identity)
        {
            vocabulary
                .defaults
                .insert(*entry, contract.defaults.clone());
        }
    }
    guarded.extend(abstract_members);
    vocabulary.closures = keys;
    vocabulary.defaults.extend(defaults);
    let mut prover = Prover {
        terms: Terms::default(),
        vocabulary,
        guarded,
        invariants: invariants(context),
        declarations: Vec::new(),
        owners: BTreeMap::new(),
        proven: BTreeSet::new(),
        discharged: BTreeSet::new(),
        activations: 0,
        decided: BTreeMap::new(),
        members,
        contracts,
        readable: readable(context),
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
        if data.implements.is_some()
            && let Some(declaration) = prover.declarations.get_mut(owner)
        {
            declaration.implementation = data.view.name().map(|name| name.span());
        }
        let entry = inputs.layout.functions.get(*index).copied().flatten();
        if let Some(node) = &checked.node {
            bodies.push(Body {
                owner,
                function: entry,
                node,
                module: data.module,
                copied: false,
                exits: true,
            });
        }
        // An implementation member's guards are its contract's, written in
        // the interface's module.
        let guard_module = data
            .implements
            .and_then(|interface| inputs.program.interfaces.get(interface))
            .map_or(data.module, |interface| interface.module);
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
                    module: guard_module,
                    copied: data.implements.is_some(),
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
                    module,
                    copied: false,
                    exits: false,
                });
            }
        }
    }
    // A `where` operand is a function value of `Fn({R} Bool)`: it converts
    // what it names, and an anonymous one is an activation of its own.
    for (index, checked) in inputs.invariants {
        let key = Owner::Type(*index);
        let Some((module, identity)) = context.owner(key) else {
            continue;
        };
        if !context.local(module) {
            continue;
        }
        let owner = prover.declare(key, module, identity);
        bodies.push(Body {
            owner,
            function: None,
            node: &checked.operand,
            module,
            copied: false,
            exits: false,
        });
    }
    // The proven `ensures`, as a least fixed point.
    let ensured = prover
        .guarded
        .iter()
        .filter(|(_, data)| data.ensures.is_some() && data.ensures_certified)
        .map(|(entry, _)| *entry)
        .collect::<BTreeSet<_>>();
    // A body is walked for them when its own `ensures` or that of an
    // anonymous callable in it is one of them.
    let anonymous = ensured
        .iter()
        .any(|entry| prover.guarded.get(entry).is_some_and(|data| data.anonymous));
    for _ in 0..=ensured.len() {
        let mut proven = BTreeSet::new();
        for body in &bodies {
            if !anonymous
                && !body
                    .exits
                    .then_some(body.function)
                    .flatten()
                    .is_some_and(|entry| ensured.contains(&entry))
            {
                continue;
            }
            let walked = prover.walk(*body, evaluator, false);
            for function in walked.started {
                // An anonymous `proc`'s `ensures` is never a certified
                // result: no `await` runs it by name (`GUARD-7`).
                let anonymous_plan = prover
                    .guarded
                    .get(&function)
                    .is_some_and(|data| data.anonymous && data.plan);
                if ensured.contains(&function)
                    && !anonymous_plan
                    && walked
                        .exits
                        .iter()
                        .filter(|exit| exit.function == function)
                        .all(|exit| matches!(exit.verdict, Verdict::Passed(_)))
                {
                    proven.insert(function);
                }
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
        for function in walked.started {
            exits.entry(function).or_default();
        }
        for exit in walked.exits {
            exits.entry(exit.function).or_default().push(exit);
        }
    }
    prover.unwalked(context, &mut found, evaluator);
    prover.report(context, found, exits, &lost);
}

impl<'v> Prover<'v> {
    /// `evidence`, having read the values of the module values `globals`.
    fn values_read(
        &self,
        mut evidence: Evidence,
        globals: &BTreeSet<usize>,
    ) -> Evidence {
        for global in globals {
            if let Some(identity) = self.readable.1.get(global) {
                evidence = evidence.reading(identity);
            }
        }
        evidence
    }

    /// `verdict`, its decision having read the values of the module values
    /// `globals`.
    fn with_values(&self, verdict: Verdict, globals: &BTreeSet<usize>) -> Verdict {
        match verdict {
            Verdict::Passed(evidence) => {
                Verdict::Passed(self.values_read(evidence, globals))
            }
            Verdict::Refuted {
                counterexample,
                related,
                mut reads,
            } => {
                reads.extend(self.values_read(Evidence::default(), globals).reads);
                Verdict::Refuted {
                    counterexample,
                    related,
                    reads,
                }
            }
            other => other,
        }
    }

    /// The identities of the declarations a compile-time evaluation read.
    fn read(&self, reads: &Reads) -> BTreeSet<String> {
        let (functions, values) = &self.readable;
        reads
            .functions
            .iter()
            .filter_map(|entry| functions.get(entry))
            .chain(
                reads
                    .module_values
                    .iter()
                    .filter_map(|global| values.get(global)),
            )
            .cloned()
            .collect()
    }

    fn declare(&mut self, owner: Owner, module: usize, identity: String) -> usize {
        self.declarations.push(Declaration {
            module,
            identity,
            implementation: None,
        });
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
        for (module, owner, site) in inputs.sites {
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
                            .map(|call| evaluator.evaluate_reading(&call));
                        match outcome {
                            Some((Outcome::LimitReached, _)) => Verdict::Limit,
                            Some((Outcome::Value(value), reads)) => {
                                let reads = self.read(&reads);
                                match facts::truth(&value) {
                                    Some(true) => Verdict::Passed(evaluated(
                                        Evidence::default(),
                                        reads,
                                    )),
                                    Some(false) => Verdict::Refuted {
                                        counterexample: None,
                                        related: *span,
                                        reads,
                                    },
                                    None => Verdict::Unknown,
                                }
                            }
                            _ => Verdict::Unknown,
                        }
                    } else {
                        Verdict::Unsupported
                    };
                    (ObligationKind::Where, *span, Guard::Type(*ty), verdict)
                }
                Site::Contract {
                    interface,
                    member,
                    span,
                } => {
                    let Some(entry) = inputs
                        .program
                        .interfaces
                        .get(*interface)
                        .and_then(|data| data.id.clone())
                        .and_then(|id| self.contracts.get(&(id, member.clone())))
                        .copied()
                    else {
                        continue;
                    };
                    let Some(data) = self.guarded.get(&entry) else {
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
                module: *module,
                copied: false,
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
        if let Some(function) = body.function {
            self.precondition(&mut env, &mut facts, function);
        }
        // The callable whose exits the `return`s of the activation walked
        // are.
        let mut current = body.function.filter(|_| body.exits);
        walked.started.extend(current);
        let mut marks = Vec::new();
        let mut closures: Vec<&'v ClosureBody> = Vec::new();
        let mut guards: Vec<&'v Node> = Vec::new();
        let mut seen = BTreeSet::new();
        let mut pending = vec![Task::Visit(body.node)];
        loop {
            let Some(task) = pending.pop() else {
                // Each anonymous callable, and each of its guards, is an
                // activation of its own, with no fact of the body it is
                // written in. A generic one used in several places is
                // walked once.
                env = Env::new(self.activation(), true);
                facts = FactSet::default();
                if let Some(guard) = guards.pop() {
                    current = None;
                    pending.push(Task::Visit(guard));
                    continue;
                }
                let Some(closure) = closures.pop() else {
                    break;
                };
                let origin = &closure.body.origin;
                let key = (Arc::from(origin.source_id()), origin.span());
                if !seen.insert(key.clone()) {
                    continue;
                }
                current = self.vocabulary.closures.get(&key).copied();
                if let Some(function) = current {
                    self.precondition(&mut env, &mut facts, function);
                    walked.started.push(function);
                    if let Some(data) = self.guarded.get(&function) {
                        let bare = data
                            .requires
                            .iter()
                            .chain(data.ensures.iter().map(|(_, guard)| guard))
                            .map(|guard| &guard.predicate);
                        guards.extend(bare);
                    }
                    let checked = closure
                        .entry_guard
                        .iter()
                        .chain(closure.exit_guard.iter().map(|exit| &exit.guard));
                    for guard in checked {
                        guards.push(&guard.predicate);
                        guards.push(&guard.consequence);
                    }
                }
                pending.push(Task::Visit(&closure.body));
                continue;
            };
            match task {
                Task::Visit(node) => visit(node, true, &mut pending, &mut closures),
                Task::Held(node) => visit(node, false, &mut pending, &mut closures),
                Task::Read(node) => {
                    if last {
                        let (term, _) = facts::build(
                            &mut self.terms,
                            &self.vocabulary,
                            &mut env,
                            node,
                        );
                        if let Some(Data::Closure(closure)) =
                            self.terms.get(term).cloned()
                            && !self.convertible(closure)
                        {
                            walked.lost.push((node.origin.span(), closure));
                        }
                    }
                }
                Task::Enter => marks.push(env.mark(&facts)),
                Task::Leave => {
                    if let Some(mark) = marks.pop() {
                        env.restore(&mut facts, mark);
                    }
                }
                Task::Assume(condition, holds) => {
                    let (term, substituted, globals) = facts::build_reading(
                        &mut self.terms,
                        &self.vocabulary,
                        &mut env,
                        condition,
                    );
                    let mut evidence = self.values_read(Evidence::default(), &globals);
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
                    let (term, _, globals) = facts::build_reading(
                        &mut self.terms,
                        &self.vocabulary,
                        &mut env,
                        value,
                    );
                    if let Some(slot) = slot {
                        env.bind_reading(slot, term, true, globals);
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
                    if let Some(function) = current
                        && let Some(exit) = self.exit(
                            &mut env,
                            &facts,
                            (body, function),
                            Some(value),
                            span,
                            evaluator,
                        )
                    {
                        walked.exits.push(exit);
                    }
                }
                Task::Try(span) => {
                    if let Some(function) = current
                        && let Some(exit) = self.exit(
                            &mut env,
                            &facts,
                            (body, function),
                            None,
                            span,
                            evaluator,
                        )
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
        let (subject, substituted, globals) =
            facts::build_reading(&mut self.terms, &self.vocabulary, env, subject);
        let mut evidence = self.values_read(Evidence::default(), &globals);
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
                Pattern::Bind(slot) => {
                    env.bind_reading(*slot, term, false, globals.clone())
                }
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
        // A contract member called at a closed receiver is the member the
        // receiver's implementation supplies, with the contract's guard. A
        // call of a value is known when its callee is an anonymous callable
        // with a guard.
        let function = match target {
            CallTarget::Direct { function, .. } => *function,
            CallTarget::Contract(call) if call.member_arguments.is_empty() => self
                .members
                .iter()
                .find(|(interface, arguments, receiver, member, _)| {
                    *interface == call.interface
                        && *arguments == call.arguments
                        && *receiver == call.receiver
                        && *member == call.member
                })
                .map(|(.., entry)| *entry)
                .or_else(|| {
                    self.contracts
                        .get(&(call.interface.clone(), call.member.clone()))
                        .copied()
                })?,
            CallTarget::Contract(call) => self
                .contracts
                .get(&(call.interface.clone(), call.member.clone()))
                .copied()?,
            CallTarget::Value(_) => {
                let (call, _) =
                    facts::build(&mut self.terms, &self.vocabulary, env, node);
                match self.terms.get(call) {
                    Some(Data::Call(Callee::Function(closure, _), _)) => *closure,
                    _ => return None,
                }
            }
        };
        let function = &function;
        let data = self.guarded.get(function)?;
        let guard = data.requires?;
        let span = node.origin.span();
        let found = |verdict| Found {
            kind: ObligationKind::Requires,
            owner: body.owner,
            module: body.module,
            copied: body.copied,
            span,
            guard: Guard::Function(*function),
            verdict,
        };
        if !data.requires_certified {
            return Some(found(Verdict::Unsupported));
        }
        let (call, substituted, globals) =
            facts::build_reading(&mut self.terms, &self.vocabulary, env, node);
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
        // The term's labelled operands are in the callee's slot order.
        for (place, at) in facts::label_order(arguments).into_iter().enumerate() {
            let Some((label, written)) = arguments.labelled.get(at) else {
                continue;
            };
            let (Some(slot), Some(term)) = (
                data.labels.iter().position(|name| name == label),
                operands.get(arguments.positional.len() + place),
            ) else {
                continue;
            };
            slots.insert(data.positional + slot, (*term, written.origin.span()));
        }
        // A contract call's term holds each label it does not write as an
        // unknown; the callee's default is the argument it passes.
        if let CallTarget::Contract(_) = target
            && let Some(defaults) = self.vocabulary.defaults.get(function)
        {
            for (label, written) in &arguments.labelled {
                if !matches!(written.expr, Expr::Default(_)) {
                    continue;
                }
                let (Some(slot), Some((_, constant))) = (
                    data.labels.iter().position(|name| name == label),
                    defaults.iter().find(|(name, _)| name == label),
                ) else {
                    continue;
                };
                let term = self.terms.intern(Data::Constant(constant.clone()));
                slots.insert(data.positional + slot, (term, written.origin.span()));
            }
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
            Decided::Truth(true, _, reads) => Verdict::Passed(evaluated(
                prefixed(substituted, Evidence::default()),
                reads,
            )),
            Decided::Truth(false, counterexample, reads) => Verdict::Refuted {
                counterexample,
                related: first_read(predicate, &slots).unwrap_or(span),
                reads,
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
                    Proof::Proven(evidence) => {
                        Verdict::Passed(prefixed(substituted, evidence))
                    }
                    Proof::Unknown => Verdict::Unknown,
                    Proof::Limit => Verdict::Budget,
                }
            }
        };
        if matches!(verdict, Verdict::Passed(_)) {
            self.discharged.insert(call);
        }
        Some(found(self.with_values(verdict, &globals)))
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
            value_type: Type::Declared(id, arguments),
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
            module: body.module,
            copied: body.copied,
            span,
            guard: Guard::Type(ty),
            verdict,
        };
        if !invariant.certified {
            return Some(found(Verdict::Unsupported));
        }
        let entry = invariant.entry;
        // A generic type's predicate is read at the construction's type
        // arguments.
        let unfolded = invariant
            .body
            .as_ref()
            .map(|(body, name)| (instantiated(body, arguments), name.clone()));
        let named = invariant.named.clone();
        let representation = invariant
            .representation
            .as_ref()
            .map(|representation| representation.instantiate(arguments));
        let (term, substituted, globals) =
            facts::build_reading(&mut self.terms, &self.vocabulary, env, node);
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
                    let call = predicate_call(entry, arguments, constant, node);
                    let (outcome, reads) = evaluator.evaluate_reading(&call);
                    let reads = self.read(&reads);
                    let decided = match outcome {
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
                                Decided::Truth(holds, counterexample, reads)
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
                Decided::Truth(true, _, reads) => Some(Verdict::Passed(evaluated(
                    prefixed(substituted, Evidence::default()),
                    reads,
                ))),
                Decided::Truth(false, counterexample, reads) => {
                    Some(Verdict::Refuted {
                        counterexample,
                        related: span,
                        reads,
                    })
                }
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
                        Callee::Function(entry, arguments.clone()),
                        vec![repr],
                    )));
                }
                let mut read = Evidence::default();
                if let Some((predicate, _)) = &unfolded {
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
                match self.establish_except(facts, &goals, Some(term)) {
                    Proof::Proven(evidence) => {
                        Verdict::Passed(prefixed(substituted, evidence).and(&read))
                    }
                    Proof::Unknown => Verdict::Unknown,
                    Proof::Limit => Verdict::Budget,
                }
            }
        };
        Some(found(self.with_values(verdict, &globals)))
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
        (body, function): (Body<'v>, usize),
        value: Option<&Node>,
        span: ByteSpan,
        evaluator: &mut Evaluator<'_>,
    ) -> Option<Exit> {
        let data = self.guarded.get(&function)?;
        let (slot, guard) = data.ensures?;
        if !data.ensures_certified {
            return None;
        }
        let names = data.names.clone();
        let mut types = data.types.clone();
        let result_name = data.result_name.clone();
        let result_type = data.result_type.clone();
        let (term, substituted, globals) = match value {
            Some(value) => {
                facts::build_reading(&mut self.terms, &self.vocabulary, env, value)
            }
            None => (self.terms.opaque(), false, BTreeSet::new()),
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
            Decided::Truth(true, _, reads) => Verdict::Passed(evaluated(
                prefixed(substituted, Evidence::default()),
                reads,
            )),
            Decided::Truth(false, counterexample, reads) => Verdict::Refuted {
                counterexample,
                related: span,
                reads,
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
                    Proof::Proven(evidence) => {
                        Verdict::Passed(prefixed(substituted, evidence))
                    }
                    Proof::Unknown => Verdict::Unknown,
                    Proof::Limit => Verdict::Budget,
                }
            }
        };
        Some(Exit {
            function,
            span,
            verdict: self.with_values(verdict, &globals),
            through_try: value.is_none(),
        })
    }

    /// Assumes in `facts` the `requires` of the function at checked index
    /// `function`, whose activation `env` is: the body runs only when it
    /// holds.
    fn precondition(&mut self, env: &mut Env, facts: &mut FactSet, function: usize) {
        let Some(guard) = self
            .guarded
            .get(&function)
            .and_then(|data| data.precondition)
        else {
            return;
        };
        let (term, _, globals) = facts::build_reading(
            &mut self.terms,
            &self.vocabulary,
            env,
            &guard.predicate,
        );
        let evidence = self.values_read(Evidence::of(SUBSTITUTION), &globals);
        facts.assume(&mut self.terms, &self.vocabulary, term, true, evidence);
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
        let (outcome, reads) =
            evaluator.evaluate_reading(&with_constants(predicate, &values));
        let reads = self.read(&reads);
        let decided = match outcome {
            Outcome::Value(value) => match facts::truth(&value) {
                Some(true) => Decided::Truth(true, None, reads),
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
                    Decided::Truth(false, counterexample(evaluator, &named), reads)
                }
                None => Decided::Open,
            },
            Outcome::LimitReached => Decided::Limit,
            Outcome::Unevaluated => Decided::Open,
        };
        self.decided.insert(key, decided.clone());
        decided
    }

    /// Whether one of `goals` holds under `facts`, with the invariants
    /// (`GUARD-2`) and certified results (`GUARD-7`) of the terms the goal
    /// is built from.
    fn establish(&mut self, facts: &FactSet, goals: &[Term]) -> Proof {
        self.establish_except(facts, goals, None)
    }

    /// [`Self::establish`], with no invariant read of `constructing`: the
    /// construction whose own `where` is the goal is no value yet.
    fn establish_except(
        &mut self,
        facts: &FactSet,
        goals: &[Term],
        constructing: Option<Term>,
    ) -> Proof {
        let mut local = facts.clone();
        let mut seen = BTreeSet::new();
        if let Some(term) = constructing {
            seen.insert(term);
            self.result_facts(&mut local, term);
        }
        for goal in goals {
            for term in self.terms.subterms(*goal) {
                if !seen.insert(term) {
                    continue;
                }
                self.invariant_facts(&mut local, term);
                self.result_facts(&mut local, term);
            }
        }
        // One budget serves the whole proof of the obligation.
        let mut budget = Budget::new(PROFILE.proof_derivations);
        let found = goals.iter().find_map(|goal| {
            facts::prove(
                &mut self.terms,
                &self.vocabulary,
                &local,
                *goal,
                &mut budget,
            )
        });
        match found {
            Some(evidence) => Proof::Proven(evidence),
            None if budget.exhausted() => Proof::Limit,
            None => Proof::Unknown,
        }
    }

    /// Assumes the invariant of the type of `term`, when it states one.
    fn invariant_facts(&mut self, facts: &mut FactSet, term: Term) {
        let Some(Type::Declared(id, arguments)) = self.terms.type_of(term) else {
            return;
        };
        let (id, arguments) = (id.clone(), arguments.clone());
        let Some((_, invariant)) = self.invariants.get(&id) else {
            return;
        };
        let mut evidence = Evidence::of(INVARIANT).reading(&invariant.identity);
        if let Some(named) = &invariant.named {
            evidence = evidence.reading(named);
        }
        let entry = invariant.entry;
        let body = invariant
            .body
            .as_ref()
            .map(|(body, _)| instantiated(body, &arguments));
        let repr = self.terms.intern(Data::Repr(term));
        if let Some(entry) = entry {
            let call = self
                .terms
                .intern(Data::Call(Callee::Function(entry, arguments), vec![repr]));
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
                facts::build(&mut self.terms, &self.vocabulary, &mut env, &body);
            facts.assume(&mut self.terms, &self.vocabulary, predicate, true, evidence);
        }
    }

    /// Assumes the proven `ensures` of the callee of `term`, when its
    /// `requires` was discharged or is absent: `term` is a call of a
    /// function, or the `await` of a plan whose operand is directly the
    /// application of a named `proc` (`GUARD-7`). Building a plan gives no
    /// fact.
    fn result_facts(&mut self, facts: &mut FactSet, term: Term) {
        let (call, awaited) = match self.terms.get(term) {
            Some(Data::Awaited(plan, _)) => (*plan, true),
            _ => (term, false),
        };
        let Some(Data::Call(Callee::Function(function, _), operands)) =
            self.terms.get(call).cloned()
        else {
            return;
        };
        if !self.proven.contains(&function) {
            return;
        }
        let Some(data) = self.guarded.get(&function) else {
            return;
        };
        if data.plan != awaited || (data.plan && data.anonymous) {
            return;
        }
        let established = match data.entry {
            Entry::None => true,
            Entry::Bare => self.discharged.contains(&call),
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
        matches!(
            self.establish(&FactSet::default(), &[goal]),
            Proof::Proven(_)
        )
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
                    "{} becomes a callable value here, which keeps no obligation, \
                     and the types of its parameters do not establish its bare \
                     `requires`; give the guard an `else`, or take a parameter of a \
                     type whose invariant states it",
                    data.shown()
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
        let module = found.module;
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
                    format!("the bare `requires` of {}", data.shown()),
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
                let diagnostic = unproved_notes(self.implementation(
                    context,
                    declaration,
                    found.copied,
                    context.program().related(
                        diagnostic,
                        guard_module,
                        name_span,
                        "the guarded declaration",
                    ),
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
                let diagnostic =
                    self.implementation(context, declaration, found.copied, diagnostic);
                context.push(guard_module, diagnostic);
            }
            Verdict::Limit => context.limit(guard_module, guard_span),
            Verdict::Budget => context.proof_limit(guard_module, guard_span),
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
        // A site of a contract guard spans the interface's module.
        let record = match context.program().modules.get(module) {
            Some(site) if module != declaration.module => Obligation {
                source_id: site.source_id.to_string(),
                ..record
            },
            _ => record,
        };
        context.reviewed.obligations.push(record);
    }

    /// `diagnostic`, related to the implementation member `declaration` when
    /// its site is in the contract guard `copied` into it: each
    /// implementation has an obligation of its own.
    fn implementation(
        &self,
        context: &Context<'_, '_>,
        declaration: &Declaration,
        copied: bool,
        diagnostic: vibra_diagnostics::Diagnostic,
    ) -> vibra_diagnostics::Diagnostic {
        match declaration.implementation {
            Some(name) if copied => context.program().related(
                diagnostic,
                declaration.module,
                name,
                "the implementation the contract guard is copied into",
            ),
            _ => diagnostic,
        }
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
        let budgeted = exits
            .iter()
            .any(|exit| matches!(exit.verdict, Verdict::Budget));
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
                    "the bare `ensures` of {} is false of the value this `return` \
                     returns",
                    data.shown()
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
        } else if budgeted {
            context.proof_limit(module, span);
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
                    "the predicate `{text}` of the bare `ensures` of {} is not \
                     established at every exit of the body by the discharge rules",
                    data.shown()
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
    let mut refuting = BTreeSet::new();
    for verdict in verdicts {
        match verdict {
            Verdict::Passed(found) => evidence = evidence.and(found),
            Verdict::Refuted {
                counterexample: found,
                reads,
                ..
            } => {
                if status != ObligationStatus::Refuted {
                    counterexample.clone_from(found);
                }
                status = ObligationStatus::Refuted;
                refuting.extend(reads.iter().cloned());
            }
            Verdict::Unknown | Verdict::Limit | Verdict::Budget => {
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
    } else if status == ObligationStatus::Refuted {
        dependencies.extend(refuting);
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

/// `evidence`, then `GUARD-1`, having read the declarations `reads`.
fn evaluated(evidence: Evidence, reads: BTreeSet<String>) -> Evidence {
    let mut evidence = evidence.with(CONSTANTS);
    evidence.reads.extend(reads);
    evidence
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
/// `body`, a predicate of a type, read at the type arguments `arguments`.
fn instantiated(body: &Node, arguments: &[Type]) -> Node {
    let mut body = body.clone();
    if !arguments.is_empty() {
        substitute(&mut body, arguments);
    }
    body
}

fn predicate_call(
    entry: usize,
    arguments: &[Type],
    constant: &Constant,
    at: &Node,
) -> Node {
    Node::new(
        Expr::Call {
            target: CallTarget::Direct {
                function: entry,
                type_arguments: arguments.to_vec(),
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
            | Expr::WidenToInterface { value: inner, .. }
            | Expr::Await { plan: inner } => pending.push(inner),
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
///
/// A read of a value that is an anonymous callable with a guard converts it
/// to a callable value, unless the read is the callee of a call or the
/// initializer of a `let`, which names it (`converts` is false).
fn visit<'n>(
    node: &'n Node,
    converts: bool,
    pending: &mut Vec<Task<'n>>,
    closures: &mut Vec<&'n ClosureBody>,
) {
    match &node.expr {
        Expr::Let { slot, value, body } => {
            pending.push(Task::Leave);
            pending.push(Task::Visit(body));
            pending.push(Task::Bind(*slot, value));
            pending.push(Task::Held(value));
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
                pending.push(Task::Held(callee));
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
            if converts {
                pending.push(Task::Read(node));
            }
            closures.push(closure);
            pending.extend(captures.iter().rev().map(Task::Visit));
        }
        Expr::Variable { .. } if converts => pending.push(Task::Read(node)),
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
        // A callable with a guard is never read through: its result is
        // not its body's when a guard with `else` fails, and its facts
        // come only from its proven `ensures` (`GUARD-7`).
        if context.standings.get(at) == Some(&Standing::Certified)
            && checked.guards.entry.is_none()
            && checked.guards.exit.is_none()
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

/// The identity of each function, by entry, and of each module value, by
/// global index, outside the embedded standard library: the declarations
/// whose checked body or value an obligation record lists when a
/// compile-time evaluation reads them.
fn readable(
    context: &Context<'_, '_>,
) -> (BTreeMap<usize, String>, BTreeMap<usize, String>) {
    let inputs = context.inputs;
    let program = inputs.program;
    let embedded =
        |module: usize| program.modules.get(module).is_none_or(|data| data.embedded);
    let functions = program
        .functions
        .iter()
        .enumerate()
        .filter(|(_, data)| !embedded(data.module))
        .filter_map(|(index, data)| {
            let entry = inputs.layout.functions.get(index).copied().flatten()?;
            Some((entry, data.identity.clone()))
        })
        .collect();
    let values = (0..program.values.len())
        .filter_map(|index| {
            let global = inputs.layout.globals.get(index).copied().flatten()?;
            let (module, identity) = context.owner(Owner::Value(index))?;
            (!embedded(module)).then_some((global, identity))
        })
        .collect();
    (functions, values)
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
                anonymous: false,
                plan: super::super::effects::procs::builds_plan(data),
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

/// What the discharge reads of each anonymous callable with a guard, by the
/// index it is given after the `base` checked entries; that index by the
/// source and the span of the callable's body; and its labelled defaults by
/// that index.
type Closures<'v> = (
    BTreeMap<usize, Guarded<'v>>,
    BTreeMap<(Arc<str>, ByteSpan), usize>,
    BTreeMap<usize, Vec<(String, Constant)>>,
);

fn closures<'v>(
    context: &Context<'v, '_>,
    certified: &super::ClosureCertified,
    base: usize,
) -> Closures<'v> {
    let mut out = BTreeMap::new();
    let mut keys = BTreeMap::new();
    let mut defaults = BTreeMap::new();
    for (owner, closures) in super::owners(context.inputs) {
        let Some((_, identity)) = context.owner(owner) else {
            continue;
        };
        for closure in closures {
            if keys.contains_key(&closure.body) {
                continue;
            }
            let at = base + keys.len();
            keys.insert(closure.body.clone(), at);
            defaults.insert(at, closure.defaults.clone());
            let (entry_ok, exit_ok) =
                certified.get(&closure.body).copied().unwrap_or_default();
            let guards = &closure.guards;
            let entry = match &guards.entry {
                None => Entry::None,
                Some(guard) if guard.consequence.is_none() => Entry::Bare,
                Some(_) => Entry::Else,
            };
            let signature = &closure.signature;
            let labels = signature
                .labels
                .entries()
                .iter()
                .map(|(label, _)| label.clone())
                .collect::<Vec<_>>();
            let mut types = signature.parameters.clone();
            types.extend(signature.labels.entries().iter().map(|(_, ty)| ty.clone()));
            out.insert(
                at,
                Guarded {
                    module: closure.module,
                    identity: identity.clone(),
                    qualified: String::new(),
                    name: closure.span,
                    positional: signature.parameters.len(),
                    labels,
                    names: closure.names.clone(),
                    types,
                    entry,
                    precondition: guards.entry.as_ref(),
                    requires: guards
                        .entry
                        .as_ref()
                        .filter(|guard| guard.consequence.is_none()),
                    requires_certified: entry_ok,
                    ensures: guards
                        .exit
                        .as_ref()
                        .filter(|(_, guard)| guard.consequence.is_none())
                        .map(|(slot, guard)| (*slot, guard)),
                    ensures_certified: exit_ok,
                    result_name: closure.result_name.clone(),
                    result_type: signature.result.clone(),
                    anonymous: true,
                    plan: closure.plan,
                    contract: None,
                },
            );
        }
    }
    (out, keys, defaults)
}

/// What the discharge reads of each abstract contract member with a bare
/// `requires`, numbered from `base`, and the entry a call of each contract
/// member at a receiver whose implementation member is not known is decided
/// from: that one, or the default body's.
fn contracts<'v>(
    context: &Context<'v, '_>,
    certified: &super::ContractCertified,
    base: usize,
) -> (
    BTreeMap<usize, Guarded<'v>>,
    BTreeMap<(TypeId, String), usize>,
) {
    let inputs = context.inputs;
    let program = inputs.program;
    let mut out = BTreeMap::new();
    let mut entries = BTreeMap::new();
    for interface in &program.interfaces {
        let Some(id) = &interface.id else { continue };
        for member in &interface.members {
            let entry = member.default.and_then(|function| {
                inputs.layout.functions.get(function).copied().flatten()
            });
            if let Some(entry) = entry {
                entries.insert((id.clone(), member.name.clone()), entry);
            }
        }
    }
    for (key, contract) in inputs.contracts {
        let Some(id) = program
            .interfaces
            .get(key.0)
            .and_then(|data| data.id.clone())
        else {
            continue;
        };
        let Some(requires) = contract
            .guards
            .entry
            .as_ref()
            .filter(|guard| guard.consequence.is_none())
        else {
            continue;
        };
        let at = base + out.len();
        let signature = &contract.signature;
        let mut types = signature.parameters.clone();
        types.extend(signature.labels.entries().iter().map(|(_, ty)| ty.clone()));
        out.insert(
            at,
            Guarded {
                module: contract.module,
                identity: contract.identity.clone(),
                qualified: contract.qualified.clone(),
                name: contract.name,
                positional: signature.parameters.len(),
                labels: signature.labels.names().map(str::to_owned).collect(),
                names: contract.names.clone(),
                types,
                entry: Entry::Bare,
                precondition: Some(requires),
                requires: Some(requires),
                requires_certified: certified.get(key).copied().unwrap_or_default(),
                ensures: None,
                ensures_certified: false,
                result_name: contract.result_name.clone(),
                result_type: signature.result.clone(),
                anonymous: false,
                plan: false,
                contract: None,
            },
        );
        entries.insert((id, key.1.clone()), at);
    }
    (out, entries)
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
        // A predicate with a guard is not unfolded: when a guard with
        // `else` fails its result is not its body's.
        let (body, named) = match &invariant.predicate {
            Predicate::Anonymous(closure) => (
                (closure.entry_guard.is_none() && closure.exit_guard.is_none())
                    .then_some((&closure.body, None)),
                None,
            ),
            Predicate::Named(function) => {
                let body = inputs
                    .functions
                    .get(function)
                    .filter(|checked| {
                        checked.guards.entry.is_none() && checked.guards.exit.is_none()
                    })
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

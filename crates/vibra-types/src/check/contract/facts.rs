//! Terms and facts: what the discharge rules reason about.
//!
//! `docs/spec/02-type-system.md`, "Bare guards", and
//! `docs/spec/07-diagnostics-and-conformance.md`, "Integer facts". A
//! [`Term`] is a value of the checked IR built only from parameters,
//! constants, calls, constructions, and the parts patterns take apart, with
//! every immutable `let` alias, literal equality, and actual argument
//! substituted: two terms are equal exactly when their resolved callee, type
//! arguments, and operands are the same entities (rule 4). A [`FactSet`]
//! holds the `Bool` terms known to be `true` or `false` at one program point,
//! each with the [`Evidence`] that established it. [`prove`] decides one goal
//! against a fact set by the Boolean structure of rule 5, the listed integer
//! facts of rule 6, and the case facts of rule 3, and nothing else.
//!
//! Every walk here is a work list, so no host stack grows with the depth of
//! a program.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use vibra_diagnostics::ByteSpan;
use vibra_ir::{
    Arguments, CallTarget, Constant, Construction, ContractCall, Expr, NamedTypes,
    Node, Pattern, Type, TypeId, Value,
};

/// A discharge rule or an integer fact, as an obligation record lists it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Rule {
    /// `GUARD-1` to `GUARD-7`.
    Guard(u8),
    /// `INT-1` to `INT-8`.
    Int(u8),
}

/// `GUARD-1`, constants.
pub(crate) const CONSTANTS: Rule = Rule::Guard(1);
/// `GUARD-2`, type invariant.
pub(crate) const INVARIANT: Rule = Rule::Guard(2);
/// `GUARD-3`, dominating branch.
pub(crate) const BRANCH: Rule = Rule::Guard(3);
/// `GUARD-4`, substitution.
pub(crate) const SUBSTITUTION: Rule = Rule::Guard(4);
/// `GUARD-5`, Boolean structure.
pub(crate) const BOOLEAN: Rule = Rule::Guard(5);
/// `GUARD-6`, the listed integer facts.
pub(crate) const INTEGER: Rule = Rule::Guard(6);
/// `GUARD-7`, certified results.
pub(crate) const RESULT: Rule = Rule::Guard(7);

/// What established a fact or a goal: the rules used, in the order first
/// used, and the declarations whose checked body or value was read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Evidence {
    rules: Vec<Rule>,
    /// The identities of the declarations read.
    pub(crate) reads: BTreeSet<String>,
}

impl Evidence {
    /// The evidence of one rule.
    pub(crate) fn of(rule: Rule) -> Self {
        Self::default().with(rule)
    }

    /// This evidence, then `rule`.
    pub(crate) fn with(mut self, rule: Rule) -> Self {
        if !self.rules.contains(&rule) {
            self.rules.push(rule);
        }
        self
    }

    /// This evidence, having read the declaration `identity`.
    pub(crate) fn reading(mut self, identity: &str) -> Self {
        if !identity.is_empty() {
            self.reads.insert(identity.to_owned());
        }
        self
    }

    /// This evidence, then `other`.
    pub(crate) fn and(mut self, other: &Self) -> Self {
        for rule in &other.rules {
            self = self.with(*rule);
        }
        self.reads.extend(other.reads.iter().cloned());
        self
    }

    /// The rule identifiers as an obligation record lists them: the
    /// discharge rules in the order first used, with the integer facts
    /// right after `GUARD-6`.
    pub(crate) fn identifiers(&self) -> Vec<String> {
        let facts = self
            .rules
            .iter()
            .filter_map(|rule| match rule {
                Rule::Int(fact) => Some(format!("INT-{fact}")),
                Rule::Guard(_) => None,
            })
            .collect::<Vec<_>>();
        let mut out = Vec::new();
        let mut placed = false;
        for rule in &self.rules {
            if let Rule::Guard(guard) = rule {
                out.push(format!("GUARD-{guard}"));
                if *guard == 6 {
                    out.extend(facts.iter().cloned());
                    placed = true;
                }
            }
        }
        if !placed && !facts.is_empty() {
            out.push("GUARD-6".to_owned());
            out.extend(facts);
        }
        out
    }
}

/// A term, by its place in a [`Terms`] arena.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Term(usize);

/// What a call of a term calls.
#[derive(Clone, Debug)]
pub(crate) enum Callee {
    /// A function, by its checked index, at its type arguments.
    Function(usize, Vec<Type>),
    /// An interface member at its substitution.
    Contract(Box<ContractCall>),
    /// A compiler intrinsic, by its spelling and type arguments.
    Intrinsic(String),
}

/// One term.
#[derive(Clone, Debug)]
pub(crate) enum Data {
    /// A value nothing more is known of; each is distinct.
    Opaque,
    /// An anonymous callable with a guard, by the index the discharge gives
    /// it: a call whose callee is this term calls it.
    Closure(usize),
    /// A slot of one activation: a parameter, or a binding nothing names.
    Slot(usize, usize),
    /// A constant.
    Constant(Constant),
    /// A call and its arguments in the callee's parameter-slot order: the
    /// positional ones, then one for each label in the callee's order, an
    /// omitted label holding its default.
    Call(Callee, Vec<Term>),
    /// A construction of the type, with its shape (`case:` and the case
    /// name for a case, `wrapper` for a wrapper, then the field names) and
    /// its operands.
    Construct(Type, Vec<String>, Vec<Term>),
    /// A field of a record.
    Field(Term, String),
    /// The representation of a value of a declared type.
    Repr(Term),
    /// A component of a tuple.
    Item(Term, usize),
    /// A field of a value of a case.
    CaseField(Term, String, String),
    /// Whether a value is of a case: a `Bool` term.
    IsCase(Term, String),
    /// The result of one `await` of the plan term, and an opaque term that
    /// tells it from every other `await`: each run of a plan is its own.
    Awaited(Term, Term),
}

/// The terms of one discharge, each stored once.
#[derive(Debug, Default)]
pub(crate) struct Terms {
    data: Vec<Data>,
    index: HashMap<String, Term>,
    /// The type of each slot term, as the slot's reads state it.
    slot_types: HashMap<Term, Type>,
}

impl Terms {
    /// The static type of `term`, when it is a slot, a constant, or a
    /// construction.
    pub(crate) fn type_of(&self, term: Term) -> Option<&Type> {
        match self.get(term)? {
            Data::Slot(..) => self.slot_types.get(&term),
            Data::Constant(constant) => Some(constant.value_type()),
            Data::Construct(ty, ..) => Some(ty),
            _ => None,
        }
    }

    /// `term` and every term it is built from, each once.
    pub(crate) fn subterms(&self, term: Term) -> Vec<Term> {
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        let mut pending = vec![term];
        while let Some(term) = pending.pop() {
            if !seen.insert(term) {
                continue;
            }
            out.push(term);
            match self.get(term) {
                Some(Data::Call(_, operands) | Data::Construct(_, _, operands)) => {
                    pending.extend(operands.iter().copied());
                }
                Some(
                    Data::Field(inner, _)
                    | Data::Repr(inner)
                    | Data::Item(inner, _)
                    | Data::CaseField(inner, ..)
                    | Data::IsCase(inner, _),
                ) => pending.push(*inner),
                _ => {}
            }
        }
        out
    }

    /// The term `data`, normalized: a part of a construction is the operand
    /// it names.
    pub(crate) fn intern(&mut self, data: Data) -> Term {
        let data = self.normalize(data);
        if let Data::Opaque = data {
            let term = Term(self.data.len());
            self.data.push(data);
            return term;
        }
        let key = key(&data);
        if let Some(term) = self.index.get(&key) {
            return *term;
        }
        let term = Term(self.data.len());
        self.data.push(data);
        self.index.insert(key, term);
        term
    }

    fn normalize(&self, data: Data) -> Data {
        let operand =
            |term: Term, wanted: &dyn Fn(&[String]) -> Option<usize>| match self
                .get(term)
            {
                Some(Data::Construct(_, shape, operands)) => {
                    wanted(shape).and_then(|at| operands.get(at).copied())
                }
                _ => None,
            };
        let found = match &data {
            Data::Repr(term) => operand(*term, &|shape| {
                (shape.first().map(String::as_str) == Some("wrapper")).then_some(0)
            }),
            Data::Field(term, field) => {
                if let Some(Data::Repr(inner)) = self.get(*term) {
                    return Data::Field(*inner, field.clone());
                }
                operand(*term, &|shape| {
                    if shape
                        .first()
                        .is_some_and(|first| first.starts_with("case:"))
                    {
                        return None;
                    }
                    shape.iter().position(|name| name == field)
                })
            }
            Data::Item(term, at) => {
                if let Some(Data::Repr(inner)) = self.get(*term) {
                    return Data::Item(*inner, *at);
                }
                operand(*term, &|shape| shape.is_empty().then_some(*at))
            }
            Data::CaseField(term, case, field) => operand(*term, &|shape| {
                let (head, names) = shape.split_first()?;
                (head.strip_prefix("case:") == Some(case.as_str()))
                    .then(|| names.iter().position(|name| name == field))
                    .flatten()
            }),
            _ => None,
        };
        match found.and_then(|term| self.get(term)) {
            Some(found) => found.clone(),
            None => data,
        }
    }

    /// A term distinct from every other.
    pub(crate) fn opaque(&mut self) -> Term {
        self.intern(Data::Opaque)
    }

    /// The term at `term`.
    pub(crate) fn get(&self, term: Term) -> Option<&Data> {
        self.data.get(term.0)
    }

    /// The constant `term` is, when it is one.
    pub(crate) fn constant(&self, term: Term) -> Option<&Constant> {
        match self.get(term)? {
            Data::Constant(constant) => Some(constant),
            _ => None,
        }
    }

    /// The case `term` is constructed with, when it is a construction of a
    /// case or a constant of one.
    pub(crate) fn case_of(&self, term: Term) -> Option<&str> {
        match self.get(term)? {
            Data::Construct(_, shape, _) => shape.first()?.strip_prefix("case:"),
            Data::Constant(constant) => match constant.value() {
                Value::Variant { case, .. } => Some(case),
                _ => None,
            },
            _ => None,
        }
    }
}

/// The text two equal terms share and two different terms do not.
fn key(data: &Data) -> String {
    match data {
        Data::Slot(activation, slot) => format!("s{activation}.{slot}"),
        Data::Call(callee, operands) => {
            let callee = match callee {
                Callee::Function(function, arguments) => {
                    format!("f{function}{arguments:?}")
                }
                Callee::Contract(call) => format!("k{call:?}"),
                Callee::Intrinsic(spelling) => format!("i{spelling}"),
            };
            format!("c{callee}{operands:?}")
        }
        Data::Awaited(plan, run) => format!("a{plan:?}.{run:?}"),
        _ => format!("{data:?}"),
    }
}

/// A `Bool` operation of the embedded library.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Connective {
    /// `Bool.not`.
    Not,
    /// `Bool.and`.
    And,
    /// `Bool.or`.
    Or,
    /// `Bool.xor`.
    Xor,
}

/// A comparison of two values of one sized integer type, read as its
/// defining relation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Relation {
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
    Equal,
    NotEqual,
}

impl Relation {
    /// The relation that holds when this one does not, by the defining
    /// relations of the `Ordered` members over a total order.
    const fn negation(self) -> Self {
        match self {
            Self::Less => Self::GreaterOrEqual,
            Self::LessOrEqual => Self::Greater,
            Self::Greater => Self::LessOrEqual,
            Self::GreaterOrEqual => Self::Less,
            Self::Equal => Self::NotEqual,
            Self::NotEqual => Self::Equal,
        }
    }
}

/// A body a call of a certified function may be read through.
#[derive(Clone, Debug)]
pub(crate) struct Readable<'v> {
    /// The body.
    pub(crate) body: &'v Node,
    /// The identity of the function.
    pub(crate) identity: String,
}

/// What the terms of one review are built against.
#[derive(Debug, Default)]
pub(crate) struct Vocabulary<'v> {
    /// The `Bool` operations, by checked function index.
    pub(crate) connectives: BTreeMap<usize, Connective>,
    /// The interfaces playing `@ordered` and `@equatable`.
    pub(crate) ordered: Option<TypeId>,
    pub(crate) equatable: Option<TypeId>,
    /// The declared types with an invariant.
    pub(crate) invariant_types: BTreeSet<TypeId>,
    /// The constant module values, by global index.
    pub(crate) globals: BTreeMap<usize, Constant>,
    /// The labelled defaults of each function, by checked index, in their
    /// checked form; one that holds a function value is no constant the
    /// prover reads.
    pub(crate) defaults: BTreeMap<usize, Vec<(String, Node)>>,
    /// The bodies of the certified functions, by checked index.
    pub(crate) certified: BTreeMap<usize, Readable<'v>>,
    /// The index of each anonymous callable with a guard, by the source and
    /// the span of its body.
    pub(crate) closures: BTreeMap<(Arc<str>, ByteSpan), usize>,
}

impl Vocabulary<'_> {
    /// The `Bool` operation `term` applies, and its operands.
    fn connective(&self, terms: &Terms, term: Term) -> Option<(Connective, Vec<Term>)> {
        match terms.get(term)? {
            Data::Call(Callee::Function(function, _), operands) => self
                .connectives
                .get(function)
                .map(|connective| (*connective, operands.clone())),
            _ => None,
        }
    }

    /// The integer comparison `term` is: its relation (`None` for
    /// `inRange`), its receiver, and its operands.
    fn comparison(
        &self,
        terms: &Terms,
        term: Term,
    ) -> Option<(Option<Relation>, ContractCall, Vec<Term>)> {
        let Some(Data::Call(Callee::Contract(call), operands)) = terms.get(term) else {
            return None;
        };
        if !sized_integer(&call.receiver) {
            return None;
        }
        let relation = if Some(&call.interface) == self.ordered.as_ref() {
            match call.member.as_str() {
                "less" => Some(Relation::Less),
                "lessOrEqual" => Some(Relation::LessOrEqual),
                "greater" => Some(Relation::Greater),
                "greaterOrEqual" => Some(Relation::GreaterOrEqual),
                "inRange" => None,
                _ => return None,
            }
        } else if Some(&call.interface) == self.equatable.as_ref()
            && call.member == "equal"
        {
            Some(Relation::Equal)
        } else {
            return None;
        };
        Some((relation, (**call).clone(), operands.clone()))
    }

    /// The call of the `Ordered` or `Equatable` member `member` at the
    /// receiver of `like`, over `left` and `right`.
    fn relation_term(
        &self,
        terms: &mut Terms,
        like: &ContractCall,
        relation: Relation,
        left: Term,
        right: Term,
    ) -> Option<Term> {
        let (interface, member) = match relation {
            Relation::Less => (self.ordered.clone()?, "less"),
            Relation::LessOrEqual => (self.ordered.clone()?, "lessOrEqual"),
            Relation::Greater => (self.ordered.clone()?, "greater"),
            Relation::GreaterOrEqual => (self.ordered.clone()?, "greaterOrEqual"),
            Relation::Equal | Relation::NotEqual => (self.equatable.clone()?, "equal"),
        };
        let call = ContractCall {
            interface,
            member: member.to_owned(),
            receiver: like.receiver.clone(),
            arguments: Vec::new(),
            choices: NamedTypes::empty(),
            member_arguments: Vec::new(),
        };
        Some(terms.intern(Data::Call(
            Callee::Contract(Box::new(call)),
            vec![left, right],
        )))
    }

    /// The `Equatable.equal` call over `left` and `right` at `receiver`.
    pub(crate) fn equality(
        &self,
        terms: &mut Terms,
        receiver: &Type,
        left: Term,
        right: Term,
    ) -> Option<Term> {
        let call = ContractCall {
            interface: self.equatable.clone()?,
            member: "equal".to_owned(),
            receiver: receiver.clone(),
            arguments: Vec::new(),
            choices: NamedTypes::empty(),
            member_arguments: Vec::new(),
        };
        Some(terms.intern(Data::Call(
            Callee::Contract(Box::new(call)),
            vec![left, right],
        )))
    }
}

/// Whether `ty` is one of the eight sized integer types.
pub(crate) const fn sized_integer(ty: &Type) -> bool {
    matches!(
        ty,
        Type::I8
            | Type::I16
            | Type::I32
            | Type::I64
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
    )
}

/// The integer a constant of a sized integer type holds.
const fn integer(value: &Value) -> Option<i128> {
    Some(match value {
        Value::I8(value) => *value as i128,
        Value::I16(value) => *value as i128,
        Value::I32(value) => *value as i128,
        Value::I64(value) => *value as i128,
        Value::U8(value) => *value as i128,
        Value::U16(value) => *value as i128,
        Value::U32(value) => *value as i128,
        Value::U64(value) => *value as i128,
        _ => return None,
    })
}

/// Whether `value` is the `Bool` value `true` (`Some(true)`) or `false`.
pub(crate) fn truth(value: &Value) -> Option<bool> {
    match value {
        Value::Variant { case, fields } if fields.is_empty() => match case.as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

/// What the slots of one activation stand for while terms are built.
#[derive(Clone, Debug, Default)]
pub(crate) struct Env {
    /// The activation.
    pub(crate) activation: usize,
    /// Whether a slot nothing binds is that slot of the activation; when it
    /// is not, it is an opaque value.
    pub(crate) open: bool,
    slots: BTreeMap<usize, Binding>,
    undo: Vec<(usize, Option<Binding>)>,
    rewrites: Vec<(Term, Term)>,
}

/// What a slot is bound to: its term, whether rule 4 substitutes it as an
/// immutable `let` alias, and the module values read to build it.
#[derive(Clone, Debug)]
struct Binding {
    term: Term,
    alias: bool,
    globals: BTreeSet<usize>,
}

/// A point of an [`Env`] and a [`FactSet`] to go back to.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Mark {
    undo: usize,
    rewrites: usize,
    facts: usize,
}

impl Env {
    /// The slots of the activation `activation`, open or not.
    pub(crate) fn new(activation: usize, open: bool) -> Self {
        Self {
            activation,
            open,
            ..Self::default()
        }
    }

    /// Binds `slot` to `term`; `alias` when the binding is an immutable
    /// `let` alias, which rule 4 substitutes.
    pub(crate) fn bind(&mut self, slot: usize, term: Term, alias: bool) {
        self.bind_reading(slot, term, alias, BTreeSet::new());
    }

    /// [`Self::bind`], the term built by reading the module values
    /// `globals`, which every read of the slot reads too.
    pub(crate) fn bind_reading(
        &mut self,
        slot: usize,
        term: Term,
        alias: bool,
        globals: BTreeSet<usize>,
    ) {
        let old = self.slots.insert(
            slot,
            Binding {
                term,
                alias,
                globals,
            },
        );
        self.undo.push((slot, old));
    }

    /// Rewrites `from` as the constant `to`: a literal equality.
    pub(crate) fn rewrite(&mut self, from: Term, to: Term) {
        self.rewrites.push((from, to));
    }

    /// The point to go back to, with `facts`.
    pub(crate) fn mark(&self, facts: &FactSet) -> Mark {
        Mark {
            undo: self.undo.len(),
            rewrites: self.rewrites.len(),
            facts: facts.facts.len(),
        }
    }

    /// Goes back to `mark`.
    pub(crate) fn restore(&mut self, facts: &mut FactSet, mark: Mark) {
        while self.undo.len() > mark.undo {
            if let Some((slot, old)) = self.undo.pop() {
                match old {
                    Some(old) => {
                        self.slots.insert(slot, old);
                    }
                    None => {
                        self.slots.remove(&slot);
                    }
                }
            }
        }
        self.rewrites.truncate(mark.rewrites);
        facts.facts.truncate(mark.facts);
    }

    fn rewritten(&self, term: Term) -> Option<Term> {
        self.rewrites
            .iter()
            .rev()
            .find(|(from, _)| *from == term)
            .map(|(_, to)| *to)
    }
}

/// The parts a pattern binds over a subject: each sub-pattern and the term
/// it matches.
pub(crate) fn pattern_parts<'p>(
    terms: &mut Terms,
    pattern: &'p Pattern,
    subject: Term,
) -> Vec<(&'p Pattern, Term)> {
    let mut parts = Vec::new();
    let mut pending = vec![(pattern, subject)];
    while let Some((pattern, term)) = pending.pop() {
        parts.push((pattern, term));
        match pattern {
            Pattern::Wrapper(inner) => {
                let repr = terms.intern(Data::Repr(term));
                pending.push((inner, repr));
            }
            Pattern::Record(fields) => {
                for (name, inner) in fields {
                    let field = terms.intern(Data::Field(term, name.clone()));
                    pending.push((inner, field));
                }
            }
            Pattern::Tuple(items) => {
                for (at, inner) in items.iter().enumerate() {
                    let item = terms.intern(Data::Item(term, at));
                    pending.push((inner, item));
                }
            }
            Pattern::Case { case, fields } => {
                for (name, inner) in fields {
                    let field =
                        terms.intern(Data::CaseField(term, case.clone(), name.clone()));
                    pending.push((inner, field));
                }
            }
            Pattern::List(items) => {
                for inner in items {
                    let element = terms.opaque();
                    pending.push((inner, element));
                }
            }
            Pattern::Discard | Pattern::Bind(_) | Pattern::Literal(_) => {}
        }
    }
    parts
}

/// Whether `pattern` matches every value of its subject's shape without a
/// test: only bindings, discards, and wrappers, records, and tuples of them.
fn destructures(pattern: &Pattern) -> bool {
    let mut pending = vec![pattern];
    while let Some(pattern) = pending.pop() {
        match pattern {
            Pattern::Discard | Pattern::Bind(_) => {}
            Pattern::Wrapper(inner) => pending.push(inner),
            Pattern::Record(fields) => pending.extend(fields.iter().map(|(_, p)| p)),
            Pattern::Tuple(items) => pending.extend(items.iter()),
            Pattern::Literal(_) | Pattern::Case { .. } | Pattern::List(_) => {
                return false;
            }
        }
    }
    true
}

enum Build<'n> {
    Visit(&'n Node),
    Make(&'n Node),
    Bind(Option<usize>),
    Pattern(&'n Pattern),
    Mark,
    Restore,
}

/// Builds the term of `node` under `env`, and whether a `let` alias or a
/// literal equality was substituted on the way (rule 4). A slot nothing
/// binds is the activation's own when `env` is open, and opaque otherwise.
pub(crate) fn build(
    terms: &mut Terms,
    vocabulary: &Vocabulary<'_>,
    env: &mut Env,
    node: &Node,
) -> (Term, bool) {
    let (term, substituted, _) = build_reading(terms, vocabulary, env, node);
    (term, substituted)
}

/// [`build`], with the global index of every constant module value whose
/// value the term was built from, directly or through a bound slot.
pub(crate) fn build_reading(
    terms: &mut Terms,
    vocabulary: &Vocabulary<'_>,
    env: &mut Env,
    node: &Node,
) -> (Term, bool, BTreeSet<usize>) {
    let mut substituted = false;
    let mut globals = BTreeSet::new();
    let mut stack: Vec<Term> = Vec::new();
    let mut marks: Vec<usize> = Vec::new();
    let mut pending = vec![Build::Visit(node)];
    while let Some(step) = pending.pop() {
        match step {
            Build::Visit(node) => match &node.expr {
                Expr::Return(inner) | Expr::WidenAtom(inner) => {
                    pending.push(Build::Visit(inner));
                }
                Expr::Sequence(nodes) => match nodes.last() {
                    Some(last) => pending.push(Build::Visit(last)),
                    None => stack.push(terms.opaque()),
                },
                Expr::Let { slot, value, body } => {
                    pending.push(Build::Restore);
                    pending.push(Build::Visit(body));
                    pending.push(Build::Bind(*slot));
                    pending.push(Build::Visit(value));
                    pending.push(Build::Mark);
                }
                Expr::Match { subject, arms, .. } => match arms.as_slice() {
                    [arm] if destructures(&arm.pattern) => {
                        pending.push(Build::Restore);
                        pending.push(Build::Visit(&arm.body));
                        pending.push(Build::Pattern(&arm.pattern));
                        pending.push(Build::Visit(subject));
                        pending.push(Build::Mark);
                    }
                    _ => stack.push(terms.opaque()),
                },
                Expr::Call {
                    target: CallTarget::Direct { .. } | CallTarget::Contract(_),
                    arguments,
                }
                | Expr::TailCall {
                    target: CallTarget::Direct { .. } | CallTarget::Contract(_),
                    arguments,
                } => {
                    pending.push(Build::Make(node));
                    let children = arguments
                        .positional
                        .iter()
                        .chain(arguments.labelled.iter().map(|(_, node)| node));
                    let children = children.collect::<Vec<_>>();
                    pending.extend(children.into_iter().rev().map(Build::Visit));
                }
                // A call of a value is known only when its callee is an
                // anonymous callable with a guard.
                Expr::Call {
                    target: CallTarget::Value(callee),
                    arguments,
                }
                | Expr::TailCall {
                    target: CallTarget::Value(callee),
                    arguments,
                } => {
                    pending.push(Build::Make(node));
                    let children = arguments
                        .positional
                        .iter()
                        .chain(arguments.labelled.iter().map(|(_, node)| node));
                    let children = children.collect::<Vec<_>>();
                    pending.extend(children.into_iter().rev().map(Build::Visit));
                    pending.push(Build::Visit(callee));
                }
                Expr::Closure { .. } => pending.push(Build::Make(node)),
                // The run of a plan whose operand is directly a call is known
                // by that call (`GUARD-7`); any other plan's run is not.
                Expr::Await { plan }
                    if matches!(
                        plan.expr,
                        Expr::Call {
                            target: CallTarget::Direct { .. },
                            ..
                        }
                    ) =>
                {
                    pending.push(Build::Make(node));
                    pending.push(Build::Visit(plan));
                }
                Expr::Intrinsic { arguments, .. } => {
                    pending.push(Build::Make(node));
                    pending.extend(arguments.iter().rev().map(Build::Visit));
                }
                Expr::Construct { construction, .. } => {
                    let children: Vec<&Node> = match construction {
                        Construction::Tuple(nodes) | Construction::List(nodes) => {
                            nodes.iter().collect()
                        }
                        Construction::Record(fields)
                        | Construction::Case { fields, .. } => {
                            fields.iter().map(|(_, node)| node).collect()
                        }
                        Construction::Wrapper(inner) => vec![&**inner],
                        Construction::Dict(_) => {
                            stack.push(terms.opaque());
                            continue;
                        }
                    };
                    pending.push(Build::Make(node));
                    pending.extend(children.into_iter().rev().map(Build::Visit));
                }
                Expr::Field { record, .. } => {
                    pending.push(Build::Make(node));
                    pending.push(Build::Visit(record));
                }
                Expr::Literal(_) | Expr::Global(_) | Expr::Variable { .. } => {
                    pending.push(Build::Make(node));
                }
                _ => stack.push(terms.opaque()),
            },
            Build::Make(node) => {
                let term = make(
                    terms,
                    vocabulary,
                    env,
                    node,
                    &mut stack,
                    (&mut substituted, &mut globals),
                );
                let term = match env.rewritten(term) {
                    Some(constant) => {
                        substituted = true;
                        constant
                    }
                    None => term,
                };
                stack.push(term);
            }
            Build::Bind(slot) => {
                let value = stack.pop().unwrap_or_else(|| terms.opaque());
                if let Some(slot) = slot {
                    env.bind(slot, value, true);
                }
            }
            Build::Pattern(pattern) => {
                let subject = stack.pop().unwrap_or_else(|| terms.opaque());
                for (part, term) in pattern_parts(terms, pattern, subject) {
                    if let Pattern::Bind(slot) = part {
                        env.bind(*slot, term, false);
                    }
                }
            }
            Build::Mark => marks.push(env.undo.len()),
            Build::Restore => {
                let mark = marks.pop().unwrap_or_default();
                while env.undo.len() > mark {
                    if let Some((slot, old)) = env.undo.pop() {
                        match old {
                            Some(old) => {
                                env.slots.insert(slot, old);
                            }
                            None => {
                                env.slots.remove(&slot);
                            }
                        }
                    }
                }
            }
        }
    }
    let term = stack.pop().unwrap_or_else(|| terms.opaque());
    (term, substituted, globals)
}

/// The term of `node`, whose operands' terms are the last ones on `stack`.
fn make(
    terms: &mut Terms,
    vocabulary: &Vocabulary<'_>,
    env: &Env,
    node: &Node,
    stack: &mut Vec<Term>,
    (substituted, globals): (&mut bool, &mut BTreeSet<usize>),
) -> Term {
    let take = |count: usize, stack: &mut Vec<Term>| {
        let start = stack.len().saturating_sub(count);
        stack.split_off(start)
    };
    match &node.expr {
        Expr::Literal(constant) => terms.intern(Data::Constant(constant.clone())),
        Expr::Global(global) => match vocabulary.globals.get(global) {
            Some(constant) => {
                globals.insert(*global);
                terms.intern(Data::Constant(constant.clone()))
            }
            None => terms.opaque(),
        },
        Expr::Variable { slot, value_type } => match env.slots.get(slot) {
            Some(binding) => {
                *substituted |= binding.alias;
                globals.extend(binding.globals.iter().copied());
                binding.term
            }
            None if env.open => {
                let term = terms.intern(Data::Slot(env.activation, *slot));
                terms.slot_types.insert(term, value_type.clone());
                term
            }
            None => terms.opaque(),
        },
        Expr::Call { target, arguments } | Expr::TailCall { target, arguments } => {
            let count = arguments.positional.len() + arguments.labelled.len();
            let mut operands = take(count, stack);
            // A label not written holds the callee's default: a function's
            // or a closure's. A contract call's default depends on the
            // implementation the receiver selects, so it stays unknown here.
            let (callee, defaults) = match target {
                CallTarget::Direct {
                    function,
                    type_arguments,
                } => (
                    Callee::Function(*function, type_arguments.clone()),
                    vocabulary.defaults.get(function),
                ),
                CallTarget::Contract(call) => (Callee::Contract(call.clone()), None),
                CallTarget::Value(_) => {
                    let callee = stack.pop().unwrap_or_else(|| terms.opaque());
                    let Some(Data::Closure(closure)) = terms.get(callee).cloned()
                    else {
                        return terms.opaque();
                    };
                    (
                        Callee::Function(closure, Vec::new()),
                        vocabulary.defaults.get(&closure),
                    )
                }
            };
            for (at, (label, written)) in arguments.labelled.iter().enumerate() {
                if !matches!(written.expr, Expr::Default(_)) {
                    continue;
                }
                let default = defaults
                    .and_then(|defaults| {
                        defaults.iter().find(|(name, _)| name == label)
                    })
                    .and_then(|(_, default)| match &default.expr {
                        Expr::Literal(constant) => Some(constant.clone()),
                        _ => None,
                    });
                if let (Some(default), Some(slot)) =
                    (default, operands.get_mut(arguments.positional.len() + at))
                {
                    *slot = terms.intern(Data::Constant(default));
                }
            }
            terms.intern(Data::Call(callee, in_slot_order(arguments, operands)))
        }
        Expr::Await { .. } => {
            let plan = take(1, stack)
                .into_iter()
                .next()
                .unwrap_or_else(|| terms.opaque());
            let run = terms.opaque();
            terms.intern(Data::Awaited(plan, run))
        }
        Expr::Closure { closure, .. } => {
            let origin = &closure.body.origin;
            let key = (Arc::from(origin.source_id()), origin.span());
            match vocabulary.closures.get(&key) {
                Some(closure) => terms.intern(Data::Closure(*closure)),
                None => terms.opaque(),
            }
        }
        Expr::Intrinsic {
            intrinsic,
            type_arguments,
            arguments,
        } => {
            let operands = take(arguments.len(), stack);
            terms.intern(Data::Call(
                Callee::Intrinsic(format!("{intrinsic:?}{type_arguments:?}")),
                operands,
            ))
        }
        Expr::Construct {
            value_type,
            construction,
        } => {
            let (shape, count) = match construction {
                Construction::Tuple(nodes) | Construction::List(nodes) => {
                    (Vec::new(), nodes.len())
                }
                Construction::Record(fields) => (
                    fields.iter().map(|(name, _)| name.clone()).collect(),
                    fields.len(),
                ),
                Construction::Case { case, fields } => {
                    let mut shape = vec![format!("case:{case}")];
                    shape.extend(fields.iter().map(|(name, _)| name.clone()));
                    (shape, fields.len())
                }
                Construction::Wrapper(_) => (vec!["wrapper".to_owned()], 1),
                Construction::Dict(_) => return terms.opaque(),
            };
            let operands = take(count, stack);
            if let Some(constant) =
                folded(terms, vocabulary, value_type, construction, &operands)
            {
                return terms.intern(Data::Constant(constant));
            }
            terms.intern(Data::Construct(value_type.clone(), shape, operands))
        }
        Expr::Field { field, .. } => {
            let record = take(1, stack)
                .into_iter()
                .next()
                .unwrap_or_else(|| terms.opaque());
            terms.intern(Data::Field(record, field.clone()))
        }
        _ => terms.opaque(),
    }
}

/// The written place of each labelled argument of `arguments`, in the
/// callee's parameter-slot order: the labels sorted by their UTF-8 bytes,
/// as the callee's signature and every activation order them.
pub(crate) fn label_order(arguments: &Arguments) -> Vec<usize> {
    let mut order = (0..arguments.labelled.len()).collect::<Vec<_>>();
    order.sort_by(|left, right| {
        let label = |at: &usize| {
            arguments
                .labelled
                .get(*at)
                .map_or(&[][..], |(label, _)| label.as_bytes())
        };
        label(left).cmp(label(right))
    });
    order
}

/// `written`, the operands of a call in written order (the positional
/// ones, then the labelled ones as written), in the callee's parameter-slot
/// order: one layout serves the matching of predicates, the keys of calls,
/// the reading of a body, and the facts of a callee's `ensures`.
fn in_slot_order(arguments: &Arguments, mut written: Vec<Term>) -> Vec<Term> {
    let positional = arguments.positional.len();
    if written.len() != positional + arguments.labelled.len() {
        return written;
    }
    let labelled = written.split_off(positional);
    written.extend(
        label_order(arguments)
            .into_iter()
            .filter_map(|at| labelled.get(at).copied()),
    );
    written
}

/// The constant a construction of constants is, when its type states no
/// invariant.
fn folded(
    terms: &Terms,
    vocabulary: &Vocabulary<'_>,
    value_type: &Type,
    construction: &Construction,
    operands: &[Term],
) -> Option<Constant> {
    if let Type::Declared(id, _) = value_type
        && vocabulary.invariant_types.contains(id)
    {
        return None;
    }
    let values = operands
        .iter()
        .map(|term| {
            terms
                .constant(*term)
                .map(|constant| constant.value().clone())
        })
        .collect::<Option<Vec<_>>>()?;
    let names = |fields: &[(String, Node)]| {
        fields
            .iter()
            .map(|(name, _)| name.clone())
            .zip(values.iter().cloned())
            .collect::<Vec<_>>()
    };
    let value = match construction {
        Construction::Tuple(_) => Value::Tuple(values.clone()),
        Construction::List(_) => Value::List(values.clone()),
        Construction::Record(fields) => Value::Record(names(fields)),
        Construction::Case { case, fields } => Value::Variant {
            case: case.clone(),
            fields: names(fields),
        },
        Construction::Wrapper(_) => Value::Wrapper(Box::new(values.first()?.clone())),
        Construction::Dict(_) => return None,
    };
    Constant::new(value_type.clone(), value).ok()
}

/// One fact: a `Bool` term known to be `true` or `false`.
#[derive(Clone, Debug)]
struct Fact {
    term: Term,
    holds: bool,
    evidence: Evidence,
}

/// The facts that hold at one program point.
#[derive(Clone, Debug, Default)]
pub(crate) struct FactSet {
    facts: Vec<Fact>,
}

impl FactSet {
    /// Assumes that `term` is `holds`, by `evidence`, with what the Boolean
    /// structure of rule 5 projects from it and the relations `inRange`
    /// stands for.
    pub(crate) fn assume(
        &mut self,
        terms: &mut Terms,
        vocabulary: &Vocabulary<'_>,
        term: Term,
        holds: bool,
        evidence: Evidence,
    ) {
        let mut pending = vec![(term, holds, evidence)];
        while let Some((term, holds, evidence)) = pending.pop() {
            if self
                .facts
                .iter()
                .any(|fact| fact.term == term && fact.holds == holds)
            {
                continue;
            }
            self.facts.push(Fact {
                term,
                holds,
                evidence: evidence.clone(),
            });
            if let Some((connective, operands)) = vocabulary.connective(terms, term) {
                let projected = evidence.clone().with(BOOLEAN);
                match (connective, holds, operands.as_slice()) {
                    (Connective::Not, _, [operand]) => {
                        pending.push((*operand, !holds, projected));
                    }
                    (Connective::And, true, [left, right])
                    | (Connective::Or, false, [left, right]) => {
                        pending.push((*right, holds, projected.clone()));
                        pending.push((*left, holds, projected));
                    }
                    _ => {}
                }
                continue;
            }
            if let Some((None, call, operands)) = vocabulary.comparison(terms, term)
                && holds
                && let [value, low, high] = operands.as_slice()
            {
                let read = evidence.clone().with(INTEGER);
                let below = vocabulary.relation_term(
                    terms,
                    &call,
                    Relation::LessOrEqual,
                    *low,
                    *value,
                );
                let above = vocabulary.relation_term(
                    terms,
                    &call,
                    Relation::LessOrEqual,
                    *value,
                    *high,
                );
                for relation in [above, below].into_iter().flatten() {
                    pending.push((relation, true, read.clone()));
                }
            }
        }
    }

    /// The evidence that `term` is `holds`, when it is a fact.
    pub(crate) fn lookup(&self, term: Term, holds: bool) -> Option<&Evidence> {
        self.facts
            .iter()
            .find(|fact| fact.term == term && fact.holds == holds)
            .map(|fact| &fact.evidence)
    }
}

/// What the proof of one obligation may still spend. Each attempted
/// derivation of an integer fact costs one, whether or not its conclusion
/// is new: one application of a fact to one premise or one pair of
/// premises, and one comparison of two constants.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Budget {
    left: u64,
    exhausted: bool,
}

impl Budget {
    /// A budget of `derivations` attempts.
    pub(crate) const fn new(derivations: u64) -> Self {
        Self {
            left: derivations,
            exhausted: false,
        }
    }

    /// Spends one attempt: `false`, and the budget exhausted, when none is
    /// left.
    const fn spend(&mut self) -> bool {
        if self.left == 0 {
            self.exhausted = true;
            false
        } else {
            self.left -= 1;
            true
        }
    }

    /// Whether an attempt was refused: a verdict the proof did not reach
    /// may have been stopped by the budget.
    pub(crate) const fn exhausted(&self) -> bool {
        self.exhausted
    }
}

/// The relations known between values of one integer type, each with the
/// evidence that first established it.
type Known = HashMap<(Relation, Term, Term), Evidence>;

/// The relations over values of the integer type of `like` that the facts
/// and the constants among `extra` give, closed under `INT-1` to `INT-8`;
/// `None` when `budget` runs out before the closure is complete. No
/// relation is dropped: the closure is complete or there is none.
fn relations(
    terms: &Terms,
    vocabulary: &Vocabulary<'_>,
    facts: &FactSet,
    like: &ContractCall,
    extra: &[Term],
    budget: &mut Budget,
) -> Option<Known> {
    let mut known = Known::new();
    let mut order: Vec<(Relation, Term, Term)> = Vec::new();
    // The places in `order` of the relations whose left operand is a term.
    let mut by_left: HashMap<Term, Vec<usize>> = HashMap::new();
    let add = |known: &mut Known,
               order: &mut Vec<(Relation, Term, Term)>,
               by_left: &mut HashMap<Term, Vec<usize>>,
               key: (Relation, Term, Term),
               evidence: Evidence| {
        if known.contains_key(&key) {
            return false;
        }
        by_left.entry(key.1).or_default().push(order.len());
        known.insert(key, evidence);
        order.push(key);
        true
    };
    let mut constants = BTreeSet::new();
    for fact in &facts.facts {
        let Some((Some(relation), call, operands)) =
            vocabulary.comparison(terms, fact.term)
        else {
            continue;
        };
        if call.receiver != like.receiver {
            continue;
        }
        let [left, right] = operands.as_slice() else {
            continue;
        };
        let (relation, evidence) = if fact.holds {
            (relation, fact.evidence.clone())
        } else if relation == Relation::Equal {
            (Relation::NotEqual, fact.evidence.clone())
        } else {
            (relation.negation(), fact.evidence.clone().with(INTEGER))
        };
        add(
            &mut known,
            &mut order,
            &mut by_left,
            (relation, *left, *right),
            evidence,
        );
        constants.extend([*left, *right]);
    }
    constants.extend(extra.iter().copied());
    // INT-8: two integer constants compare by evaluation.
    let constants = constants
        .into_iter()
        .filter_map(|term| {
            let constant = terms.constant(term)?;
            (constant.value_type() == &like.receiver)
                .then(|| integer(constant.value()))
                .flatten()
                .map(|value| (term, value))
        })
        .collect::<Vec<_>>();
    for (left, a) in &constants {
        for (right, b) in &constants {
            if left == right {
                continue;
            }
            if !budget.spend() {
                return None;
            }
            let evidence = Evidence::of(INTEGER).with(Rule::Int(8));
            let holds = [
                (Relation::Less, a < b),
                (Relation::LessOrEqual, a <= b),
                (Relation::Greater, a > b),
                (Relation::GreaterOrEqual, a >= b),
                (Relation::Equal, a == b),
                (Relation::NotEqual, a != b),
            ];
            for (relation, holds) in holds {
                if holds {
                    add(
                        &mut known,
                        &mut order,
                        &mut by_left,
                        (relation, *left, *right),
                        evidence.clone(),
                    );
                }
            }
        }
    }
    loop {
        // Each round derives from the relations known when it starts, in
        // the order they became known; a conclusion is kept with the
        // evidence of its first derivation.
        let count = order.len();
        let mut found: Vec<((Relation, Term, Term), Evidence)> = Vec::new();
        let mut seen: BTreeSet<(Relation, Term, Term)> = BTreeSet::new();
        let derive = |premises: &[&(Relation, Term, Term)], fact: u8| {
            premises
                .iter()
                .fold(Evidence::default(), |evidence, premise| {
                    evidence.and(known.get(premise).unwrap_or(&Evidence::default()))
                })
                .with(INTEGER)
                .with(Rule::Int(fact))
        };
        let mut keep =
            |key: (Relation, Term, Term),
             evidence: &dyn Fn() -> Evidence,
             found: &mut Vec<((Relation, Term, Term), Evidence)>| {
                if !known.contains_key(&key) && seen.insert(key) {
                    found.push((key, evidence()));
                }
            };
        for first in order.iter().take(count) {
            let (relation, x, y) = *first;
            let single: &[(Relation, Term, Term)] = match relation {
                Relation::Less => &[
                    (Relation::NotEqual, x, y),
                    (Relation::LessOrEqual, x, y),
                    (Relation::Greater, y, x),
                ],
                Relation::Greater => &[
                    (Relation::NotEqual, x, y),
                    (Relation::GreaterOrEqual, x, y),
                    (Relation::Less, y, x),
                ],
                Relation::Equal => &[
                    (Relation::Equal, y, x),
                    (Relation::LessOrEqual, x, y),
                    (Relation::GreaterOrEqual, x, y),
                ],
                _ => &[],
            };
            let fact = match relation {
                Relation::Less => 1,
                Relation::Greater => 2,
                _ => 3,
            };
            for key in single {
                if !budget.spend() {
                    return None;
                }
                keep(*key, &|| derive(&[first], fact), &mut found);
            }
            // `x ≥ y` is `y ≤ x` and `x > y` is `y < x`: the defining
            // relations read in either direction.
            let converse = match relation {
                Relation::Less => Some(Relation::Greater),
                Relation::Greater => Some(Relation::Less),
                Relation::LessOrEqual => Some(Relation::GreaterOrEqual),
                Relation::GreaterOrEqual => Some(Relation::LessOrEqual),
                Relation::Equal | Relation::NotEqual => None,
            };
            if let Some(converse) = converse {
                if !budget.spend() {
                    return None;
                }
                keep(
                    (converse, y, x),
                    &|| known.get(first).cloned().unwrap_or_default().with(INTEGER),
                    &mut found,
                );
            }
            // The second premise of INT-4, INT-6, and INT-7 starts at `y`;
            // that of INT-5 at `x`.
            let mut seconds = by_left
                .get(&y)
                .into_iter()
                .chain(by_left.get(&x))
                .flatten()
                .copied()
                .filter(|at| *at < count)
                .collect::<Vec<_>>();
            seconds.sort_unstable();
            seconds.dedup();
            for at in seconds {
                let Some(second) = order.get(at) else {
                    continue;
                };
                if !budget.spend() {
                    return None;
                }
                let (other, y2, z) = *second;
                if relation == Relation::LessOrEqual
                    && other == Relation::LessOrEqual
                    && y2 == y
                    && z == x
                {
                    // INT-4: x ≤ y and y ≤ x.
                    keep(
                        (Relation::Equal, x, y),
                        &|| derive(&[first, second], 4),
                        &mut found,
                    );
                }
                if relation == Relation::LessOrEqual
                    && other == Relation::NotEqual
                    && y2 == x
                    && z == y
                {
                    keep(
                        (Relation::Less, x, y),
                        &|| derive(&[first, second], 5),
                        &mut found,
                    );
                }
                if y2 != y {
                    continue;
                }
                match (relation, other) {
                    (Relation::Less, Relation::LessOrEqual)
                    | (Relation::LessOrEqual, Relation::Less) => {
                        keep(
                            (Relation::Less, x, z),
                            &|| derive(&[first, second], 6),
                            &mut found,
                        );
                    }
                    (Relation::LessOrEqual, Relation::LessOrEqual) => {
                        keep(
                            (Relation::LessOrEqual, x, z),
                            &|| derive(&[first, second], 7),
                            &mut found,
                        );
                    }
                    _ => {}
                }
            }
        }
        let mut grew = false;
        for (key, evidence) in found {
            grew |= add(&mut known, &mut order, &mut by_left, key, evidence);
        }
        if !grew {
            return Some(known);
        }
    }
}

/// The closures already computed in one proof, by receiver and the
/// constants of the goal: `None` for one the budget stopped.
type Closures = HashMap<(String, Vec<Term>), Option<Known>>;

/// Decides a comparison goal by the integer facts: the evidence that the
/// comparison `term` is `holds`.
fn compare(
    terms: &Terms,
    vocabulary: &Vocabulary<'_>,
    facts: &FactSet,
    term: Term,
    holds: bool,
    (budget, closures): (&mut Budget, &mut Closures),
) -> Option<Evidence> {
    let (relation, call, operands) = vocabulary.comparison(terms, term)?;
    let key = (format!("{:?}", call.receiver), operands.clone());
    if !closures.contains_key(&key) {
        let closed = relations(terms, vocabulary, facts, &call, &operands, budget);
        closures.insert(key.clone(), closed);
    }
    let known = closures.get(&key)?.as_ref()?;
    let found = |relation: Relation, left: Term, right: Term| {
        known.get(&(relation, left, right)).cloned()
    };
    match (relation, operands.as_slice()) {
        (Some(relation), [left, right]) => {
            let wanted = if holds { relation } else { relation.negation() };
            let evidence = found(wanted, *left, *right)?;
            Some(if holds || relation == Relation::Equal {
                evidence
            } else {
                evidence.with(INTEGER)
            })
        }
        (None, [value, low, high]) => {
            if holds {
                let below = found(Relation::LessOrEqual, *low, *value)?;
                let above = found(Relation::LessOrEqual, *value, *high)?;
                Some(below.and(&above).with(INTEGER))
            } else {
                found(Relation::Greater, *low, *value)
                    .or_else(|| found(Relation::Greater, *value, *high))
                    .map(|evidence| evidence.with(INTEGER))
            }
        }
        _ => None,
    }
}

/// What is known of a pattern over a subject.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArmOutcome {
    /// The pattern cannot match the subject.
    Disjoint,
    /// The pattern matches the subject.
    Matches,
    /// Neither is known.
    Possible,
}

/// Whether `pattern` matches `subject` under `facts`, with the evidence of
/// each case fact used added to `evidence`. A part of the subject known to
/// be of another case, or a constant other than a literal, makes the
/// pattern disjoint; a part nothing decides makes it possible.
fn arm_outcome(
    terms: &mut Terms,
    facts: &FactSet,
    pattern: &Pattern,
    subject: Term,
    evidence: &mut Evidence,
) -> ArmOutcome {
    let mut outcome = ArmOutcome::Matches;
    let mut used = Evidence::default();
    let mut pending = vec![(pattern, subject)];
    while let Some((pattern, term)) = pending.pop() {
        if let Some(constant) = terms.constant(term) {
            match value_outcome(pattern, constant.value()) {
                ArmOutcome::Disjoint => return ArmOutcome::Disjoint,
                ArmOutcome::Possible => outcome = ArmOutcome::Possible,
                ArmOutcome::Matches => {}
            }
            continue;
        }
        match pattern {
            Pattern::Discard | Pattern::Bind(_) => {}
            Pattern::Wrapper(inner) => {
                let repr = terms.intern(Data::Repr(term));
                pending.push((inner, repr));
            }
            Pattern::Record(fields) => {
                for (name, inner) in fields {
                    let field = terms.intern(Data::Field(term, name.clone()));
                    pending.push((inner, field));
                }
            }
            Pattern::Tuple(items) => {
                for (at, inner) in items.iter().enumerate() {
                    let item = terms.intern(Data::Item(term, at));
                    pending.push((inner, item));
                }
            }
            Pattern::Case { case, fields } => {
                match known_case(terms, facts, term, case) {
                    Some((true, found)) => {
                        used = used.and(&found);
                        for (name, inner) in fields {
                            let field = terms.intern(Data::CaseField(
                                term,
                                case.clone(),
                                name.clone(),
                            ));
                            pending.push((inner, field));
                        }
                    }
                    Some((false, found)) => {
                        *evidence = std::mem::take(evidence).and(&found);
                        return ArmOutcome::Disjoint;
                    }
                    None => outcome = ArmOutcome::Possible,
                }
            }
            Pattern::Literal(constant) => {
                let disjoint = match constant.value() {
                    Value::Variant { case, .. } => {
                        known_case(terms, facts, term, case).filter(|(is, _)| !is)
                    }
                    _ => None,
                };
                if let Some((_, found)) = disjoint {
                    *evidence = std::mem::take(evidence).and(&found);
                    return ArmOutcome::Disjoint;
                }
                outcome = ArmOutcome::Possible;
            }
            Pattern::List(_) => outcome = ArmOutcome::Possible,
        }
    }
    if outcome == ArmOutcome::Matches {
        *evidence = std::mem::take(evidence).and(&used);
    }
    outcome
}

/// Whether `term` is of the case `case` (`true`) or of another (`false`),
/// when its construction or a case fact tells, with that fact's evidence.
fn known_case(
    terms: &Terms,
    facts: &FactSet,
    term: Term,
    case: &str,
) -> Option<(bool, Evidence)> {
    if let Some(known) = terms.case_of(term) {
        return Some((known == case, Evidence::default()));
    }
    facts
        .facts
        .iter()
        .find_map(|fact| match terms.get(fact.term) {
            Some(Data::IsCase(of, known)) if *of == term => {
                if fact.holds {
                    Some((known == case, fact.evidence.clone()))
                } else if known == case {
                    Some((false, fact.evidence.clone()))
                } else {
                    None
                }
            }
            _ => None,
        })
}

/// Whether `pattern` matches the constant `value`. A shape the pattern
/// does not take apart is not decided.
fn value_outcome(pattern: &Pattern, value: &Value) -> ArmOutcome {
    let mut outcome = ArmOutcome::Matches;
    let mut pending = vec![(pattern, value)];
    while let Some((pattern, value)) = pending.pop() {
        let decided = match (pattern, value) {
            (Pattern::Discard | Pattern::Bind(_), _) => true,
            (Pattern::Literal(constant), _) => {
                if constant.value() != value {
                    return ArmOutcome::Disjoint;
                }
                true
            }
            (Pattern::Wrapper(inner), Value::Wrapper(held)) => {
                pending.push((inner, held));
                true
            }
            (Pattern::Tuple(items), Value::Tuple(values))
            | (Pattern::List(items), Value::List(values)) => {
                if items.len() != values.len() {
                    if matches!(pattern, Pattern::List(_)) {
                        return ArmOutcome::Disjoint;
                    }
                    false
                } else {
                    pending.extend(items.iter().zip(values));
                    true
                }
            }
            (Pattern::Record(fields), Value::Record(values)) => {
                named_parts(fields, values, &mut pending)
            }
            (
                Pattern::Case { case, fields },
                Value::Variant {
                    case: held,
                    fields: values,
                },
            ) => {
                if case != held {
                    return ArmOutcome::Disjoint;
                }
                named_parts(fields, values, &mut pending)
            }
            _ => false,
        };
        if !decided {
            outcome = ArmOutcome::Possible;
        }
    }
    outcome
}

/// Pairs each field pattern of `fields` with the value of that name in
/// `values`; `false` when a name is missing.
fn named_parts<'p>(
    fields: &'p [(String, Pattern)],
    values: &'p [(String, Value)],
    pending: &mut Vec<(&'p Pattern, &'p Value)>,
) -> bool {
    for (name, inner) in fields {
        let Some((_, value)) = values.iter().find(|(held, _)| held == name) else {
            return false;
        };
        pending.push((inner, value));
    }
    true
}

/// Reads a call of a certified function with no guard whose body takes its
/// value apart by a `match` on one parameter: the `Bool` the first arm that
/// runs returns, with its evidence, when every earlier arm is known not to
/// match and that arm is known to match.
fn read_through(
    terms: &mut Terms,
    vocabulary: &Vocabulary<'_>,
    facts: &FactSet,
    term: Term,
) -> Option<(bool, Evidence)> {
    let Some(Data::Call(Callee::Function(function, _), operands)) = terms.get(term)
    else {
        return None;
    };
    let operands = operands.clone();
    let readable = vocabulary.certified.get(function)?;
    let mut body = readable.body;
    loop {
        match &body.expr {
            Expr::Return(inner) => body = inner,
            Expr::Sequence(nodes) if nodes.len() == 1 => body = nodes.first()?,
            _ => break,
        }
    }
    let Expr::Match { subject, arms, .. } = &body.expr else {
        return None;
    };
    let Expr::Variable { slot, .. } = subject.expr else {
        return None;
    };
    let argument = *operands.get(slot)?;
    // The first arm that runs: every earlier arm is known not to match it,
    // and it is known to match. An arm that may match stops the reading.
    let mut evidence = Evidence::default();
    let mut selected = None;
    for arm in arms {
        match arm_outcome(terms, facts, &arm.pattern, argument, &mut evidence) {
            ArmOutcome::Disjoint => {}
            ArmOutcome::Matches => {
                selected = Some(arm);
                break;
            }
            ArmOutcome::Possible => return None,
        }
    }
    let mut value = &selected?.body;
    loop {
        match &value.expr {
            Expr::Return(inner) => value = inner,
            Expr::Sequence(nodes) if nodes.len() == 1 => value = nodes.first()?,
            _ => break,
        }
    }
    let truth = match &value.expr {
        Expr::Literal(constant) => truth(constant.value())?,
        Expr::Global(global) => truth(vocabulary.globals.get(global)?.value())?,
        _ => return None,
    };
    Some((truth, evidence.reading(&readable.identity)))
}

/// The evidence that the `Bool` term `goal` is `true` under `facts`, by the
/// facts themselves, the Boolean structure of rule 5, the integer facts of
/// rule 6, and the case facts of rule 3. Each attempted derivation of an
/// integer fact spends one of `budget`; a goal not established once it is
/// exhausted may have been stopped by it.
pub(crate) fn prove(
    terms: &mut Terms,
    vocabulary: &Vocabulary<'_>,
    facts: &FactSet,
    goal: Term,
    budget: &mut Budget,
) -> Option<Evidence> {
    let mut closures = Closures::new();
    // Each term's evidence of being `true` and of being `false`.
    let mut decided: HashMap<Term, (Option<Evidence>, Option<Evidence>)> =
        HashMap::new();
    let mut pending = vec![(goal, false)];
    while let Some((term, expanded)) = pending.pop() {
        if decided.contains_key(&term) {
            continue;
        }
        let structure = vocabulary.connective(terms, term);
        if let (Some((_, operands)), false) = (&structure, expanded) {
            pending.push((term, true));
            for operand in operands.iter().rev() {
                pending.push((*operand, false));
            }
            continue;
        }
        let direct = (
            facts.lookup(term, true).cloned(),
            facts.lookup(term, false).cloned(),
        );
        let derived = match structure {
            Some((connective, operands)) => {
                let side = |operand: Option<&Term>, holds: bool| {
                    operand.and_then(|operand| {
                        let (yes, no) = decided.get(operand)?;
                        if holds { yes.clone() } else { no.clone() }
                    })
                };
                let both = |left: Option<Evidence>, right: Option<Evidence>| {
                    Some(left?.and(&right?).with(BOOLEAN))
                };
                let either = |left: Option<Evidence>, right: Option<Evidence>| {
                    left.or(right).map(|evidence| evidence.with(BOOLEAN))
                };
                let (a, b) = (operands.first(), operands.get(1));
                match connective {
                    Connective::Not => (
                        side(a, false).map(|evidence| evidence.with(BOOLEAN)),
                        side(a, true).map(|evidence| evidence.with(BOOLEAN)),
                    ),
                    Connective::And => (
                        both(side(a, true), side(b, true)),
                        either(side(a, false), side(b, false)),
                    ),
                    Connective::Or => (
                        either(side(a, true), side(b, true)),
                        both(side(a, false), side(b, false)),
                    ),
                    Connective::Xor => (
                        either(
                            both(side(a, true), side(b, false)),
                            both(side(a, false), side(b, true)),
                        ),
                        either(
                            both(side(a, true), side(b, true)),
                            both(side(a, false), side(b, false)),
                        ),
                    ),
                }
            }
            None => leaf(terms, vocabulary, facts, term, (budget, &mut closures)),
        };
        decided.insert(term, (direct.0.or(derived.0), direct.1.or(derived.1)));
    }
    decided.remove(&goal).and_then(|(yes, _)| yes)
}

/// What decides a term with no Boolean structure: its constant, the integer
/// facts, or a read through a certified function under a case fact.
fn leaf(
    terms: &mut Terms,
    vocabulary: &Vocabulary<'_>,
    facts: &FactSet,
    term: Term,
    (budget, closures): (&mut Budget, &mut Closures),
) -> (Option<Evidence>, Option<Evidence>) {
    if let Some(holds) = terms
        .constant(term)
        .and_then(|constant| truth(constant.value()))
    {
        let evidence = Some(Evidence::of(CONSTANTS));
        return if holds {
            (evidence, None)
        } else {
            (None, evidence)
        };
    }
    let mut yes = compare(terms, vocabulary, facts, term, true, (budget, closures));
    let mut no = compare(terms, vocabulary, facts, term, false, (budget, closures));
    if yes.is_none()
        && no.is_none()
        && let Some((holds, evidence)) = read_through(terms, vocabulary, facts, term)
    {
        if holds {
            yes = Some(evidence);
        } else {
            no = Some(evidence);
        }
    }
    (yes, no)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(evidence: &Evidence) -> Vec<String> {
        evidence.identifiers()
    }

    #[test]
    fn identifiers_put_the_integer_facts_after_guard_6() {
        let evidence = Evidence::of(BRANCH)
            .with(Rule::Int(1))
            .with(INTEGER)
            .with(BOOLEAN)
            .with(Rule::Int(6));
        assert_eq!(
            rules(&evidence),
            vec!["GUARD-3", "GUARD-6", "INT-1", "INT-6", "GUARD-5"]
        );
    }

    #[test]
    fn a_term_is_stored_once_and_a_part_of_a_construction_is_its_operand() {
        let mut terms = Terms::default();
        let x = terms.intern(Data::Slot(0, 0));
        let again = terms.intern(Data::Slot(0, 0));
        assert_eq!(x, again);
        let wrapped = terms.intern(Data::Construct(
            Type::I32,
            vec!["wrapper".to_owned()],
            vec![x],
        ));
        assert_eq!(terms.intern(Data::Repr(wrapped)), x);
        let record = terms.intern(Data::Construct(
            Type::I32,
            vec!["a".to_owned(), "b".to_owned()],
            vec![x, wrapped],
        ));
        assert_eq!(terms.intern(Data::Field(record, "b".to_owned())), wrapped);
        assert_ne!(terms.opaque(), terms.opaque());
    }
}

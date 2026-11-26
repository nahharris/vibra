//! Contracts: guards with and without `else`, and type invariants.
//!
//! `docs/spec/02-type-system.md`, "Type invariants" and "Guards". The bodies
//! pass checks each guard and each `where` operand as an expression
//! ([`super::expr`]) and records, as a [`Site`], every place a bare guard
//! states an obligation. Once every body is checked, [`review`] decides the
//! [certified fragment](fragment) over the checked IR and reports what is
//! outside it, discharges every bare-guard obligation by the rules of "Bare
//! guards" ([`prove`], over the terms and facts of [`facts`]), decides each
//! `let` pattern that a constant construction makes irrefutable, evaluating
//! constants through [`ctfe`], and folds each guard with `else` whose
//! predicate is constant.
//!
//! [`ctfe`]: super::ctfe
mod facts;
pub(crate) mod fragment;
mod obligations;
mod prove;

use std::collections::BTreeMap;
use std::sync::Arc;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::external::CompilerIntrinsic;
use vibra_ir::{
    CheckedModuleSet, ClosureBody, Constant, Expr, Library, Node, Pattern, Value,
};

use super::ctfe::{Evaluator, Outcome};
use super::declare::{Entity, Program};
use super::expr::{Checked, Layout};
use super::obligation::Obligation;
use super::{COMPILE_TIME_REDUCTIONS, PROOF_DERIVATIONS};
use fragment::{Facts, Standing, walk};

/// A checked guard of a callable.
#[derive(Clone, Debug)]
pub(crate) struct CheckedGuard {
    /// The predicate, a `Bool`.
    pub(crate) predicate: Node,
    /// The consequence of a guard with `else`; `None` for a bare guard.
    pub(crate) consequence: Option<Node>,
    /// Where the predicate is written.
    pub(crate) span: ByteSpan,
}

/// The checked guards of one callable.
#[derive(Clone, Debug, Default)]
pub(crate) struct Guards {
    /// The `requires`.
    pub(crate) entry: Option<CheckedGuard>,
    /// The `ensures`, with the slot its result name is bound to.
    pub(crate) exit: Option<(usize, CheckedGuard)>,
}

/// The guards of one anonymous callable, which a checked body holds: the
/// IR keeps only the guards with `else`, and the review reads them all.
#[derive(Clone, Debug)]
pub(crate) struct ClosureContract {
    /// The source and the span of its body, which identify it in the
    /// checked IR.
    pub(crate) body: (Arc<str>, ByteSpan),
    /// The module it is written in.
    pub(crate) module: usize,
    /// Where it is written.
    pub(crate) span: ByteSpan,
    /// Its type.
    pub(crate) signature: vibra_ir::FnType,
    /// Whether applying it builds a plan: it is an anonymous `proc`, whose
    /// `ensures` is never a certified result.
    pub(crate) plan: bool,
    /// The name of each argument slot: the positional parameters, then the
    /// labels in canonical order.
    pub(crate) names: Vec<String>,
    /// The result name.
    pub(crate) result_name: String,
    /// The constant default of each labelled parameter.
    pub(crate) defaults: Vec<(String, Constant)>,
    /// Its guards.
    pub(crate) guards: Guards,
}

/// The predicate a `where` names.
#[derive(Clone, Debug)]
pub(crate) enum Predicate {
    /// An anonymous `fn`, which becomes a function of its own.
    Anonymous(Arc<ClosureBody>),
    /// A module `fn`, by its program index.
    Named(usize),
}

/// A checked `where` of a type.
#[derive(Clone, Debug)]
pub(crate) struct CheckedInvariant {
    /// The predicate.
    pub(crate) predicate: Predicate,
    /// The constant consequence of a `where … else`.
    pub(crate) consequence: Option<Constant>,
    /// Where the predicate is written.
    pub(crate) span: ByteSpan,
    /// The checked operand, a function value the prover walks as a body.
    pub(crate) operand: Node,
    /// The guards of each anonymous callable in the operand.
    pub(crate) closures: Vec<ClosureContract>,
}

/// A place where a bare guard states an obligation, or where a constant
/// construction decides whether a `let` pattern covers its value.
#[derive(Clone, Debug)]
pub(crate) enum Site {
    /// A call, at `span`, of the function at program index `function`,
    /// whose `requires` is bare.
    Requires {
        /// The function called.
        function: usize,
        /// The application.
        span: ByteSpan,
    },
    /// A construction, at `span`, of the type at program index `ty`, whose
    /// `where` is bare; `node` is the construction when its operands are
    /// constant.
    Where {
        /// The type constructed.
        ty: usize,
        /// The application.
        span: ByteSpan,
        /// The constant construction.
        node: Option<Node>,
    },
    /// A `let` without `else` whose pattern `pattern`, written at `span`,
    /// covers the constant construction `value` of the type at program
    /// index `ty`, whose `where` has an `else`, only when the construction
    /// succeeds.
    Pattern {
        /// The type constructed.
        ty: usize,
        /// The pattern.
        span: ByteSpan,
        /// The lowered pattern.
        pattern: Pattern,
        /// The construction.
        value: Node,
    },
    /// A call, at `span`, of the abstract member `member` of the interface
    /// at program index `interface`, whose `requires` is bare, at a receiver
    /// whose implementation member is not known: decided from the contract.
    Contract {
        interface: usize,
        member: String,
        span: ByteSpan,
    },
}

/// The run-time check of a guard with `else` whose predicate is constant:
/// `Some(true)` when it always holds, so that the check and the consequence
/// are removed, `Some(false)` when it never does, so that the consequence
/// is selected.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Fold {
    /// The `requires`.
    pub(crate) entry: Option<bool>,
    /// The `ensures`.
    pub(crate) exit: Option<bool>,
}

/// What the review found.
#[derive(Debug, Default)]
pub(crate) struct Reviewed {
    /// The diagnostics, of local declarations only.
    pub(crate) diagnostics: Vec<Diagnostic>,
    /// The diagnostics of the embedded library's declarations: each is a
    /// defect of the library, which the loader refuses.
    pub(crate) library: Vec<Diagnostic>,
    /// The folded guards of each function, by program index.
    pub(crate) folds: BTreeMap<usize, Fold>,
    /// The obligation of every bare-guard site of a local declaration.
    pub(crate) obligations: Vec<Obligation>,
    /// The name of each limit a local declaration reached, in the order
    /// first reached.
    pub(crate) limits: Vec<&'static str>,
}

/// A declaration whose body holds sites: a function, a module value, a
/// test, or a type whose `where` operand does, by program index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Owner {
    /// A function.
    Function(usize),
    /// A module value.
    Value(usize),
    /// A test.
    Test(usize),
    /// A type, whose `where` operand is checked as a body of its own.
    Type(usize),
}

/// The guards of an abstract contract member, checked once with `Self` and
/// the interface's parameters open: what a call at a receiver whose
/// implementation member is not known is decided from.
#[derive(Clone, Debug)]
pub(crate) struct ContractGuard {
    /// The interface's module, which the guards' spans are in.
    pub(crate) module: usize,
    /// The contract member's identity and its name as messages show it.
    pub(crate) identity: String,
    pub(crate) qualified: String,
    /// The member's name as written.
    pub(crate) name: ByteSpan,
    pub(crate) signature: vibra_ir::FnType,
    /// The name of each argument slot.
    pub(crate) names: Vec<String>,
    pub(crate) result_name: String,
    /// The constant default of each labelled parameter, which a call that
    /// does not write the label passes.
    pub(crate) defaults: Vec<(String, Constant)>,
    pub(crate) guards: Guards,
}

/// What the review looks at: the program, its layout, its checked
/// functions, module values, tests, and invariants by program index, and its
/// sites with the module and the declaration of each.
pub(crate) struct Inputs<'r, 'a> {
    pub(crate) program: &'r Program<'a>,
    pub(crate) layout: &'r Layout,
    pub(crate) functions: &'r BTreeMap<usize, Checked>,
    pub(crate) values: &'r BTreeMap<usize, Checked>,
    pub(crate) tests: &'r BTreeMap<usize, Checked>,
    pub(crate) invariants: &'r BTreeMap<usize, CheckedInvariant>,
    /// The guards of each abstract contract member, by interface and name.
    pub(crate) contracts: &'r BTreeMap<(usize, String), ContractGuard>,
    pub(crate) sites: &'r [(usize, Owner, Site)],
}

/// The state the parts of a review share.
struct Context<'r, 'a> {
    inputs: &'r Inputs<'r, 'a>,
    /// The program index of each function of the layout.
    functions: Vec<usize>,
    /// The entry of the anonymous predicate of each type, by program index.
    anonymous: BTreeMap<usize, usize>,
    standings: Vec<Standing>,
    reviewed: Reviewed,
    /// The spans of the predicates a limit already stopped, by module.
    limited: Vec<(usize, ByteSpan)>,
}

impl Context<'_, '_> {
    fn program(&self) -> &Program<'_> {
        self.inputs.program
    }

    fn local(&self, module: usize) -> bool {
        self.program()
            .modules
            .get(module)
            .is_some_and(|data| !data.embedded)
    }

    fn diagnostic(
        &self,
        code: DiagnosticCode,
        module: usize,
        span: ByteSpan,
        message: impl Into<String>,
    ) -> Diagnostic {
        let diagnostic = Diagnostic::new(code, span, message);
        match self.program().modules.get(module) {
            Some(data) => diagnostic.with_source_id(data.source_id.as_ref()),
            None => diagnostic,
        }
    }

    /// Reports `diagnostic` once: a contract guard is reviewed for each
    /// place it is copied to, and its faults are the contract's. A fault of
    /// the embedded library is kept apart, as a defect of the library.
    fn push(&mut self, module: usize, diagnostic: Diagnostic) {
        let found = if self.local(module) {
            &mut self.reviewed.diagnostics
        } else {
            &mut self.reviewed.library
        };
        if !found.contains(&diagnostic) {
            found.push(diagnostic);
        }
    }

    /// The module and identity of `owner`.
    fn owner(&self, owner: Owner) -> Option<(usize, String)> {
        let program = self.program();
        match owner {
            Owner::Function(index) => program
                .functions
                .get(index)
                .map(|data| (data.module, data.identity.clone())),
            Owner::Value(index) => program
                .values
                .get(index)
                .map(|data| (data.module, data.identity.clone())),
            Owner::Test(index) => {
                let data = program.tests.get(index)?;
                let module = program.modules.get(data.module)?;
                Some((
                    data.module,
                    program.identity_of(
                        &module.path,
                        module.embedded,
                        "",
                        "@test",
                        &data.name,
                    ),
                ))
            }
            Owner::Type(index) => program
                .types
                .get(index)
                .map(|data| (data.module, data.identity.clone())),
        }
    }

    /// The source text at `span` of the module `module`.
    fn source_text(&self, module: usize, span: ByteSpan) -> &str {
        self.program()
            .modules
            .get(module)
            .and_then(|data| data.text)
            .and_then(|text| text.get(span.start()..span.end()))
            .unwrap_or_default()
    }

    /// The entry of the predicate of the type at program index `ty`.
    fn predicate_entry(&self, ty: usize) -> Option<usize> {
        match &self.inputs.invariants.get(&ty)?.predicate {
            Predicate::Anonymous(_) => self.anonymous.get(&ty).copied(),
            Predicate::Named(function) => self
                .inputs
                .layout
                .functions
                .get(*function)
                .copied()
                .flatten(),
        }
    }

    /// Whether the predicate of the type at program index `ty` is
    /// certified.
    fn certified_predicate(&self, ty: usize) -> bool {
        self.predicate_entry(ty)
            .and_then(|entry| self.standings.get(entry))
            .is_some_and(|standing| *standing == Standing::Certified)
    }

    /// Reports `@tool.limitExceeded` at the predicate at `span` of the
    /// module `module` of an obligation, once.
    fn limit(&mut self, module: usize, span: ByteSpan) {
        self.limit_reached(module, span, "the obligation's status is unknown");
    }

    /// Reports `@tool.limitExceeded` at the predicate at `span` of the
    /// module `module` when the proof budget stopped the discharge rules
    /// before a verdict: the obligation's status is unknown.
    fn proof_limit(&mut self, module: usize, span: ByteSpan) {
        let message = format!(
            "deciding this predicate by the integer facts reached the proof limit \
             of the limit profile `{}` ({} attempted derivations); the \
             obligation's status is unknown",
            super::ctfe::PROFILE.name,
            super::ctfe::PROFILE.proof_derivations
        );
        self.limit_message(module, span, message, PROOF_DERIVATIONS);
    }

    /// Reports `@tool.limitExceeded` at the predicate at `span` of the
    /// module `module`, once, with what follows from it, `outcome`.
    fn limit_reached(&mut self, module: usize, span: ByteSpan, outcome: &str) {
        let message = format!(
            "evaluating this predicate over constant operands reached the \
             reduction limit of the limit profile `{}` ({} \
             reductions); {outcome}",
            super::ctfe::PROFILE.name,
            super::ctfe::PROFILE.compile_time.reductions
        );
        self.limit_message(module, span, message, COMPILE_TIME_REDUCTIONS);
    }

    /// Reports `@tool.limitExceeded` with `message` at the predicate at
    /// `span` of the module `module`, once, and names the limit reached,
    /// `limit`, when the declaration is local.
    fn limit_message(
        &mut self,
        module: usize,
        span: ByteSpan,
        message: String,
        limit: &'static str,
    ) {
        if self.limited.contains(&(module, span)) {
            return;
        }
        self.limited.push((module, span));
        if self.local(module) && !self.reviewed.limits.contains(&limit) {
            self.reviewed.limits.push(limit);
        }
        let diagnostic =
            self.diagnostic(DiagnosticCode::ToolLimitExceeded, module, span, message);
        self.push(module, diagnostic);
    }

    /// Reports every node of the guard or predicate bodies `roots`, of the
    /// module `module`, that is outside the fragment: the outermost such
    /// nodes, and each call of a function that is not certified. Whether
    /// `roots` are certified.
    fn certify(&mut self, facts: &Facts, module: usize, roots: &[&Node]) -> bool {
        let found = walk(facts, roots.iter().copied(), Some);
        let mut certified = true;
        let mut faults = Vec::new();
        for node in &found.outside {
            certified = false;
            faults.push(self.diagnostic(
                DiagnosticCode::ContractUnsupportedPredicate,
                module,
                node.origin.span(),
                "this expression is outside the certified fragment: a predicate \
                 or a consequence is built only from parameters, constants, \
                 constructions, field reads, `if` and `match`, the primitive \
                 comparisons and `Bool` operations, calls of the library's \
                 functions bound to total compiler rows, and calls of certified \
                 functions",
            ));
        }
        for (node, entry) in &found.calls {
            match self.standings.get(*entry) {
                Some(Standing::Certified) => {}
                Some(Standing::Faulty) | None => certified = false,
                Some(Standing::Outside) => {
                    certified = false;
                    let name = self.entry_name(*entry);
                    faults.push(self.diagnostic(
                        DiagnosticCode::ContractUnsupportedPredicate,
                        module,
                        node.origin.span(),
                        format!(
                            "this call is outside the certified fragment: {name} is \
                             not certified, because it is recursive, directly or \
                             through other functions, or its body leaves the fragment"
                        ),
                    ));
                }
            }
        }
        for fault in faults {
            self.push(module, fault);
        }
        certified
    }

    /// The name of the entry `entry` in a message.
    fn entry_name(&self, entry: usize) -> String {
        if let Some(function) = self
            .functions
            .get(entry)
            .and_then(|index| self.program().functions.get(*index))
        {
            return format!("`{}`", function.qualified);
        }
        self.anonymous
            .iter()
            .find(|(_, at)| **at == entry)
            .and_then(|(ty, _)| self.program().types.get(*ty))
            .map_or_else(
                || "the function".to_owned(),
                |data| format!("the invariant of `{}`", data.name),
            )
    }
}

/// The facts the fragment is decided against in `inputs`, and the program
/// index of each function of the layout.
fn facts(inputs: &Inputs<'_, '_>, anonymous: &BTreeMap<usize, usize>) -> Facts {
    let program = inputs.program;
    let mut facts = Facts {
        constants: inputs.layout.constants.keys().copied().collect(),
        ..Facts::default()
    };
    for (binding, entity) in &program.library {
        if let (Library::Equatable | Library::Ordered, Entity::Interface(index)) =
            (binding, entity)
            && let Some(id) = program
                .interfaces
                .get(*index)
                .and_then(|data| data.id.clone())
        {
            facts.comparisons.insert(id);
        }
    }
    for (index, data) in program.types.iter().enumerate() {
        let Some(id) = data.id.clone() else { continue };
        if program
            .modules
            .get(data.module)
            .is_some_and(|module| module.embedded)
        {
            facts.library.insert(id.clone());
        }
        let entry = match inputs.invariants.get(&index).map(|data| &data.predicate) {
            Some(Predicate::Anonymous(_)) => anonymous.get(&index).copied(),
            Some(Predicate::Named(function)) => {
                inputs.layout.functions.get(*function).copied().flatten()
            }
            None => None,
        };
        if let Some(entry) = entry {
            facts.invariants.insert(id, entry);
        }
    }
    for data in &program.impls {
        let embedded = program
            .modules
            .get(data.module)
            .is_none_or(|module| module.embedded);
        let Some(interface) = program
            .interfaces
            .get(data.interface)
            .and_then(|interface| interface.id.clone())
        else {
            continue;
        };
        if embedded || !data.usable || !data.binders.is_empty() {
            continue;
        }
        for (member, function) in &data.members {
            if let Some(entry) =
                inputs.layout.functions.get(*function).copied().flatten()
            {
                facts.members.push((
                    interface.clone(),
                    data.arguments.clone(),
                    data.receiver.clone(),
                    member.clone(),
                    entry,
                ));
            }
        }
    }
    facts
}

/// The entry of each anonymous invariant predicate, after the `count`
/// functions of the layout, in the order of the types' program indices: the
/// function index [`super::lower`] gives it in a module set without tests.
pub(crate) fn anonymous_entries(
    invariants: &BTreeMap<usize, CheckedInvariant>,
    count: usize,
) -> BTreeMap<usize, usize> {
    invariants
        .iter()
        .filter(|(_, data)| matches!(data.predicate, Predicate::Anonymous(_)))
        .enumerate()
        .map(|(at, (ty, _))| (*ty, count + at))
        .collect()
}

/// Reviews the contracts of `inputs`. `build` makes the module set the
/// compile-time evaluations run over, once, when one is needed: every
/// function of the layout, then each anonymous invariant predicate, as
/// [`anonymous_entries`] numbers them.
pub(crate) fn review(
    inputs: &Inputs<'_, '_>,
    build: impl FnMut() -> Option<Arc<CheckedModuleSet>>,
) -> Reviewed {
    let program = inputs.program;
    let mut functions = Vec::new();
    for (index, at) in inputs.layout.functions.iter().enumerate() {
        if let Some(at) = at {
            if functions.len() <= *at {
                functions.resize(*at + 1, usize::MAX);
            }
            if let Some(slot) = functions.get_mut(*at) {
                *slot = index;
            }
        }
    }
    let count = functions.len();
    let anonymous = anonymous_entries(inputs.invariants, count);
    let facts = facts(inputs, &anonymous);
    let mut entries: Vec<Option<Vec<&Node>>> = Vec::new();
    let mut external = Vec::new();
    for index in &functions {
        let data = program.functions.get(*index);
        // A function bound to a total compiler row is certified: it has no
        // body, depends on nothing, and its application is an opaque term.
        if data
            .is_some_and(|data| data.external.is_some_and(CompilerIntrinsic::is_total))
        {
            external.push(false);
            entries.push(Some(Vec::new()));
            continue;
        }
        // Any other external function and a `proc`, whose application builds
        // a plan, are not certified functions.
        external.push(data.is_some_and(|data| {
            data.external.is_some() || super::effects::procs::builds_plan(data)
        }));
        let bodies = inputs.functions.get(index).and_then(|checked| {
            let body = checked.node.as_ref()?;
            let mut bodies = vec![body];
            let guards = [
                checked.guards.entry.as_ref(),
                checked.guards.exit.as_ref().map(|(_, guard)| guard),
            ];
            for guard in guards.into_iter().flatten() {
                bodies.push(&guard.predicate);
                bodies.extend(guard.consequence.as_ref());
            }
            Some(bodies)
        });
        entries.push(bodies);
    }
    for ty in anonymous.keys() {
        if let Some(Predicate::Anonymous(closure)) =
            inputs.invariants.get(ty).map(|data| &data.predicate)
        {
            entries.push(Some(closure.nodes()));
            external.push(false);
        }
    }
    let standings = fragment::standings(&facts, &entries, &external, Some);
    let mut context = Context {
        inputs,
        functions,
        anonymous,
        standings,
        reviewed: Reviewed::default(),
        limited: Vec::new(),
    };
    let certified = guards(&mut context, &facts);
    let closures = closure_guards(&mut context, &facts);
    let contracts = contract_guards(&mut context, &facts);
    invariants(&mut context, &facts);
    let standings = std::mem::take(&mut context.standings);
    let mut evaluator = Evaluator::new(&facts, &standings, build);
    context.standings.clone_from(&standings);
    prove::discharge(
        &mut context,
        (&certified, &closures, &contracts),
        &facts.members,
        &mut evaluator,
    );
    obligations::patterns(&mut context, &mut evaluator);
    folds(&mut context, &certified, &mut evaluator);
    context.reviewed
}

/// Which guards are certified, by program index: the `requires`, then the
/// `ensures`.
type Certified = BTreeMap<usize, (bool, bool)>;

/// Reports what is outside the fragment in every guard.
fn guards(context: &mut Context<'_, '_>, facts: &Facts) -> Certified {
    let mut certified = BTreeMap::new();
    let inputs = context.inputs;
    for (index, checked) in inputs.functions {
        let Some(module) = inputs.program.functions.get(*index).map(|data| data.module)
        else {
            continue;
        };
        let mut check = |guard: Option<&CheckedGuard>| match guard {
            Some(guard) => {
                let mut roots = vec![&guard.predicate];
                roots.extend(guard.consequence.as_ref());
                context.certify(facts, module, &roots)
            }
            None => false,
        };
        let entry = check(checked.guards.entry.as_ref());
        let exit = check(checked.guards.exit.as_ref().map(|(_, guard)| guard));
        certified.insert(*index, (entry, exit));
    }
    certified
}

/// Which guards of each anonymous callable are certified, by the source and
/// the span of its body: the `requires`, then the `ensures`.
type ClosureCertified = BTreeMap<(Arc<str>, ByteSpan), (bool, bool)>;

/// The guards of the anonymous callables of every checked body and `where`
/// operand of `inputs`, each with its owner.
fn owners<'r>(
    inputs: &'r Inputs<'_, '_>,
) -> impl Iterator<Item = (Owner, &'r [ClosureContract])> {
    let functions = inputs
        .functions
        .iter()
        .map(|(index, checked)| (Owner::Function(*index), checked.closures.as_slice()));
    let values = inputs
        .values
        .iter()
        .map(|(index, checked)| (Owner::Value(*index), checked.closures.as_slice()));
    let tests = inputs
        .tests
        .iter()
        .map(|(index, checked)| (Owner::Test(*index), checked.closures.as_slice()));
    let types = inputs
        .invariants
        .iter()
        .map(|(index, checked)| (Owner::Type(*index), checked.closures.as_slice()));
    functions.chain(values).chain(tests).chain(types)
}

/// Reports what is outside the fragment in the guards of every anonymous
/// callable.
fn closure_guards(context: &mut Context<'_, '_>, facts: &Facts) -> ClosureCertified {
    let mut certified = BTreeMap::new();
    let inputs = context.inputs;
    for (_, closures) in owners(inputs) {
        for closure in closures {
            let mut check = |guard: Option<&CheckedGuard>| match guard {
                Some(guard) => {
                    let mut roots = vec![&guard.predicate];
                    roots.extend(guard.consequence.as_ref());
                    context.certify(facts, closure.module, &roots)
                }
                None => false,
            };
            let entry = check(closure.guards.entry.as_ref());
            let exit = check(closure.guards.exit.as_ref().map(|(_, guard)| guard));
            certified.insert(closure.body.clone(), (entry, exit));
        }
    }
    certified
}

/// Whether the `requires` of each abstract contract member is certified,
/// by interface and member name.
type ContractCertified = BTreeMap<(usize, String), bool>;

/// Reports what is outside the fragment in the `requires` of every abstract
/// contract member.
fn contract_guards(context: &mut Context<'_, '_>, facts: &Facts) -> ContractCertified {
    let mut certified = BTreeMap::new();
    let inputs = context.inputs;
    for (key, contract) in inputs.contracts {
        let entry = match &contract.guards.entry {
            Some(guard) => {
                let mut roots = vec![&guard.predicate];
                roots.extend(guard.consequence.as_ref());
                context.certify(facts, contract.module, &roots)
            }
            None => false,
        };
        certified.insert(key.clone(), entry);
    }
    certified
}

/// Reports what is outside the fragment in every invariant predicate.
fn invariants(context: &mut Context<'_, '_>, facts: &Facts) {
    let inputs = context.inputs;
    for (ty, invariant) in inputs.invariants {
        let Some(module) = inputs.program.types.get(*ty).map(|data| data.module) else {
            continue;
        };
        match &invariant.predicate {
            Predicate::Anonymous(closure) => {
                context.certify(facts, module, &closure.nodes());
            }
            Predicate::Named(function) => {
                let standing = inputs
                    .layout
                    .functions
                    .get(*function)
                    .copied()
                    .flatten()
                    .and_then(|entry| context.standings.get(entry));
                if standing == Some(&Standing::Outside) {
                    let name = inputs
                        .program
                        .functions
                        .get(*function)
                        .map(|data| data.qualified.clone())
                        .unwrap_or_default();
                    let diagnostic = context.diagnostic(
                        DiagnosticCode::ContractUnsupportedPredicate,
                        module,
                        invariant.span,
                        format!(
                            "`{name}` is not certified, because it is recursive, \
                             directly or through other functions, or its body \
                             leaves the fragment, so it is no predicate"
                        ),
                    );
                    context.push(module, diagnostic);
                }
            }
        }
    }
}

/// Folds every certified guard with `else` whose predicate reads no slot.
/// A predicate whose evaluation reaches the limit is `@tool.limitExceeded`,
/// as an obligation's is, and stays a check when the guard runs.
fn folds(
    context: &mut Context<'_, '_>,
    certified: &Certified,
    evaluator: &mut Evaluator<'_>,
) {
    let inputs = context.inputs;
    for (index, checked) in inputs.functions {
        let (entry_ok, exit_ok) = certified.get(index).copied().unwrap_or_default();
        let mut fold = Fold::default();
        let mut limited = Vec::new();
        let mut decide = |guard: Option<&CheckedGuard>, ok: bool| {
            let guard = guard.filter(|guard| ok && guard.consequence.is_some())?;
            match evaluator.evaluate(&guard.predicate) {
                Outcome::Value(Value::Variant { ref case, .. }) => Some(case == "true"),
                Outcome::LimitReached => {
                    limited.push(guard.predicate.origin.span());
                    None
                }
                _ => None,
            }
        };
        fold.entry = decide(checked.guards.entry.as_ref(), entry_ok);
        fold.exit = decide(
            checked.guards.exit.as_ref().map(|(_, guard)| guard),
            exit_ok,
        );
        if fold != Fold::default() {
            context.reviewed.folds.insert(*index, fold);
        }
        // An implementation member's guards are its contract's, written in
        // the interface's module.
        let module = inputs.program.functions.get(*index).map(|data| {
            data.implements
                .and_then(|interface| inputs.program.interfaces.get(interface))
                .map_or(data.module, |interface| interface.module)
        });
        if let Some(module) = module {
            for span in limited {
                context.limit_reached(
                    module,
                    span,
                    "the guard is checked when it runs",
                );
            }
        }
    }
}

/// `pattern` with every binding replaced by `_`.
fn without_bindings(pattern: &Pattern) -> Pattern {
    let mut pattern = pattern.clone();
    let mut pending = vec![&mut pattern];
    while let Some(pattern) = pending.pop() {
        if matches!(pattern, Pattern::Bind(_)) {
            *pattern = Pattern::Discard;
            continue;
        }
        match pattern {
            Pattern::Tuple(patterns) | Pattern::List(patterns) => {
                pending.extend(patterns.iter_mut());
            }
            Pattern::Record(fields) | Pattern::Case { fields, .. } => {
                pending.extend(fields.iter_mut().map(|(_, pattern)| pattern));
            }
            Pattern::Wrapper(inner) => pending.push(inner),
            Pattern::Discard | Pattern::Bind(_) | Pattern::Literal(_) => {}
        }
    }
    pattern
}

/// Whether `node` is a construction whose every operand is constant.
/// `library` names the library type an identity plays.
pub(crate) fn constant_operands(
    node: &Node,
    constants: &BTreeMap<usize, Value>,
    library: &dyn Fn(&vibra_ir::TypeId) -> Option<vibra_ir::Library>,
) -> bool {
    let Expr::Construct { .. } = &node.expr else {
        return false;
    };
    fragment::operands(node)
        .into_iter()
        .all(|operand| super::callables::fold(operand, constants, library).is_ok())
}

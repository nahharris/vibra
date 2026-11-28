//! The performed row of every checked body, against its ceiling.
//!
//! `docs/spec/03-effects.md`, "Effect ceilings" and "Cold plans and
//! performed rows", and `docs/spec/07-diagnostics-and-conformance.md`,
//! "Contract, effect and authority diagnostics".
//!
//! A body runs in a procedure context when it is a `proc`, a default or
//! `impl` `proc` member, a test, or an anonymous `proc`, whose ceiling is its
//! written row and whose `await` nodes are its own, not those of the body it
//! is written in. Its performed row is the union, over its
//! `await` nodes, of the row of each awaited plan's `Intent` type: building a
//! plan contributes nothing, and a named `proc` contributes its written
//! ceiling, so recursion closes without a call graph. The ceiling of a
//! `proc` or a test is its written row; that of a member is the member's
//! complete row, its owner included.
//!
//! - A performed root outside the ceiling is one `@effect.outsideCeiling`
//!   per body and root, at the earliest `await` that contributes it. Its
//!   related spans are the shortest chain of calls from that `await` to an
//!   operation member of the root, ties in source order, each with the note
//!   `performs <root> via <callee>`; a hop through an interface member whose
//!   receiver is open or an interface value, through a callback (a procedure
//!   value bound to a name, which the note names), or through a plan value,
//!   is marked indirect. The chain is searched breadth first over an explicit
//!   queue, never on the host stack. A chain that reaches no operation
//!   member of the root ends at the first call it cannot follow, and one
//!   that meets only cycles keeps its prefix up to the first repeated call.
//! - A root of a row written on a `proc`, an anonymous `proc`, a test, or the
//!   additive `performs` of a default member that the body does not perform
//!   is the warning `@contract.unusedEffect` at its entry. It is never
//!   reported for an `impl` member, whose contract fixes its row, nor for the
//!   owner of a member, which is not written, nor for a body whose
//!   declaration has an error other than that body's own
//!   `@effect.outsideCeiling`. The faults of an anonymous `proc` join the
//!   declaration that holds it.
//!
//! A body with `@effect.outsideCeiling` is not lowered: the checked IR
//! rejects it (`vibra-ir::effects`).

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode, Level};
use vibra_ir::{
    BodyContext, CallTarget, EntityRef, Expr as IrExpr, IntentType, Node, Row,
};
use vibra_syntax::source::{CallableKind, Row as Written};

use super::procs;
use crate::check::declare::{Entity, Program};
use crate::check::expr::{BodyChecker, Checked, Target};
use crate::check::lower::Pass;

/// One checked `await`: where it is written, the plan it awaits, and the
/// call that plan runs when the operand's node is known.
#[derive(Clone, Debug)]
pub(crate) struct Await {
    /// The `await` expression.
    pub(crate) span: ByteSpan,
    /// The type of the awaited plan, whose row the `await` contributes.
    pub(crate) plan: IntentType,
    /// The call the plan runs.
    pub(crate) hop: Hop,
}

/// One call of a witness chain: the awaited operand and what it calls.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Hop {
    /// The awaited operand.
    pub(crate) span: ByteSpan,
    /// What the plan runs.
    pub(crate) via: Via,
}

/// What an awaited plan runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Via {
    /// A named `proc`, by its program index, applied directly.
    Direct(usize),
    /// A `proc` member of an interface, by the interface's program index and
    /// the member's name. The hop is indirect when the receiver is open or
    /// an interface value, so that the implementation is not known here.
    Member {
        interface: usize,
        member: String,
        indirect: bool,
    },
    /// A plan value: a local, a parameter, a field, or a plan a `fn`
    /// returns. The hop is indirect.
    Value,
    /// A procedure value bound to a name, a callback, applied: the span of
    /// the name, which a note gives. The hop is indirect.
    Callback(ByteSpan),
}

impl Via {
    /// Whether the hop is marked indirect.
    const fn indirect(&self) -> bool {
        match self {
            Self::Direct(_) => false,
            Self::Member { indirect, .. } => *indirect,
            Self::Value | Self::Callback(_) => true,
        }
    }
}

/// The roots one body performs, each with the `await` that first
/// contributes it and the call that `await` runs.
#[derive(Debug, Default)]
pub(crate) struct Performed {
    pub(crate) roots: BTreeMap<EntityRef, Witness>,
}

/// Where a body first performs one root.
#[derive(Clone, Debug)]
pub(crate) struct Witness {
    /// The earliest `await` that contributes the root.
    pub(crate) first_await: ByteSpan,
    /// The call it runs.
    pub(crate) via: Hop,
}

impl Performed {
    /// The roots `awaits` perform, each first contributed by the earliest
    /// `await` in source order.
    pub(crate) fn of(awaits: &[Await]) -> Self {
        let mut performed = Self::default();
        for awaited in in_source_order(awaits) {
            for root in awaited.plan.effects.roots() {
                performed
                    .roots
                    .entry(root.clone())
                    .or_insert_with(|| Witness {
                        first_await: awaited.span,
                        via: awaited.hop.clone(),
                    });
            }
        }
        performed
    }
}

/// `awaits` sorted by where each is written: an `await` around another
/// starts first.
fn in_source_order(awaits: &[Await]) -> Vec<&Await> {
    let mut sorted = awaits.iter().collect::<Vec<_>>();
    sorted.sort_by_key(|awaited| (awaited.span.start(), awaited.span.end()));
    sorted
}

/// The call the awaited `operand`, checked by `checker`, runs: a named
/// `proc` applied directly, a `proc` member of an interface applied to a
/// receiver, or else a plan value. Arguments hoisted to keep their written
/// order bind around the call at the call's own span, and are looked
/// through.
pub(crate) fn hop(checker: &BodyChecker<'_, '_>, operand: &Node) -> Hop {
    let program = checker.program();
    let mut node = operand;
    while let IrExpr::Let { body, .. } = &node.expr {
        if body.origin.span() != node.origin.span() {
            break;
        }
        node = body;
    }
    let via = match &node.expr {
        IrExpr::Call {
            target: CallTarget::Direct { function, .. },
            ..
        } => checker
            .function_at(*function)
            .filter(|index| {
                program
                    .functions
                    .get(*index)
                    .is_some_and(procs::builds_plan)
            })
            .map_or(Via::Value, Via::Direct),
        IrExpr::Call {
            target: CallTarget::Contract(call),
            ..
        } => program
            .interfaces
            .iter()
            .position(|data| data.id.as_ref() == Some(&call.interface))
            .filter(|index| {
                program
                    .interfaces
                    .get(*index)
                    .and_then(|data| data.member(&call.member))
                    .is_some_and(|member| member.row.is_some())
            })
            .map_or(Via::Value, |interface| Via::Member {
                interface,
                member: call.member.clone(),
                indirect: call.receiver.is_interface_value() || call.receiver.is_open(),
            }),
        IrExpr::Call {
            target: CallTarget::Value(callee),
            ..
        } if matches!(callee.expr, IrExpr::Variable { .. }) => {
            Via::Callback(callee.origin.span())
        }
        _ => Via::Value,
    };
    Hop {
        span: operand.origin.span(),
        via,
    }
}

/// One body that runs in a procedure context: of a declaration, or of an
/// anonymous `proc` written in the body of a declaration.
struct Body<'c> {
    /// The declaration whose body it is or holds it.
    target: Target,
    module: usize,
    ceiling: Row,
    /// How a diagnostic names it.
    what: String,
    /// Each entry of the row written on it whose unused roots are reported:
    /// where it is written, the interface it names, and how it is written.
    written: Vec<(ByteSpan, usize, String)>,
    /// Its own `await` nodes.
    awaits: &'c [Await],
    /// Whether it is an anonymous `proc`.
    #[cfg_attr(not(test), expect(dead_code, reason = "read by `observed`"))]
    anonymous: bool,
}

/// Checks the rows of the bodies of `pass`, once per checked program, after
/// every body is typed: each `@effect.outsideCeiling` joins the faults of
/// the declaration whose body holds the `await`, and that body is not
/// lowered. Returns the `@contract.unusedEffect` warnings and the
/// `@type.suspiciousRow` warnings of the written callable types.
pub(crate) fn check_bodies(program: &Program<'_>, pass: &mut Pass) -> Vec<Diagnostic> {
    let mut faults = Vec::new();
    let mut warnings = Vec::new();
    let bodies = bodies(program, pass);
    let mut rows = Vec::with_capacity(bodies.len());
    for (at, body) in bodies.iter().enumerate() {
        let performed = Performed::of(body.awaits);
        #[cfg(test)]
        observed::record(program, body, &performed);
        if let Some(module) = program.modules.get(body.module) {
            for (root, witness) in &performed.roots {
                if body.ceiling.roots().contains(root) {
                    continue;
                }
                let diagnostic = outside(program, pass, body, root, witness)
                    .with_source_id(module.source_id.as_ref());
                faults.push((body.target, at, diagnostic));
            }
        }
        rows.push(performed);
    }
    for (at, (body, performed)) in bodies.iter().zip(&rows).enumerate() {
        let Some(module) = program.modules.get(body.module) else {
            continue;
        };
        // A body's own `@effect.outsideCeiling` leaves its `await` nodes
        // known; any other error of its declaration, one of an anonymous
        // `proc` it holds included, means it has an error.
        let faulty = pass.faults.get(&body.target).is_some_and(|found| {
            found
                .iter()
                .any(|diagnostic| diagnostic.level() == Level::Error)
        }) || faults
            .iter()
            .any(|(target, from, _)| *target == body.target && *from != at);
        if module.embedded || faulty {
            continue;
        }
        for (span, index, dotted) in &body.written {
            let Some(data) = program.interfaces.get(*index) else {
                continue;
            };
            let Some(id) = data.id.as_ref() else {
                continue;
            };
            if performed.roots.contains_key(id.entity()) {
                continue;
            }
            warnings.push(
                Diagnostic::new(
                    DiagnosticCode::ContractUnusedEffect,
                    *span,
                    format!(
                        "{} names `{dotted}` in its row and performs none of it: no \
                         `await` in its body runs a plan of `{}`",
                        body.what, data.name
                    ),
                )
                .with_source_id(module.source_id.as_ref()),
            );
        }
    }
    for (target, _, diagnostic) in faults {
        match target {
            Target::Function(index) => {
                if let Some(checked) = pass.functions.get_mut(&index) {
                    checked.node = None;
                }
            }
            Target::Test(index) => {
                if let Some(checked) = pass.tests.get_mut(&index) {
                    checked.node = None;
                }
            }
            Target::Value(_) => {}
        }
        pass.faults.entry(target).or_default().push(diagnostic);
    }
    warnings.extend(super::values::warnings(program));
    warnings
}

/// Every checked body of `pass` that runs in a procedure context, with its
/// ceiling, and every anonymous `proc` of a checked body.
fn bodies<'c>(program: &'c Program<'_>, pass: &'c Pass) -> Vec<Body<'c>> {
    let mut found = Vec::new();
    for (index, checked) in &pass.functions {
        let Some(data) = program.functions.get(*index) else {
            continue;
        };
        let target = Target::Function(*index);
        anonymous(&mut found, target, data.module, checked);
        let BodyContext::Procedure(ceiling) = procs::lower(program, data) else {
            continue;
        };
        let written = (data.view.kind() == CallableKind::Proc
            && data.implements.is_none())
        .then(|| data.view.performs())
        .flatten();
        found.push(Body {
            target,
            module: data.module,
            ceiling,
            what: procs::describe(data),
            written: entries(program, data.module, written),
            awaits: &checked.awaits.own,
            anonymous: false,
        });
    }
    for (index, checked) in &pass.values {
        if let Some(data) = program.values.get(*index) {
            anonymous(&mut found, Target::Value(*index), data.module, checked);
        }
    }
    for (index, checked) in &pass.tests {
        let Some(test) = program.tests.get(*index) else {
            continue;
        };
        let target = Target::Test(*index);
        anonymous(&mut found, target, test.module, checked);
        let BodyContext::Procedure(ceiling) = procs::test_context(program, test) else {
            continue;
        };
        found.push(Body {
            target,
            module: test.module,
            ceiling,
            what: format!("the test \"{}\"", test.name),
            written: entries(program, test.module, test.view.performs()),
            awaits: &checked.awaits.own,
            anonymous: false,
        });
    }
    found
}

/// Adds each anonymous `proc` of `checked`, the body of `target` written in
/// `module`, to `found`. Its ceiling is its written row, every root of
/// which it names.
fn anonymous<'c>(
    found: &mut Vec<Body<'c>>,
    target: Target,
    module: usize,
    checked: &'c Checked,
) {
    for open in &checked.awaits.anonymous {
        found.push(Body {
            target,
            module,
            ceiling: open.ceiling.clone(),
            what: "this anonymous `proc`".to_owned(),
            written: open.written.clone(),
            awaits: &open.awaits,
            anonymous: true,
        });
    }
}

/// Each entry of the row `written` of `module` that names an interface:
/// where it is written, the interface, and how it is written.
pub(crate) fn entries(
    program: &Program<'_>,
    module: usize,
    written: Option<Written<'_>>,
) -> Vec<(ByteSpan, usize, String)> {
    written
        .iter()
        .flat_map(Written::paths)
        .filter_map(|path| match program.path_entity(module, &path) {
            Some(Entity::Interface(index)) => {
                Some((path.node().span(), index, path.dotted()))
            }
            _ => None,
        })
        .collect()
}

/// The `@effect.outsideCeiling` of `body`, which performs `root` first at
/// `witness`, with its chain of calls as related spans.
fn outside(
    program: &Program<'_>,
    pass: &Pass,
    body: &Body<'_>,
    root: &EntityRef,
    witness: &Witness,
) -> Diagnostic {
    let name = root_name(program, root);
    let mut diagnostic = Diagnostic::new(
        DiagnosticCode::EffectOutsideCeiling,
        witness.first_await,
        format!(
            "this `await` performs `{name}`, which the ceiling {} of {} does not name",
            body.ceiling, body.what
        ),
    );
    for (module, hop) in chain(program, pass, body.module, &witness.via, root) {
        let mut note = format!("performs {name} via {}", callee(program, module, &hop));
        if hop.via.indirect() {
            note.push_str(" (indirect)");
        }
        diagnostic = program.related(diagnostic, module, hop.span, &note);
    }
    diagnostic
}

/// One call of the search: the module it is written in, the hop, and the
/// call before it.
struct Step {
    module: usize,
    hop: Hop,
    parent: Option<usize>,
}

/// The shortest chain of calls from `first`, written in `module`, to an
/// operation member of `root`, ties in source order, each with the module it
/// is written in. A chain that reaches none is the shortest that ends at a
/// call it cannot follow: a plan value, a member of another interface, or a
/// `proc` whose body performs `root` nowhere. A chain that reaches neither
/// only meets cycles, and ends at the first call of a `proc` it already
/// followed, so the chain is never empty.
pub(crate) fn chain(
    program: &Program<'_>,
    pass: &Pass,
    module: usize,
    first: &Hop,
    root: &EntityRef,
) -> Vec<(usize, Hop)> {
    let mut steps = vec![Step {
        module,
        hop: first.clone(),
        parent: None,
    }];
    let mut queue = VecDeque::from([0]);
    let mut seen = BTreeSet::new();
    let mut dead_end = None;
    let mut repeated = None;
    let mut reached = None;
    while let Some(at) = queue.pop_front() {
        let Some(step) = steps.get(at) else { break };
        match &step.hop.via {
            Via::Member { interface, .. }
                if root_of(program, *interface) == Some(root) =>
            {
                reached = Some(at);
                break;
            }
            Via::Direct(function) => {
                if !seen.insert(*function) {
                    repeated.get_or_insert(at);
                    continue;
                }
                let next = calls(program, pass, *function, root);
                if next.is_empty() {
                    dead_end.get_or_insert(at);
                }
                for (module, hop) in next {
                    steps.push(Step {
                        module,
                        hop,
                        parent: Some(at),
                    });
                    queue.push_back(steps.len() - 1);
                }
            }
            Via::Member { .. } | Via::Value | Via::Callback(_) => {
                dead_end.get_or_insert(at);
            }
        }
    }
    let mut path = Vec::new();
    let mut at = reached.or(dead_end).or(repeated);
    while let Some(index) = at {
        let Some(step) = steps.get(index) else { break };
        path.push((step.module, step.hop.clone()));
        at = step.parent;
    }
    path.reverse();
    path
}

/// The calls of the body of `function` whose plans perform `root`, in source
/// order, each with the module the body is written in.
fn calls(
    program: &Program<'_>,
    pass: &Pass,
    function: usize,
    root: &EntityRef,
) -> Vec<(usize, Hop)> {
    let (Some(data), Some(checked)) = (
        program.functions.get(function),
        pass.functions.get(&function),
    ) else {
        return Vec::new();
    };
    in_source_order(&checked.awaits.own)
        .into_iter()
        .filter(|awaited| awaited.plan.effects.roots().contains(root))
        .map(|awaited| awaited.hop.clone())
        .map(|hop| (data.module, hop))
        .collect()
}

/// The root of the interface `interface`.
fn root_of<'p>(program: &'p Program<'_>, interface: usize) -> Option<&'p EntityRef> {
    program
        .interfaces
        .get(interface)
        .and_then(|data| data.id.as_ref())
        .map(vibra_ir::TypeId::entity)
}

/// How a note names `root`: its interface's declared name.
fn root_name(program: &Program<'_>, root: &EntityRef) -> String {
    program
        .interfaces
        .iter()
        .find(|data| data.id.as_ref().map(vibra_ir::TypeId::entity) == Some(root))
        .map_or_else(|| root.name().to_owned(), |data| data.name.clone())
}

/// How a note names the callee of `hop`, written in `module`: a `proc` by
/// its name, after its type or interface for a member; a member by its
/// interface and name; a plan value by its source text.
fn callee(program: &Program<'_>, module: usize, hop: &Hop) -> String {
    match &hop.via {
        Via::Direct(function) => {
            program
                .functions
                .get(*function)
                .map_or_else(String::new, |data| {
                    let owner = data
                        .owner
                        .and_then(|owner| program.types.get(owner))
                        .map(|owner| owner.name.clone())
                        .or_else(|| {
                            data.contract
                                .and_then(|contract| program.interfaces.get(contract))
                                .map(|contract| contract.name.clone())
                        });
                    match owner {
                        Some(owner) => format!("{owner}.{}", data.name),
                        None => data.name.clone(),
                    }
                })
        }
        Via::Member {
            interface, member, ..
        } => program
            .interfaces
            .get(*interface)
            .map_or_else(|| member.clone(), |data| format!("{}.{member}", data.name)),
        Via::Value => text(program, module, hop.span),
        Via::Callback(name) => text(program, module, *name),
    }
}

/// The source text of `module` at `span`, its whitespace collapsed.
fn text(program: &Program<'_>, module: usize, span: ByteSpan) -> String {
    program
        .modules
        .get(module)
        .and_then(|data| data.text)
        .and_then(|text| text.get(span.start()..span.end()))
        .map_or_else(String::new, |text| {
            text.split_whitespace().collect::<Vec<_>>().join(" ")
        })
}

/// The performed rows the checker computed, kept for the host test that
/// compares them with the rows the checked IR recomputes (architecture,
/// risk 1).
#[cfg(test)]
pub(crate) mod observed {
    use std::cell::RefCell;
    use std::collections::BTreeSet;

    use vibra_ir::EntityRef;

    use super::{Body, Performed};
    use crate::check::declare::Program;
    use crate::check::expr::Target;

    /// The performed row of one body.
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub(crate) struct Observed {
        /// The qualified name of the declaration whose body it is or holds
        /// it, as the checked IR records it.
        pub(crate) declaration: String,
        /// Whether it is an anonymous `proc` of that declaration.
        pub(crate) anonymous: bool,
        /// The roots it performs.
        pub(crate) roots: BTreeSet<EntityRef>,
    }

    thread_local! {
        static ROWS: RefCell<Vec<Observed>> = const { RefCell::new(Vec::new()) };
    }

    /// Keeps the row `performed` of `body`.
    pub(super) fn record(
        program: &Program<'_>,
        body: &Body<'_>,
        performed: &Performed,
    ) {
        let declaration = match body.target {
            Target::Function(index) => program
                .functions
                .get(index)
                .map(|data| data.qualified.clone()),
            Target::Test(index) => {
                program.tests.get(index).map(|data| data.qualified.clone())
            }
            Target::Value(index) => {
                program.values.get(index).map(|data| data.qualified.clone())
            }
        };
        let observed = Observed {
            declaration: declaration.unwrap_or_default(),
            anonymous: body.anonymous,
            roots: performed.roots.keys().cloned().collect(),
        };
        ROWS.with(|rows| rows.borrow_mut().push(observed));
    }

    /// Every row kept on this thread since the last call.
    pub(crate) fn take() -> Vec<Observed> {
        ROWS.with(|rows| std::mem::take(&mut *rows.borrow_mut()))
    }
}

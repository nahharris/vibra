//! The performed row of every checked body, against its ceiling.
//!
//! `docs/spec/03-effects.md`, "Effect ceilings" and "Cold plans and
//! performed rows", and `docs/spec/07-diagnostics-and-conformance.md`,
//! "Contract, effect and authority diagnostics".
//!
//! A body runs in a procedure context when it is a `proc`, a default or
//! `impl` `proc` member, or a test. Its performed row is the union, over its
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
//!   receiver is open or an interface value, or through a plan value, is
//!   marked indirect. The chain is searched breadth first over an explicit
//!   queue, never on the host stack. A chain that reaches no operation
//!   member of the root ends at the first call it cannot follow.
//! - A root of a row written on a `proc`, a test, or the additive
//!   `performs` of a default member that the body does not perform is the
//!   warning `@contract.unusedEffect` at its entry. It is never reported for
//!   an `impl` member, whose contract fixes its row, nor for the owner of a
//!   member, which is not written (Q5), nor for a body that has an error,
//!   whose `await` nodes may not all be known.
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
    pub(crate) hop: Option<Hop>,
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
}

impl Via {
    /// Whether the hop is marked indirect.
    const fn indirect(&self) -> bool {
        match self {
            Self::Direct(_) => false,
            Self::Member { indirect, .. } => *indirect,
            Self::Value => true,
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
    pub(crate) via: Option<Hop>,
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
        _ => Via::Value,
    };
    Hop {
        span: operand.origin.span(),
        via,
    }
}

/// One body that runs in a procedure context.
struct Body<'c> {
    target: Target,
    module: usize,
    ceiling: Row,
    /// How a diagnostic names it.
    what: String,
    /// The row written on it whose unused roots are reported, if any.
    written: Option<Written<'c>>,
    checked: &'c Checked,
}

/// Checks the rows of the bodies of `pass`, once per checked program, after
/// every body is typed: each `@effect.outsideCeiling` joins the faults of
/// the declaration whose body holds the `await`, and that body is not
/// lowered. Returns the `@contract.unusedEffect` warnings.
pub(crate) fn check_bodies(program: &Program<'_>, pass: &mut Pass) -> Vec<Diagnostic> {
    let mut faults = Vec::new();
    let mut warnings = Vec::new();
    for body in bodies(program, pass) {
        let Some(module) = program.modules.get(body.module) else {
            continue;
        };
        let performed = Performed::of(&body.checked.awaits);
        for (root, witness) in &performed.roots {
            if body.ceiling.roots().contains(root) {
                continue;
            }
            let diagnostic = outside(program, pass, &body, root, witness)
                .with_source_id(module.source_id.as_ref());
            faults.push((body.target, diagnostic));
        }
        let faulty = pass.faults.get(&body.target).is_some_and(|found| {
            found
                .iter()
                .any(|diagnostic| diagnostic.level() == Level::Error)
        });
        if module.embedded || faulty {
            continue;
        }
        let Some(written) = body.written else {
            continue;
        };
        for path in written.paths() {
            let Some(Entity::Interface(index)) =
                program.path_entity(body.module, &path)
            else {
                continue;
            };
            let Some(data) = program.interfaces.get(index) else {
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
                    path.node().span(),
                    format!(
                        "{} names `{}` in its row and performs none of it: no `await` \
                         in its body runs a plan of `{}`",
                        body.what,
                        path.dotted(),
                        data.name
                    ),
                )
                .with_source_id(module.source_id.as_ref()),
            );
        }
    }
    for (target, diagnostic) in faults {
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
    warnings
}

/// Every checked body of `pass` that runs in a procedure context, with its
/// ceiling.
fn bodies<'c>(program: &'c Program<'_>, pass: &'c Pass) -> Vec<Body<'c>> {
    let mut found = Vec::new();
    for (index, checked) in &pass.functions {
        let Some(data) = program.functions.get(*index) else {
            continue;
        };
        let BodyContext::Procedure(ceiling) = procs::lower(program, data) else {
            continue;
        };
        let written = (data.view.kind() == CallableKind::Proc
            && data.implements.is_none())
        .then(|| data.view.performs())
        .flatten();
        found.push(Body {
            target: Target::Function(*index),
            module: data.module,
            ceiling,
            what: procs::describe(data),
            written,
            checked,
        });
    }
    for (index, checked) in &pass.tests {
        let Some(test) = program.tests.get(*index) else {
            continue;
        };
        let BodyContext::Procedure(ceiling) = procs::test_context(program, test) else {
            continue;
        };
        found.push(Body {
            target: Target::Test(*index),
            module: test.module,
            ceiling,
            what: format!("the test \"{}\"", test.name),
            written: test.view.performs(),
            checked,
        });
    }
    found
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
    let Some(first) = &witness.via else {
        return diagnostic;
    };
    for (module, hop) in chain(program, pass, body.module, first, root) {
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
/// `proc` whose body performs `root` nowhere.
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
            Via::Member { .. } | Via::Value => {
                dead_end.get_or_insert(at);
            }
        }
    }
    let mut path = Vec::new();
    let mut at = reached.or(dead_end);
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
    in_source_order(&checked.awaits)
        .into_iter()
        .filter(|awaited| awaited.plan.effects.roots().contains(root))
        .filter_map(|awaited| awaited.hop.clone())
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
        Via::Value => program
            .modules
            .get(module)
            .and_then(|data| data.text)
            .and_then(|text| text.get(hop.span.start()..hop.span.end()))
            .map_or_else(String::new, |text| {
                text.split_whitespace().collect::<Vec<_>>().join(" ")
            }),
    }
}

//! The instantiation graph (`docs/spec/02-type-system.md`, "Generics").
//!
//! A generic function may reach itself only at type arguments that are its
//! own type parameters or closed types. One that reaches itself at a type
//! built around one of its parameters, as `f(T)` calling `f(List(T))` does,
//! has no finite set of instantiations, so a backend could neither compile
//! each one nor bound the type descriptors a run builds.
//!
//! The graph has one node for each type parameter of each function. An
//! instantiation of a function `g` inside a function `f` gives an edge from
//! each parameter of `f` that a type argument mentions to the parameter of
//! `g` the argument is for. The edge is *expanding* when the argument is
//! not that parameter alone. The module set is rejected when an expanding
//! edge lies on a cycle.
//!
//! The instantiations of a body are its direct calls, its function values,
//! its contract calls whose receiver selects an implementation, and the
//! members of every implementation that the evidence for a bound selects.
//! Every walk keeps a work list, so no host stack grows with what a checker
//! built.

use crate::conformance::{Evidence, select_implementation};
use crate::expr::{CallTarget, ContractCall, Expr, Node, Tables};
use crate::external::CompilerIntrinsic;
use crate::{Binder, FunctionBody, Implementation, IrError, Type, TypeId};

/// The most goals the evidence of one instantiation may visit: the limit
/// of the conformance search, which accepted the same goals.
const GOAL_LIMIT: usize = 4096;

/// One instantiation: the function it is in, the function it names, and one
/// type for each leading binder of that function.
struct Instantiation {
    caller: usize,
    callee: usize,
    arguments: Vec<Type>,
}

/// An edge between two type parameters, numbered across the module set.
struct Flow {
    from: usize,
    to: usize,
    expanding: bool,
    instantiation: usize,
}

/// A receiver, an interface, and the interface's positional arguments.
type Goal = (Type, TypeId, Vec<Type>);

struct Collector<'a> {
    tables: Tables<'a>,
    caller: usize,
    found: Vec<Instantiation>,
}

impl<'a> Collector<'a> {
    fn push(&mut self, callee: usize, arguments: Vec<Type>) {
        self.found.push(Instantiation {
            caller: self.caller,
            callee,
            arguments,
        });
    }

    /// An instantiation of the function `callee`, and the evidence each
    /// bound of its binders needs at `arguments`.
    fn instantiate(&mut self, callee: usize, arguments: &[Type]) {
        if let Some(function) = self.tables.functions.get(callee) {
            self.bounds(function.signature.binders(), arguments);
        }
        self.push(callee, arguments.to_vec());
    }

    fn bounds(&mut self, binders: &[Binder], arguments: &[Type]) {
        let goals = binders
            .iter()
            .zip(arguments)
            .filter_map(|(binder, actual)| {
                let contract = &binder.bound()?.contract;
                Some((
                    actual.clone(),
                    contract.interface.clone(),
                    contract
                        .arguments
                        .iter()
                        .map(|argument| argument.instantiate(arguments))
                        .collect(),
                ))
            })
            .collect();
        self.evidence(goals);
    }

    /// The implementation whose key matches the goal, with the type the
    /// match gives each of its binders. A receiver that is a type parameter
    /// has its evidence from the scope, which selects nothing here.
    fn select(
        &self,
        receiver: &Type,
        interface: &TypeId,
        arguments: &[Type],
    ) -> Option<(&'a Implementation, Vec<Type>)> {
        if matches!(
            receiver,
            Type::Binder(_) | Type::SelfType | Type::Associated(_)
        ) {
            return None;
        }
        select_implementation(
            self.tables.implementations,
            receiver,
            interface,
            arguments,
            |_, _, _| None,
        )
        .ok()
        .flatten()
    }

    /// Every member of each implementation the goals select is instantiated
    /// at the types the selection gives the implementation's binders, and
    /// the bounds of those binders are further goals.
    fn evidence(&mut self, mut goals: Vec<Goal>) {
        let mut visited = 0_usize;
        while let Some((receiver, interface, arguments)) = goals.pop() {
            visited = visited.saturating_add(1);
            if visited > GOAL_LIMIT {
                return;
            }
            let Some((implementation, binders)) =
                self.select(&receiver, &interface, &arguments)
            else {
                continue;
            };
            for (_, member) in implementation.members() {
                self.push(*member, binders.clone());
            }
            for (binder, actual) in implementation.binders().iter().zip(&binders) {
                if let Some(bound) = binder.bound() {
                    goals.push((
                        actual.clone(),
                        bound.contract.interface.clone(),
                        bound
                            .contract
                            .arguments
                            .iter()
                            .map(|argument| argument.instantiate(&binders))
                            .collect(),
                    ));
                }
            }
        }
    }

    /// A contract call instantiates the member it selects: the function of
    /// the matching implementation at the implementation's binders, or the
    /// interface's default at the interface's arguments, each followed by
    /// the member's own type arguments.
    fn contract(&mut self, call: &ContractCall) {
        let selected = self.select(&call.receiver, &call.interface, &call.arguments);
        let supplied = selected.as_ref().and_then(|(implementation, binders)| {
            Some((implementation.member(&call.member)?, binders.clone()))
        });
        let default = || {
            let member = self
                .tables
                .interfaces
                .get(&call.interface)?
                .member(&call.member)?
                .default?;
            Some((member, call.arguments.clone()))
        };
        if let Some((member, mut arguments)) = supplied.or_else(default) {
            arguments.extend(call.member_arguments.iter().cloned());
            self.instantiate(member, &arguments);
        }
        self.evidence(vec![(
            call.receiver.clone(),
            call.interface.clone(),
            call.arguments.clone(),
        )]);
    }

    /// Every instantiation in the expression under `root`, its closures'
    /// bodies included.
    fn body(&mut self, root: &Node) {
        let mut pending = vec![&root.expr];
        while let Some(expr) = pending.pop() {
            match expr {
                Expr::Function {
                    function,
                    type_arguments,
                } => self.instantiate(*function, type_arguments),
                Expr::Call { target, .. } | Expr::TailCall { target, .. } => {
                    match target {
                        CallTarget::Direct {
                            function,
                            type_arguments,
                        } => self.instantiate(*function, type_arguments),
                        CallTarget::Contract(call) => self.contract(call),
                        CallTarget::Value(_) => {}
                    }
                }
                _ => {}
            }
            pending.extend(expr.operands().into_iter().map(|node| &node.expr));
        }
    }
}

/// The strongly connected component of each of `count` nodes, as a number
/// two nodes share exactly when each reaches the other.
fn components(count: usize, flows: &[Flow]) -> Vec<usize> {
    let mut forward = vec![Vec::new(); count];
    let mut backward = vec![Vec::new(); count];
    for flow in flows {
        if let (Some(out), Some(back)) =
            (forward.get_mut(flow.from), backward.get_mut(flow.to))
        {
            out.push(flow.to);
            back.push(flow.from);
        }
    }
    // The order in which a depth-first walk leaves each node.
    let mut seen = vec![false; count];
    let mut left = Vec::with_capacity(count);
    for start in 0..count {
        if mark_seen(&mut seen, start) {
            continue;
        }
        let mut stack = vec![(start, 0_usize)];
        while let Some((node, next)) = stack.last_mut() {
            let node = *node;
            let successor = forward.get(node).and_then(|out| out.get(*next)).copied();
            *next = next.saturating_add(1);
            match successor {
                Some(to) => {
                    if !mark_seen(&mut seen, to) {
                        stack.push((to, 0));
                    }
                }
                None => {
                    left.push(node);
                    stack.pop();
                }
            }
        }
    }
    // Walking backward from the nodes left last gives one component a walk.
    let mut component = vec![usize::MAX; count];
    for (number, start) in left.into_iter().rev().enumerate() {
        if component.get(start).copied() != Some(usize::MAX) {
            continue;
        }
        let mut stack = vec![start];
        while let Some(node) = stack.pop() {
            match component.get_mut(node) {
                Some(place) if *place == usize::MAX => *place = number,
                _ => continue,
            }
            stack.extend(backward.get(node).into_iter().flatten().copied());
        }
    }
    component
}

/// Marks `index` seen and says whether it already was. An index past the
/// end counts as seen.
fn mark_seen(seen: &mut [bool], index: usize) -> bool {
    seen.get_mut(index)
        .is_none_or(|place| std::mem::replace(place, true))
}

/// Checks that no generic function of `tables` reaches itself at type
/// arguments built around its own type parameters.
///
/// # Errors
///
/// [`IrError::ExpandingInstantiation`] for the first instantiation, in
/// function order, with a type argument that is built around a type
/// parameter and lies on a cycle of the instantiation graph.
pub(crate) fn check_instantiations(tables: &Tables<'_>) -> Result<(), IrError> {
    let functions = tables.functions;
    let mut offsets = Vec::with_capacity(functions.len());
    let mut count = 0_usize;
    for function in functions {
        offsets.push(count);
        count = count.saturating_add(function.signature.binders().len());
    }
    let mut collector = Collector {
        tables: *tables,
        caller: 0,
        found: Vec::new(),
    };
    for (index, function) in functions.iter().enumerate() {
        // A function without type parameters carries none to its callees.
        if function.signature.binders().is_empty() {
            continue;
        }
        collector.caller = index;
        if let FunctionBody::Expr(body) = &function.body {
            collector.body(body);
        }
        let guards = function
            .entry_guard
            .iter()
            .chain(function.exit_guard.iter().map(|exit| &exit.guard));
        for guard in guards {
            collector.body(&guard.predicate);
            collector.body(&guard.consequence);
        }
    }
    let binders_of = |function: usize| {
        functions
            .get(function)
            .map_or(0, |function| function.signature.binders().len())
    };
    let mut flows = Vec::new();
    for (instantiation, found) in collector.found.iter().enumerate() {
        let (Some(from), Some(to)) =
            (offsets.get(found.caller), offsets.get(found.callee))
        else {
            continue;
        };
        let parameters = binders_of(found.caller);
        let arguments = found.arguments.iter().take(binders_of(found.callee));
        for (position, argument) in arguments.enumerate() {
            argument.visit(&mut |inner| {
                if let Type::Binder(parameter) = inner
                    && *parameter < parameters
                {
                    flows.push(Flow {
                        from: from.saturating_add(*parameter),
                        to: to.saturating_add(position),
                        expanding: argument != inner,
                        instantiation,
                    });
                }
            });
        }
    }
    let component = components(count, &flows);
    let closes = |flow: &&Flow| {
        flow.expanding && component.get(flow.from) == component.get(flow.to)
    };
    let Some(found) = flows
        .iter()
        .find(closes)
        .and_then(|flow| collector.found.get(flow.instantiation))
    else {
        return Ok(());
    };
    let name = |function: usize| {
        functions
            .get(function)
            .map(|function| function.name.clone())
            .unwrap_or_default()
    };
    Err(IrError::ExpandingInstantiation {
        function: found.caller,
        name: name(found.caller),
        callee: name(found.callee),
    })
}

/// Checks that every instantiation of a function whose body is the row
/// `dict.fromList` keys its dict with a type that implements `Ordered`, as a
/// dict literal's key must: a key that mentions a type parameter of the
/// caller implements it under the bounds of the caller's parameters. The
/// walk covers every function, guard, and global, and the members the
/// evidence for each bound selects.
///
/// # Errors
///
/// [`IrError::InvalidType`] for the first such instantiation whose key
/// keys no dict.
pub(crate) fn check_built_dicts(
    tables: &Tables<'_>,
    globals: &[&Node],
) -> Result<(), IrError> {
    let mut collector = Collector {
        tables: *tables,
        caller: 0,
        found: Vec::new(),
    };
    for (index, function) in tables.functions.iter().enumerate() {
        collector.caller = index;
        if let FunctionBody::Expr(body) = &function.body {
            collector.body(body);
        }
        let guards = function
            .entry_guard
            .iter()
            .chain(function.exit_guard.iter().map(|exit| &exit.guard));
        for guard in guards {
            collector.body(&guard.predicate);
            collector.body(&guard.consequence);
        }
    }
    for initializer in globals {
        collector.body(initializer);
    }
    for found in &collector.found {
        let Some(FunctionBody::Intrinsic {
            intrinsic: CompilerIntrinsic::DictFromList,
            type_arguments,
        }) = tables
            .functions
            .get(found.callee)
            .map(|callee| &callee.body)
        else {
            continue;
        };
        let Some(key) = type_arguments.first() else {
            continue;
        };
        let key = key.instantiate(&found.arguments);
        // A key that stays open is read under the bounds of the caller's
        // type parameters.
        let evidence = Evidence {
            binders: tables
                .functions
                .get(found.caller)
                .map_or(&[], |caller| caller.signature.binders()),
            self_interface: None,
        };
        if !tables.is_dict_key(evidence, &key)? {
            return Err(IrError::InvalidType(format!(
                "{key} does not implement `Ordered` and keys no dict built from a list"
            )));
        }
    }
    Ok(())
}

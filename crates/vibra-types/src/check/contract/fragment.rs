//! The certified fragment.
//!
//! `docs/spec/02-type-system.md`, "Predicates": a guard's predicate and its
//! `else` consequence are built only from parameters and constants,
//! constructions, field reads, `if` and `match` with full coverage, the
//! primitive comparison and `Bool` operations, and calls of certified
//! functions. A certified function is a `fn` whose body is certified and
//! that is not recursive, directly or through other functions.
//!
//! The fragment is decided over the checked IR, whose every node keeps the
//! span it was written at:
//!
//! - a parameter is a slot read, and a constant is a literal, the read of a
//!   module value whose initializer is a constant expression, a labelled
//!   default, or an atom singleton widened to `Atom`;
//! - the body of a `fn` is a sequence of statements that leave through
//!   `return`, and a `match` lowered by the checker covers its subject, as
//!   does an `if` with an `else`;
//! - a construction of a type with an invariant runs that invariant's
//!   predicate, which is then one more function the construction calls;
//! - the primitive comparison is a call of a member of the interfaces
//!   playing `@equatable` and `@ordered` at a closed receiver built only from
//!   compiler scalars and types of the embedded standard library, whose
//!   implementations are the library's own; a call of a member whose
//!   selected implementation is a program's own `impl`, at a closed
//!   receiver, is an ordinary call of that member; the `Bool` operations are
//!   the members of `Bool`, ordinary functions whose bodies are certified.
//!
//! Every other node is outside the fragment: a `let`, a lookup, a `try`, a
//! closure or a function as a value, a call through a value, an intrinsic,
//! and a widening to an interface value.
//!
//! Each walk is a work list, so no host stack grows with the depth of the
//! program.

use std::collections::{BTreeMap, BTreeSet};

use vibra_ir::{CallTarget, Construction, Expr, Node, Type, TypeId};

/// What the fragment says of one node, before its operands are looked at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Admission {
    /// The node is in the fragment when its operands are.
    Admitted,
    /// The node is a call of the function at this checked index, in the
    /// fragment when that function is certified and its operands are.
    Calls(usize),
    /// The node is a construction of the type at this identity, in the
    /// fragment when that type's invariant predicate is certified and its
    /// operands are.
    Constructs(usize),
    /// The node is outside the fragment.
    Outside,
}

/// The facts the fragment is decided against.
#[derive(Debug, Default)]
pub(crate) struct Facts {
    /// The global index of each module value whose initializer is a
    /// constant expression.
    pub(crate) constants: BTreeSet<usize>,
    /// The identities of the interfaces playing `@equatable` and
    /// `@ordered`.
    pub(crate) comparisons: BTreeSet<TypeId>,
    /// The identities of the declared types of the embedded standard
    /// library.
    pub(crate) library: BTreeSet<TypeId>,
    /// The entry, among the entries of a [`Standings`] table, of the
    /// invariant predicate of each declared type that has an invariant.
    pub(crate) invariants: BTreeMap<TypeId, usize>,
    /// The checked index of each member a program's own `impl` supplies at
    /// a closed receiver, by interface, interface arguments, receiver, and
    /// member name.
    pub(crate) members: Vec<(TypeId, Vec<Type>, Type, String, usize)>,
}

impl Facts {
    /// What the fragment says of `node` itself.
    pub(crate) fn admits(&self, node: &Node) -> Admission {
        match &node.expr {
            Expr::Literal(_)
            | Expr::Variable { .. }
            | Expr::Sequence(_)
            | Expr::Match { .. }
            | Expr::Field { .. }
            | Expr::Return(_)
            | Expr::WidenAtom(_)
            | Expr::Default(_) => Admission::Admitted,
            Expr::If { else_branch, .. } if else_branch.is_some() => {
                Admission::Admitted
            }
            Expr::Global(global) if self.constants.contains(global) => {
                Admission::Admitted
            }
            Expr::Construct { value_type, .. } => match value_type {
                Type::Declared(id, _) => match self.invariants.get(id) {
                    Some(entry) => Admission::Constructs(*entry),
                    None => Admission::Admitted,
                },
                _ => Admission::Admitted,
            },
            Expr::Call { target, .. } | Expr::TailCall { target, .. } => match target {
                CallTarget::Direct { function, .. } => Admission::Calls(*function),
                CallTarget::Contract(call)
                    if self.comparisons.contains(&call.interface)
                        && call.member_arguments.is_empty()
                        && call.arguments.iter().all(|ty| self.primitive(ty))
                        && self.primitive(&call.receiver) =>
                {
                    Admission::Admitted
                }
                CallTarget::Contract(call) if call.member_arguments.is_empty() => self
                    .members
                    .iter()
                    .find(|(interface, arguments, receiver, member, _)| {
                        *interface == call.interface
                            && *arguments == call.arguments
                            && *receiver == call.receiver
                            && *member == call.member
                    })
                    .map_or(Admission::Outside, |(.., entry)| Admission::Calls(*entry)),
                CallTarget::Contract(_) | CallTarget::Value(_) => Admission::Outside,
            },
            Expr::If { .. }
            | Expr::Global(_)
            | Expr::Intrinsic { .. }
            | Expr::Function { .. }
            | Expr::Closure { .. }
            | Expr::Captured { .. }
            | Expr::Let { .. }
            | Expr::LetElse { .. }
            | Expr::Lookup { .. }
            | Expr::Try(_)
            | Expr::WidenToInterface { .. }
            | Expr::Await { .. } => Admission::Outside,
        }
    }

    /// Whether `ty` is closed and built only from compiler scalars and types
    /// of the embedded standard library.
    fn primitive(&self, ty: &Type) -> bool {
        let mut pending = vec![ty];
        while let Some(ty) = pending.pop() {
            match ty {
                Type::I8
                | Type::I16
                | Type::I32
                | Type::I64
                | Type::U8
                | Type::U16
                | Type::U32
                | Type::U64
                | Type::F32
                | Type::F64
                | Type::Char
                | Type::Atom
                | Type::Void
                | Type::AtomSingleton(_) => {}
                Type::List(element) => pending.push(element),
                Type::Tuple(components) => pending.extend(components.iter()),
                Type::Record(fields) => {
                    pending.extend(fields.entries().iter().map(|(_, ty)| ty));
                }
                Type::Declared(id, arguments) if self.library.contains(id) => {
                    pending.extend(arguments.iter());
                }
                _ => return false,
            }
        }
        true
    }
}

/// The operands of `node` the fragment looks at: every child but the body
/// of a closure, which is another activation.
pub(crate) fn operands(node: &Node) -> Vec<&Node> {
    let mut children = Vec::new();
    match &node.expr {
        Expr::Literal(_)
        | Expr::Variable { .. }
        | Expr::Global(_)
        | Expr::Function { .. }
        | Expr::Captured { .. }
        | Expr::Default(_) => {}
        Expr::Intrinsic { arguments, .. } => children.extend(arguments.iter()),
        Expr::Sequence(nodes) => children.extend(nodes.iter()),
        Expr::Closure { captures, .. } => children.extend(captures.iter()),
        Expr::Let { value, body, .. } => {
            children.push(&**value);
            children.push(&**body);
        }
        Expr::LetElse {
            value,
            fallback,
            body,
            ..
        } => {
            children.push(&**value);
            children.push(&**fallback);
            children.push(&**body);
        }
        Expr::Match { subject, arms, .. } => {
            children.push(&**subject);
            children.extend(arms.iter().map(|arm| &arm.body));
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            children.push(&**condition);
            children.push(&**then_branch);
            if let Some(branch) = else_branch {
                children.push(&**branch);
            }
        }
        Expr::Construct { construction, .. } => match construction {
            Construction::Tuple(nodes) | Construction::List(nodes) => {
                children.extend(nodes.iter());
            }
            Construction::Record(fields) | Construction::Case { fields, .. } => {
                children.extend(fields.iter().map(|(_, node)| node));
            }
            Construction::Wrapper(node) => children.push(&**node),
            Construction::Dict(entries) => {
                for (key, value) in entries {
                    children.push(key);
                    children.push(value);
                }
            }
        },
        Expr::Field { record, .. } => children.push(&**record),
        Expr::Lookup { collection, key } => {
            children.push(&**collection);
            children.push(&**key);
        }
        Expr::Call { target, arguments } | Expr::TailCall { target, arguments } => {
            if let CallTarget::Value(callee) = target {
                children.push(&**callee);
            }
            children.extend(arguments.positional.iter());
            children.extend(arguments.labelled.iter().map(|(_, node)| node));
        }
        Expr::Return(value)
        | Expr::Try(value)
        | Expr::WidenAtom(value)
        | Expr::WidenToInterface { value, .. }
        | Expr::Await { plan: value } => children.push(&**value),
    }
    children
}

/// Whether `node` reads a slot or a capture of its activation.
pub(crate) fn reads_slots(node: &Node) -> bool {
    let mut pending = vec![node];
    while let Some(node) = pending.pop() {
        if matches!(node.expr, Expr::Variable { .. } | Expr::Captured { .. }) {
            return true;
        }
        pending.extend(operands(node));
    }
    false
}

/// Whether `node` reads one of the slots `slots`.
pub(crate) fn reads_any(node: &Node, slots: &std::ops::Range<usize>) -> bool {
    let mut pending = vec![node];
    while let Some(node) = pending.pop() {
        if let Expr::Variable { slot, .. } = node.expr
            && slots.contains(&slot)
        {
            return true;
        }
        pending.extend(operands(node));
    }
    false
}

/// What one walk of the fragment found.
#[derive(Debug, Default)]
pub(crate) struct Walk<'n> {
    /// The outermost nodes outside the fragment, in walk order.
    pub(crate) outside: Vec<&'n Node>,
    /// Each call of a function, and each construction of a type with an
    /// invariant, with the [`Standings`] entry it depends on.
    pub(crate) calls: Vec<(&'n Node, usize)>,
}

/// Walks the expressions `roots`: every node outside the fragment, with
/// nothing below it, and every call the fragment depends on, with its
/// operands walked too. A call of a function at checked index `function`
/// depends on the entry `entry(function)`.
pub(crate) fn walk<'n>(
    facts: &Facts,
    roots: impl IntoIterator<Item = &'n Node>,
    entry: impl Fn(usize) -> Option<usize>,
) -> Walk<'n> {
    let mut found = Walk::default();
    let mut pending = roots.into_iter().collect::<Vec<_>>();
    pending.reverse();
    while let Some(node) = pending.pop() {
        let admission = facts.admits(node);
        match admission {
            Admission::Outside => {
                found.outside.push(node);
                continue;
            }
            Admission::Calls(function) => match entry(function) {
                Some(at) => found.calls.push((node, at)),
                None => {
                    found.outside.push(node);
                    continue;
                }
            },
            Admission::Constructs(at) => found.calls.push((node, at)),
            Admission::Admitted => {}
        }
        let mut children = operands(node);
        children.reverse();
        pending.extend(children);
    }
    found
}

/// Where an entry of a [`Standings`] table stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Standing {
    /// Certified: in the fragment and not recursive.
    Certified,
    /// Outside the fragment, or recursive.
    Outside,
    /// Not checked, or with a fault of its own: a call of it reports
    /// nothing more.
    Faulty,
}

/// The bodies whose standing is decided together: every function and every
/// invariant predicate. An entry with no bodies is faulty; an external
/// function is outside; any other entry is certified when its bodies are in
/// the fragment and every entry they depend on is certified, which no entry
/// on a cycle of dependencies is.
pub(crate) fn standings(
    facts: &Facts,
    entries: &[Option<Vec<&Node>>],
    external: &[bool],
    entry: impl Fn(usize) -> Option<usize>,
) -> Vec<Standing> {
    let mut standing = Vec::with_capacity(entries.len());
    let mut depends = Vec::with_capacity(entries.len());
    for (at, bodies) in entries.iter().enumerate() {
        match bodies {
            _ if external.get(at).copied().unwrap_or(false) => {
                standing.push(Some(Standing::Outside));
                depends.push(Vec::new());
            }
            None => {
                standing.push(Some(Standing::Faulty));
                depends.push(Vec::new());
            }
            Some(bodies) => {
                let found = walk(facts, bodies.iter().copied(), &entry);
                if found.outside.is_empty() {
                    standing.push(None);
                    depends.push(found.calls.iter().map(|(_, at)| *at).collect());
                } else {
                    standing.push(Some(Standing::Outside));
                    depends.push(Vec::new());
                }
            }
        }
    }
    // A pending entry is certified once every entry it depends on is, and
    // faulty or outside once one of them is; what stays pending is on, or
    // depends on, a cycle.
    loop {
        let mut changed = false;
        for at in 0..standing.len() {
            if standing.get(at).copied().flatten().is_some() {
                continue;
            }
            let Some(needs) = depends.get(at) else {
                continue;
            };
            let mut decided = Some(Standing::Certified);
            for need in needs {
                match standing.get(*need).copied().flatten() {
                    Some(Standing::Certified) => {}
                    Some(Standing::Faulty) => {
                        decided = Some(Standing::Faulty);
                        break;
                    }
                    Some(Standing::Outside) => {
                        if decided == Some(Standing::Certified) {
                            decided = Some(Standing::Outside);
                        }
                    }
                    None => {
                        if decided == Some(Standing::Certified) {
                            decided = None;
                        }
                    }
                }
            }
            if let Some(decided) = decided
                && let Some(slot) = standing.get_mut(at)
            {
                *slot = Some(decided);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    standing
        .into_iter()
        .map(|standing| standing.unwrap_or(Standing::Outside))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use vibra_diagnostics::ByteSpan;
    use vibra_ir::external::compiler::CompilerIntrinsic;
    use vibra_ir::{
        Arguments, ClosureBody, Constant, ContractCall, FnType, MatchArm, NamedTypes,
        Pattern, SourceOrigin, Value,
    };

    use super::*;

    fn node(expr: Expr) -> Node {
        Node::new(expr, SourceOrigin::new("main.vib", ByteSpan::new(0, 1)))
    }

    /// A leaf the fragment admits: a labelled default.
    fn leaf() -> Node {
        node(Expr::Default(Type::I32))
    }

    fn id(name: &str) -> Result<TypeId, String> {
        let seed: u64 = name.bytes().map(u64::from).sum();
        TypeId::new(format!("std.{name}"), format!("sha256:{seed:064x}"))
            .map_err(|error| error.to_string())
    }

    fn facts() -> Result<Facts, String> {
        let mut facts = Facts::default();
        facts.constants.insert(0);
        facts.comparisons.insert(id("Ordered")?);
        facts.library.insert(id("Bool")?);
        facts.invariants.insert(id("Positive")?, 7);
        Ok(facts)
    }

    fn contract(interface: &str, receiver: Type) -> Result<Node, String> {
        Ok(node(Expr::Call {
            target: CallTarget::Contract(Box::new(ContractCall {
                interface: id(interface)?,
                member: "greater".to_owned(),
                receiver,
                arguments: Vec::new(),
                choices: NamedTypes::empty(),
                member_arguments: Vec::new(),
            })),
            arguments: Arguments::default(),
        }))
    }

    fn call(function: usize) -> Node {
        node(Expr::Call {
            target: CallTarget::Direct {
                function,
                type_arguments: Vec::new(),
            },
            arguments: Arguments::default(),
        })
    }

    /// Every node kind of the checked IR, with what the fragment says of it.
    fn kinds() -> Result<Vec<(&'static str, Node, Admission)>, String> {
        let constant = Constant::new(Type::I32, Value::I32(1))
            .map_err(|error| error.to_string())?;
        let closure = Arc::new(ClosureBody {
            value_type: Type::function(FnType::new(Vec::new(), Type::I32)),
            slot_count: 0,
            captures: Vec::new(),
            defaults: Vec::new(),
            body: leaf(),
            entry_guard: None,
            exit_guard: None,
        });
        let admitted = vec![
            ("literal", node(Expr::Literal(constant))),
            (
                "variable",
                node(Expr::Variable {
                    slot: 0,
                    value_type: Type::I32,
                }),
            ),
            ("sequence", node(Expr::Sequence(vec![leaf()]))),
            (
                "match",
                node(Expr::Match {
                    subject: Box::new(leaf()),
                    arms: vec![MatchArm {
                        pattern: Pattern::Discard,
                        body: leaf(),
                    }],
                    value_type: Type::I32,
                }),
            ),
            (
                "field",
                node(Expr::Field {
                    record: Box::new(leaf()),
                    field: "x".to_owned(),
                }),
            ),
            ("return", node(Expr::Return(Box::new(leaf())))),
            ("widenAtom", node(Expr::WidenAtom(Box::new(leaf())))),
            ("default", leaf()),
            (
                "ifElse",
                node(Expr::If {
                    condition: Box::new(leaf()),
                    then_branch: Box::new(leaf()),
                    else_branch: Some(Box::new(leaf())),
                }),
            ),
            ("constantGlobal", node(Expr::Global(0))),
            (
                "construct",
                node(Expr::Construct {
                    value_type: Type::Tuple(vec![Type::I32]),
                    construction: Construction::Tuple(vec![leaf()]),
                }),
            ),
            ("comparison", contract("Ordered", Type::I32)?),
            (
                "comparisonLibraryReceiver",
                contract("Ordered", Type::Declared(id("Bool")?, Vec::new()))?,
            ),
        ];
        let outside = vec![
            (
                "comparisonUserReceiver",
                contract("Ordered", Type::Declared(id("Mine")?, Vec::new()))?,
            ),
            (
                "comparisonOpenReceiver",
                contract("Ordered", Type::Binder(0))?,
            ),
            ("otherContract", contract("Iter", Type::I32)?),
            (
                "ifWithoutElse",
                node(Expr::If {
                    condition: Box::new(leaf()),
                    then_branch: Box::new(leaf()),
                    else_branch: None,
                }),
            ),
            ("otherGlobal", node(Expr::Global(1))),
            (
                "intrinsic",
                node(Expr::Intrinsic {
                    intrinsic: CompilerIntrinsic::CharToU32,
                    type_arguments: Vec::new(),
                    arguments: vec![leaf()],
                }),
            ),
            (
                "function",
                node(Expr::Function {
                    function: 0,
                    type_arguments: Vec::new(),
                }),
            ),
            (
                "closure",
                node(Expr::Closure {
                    closure,
                    captures: Vec::new(),
                }),
            ),
            (
                "captured",
                node(Expr::Captured {
                    slot: 0,
                    value_type: Type::I32,
                }),
            ),
            (
                "let",
                node(Expr::Let {
                    slot: Some(0),
                    value: Box::new(leaf()),
                    body: Box::new(leaf()),
                }),
            ),
            (
                "letElse",
                node(Expr::LetElse {
                    pattern: Pattern::Discard,
                    value: Box::new(leaf()),
                    fallback: Box::new(leaf()),
                    body: Box::new(leaf()),
                }),
            ),
            (
                "lookup",
                node(Expr::Lookup {
                    collection: Box::new(leaf()),
                    key: Box::new(leaf()),
                }),
            ),
            (
                "callValue",
                node(Expr::Call {
                    target: CallTarget::Value(Box::new(leaf())),
                    arguments: Arguments::default(),
                }),
            ),
            ("try", node(Expr::Try(Box::new(leaf())))),
            (
                "widenToInterface",
                node(Expr::WidenToInterface {
                    value: Box::new(leaf()),
                    value_type: Type::Any,
                }),
            ),
        ];
        let mut kinds = admitted
            .into_iter()
            .map(|(name, node)| (name, node, Admission::Admitted))
            .chain(
                outside
                    .into_iter()
                    .map(|(name, node)| (name, node, Admission::Outside)),
            )
            .collect::<Vec<_>>();
        kinds.push((
            "constructInvariant",
            node(Expr::Construct {
                value_type: Type::Declared(id("Positive")?, Vec::new()),
                construction: Construction::Wrapper(Box::new(leaf())),
            }),
            Admission::Constructs(7),
        ));
        let direct = CallTarget::Direct {
            function: 3,
            type_arguments: Vec::new(),
        };
        kinds.push(("callDirect", call(3), Admission::Calls(3)));
        kinds.push((
            "tailCallDirect",
            node(Expr::TailCall {
                target: direct,
                arguments: Arguments::default(),
            }),
            Admission::Calls(3),
        ));
        Ok(kinds)
    }

    #[test]
    fn the_fragment_admits_its_node_kinds_and_rejects_every_other() -> Result<(), String>
    {
        let facts = facts()?;
        for (name, node, expected) in kinds()? {
            assert_eq!(facts.admits(&node), expected, "{name}");
        }
        Ok(())
    }

    #[test]
    fn a_walk_reports_the_outermost_node_outside_only() -> Result<(), String> {
        let facts = facts()?;
        let lookup = node(Expr::Lookup {
            collection: Box::new(node(Expr::Try(Box::new(leaf())))),
            key: Box::new(leaf()),
        });
        let root = node(Expr::Sequence(vec![leaf(), lookup]));
        let found = walk(&facts, [&root], Some);
        assert_eq!(found.outside.len(), 1);
        assert!(matches!(
            found.outside.first().map(|node| &node.expr),
            Some(Expr::Lookup { .. })
        ));
        Ok(())
    }

    #[test]
    fn recursion_and_its_callers_are_outside_and_faults_spread() -> Result<(), String> {
        let facts = facts()?;
        // 0 is a leaf; 1 calls 0; 2 and 3 call each other; 4 calls 2;
        // 5 calls the faulty 6; 7 is external; 8 calls 7; 9 calls itself.
        let bodies = [
            leaf(),
            call(0),
            call(3),
            call(2),
            call(2),
            call(6),
            leaf(),
            leaf(),
            call(7),
            call(9),
        ];
        let entries = bodies
            .iter()
            .enumerate()
            .map(|(at, body)| (at != 6).then(|| vec![body]))
            .collect::<Vec<_>>();
        let external = (0..bodies.len()).map(|at| at == 7).collect::<Vec<_>>();
        let found = standings(&facts, &entries, &external, Some);
        assert_eq!(
            found,
            vec![
                Standing::Certified,
                Standing::Certified,
                Standing::Outside,
                Standing::Outside,
                Standing::Outside,
                Standing::Faulty,
                Standing::Faulty,
                Standing::Outside,
                Standing::Outside,
                Standing::Outside,
            ]
        );
        Ok(())
    }
}

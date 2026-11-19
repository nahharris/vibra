//! Checked expressions and the shape check every one of them passes before a
//! module set exists.
//!
//! An expression is a tree of [`Node`]s, each an [`Expr`] and the source
//! origin a trap or a diagnostic names. Operands are evaluated in written
//! order (`docs/spec/06-runtime.md`, "Evaluation"): a callee first, then the
//! positional arguments, then the labelled arguments in the order they are
//! listed, which is the order they were written. Argument slots are the
//! positional parameters, then the labelled ones in canonical label order.
//!
//! There is no union widening, no member pattern, no tuple projection, no
//! variadic tail, no `ArrayOf` or `DictOf`, no integer `Div` or `Rem`, no row
//! tail, and no plan or `await` node. A bare guard is discharged by the checker
//! and leaves no node; a guard with a consequence lives on
//! [`crate::CheckedFunction`] or on [`crate::Invariant`].

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::conformance::Evidence;
use crate::encode::{TypeTable, encode_value};
use crate::external::CompilerIntrinsic;
use crate::{
    CheckedFunction, CheckedGlobal, Constant, Implementation, InterfaceDefinition,
    IrError, Library, LibraryTypes, MatchArm, NamedTypes, OpenScope, Pattern,
    SourceOrigin, Substitution, Type, TypeBody, TypeDefinition, TypeId,
};

/// An expression and where its source is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    /// The expression.
    pub expr: Expr,
    /// Where it was written.
    pub origin: SourceOrigin,
}

impl Node {
    /// The node of `expr`, written at `origin`.
    #[must_use]
    pub const fn new(expr: Expr, origin: SourceOrigin) -> Self {
        Self { expr, origin }
    }
}

/// Dropping a tree of nodes moves each node's operands to a work list, so
/// that no host stack grows with the depth of the tree.
impl Drop for Node {
    fn drop(&mut self) {
        let mut pending = Vec::new();
        self.expr.take_operands(&mut pending);
        while let Some(mut expr) = pending.pop() {
            expr.take_operands(&mut pending);
        }
    }
}

impl Expr {
    /// Checks that every type written in this expression, its operands,
    /// patterns, constants, and closure bodies included, nests at most
    /// [`Type::MAX_DEPTH`] levels. The expression is walked with a work
    /// list.
    ///
    /// # Errors
    ///
    /// [`IrError::InvalidType`] for the first type nested deeper.
    pub fn check_type_depth(&self) -> Result<(), IrError> {
        let mut exprs = vec![self];
        let mut patterns: Vec<&Pattern> = Vec::new();
        while let Some(expr) = exprs.pop() {
            let mut types: Vec<&Type> = Vec::new();
            let mut nodes: Vec<&Node> = Vec::new();
            match expr {
                Self::Literal(constant) => types.push(constant.value_type()),
                Self::Intrinsic {
                    type_arguments,
                    arguments,
                    ..
                } => {
                    types.extend(type_arguments);
                    nodes.extend(arguments);
                }
                Self::Sequence(statements) => nodes.extend(statements),
                Self::Variable { value_type, .. }
                | Self::Captured { value_type, .. }
                | Self::Default(value_type) => types.push(value_type),
                Self::Global(_) => {}
                Self::Function { type_arguments, .. } => types.extend(type_arguments),
                Self::Closure { closure, captures } => {
                    types.push(&closure.value_type);
                    types.extend(&closure.captures);
                    types.extend(closure.defaults.iter().map(|(_, c)| c.value_type()));
                    nodes.extend(captures);
                    nodes.push(&closure.body);
                }
                Self::Let { value, body, .. } => nodes.extend([&**value, &**body]),
                Self::LetElse {
                    pattern,
                    value,
                    fallback,
                    body,
                } => {
                    patterns.push(pattern);
                    nodes.extend([&**value, &**fallback, &**body]);
                }
                Self::Match {
                    subject,
                    arms,
                    value_type,
                } => {
                    types.push(value_type);
                    nodes.push(subject);
                    for arm in arms {
                        patterns.push(&arm.pattern);
                        nodes.push(&arm.body);
                    }
                }
                Self::If {
                    condition,
                    then_branch,
                    else_branch,
                } => {
                    nodes.extend([&**condition, &**then_branch]);
                    nodes.extend(else_branch.as_deref());
                }
                Self::Construct {
                    value_type,
                    construction,
                } => {
                    types.push(value_type);
                    match construction {
                        Construction::Tuple(operands)
                        | Construction::List(operands) => {
                            nodes.extend(operands);
                        }
                        Construction::Record(fields)
                        | Construction::Case { fields, .. } => {
                            nodes.extend(fields.iter().map(|(_, node)| node));
                        }
                        Construction::Wrapper(operand) => nodes.push(operand),
                        Construction::Dict(entries) => {
                            for (key, value) in entries {
                                nodes.extend([key, value]);
                            }
                        }
                    }
                }
                Self::Lookup { collection, key } => {
                    nodes.extend([&**collection, &**key])
                }
                Self::Call { target, arguments }
                | Self::TailCall { target, arguments } => {
                    match target {
                        CallTarget::Direct { type_arguments, .. } => {
                            types.extend(type_arguments);
                        }
                        CallTarget::Value(callee) => nodes.push(callee),
                        CallTarget::Contract(call) => {
                            types.push(&call.receiver);
                            types.extend(&call.arguments);
                            types.extend(&call.member_arguments);
                            types.extend(
                                call.choices.entries().iter().map(|(_, ty)| ty),
                            );
                        }
                    }
                    nodes.extend(&arguments.positional);
                    nodes.extend(arguments.labelled.iter().map(|(_, node)| node));
                }
                Self::WidenToInterface { value, value_type } => {
                    types.push(value_type);
                    nodes.push(value);
                }
                Self::Field { record: value, .. }
                | Self::Return(value)
                | Self::Try(value)
                | Self::WidenAtom(value) => nodes.push(value),
            }
            while let Some(pattern) = patterns.pop() {
                match pattern {
                    Pattern::Literal(constant) => types.push(constant.value_type()),
                    Pattern::Tuple(inner) | Pattern::List(inner) => {
                        patterns.extend(inner)
                    }
                    Pattern::Record(fields) | Pattern::Case { fields, .. } => {
                        patterns.extend(fields.iter().map(|(_, pattern)| pattern));
                    }
                    Pattern::Wrapper(inner) => patterns.push(inner),
                    Pattern::Discard | Pattern::Bind(_) => {}
                }
            }
            for ty in types {
                ty.check_depth()?;
            }
            exprs.extend(nodes.into_iter().map(|node| &node.expr));
        }
        Ok(())
    }

    /// The nodes directly under this expression: its operands, the callee
    /// of a call of a value, and the body of a closure.
    pub(crate) fn operands(&self) -> Vec<&Node> {
        let mut nodes: Vec<&Node> = Vec::new();
        match self {
            Self::Intrinsic { arguments, .. } => nodes.extend(arguments),
            Self::Sequence(statements) => nodes.extend(statements),
            Self::Closure { closure, captures } => {
                nodes.extend(captures);
                nodes.push(&closure.body);
            }
            Self::Let { value, body, .. } => nodes.extend([&**value, &**body]),
            Self::LetElse {
                value,
                fallback,
                body,
                ..
            } => nodes.extend([&**value, &**fallback, &**body]),
            Self::Match { subject, arms, .. } => {
                nodes.push(subject);
                nodes.extend(arms.iter().map(|arm| &arm.body));
            }
            Self::If {
                condition,
                then_branch,
                else_branch,
            } => {
                nodes.extend([&**condition, &**then_branch]);
                nodes.extend(else_branch.as_deref());
            }
            Self::Construct { construction, .. } => match construction {
                Construction::Tuple(operands) | Construction::List(operands) => {
                    nodes.extend(operands);
                }
                Construction::Record(fields) | Construction::Case { fields, .. } => {
                    nodes.extend(fields.iter().map(|(_, node)| node));
                }
                Construction::Wrapper(operand) => nodes.push(operand),
                Construction::Dict(entries) => {
                    for (key, value) in entries {
                        nodes.extend([key, value]);
                    }
                }
            },
            Self::Lookup { collection, key } => nodes.extend([&**collection, &**key]),
            Self::Call { target, arguments } | Self::TailCall { target, arguments } => {
                if let CallTarget::Value(callee) = target {
                    nodes.push(callee);
                }
                nodes.extend(&arguments.positional);
                nodes.extend(arguments.labelled.iter().map(|(_, node)| node));
            }
            Self::Field { record: value, .. }
            | Self::Return(value)
            | Self::Try(value)
            | Self::WidenToInterface { value, .. }
            | Self::WidenAtom(value) => nodes.push(value),
            Self::Literal(_)
            | Self::Variable { .. }
            | Self::Global(_)
            | Self::Function { .. }
            | Self::Captured { .. }
            | Self::Default(_) => {}
        }
        nodes
    }

    /// Moves the expression of each operand of this one to `pending`,
    /// leaving a placeholder that owns nothing.
    fn take_operands(&mut self, pending: &mut Vec<Self>) {
        let mut take = |node: &mut Node| {
            pending.push(std::mem::replace(&mut node.expr, Self::Default(Type::Void)));
        };
        match self {
            Self::Intrinsic {
                arguments: nodes, ..
            }
            | Self::Sequence(nodes) => {
                nodes.iter_mut().for_each(&mut take);
            }
            Self::Closure { closure, captures } => {
                captures.iter_mut().for_each(&mut take);
                if let Some(closure) = Arc::get_mut(closure) {
                    take(&mut closure.body);
                }
            }
            Self::Let { value, body, .. } => {
                take(value);
                take(body);
            }
            Self::LetElse {
                value,
                fallback,
                body,
                ..
            } => {
                take(value);
                take(fallback);
                take(body);
            }
            Self::Match { subject, arms, .. } => {
                take(subject);
                for arm in arms {
                    take(&mut arm.body);
                }
            }
            Self::If {
                condition,
                then_branch,
                else_branch,
            } => {
                take(condition);
                take(then_branch);
                if let Some(else_branch) = else_branch {
                    take(else_branch);
                }
            }
            Self::Construct { construction, .. } => match construction {
                Construction::Tuple(nodes) | Construction::List(nodes) => {
                    nodes.iter_mut().for_each(&mut take);
                }
                Construction::Record(fields) | Construction::Case { fields, .. } => {
                    for (_, node) in fields {
                        take(node);
                    }
                }
                Construction::Wrapper(node) => take(node),
                Construction::Dict(entries) => {
                    for (key, value) in entries {
                        take(key);
                        take(value);
                    }
                }
            },
            Self::Lookup { collection, key } => {
                take(collection);
                take(key);
            }
            Self::Call { target, arguments } | Self::TailCall { target, arguments } => {
                if let CallTarget::Value(callee) = target {
                    take(callee);
                }
                arguments.positional.iter_mut().for_each(&mut take);
                for (_, node) in &mut arguments.labelled {
                    take(node);
                }
            }
            Self::Field { record: node, .. }
            | Self::Return(node)
            | Self::Try(node)
            | Self::WidenToInterface { value: node, .. }
            | Self::WidenAtom(node) => take(node),
            Self::Literal(_)
            | Self::Variable { .. }
            | Self::Global(_)
            | Self::Function { .. }
            | Self::Captured { .. }
            | Self::Default(_) => {}
        }
    }
}

/// A checked expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    /// A constant.
    Literal(Constant),
    /// An application of a `@compiler` row at its type arguments.
    Intrinsic {
        /// The row.
        intrinsic: CompilerIntrinsic,
        /// One type for each of the row's generics.
        type_arguments: Vec<Type>,
        /// The operands, in the row's parameter order.
        arguments: Vec<Node>,
    },
    /// Statements run first to last; the value is the last one's. Never
    /// empty.
    Sequence(Vec<Node>),
    /// A read of a slot of the current activation.
    Variable {
        /// The slot.
        slot: usize,
        /// The type the slot holds.
        value_type: Type,
    },
    /// A read of the module value at this index.
    Global(usize),
    /// A module function as a value, at its type arguments.
    Function {
        /// The function's index.
        function: usize,
        /// One type for each binder of its signature.
        type_arguments: Vec<Type>,
    },
    /// An anonymous callable and the values it captures.
    Closure {
        /// The callable's body and types.
        closure: Arc<ClosureBody>,
        /// One operand for each captured slot, in environment order.
        captures: Vec<Node>,
    },
    /// A read of the environment of the closure being run.
    Captured {
        /// The environment slot.
        slot: usize,
        /// The type it holds.
        value_type: Type,
    },
    /// `let`: the value, bound to a slot or discarded, then the body.
    Let {
        /// The slot the value is bound to, `None` for `_`.
        slot: Option<usize>,
        /// The initializer.
        value: Box<Node>,
        /// The rest of the enclosing sequence.
        body: Box<Node>,
    },
    /// A refutable `let … else`.
    LetElse {
        /// The pattern.
        pattern: Pattern,
        /// The initializer.
        value: Box<Node>,
        /// The fallback, of type `Never`: every path of it leaves.
        fallback: Box<Node>,
        /// The rest of the enclosing sequence, with the pattern's slots bound.
        body: Box<Node>,
    },
    /// `match`: the first arm whose pattern matches runs.
    Match {
        /// The subject.
        subject: Box<Node>,
        /// The arms, in written order. Never empty.
        arms: Vec<MatchArm>,
        /// The type of every arm's body that is not `Never`.
        value_type: Type,
    },
    /// `if`, with an `else` or without one.
    If {
        /// The condition, a `Bool`.
        condition: Box<Node>,
        /// The branch taken when the condition is `true`.
        then_branch: Box<Node>,
        /// The branch taken when it is `false`; without one, the `if` is a
        /// `Void` statement.
        else_branch: Option<Box<Node>>,
    },
    /// A construction. When the declared type has an invariant with a
    /// consequence, its value is `Result(T E)`.
    Construct {
        /// The type constructed.
        value_type: Type,
        /// The operands.
        construction: Construction,
    },
    /// A field read of a record.
    Field {
        /// The record.
        record: Box<Node>,
        /// The field name.
        field: String,
    },
    /// A lookup by application on a `List`, `Dict`, `Str`, or `Bytes`,
    /// whose value is an `Option`.
    Lookup {
        /// The collection.
        collection: Box<Node>,
        /// The index or key.
        key: Box<Node>,
    },
    /// A call that is not in tail position.
    Call {
        /// The callee.
        target: CallTarget,
        /// The arguments.
        arguments: Arguments,
    },
    /// A call in tail position, which replaces the activation.
    TailCall {
        /// The callee.
        target: CallTarget,
        /// The arguments.
        arguments: Arguments,
    },
    /// `return`, which leaves the nearest callable activation.
    Return(Box<Node>),
    /// `try` on an `Option` or a `Result`.
    Try(Box<Node>),
    /// A value widened to an interface value type.
    WidenToInterface {
        /// The value.
        value: Box<Node>,
        /// The interface value type.
        value_type: Type,
    },
    /// An atom-singleton value widened to `Atom`.
    WidenAtom(Box<Node>),
    /// The callee's default for a labelled argument not written; valid only
    /// as a labelled argument.
    Default(Type),
}

/// The operands of a construction, in written order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Construction {
    /// A tuple's components.
    Tuple(Vec<Node>),
    /// A record's fields, in written order.
    Record(Vec<(String, Node)>),
    /// A wrapper's representation.
    Wrapper(Box<Node>),
    /// A case of a variant type and its fields, in written order.
    Case {
        /// The case name.
        case: String,
        /// The fields.
        fields: Vec<(String, Node)>,
    },
    /// A list's elements.
    List(Vec<Node>),
    /// A dict's keys and values, in written order; a later duplicate key
    /// wins.
    Dict(Vec<(Node, Node)>),
}

/// What a call calls.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallTarget {
    /// A module function, at its type arguments.
    Direct {
        /// The function's index.
        function: usize,
        /// One type for each binder of its signature.
        type_arguments: Vec<Type>,
    },
    /// A callable value: a parameter, a capture, a field, a closure.
    Value(Box<Node>),
    /// A member of an interface, selected by the closed substitution of the
    /// type chapter.
    Contract(Box<ContractCall>),
}

/// A call of an interface member: the interface, the member, and the
/// substitution that selects the implementation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContractCall {
    /// The interface.
    pub interface: TypeId,
    /// The member name.
    pub member: String,
    /// The receiver type, `Self`.
    pub receiver: Type,
    /// The interface's positional arguments.
    pub arguments: Vec<Type>,
    /// One choice for each associated type of the interface.
    pub choices: NamedTypes,
    /// One type for each binder the member declares itself.
    pub member_arguments: Vec<Type>,
}

/// The arguments of a call.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Arguments {
    /// The positional arguments, in order.
    pub positional: Vec<Node>,
    /// One argument for every label of the callee, in written order; a label
    /// not written holds [`Expr::Default`].
    pub labelled: Vec<(String, Node)>,
}

/// An anonymous callable: its type, its activation's slots, its captured
/// environment, and its body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosureBody {
    /// The callable type, a `Fn` or a `Proc`.
    pub value_type: Type,
    /// The number of slots its activation has: its argument slots first.
    pub slot_count: usize,
    /// The type of each captured slot.
    pub captures: Vec<Type>,
    /// The constant default of each labelled parameter, which the closure
    /// value carries to every call through it.
    pub defaults: Vec<(String, Constant)>,
    /// The body.
    pub body: Node,
}

/// A guard with a consequence: `requires C else V`, or the `C else V` of an
/// `ensures`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Guard {
    /// The certified predicate, a `Bool`.
    pub predicate: Node,
    /// The certified consequence, of the callable's result type.
    pub consequence: Node,
}

/// `ensures C else V`: the candidate's slot and the guard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExitGuard {
    /// The slot the candidate is bound to, the result name.
    pub candidate: usize,
    /// The guard.
    pub guard: Guard,
}

/// The tables an expression is checked against.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Tables<'a> {
    pub(crate) library: &'a LibraryTypes,
    pub(crate) types: &'a BTreeMap<TypeId, TypeDefinition>,
    pub(crate) table: &'a TypeTable,
    pub(crate) interfaces: &'a BTreeMap<TypeId, InterfaceDefinition>,
    pub(crate) implementations: &'a [Implementation],
    pub(crate) globals: &'a [CheckedGlobal],
    pub(crate) functions: &'a [CheckedFunction],
}

/// The callable types: the parameters, the labels, and the result of a
/// call's value.
pub(crate) fn call_shape(callee: &Type) -> Option<(&[Type], &NamedTypes, Type)> {
    match callee {
        Type::Fn(signature) => Some((
            &signature.parameters,
            &signature.labels,
            signature.result.clone(),
        )),
        Type::Proc(signature) => Some((
            &signature.parameters,
            &signature.labels,
            Type::Intent(Box::new(crate::IntentType {
                result: signature.result.clone(),
                effects: signature.effects.clone(),
            })),
        )),
        _ => None,
    }
}

fn names_equal<'a>(
    left: impl IntoIterator<Item = &'a str>,
    right: impl IntoIterator<Item = &'a str>,
) -> bool {
    left.into_iter().collect::<BTreeSet<_>>()
        == right.into_iter().collect::<BTreeSet<_>>()
}

impl<'a> Tables<'a> {
    /// Checks the shape of `ty` in `scope`, and that every declared type and
    /// interface it names exists at its arity, with every choice.
    pub(crate) fn check_type(
        &self,
        ty: &Type,
        scope: OpenScope<'_>,
    ) -> Result<(), IrError> {
        ty.validate(scope)?;
        self.check_names(ty)
    }

    fn check_names(&self, ty: &Type) -> Result<(), IrError> {
        match ty {
            Type::Declared(id, arguments) => {
                let definition = self
                    .types
                    .get(id)
                    .ok_or_else(|| IrError::UnknownType(id.path().to_owned()))?;
                if definition.parameters().len() != arguments.len() {
                    return Err(IrError::ArityMismatch {
                        what: "type arguments",
                        expected: definition.parameters().len(),
                        actual: arguments.len(),
                    });
                }
            }
            Type::Interface(id, arguments, choices) => {
                let interface = self
                    .interfaces
                    .get(id)
                    .ok_or_else(|| IrError::UnknownInterface(id.path().to_owned()))?;
                if interface.parameters().len() != arguments.len() {
                    return Err(IrError::ArityMismatch {
                        what: "interface arguments",
                        expected: interface.parameters().len(),
                        actual: arguments.len(),
                    });
                }
                if interface.associated().len() != choices.len()
                    || !names_equal(
                        interface.associated().iter().map(String::as_str),
                        choices.names(),
                    )
                {
                    return Err(IrError::InvalidType(format!(
                        "an interface value of `{}` fixes every associated type",
                        id.path()
                    )));
                }
            }
            _ => {}
        }
        ty.components()
            .into_iter()
            .try_for_each(|component| self.check_names(component))
    }

    pub(crate) fn definition(
        &self,
        id: &TypeId,
    ) -> Result<&'a TypeDefinition, IrError> {
        self.types
            .get(id)
            .ok_or_else(|| IrError::UnknownType(id.path().to_owned()))
    }

    pub(crate) fn library_type(
        &self,
        library: Library,
        arguments: Vec<Type>,
    ) -> Result<Type, IrError> {
        self.library.declared(library, arguments)
    }

    pub(crate) fn function(
        &self,
        index: usize,
    ) -> Result<&'a CheckedFunction, IrError> {
        self.functions
            .get(index)
            .ok_or(IrError::UnknownFunction(index))
    }

    /// The type of the function at `index` as a value at `type_arguments`.
    pub(crate) fn function_type(
        &self,
        index: usize,
        type_arguments: &[Type],
    ) -> Result<Type, IrError> {
        let function = self.function(index)?;
        if function.signature.associated().is_some() {
            return Err(IrError::InvalidExpression(format!(
                "the default member `{}` is reached only through its interface",
                function.name
            )));
        }
        let binders = function.signature.binders().len();
        if binders != type_arguments.len() {
            return Err(IrError::ArityMismatch {
                what: "type arguments",
                expected: binders,
                actual: type_arguments.len(),
            });
        }
        Ok(function.signature.type_info().instantiate(type_arguments))
    }
}

/// The frame an expression is checked in.
#[derive(Debug)]
pub(crate) struct Checker<'a> {
    pub(crate) tables: Tables<'a>,
    pub(crate) scope: OpenScope<'a>,
    /// The bounds of the binders in scope, and the interface `Self` belongs
    /// to in a default body.
    pub(crate) evidence: Evidence<'a>,
    pub(crate) slots: Slots,
    pub(crate) captures: Vec<Type>,
    /// The enclosing callable's result, `None` in a module-value
    /// initializer.
    pub(crate) result: Option<Type>,
    /// Whether the expression is a guard's predicate or consequence, where
    /// nothing leaves the activation.
    pub(crate) in_guard: bool,
}

/// The slots of an activation being checked: the declared count bounds the
/// slot numbers, and only the slots bound hold storage, so a count taken
/// from the IR allocates nothing in proportion to itself.
#[derive(Clone, Debug, Default)]
pub(crate) struct Slots {
    count: usize,
    held: BTreeMap<usize, Type>,
}

impl Slots {
    pub(crate) const fn new(count: usize) -> Self {
        Self {
            count,
            held: BTreeMap::new(),
        }
    }

    const fn check(&self, slot: usize) -> Result<(), IrError> {
        if slot < self.count {
            Ok(())
        } else {
            Err(IrError::SlotOutOfRange {
                slot,
                count: self.count,
            })
        }
    }

    /// The type `slot` holds, `None` when it is unset.
    pub(crate) fn get(&self, slot: usize) -> Result<Option<&Type>, IrError> {
        self.check(slot)?;
        Ok(self.held.get(&slot))
    }

    /// Sets `slot` to `ty`, or unsets it, and returns what it held.
    fn set(&mut self, slot: usize, ty: Option<Type>) -> Result<Option<Type>, IrError> {
        self.check(slot)?;
        Ok(match ty {
            Some(ty) => self.held.insert(slot, ty),
            None => self.held.remove(&slot),
        })
    }
}

/// `actual` is a value of `expected`: they are equal, or `actual` is `Never`.
pub(crate) fn expect(expected: &Type, actual: &Type) -> Result<(), IrError> {
    if actual == expected || *actual == Type::Never {
        Ok(())
    } else {
        Err(IrError::TypeMismatch {
            expected: Box::new(expected.clone()),
            actual: Box::new(actual.clone()),
        })
    }
}

/// The fields of `fields` are exactly `declared`, each once.
pub(crate) fn check_field_set<'n>(
    declared: impl IntoIterator<Item = &'n str>,
    written: impl IntoIterator<Item = &'n str>,
    subset: bool,
) -> Result<(), IrError> {
    let declared = declared.into_iter().collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    for name in written {
        if !declared.contains(name) {
            return Err(IrError::InvalidExpression(format!("no field `{name}`")));
        }
        if !seen.insert(name) {
            return Err(IrError::Duplicate {
                what: "field",
                name: name.to_owned(),
            });
        }
    }
    if !subset && seen.len() != declared.len() {
        return Err(IrError::ArityMismatch {
            what: "fields",
            expected: declared.len(),
            actual: seen.len(),
        });
    }
    Ok(())
}

/// The declared components of a value of `ty`, instantiated at its
/// arguments.
pub(crate) enum Shape<'a> {
    Tuple(Vec<Type>),
    Record(Vec<(String, Type)>),
    Wrapper(Type),
    Variant(&'a [crate::Case], Vec<Type>),
    List(Type),
}

impl<'a> Checker<'a> {
    pub(crate) fn new(
        tables: Tables<'a>,
        scope: OpenScope<'a>,
        slot_count: usize,
    ) -> Self {
        Self {
            tables,
            scope,
            evidence: Evidence::default(),
            slots: Slots::new(slot_count),
            captures: Vec::new(),
            result: None,
            in_guard: false,
        }
    }

    pub(crate) fn check_type(&self, ty: &Type) -> Result<(), IrError> {
        self.tables.check_type(ty, self.scope)
    }

    /// Checks that a constant's value fits its type, as its encoding does.
    pub(crate) fn check_constant(&self, constant: &Constant) -> Result<(), IrError> {
        self.check_type(constant.value_type())?;
        encode_value(constant.value(), constant.value_type(), self.tables.table)
            .map(drop)
            .map_err(|error| IrError::InvalidExpression(format!("a constant: {error}")))
    }

    pub(crate) fn bool_type(&self) -> Result<Type, IrError> {
        self.tables.library_type(Library::Bool, Vec::new())
    }

    /// Checks the defaults of a callable with the labels `labels`: one
    /// constant of the label's type for each label, as the source chapter's
    /// `labelled` gives every optional parameter, and no other.
    pub(crate) fn check_defaults(
        &self,
        labels: &NamedTypes,
        defaults: &[(String, Constant)],
    ) -> Result<(), IrError> {
        let mut seen = BTreeSet::new();
        for (label, default) in defaults {
            let expected = labels.get(label).ok_or_else(|| {
                IrError::InvalidExpression(format!("a default for no label `{label}`"))
            })?;
            if !seen.insert(label.as_str()) {
                return Err(IrError::Duplicate {
                    what: "default",
                    name: label.clone(),
                });
            }
            self.check_constant(default)?;
            expect(expected, default.value_type())?;
        }
        if let Some((label, _)) = labels
            .entries()
            .iter()
            .find(|(label, _)| !seen.contains(label.as_str()))
        {
            return Err(IrError::InvalidExpression(format!(
                "the labelled parameter `{label}` has no default"
            )));
        }
        Ok(())
    }

    /// Checks that `key` implements `Ordered`, as every dict key must.
    pub(crate) fn check_dict_key(&self, key: &Type) -> Result<(), IrError> {
        if self.tables.is_dict_key(self.evidence, key)? {
            Ok(())
        } else {
            Err(IrError::InvalidType(format!(
                "{key} does not implement `Ordered` and keys no dict"
            )))
        }
    }

    /// The type of the function at `index` as a value at `type_arguments`,
    /// whose bounds hold.
    pub(crate) fn instantiated_function(
        &self,
        index: usize,
        type_arguments: &[Type],
    ) -> Result<Type, IrError> {
        for argument in type_arguments {
            self.check_type(argument)?;
        }
        let ty = self.tables.function_type(index, type_arguments)?;
        self.tables.check_bounds(
            self.evidence,
            self.tables.function(index)?.signature.binders(),
            &Substitution {
                binders: type_arguments,
                ..Substitution::default()
            },
        )?;
        Ok(ty)
    }

    /// Sets `slot` to `ty` and returns what it held.
    pub(crate) fn bind(
        &mut self,
        slot: usize,
        ty: Type,
    ) -> Result<Option<Type>, IrError> {
        self.slots.set(slot, Some(ty))
    }

    /// Restores slots bound by [`Self::bind`], last first.
    pub(crate) fn restore(&mut self, bound: Vec<(usize, Option<Type>)>) {
        for (slot, held) in bound.into_iter().rev() {
            // Every slot here was bound, so it is in range.
            let _ = self.slots.set(slot, held);
        }
    }

    /// The components of a value of `ty`.
    pub(crate) fn shape(&self, ty: &Type) -> Result<Shape<'a>, IrError> {
        Ok(match ty {
            Type::Tuple(values) => Shape::Tuple(values.clone()),
            Type::Record(fields) => Shape::Record(fields.entries().to_vec()),
            Type::List(element) => Shape::List(element.as_ref().clone()),
            Type::Declared(id, arguments) => {
                let definition = self.tables.definition(id)?;
                match definition.body() {
                    TypeBody::Tuple(values) => Shape::Tuple(
                        values
                            .iter()
                            .map(|value| value.instantiate(arguments))
                            .collect(),
                    ),
                    TypeBody::Record(fields) => Shape::Record(
                        fields
                            .iter()
                            .map(|(name, value)| {
                                (name.clone(), value.instantiate(arguments))
                            })
                            .collect(),
                    ),
                    TypeBody::Wrapper(value) => {
                        Shape::Wrapper(value.instantiate(arguments))
                    }
                    TypeBody::Variant(cases) => {
                        Shape::Variant(cases, arguments.clone())
                    }
                }
            }
            other => {
                return Err(IrError::InvalidExpression(format!(
                    "a value of {other} has no components"
                )));
            }
        })
    }

    pub(crate) fn check_try(
        &self,
        value: &Type,
        result: &Type,
    ) -> Result<Type, IrError> {
        let library = self.tables.library;
        if let Some([item]) = library.arguments_of(Library::Option, value)
            && library.arguments_of(Library::Option, result).is_some()
        {
            return Ok(item.clone());
        }
        if let Some([item, error]) = library.arguments_of(Library::Result, value)
            && let Some([_, exit]) = library.arguments_of(Library::Result, result)
            && exit == error
        {
            return Ok(item.clone());
        }
        Err(IrError::InvalidExpression(format!(
            "a try on {value} inside a callable returning {result}"
        )))
    }

    pub(crate) fn check_field_subset(
        declared: &[(String, Type)],
        written: &[(String, Pattern)],
    ) -> Result<(), IrError> {
        check_field_set(
            declared.iter().map(|(name, _)| name.as_str()),
            written.iter().map(|(name, _)| name.as_str()),
            true,
        )
    }

    /// The type of the member a contract call calls, at the call's
    /// substitution, once the call names the member, its interface's
    /// arguments and choices, a receiver that implements it, and type
    /// arguments that satisfy the member's bounds.
    pub(crate) fn contract_callee(&self, call: &ContractCall) -> Result<Type, IrError> {
        let ContractCall {
            interface,
            member,
            receiver,
            arguments,
            choices,
            member_arguments,
        } = call;
        for ty in std::iter::once(receiver)
            .chain(arguments)
            .chain(member_arguments)
            .chain(choices.entries().iter().map(|(_, value)| value))
        {
            self.check_type(ty)?;
        }
        let definition =
            self.tables.interfaces.get(interface).ok_or_else(|| {
                IrError::UnknownInterface(interface.path().to_owned())
            })?;
        let declared = definition.member(member).ok_or_else(|| {
            IrError::InvalidExpression(format!(
                "`{}` has no member `{member}`",
                interface.path()
            ))
        })?;
        if definition.parameters().len() != arguments.len() {
            return Err(IrError::ArityMismatch {
                what: "interface arguments",
                expected: definition.parameters().len(),
                actual: arguments.len(),
            });
        }
        let own = declared
            .signature
            .binders()
            .len()
            .saturating_sub(arguments.len());
        if own != member_arguments.len() {
            return Err(IrError::ArityMismatch {
                what: "member type arguments",
                expected: own,
                actual: member_arguments.len(),
            });
        }
        if definition.associated().len() != choices.len()
            || !names_equal(
                definition.associated().iter().map(String::as_str),
                choices.names(),
            )
        {
            return Err(IrError::InvalidExpression(format!(
                "a call of `{}.{member}` fixes every associated type",
                interface.path()
            )));
        }
        self.check_receiver(interface, member, declared, call)?;
        let binders = arguments
            .iter()
            .chain(member_arguments)
            .cloned()
            .collect::<Vec<_>>();
        let substitution = Substitution {
            binders: &binders,
            self_type: Some(receiver),
            associated: Some(choices),
        };
        self.tables.check_bounds(
            self.evidence,
            declared.signature.binders(),
            &substitution,
        )?;
        Ok(declared.signature.type_info().substitute(&substitution))
    }

    /// Checks the receiver of a contract call. Through an interface value,
    /// the value's interface, positional arguments, and choices are the
    /// call's, the member has a receiver slot, and no other parameter
    /// mentions `Self`. Otherwise evidence that the receiver implements the
    /// interface gives exactly the call's choices.
    fn check_receiver(
        &self,
        interface: &TypeId,
        member: &str,
        declared: &crate::InterfaceMember,
        call: &ContractCall,
    ) -> Result<(), IrError> {
        let path = interface.path();
        if call.receiver.is_interface_value() {
            let same = matches!(
                &call.receiver,
                Type::Interface(held, arguments, choices)
                    if held == interface
                        && *arguments == call.arguments
                        && *choices == call.choices
            );
            if !same {
                return Err(IrError::InvalidExpression(format!(
                    "a call of `{path}.{member}` through a value of {}",
                    call.receiver
                )));
            }
            let parameters = match declared.signature.type_info() {
                Type::Fn(signature) => signature.parameters.as_slice(),
                Type::Proc(signature) => signature.parameters.as_slice(),
                _ => &[],
            };
            let slot = declared
                .signature
                .receiver()
                .and_then(|slot| usize::try_from(slot).ok())
                .ok_or_else(|| {
                    IrError::InvalidExpression(format!(
                        "`{path}.{member}` has no receiver slot and is not called through an interface value"
                    ))
                })?;
            let mentions_self = |ty: &Type| {
                let mut found = false;
                ty.visit(&mut |inner| found |= *inner == Type::SelfType);
                found
            };
            let labels = match declared.signature.type_info() {
                Type::Fn(signature) => signature.labels.entries(),
                Type::Proc(signature) => signature.labels.entries(),
                _ => &[],
            };
            let other_self = parameters
                .iter()
                .enumerate()
                .any(|(index, ty)| index != slot && mentions_self(ty))
                || labels.iter().any(|(_, ty)| mentions_self(ty));
            if other_self {
                return Err(IrError::InvalidExpression(format!(
                    "`{path}.{member}` takes `Self` outside its receiver slot and is not called through an interface value"
                )));
            }
            return Ok(());
        }
        let provided = self
            .tables
            .conformance(self.evidence, &call.receiver, interface, &call.arguments)?
            .ok_or_else(|| {
                IrError::InvalidExpression(format!(
                    "no implementation of `{path}` for {}",
                    call.receiver
                ))
            })?;
        if provided == call.choices {
            Ok(())
        } else {
            Err(IrError::InvalidExpression(format!(
                "the choices of the call of `{path}` are not the selected implementation's"
            )))
        }
    }
}

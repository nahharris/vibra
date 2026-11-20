//! The check of an expression, walked with an explicit stack of frames, one
//! for each node whose operands are being checked, so that no host stack
//! grows with the depth of the tree. The same walk gathers a body's facts.
//!
//! Each node runs the steps its rule gives in order: the checks before an
//! operand, the operand, the checks on the operand's type, and so on. A
//! frame keeps what the node needs between its operands; when a check
//! fails, every frame still open gives back the slots it bound and the
//! activation it entered.

use std::collections::BTreeSet;
use std::mem;

use crate::expr::{
    Arguments, CallTarget, Checker, ClosureBody, Construction, Expr, Node, Shape,
    Slots, call_shape, check_field_set, expect,
};
use crate::external::{CompilerIntrinsic, RowType};
use crate::module_set::{Gathered, contract_targets, ordered_targets};
use crate::{Callable, Facts, IrError, Library, NamedTypes, Type};

/// What a node asks for next.
enum Next<'n> {
    /// The type of this operand.
    Operand(&'n Node),
    /// Nothing: the node has this type.
    Done(Type),
}

/// The activation a closure's body is checked in replaces this one, which
/// comes back when the body is done.
#[derive(Debug)]
struct Activation {
    slots: Slots,
    captures: Vec<Type>,
    result: Option<Type>,
}

/// One node whose operands are being checked.
struct Frame<'n> {
    node: &'n Node,
    /// How far the node's rule has run.
    stage: u8,
    /// The next operand or arm by position.
    index: usize,
    /// The types the node keeps between its operands.
    kept: Vec<Type>,
    /// The operands still to check, last first, each with its type.
    operands: Vec<(&'n Node, Type)>,
    /// The type the operand being checked must have.
    expecting: Option<Type>,
    /// The labels of a call's callee.
    labels: Option<NamedTypes>,
    /// The labelled arguments of a call written so far.
    seen: BTreeSet<&'n str>,
    /// The slots bound for the node's scope and what each held.
    bound: Vec<(usize, Option<Type>)>,
    /// The enclosing activation, while a closure's body is checked.
    outer: Option<Box<Activation>>,
    /// The position of this node's facts in the call-of-a-value list.
    at: usize,
}

impl<'n> Frame<'n> {
    fn new(node: &'n Node) -> Self {
        Self {
            node,
            stage: 0,
            index: 0,
            kept: Vec::new(),
            operands: Vec::new(),
            expecting: None,
            labels: None,
            seen: BTreeSet::new(),
            bound: Vec::new(),
            outer: None,
            at: 0,
        }
    }

    /// Checks the operand just checked against the type it must have, and
    /// gives the next operand, `None` when every one is checked.
    fn next_operand(
        &mut self,
        input: Option<Type>,
    ) -> Result<Option<&'n Node>, IrError> {
        if let (Some(actual), Some(expected)) = (input, self.expecting.take()) {
            expect(&expected, &actual)?;
        }
        Ok(self.operands.pop().map(|(node, expected)| {
            self.expecting = Some(expected);
            node
        }))
    }

    /// Queues `operands`, each with the type it must have, first first.
    fn queue(&mut self, operands: Vec<(&'n Node, Type)>) {
        self.operands = operands;
        self.operands.reverse();
    }
}

/// Where the gathered facts go: the body's own, or a closure's while its
/// body is checked. Without a body to gather for, nothing is kept.
///
/// A closure is numbered when its body is done, after every closure inside
/// it, in the one list of the body's closures: each closure's facts are
/// written once, so gathering is linear in the body however deep its
/// closures nest.
struct Sink<'g> {
    root: Option<&'g mut Gathered>,
    nested: Vec<Facts>,
}

impl Sink<'_> {
    fn facts(&mut self) -> Option<&mut Facts> {
        match self.nested.last_mut() {
            Some(facts) => Some(facts),
            None => self.root.as_deref_mut().map(|gathered| &mut gathered.facts),
        }
    }

    fn enter_closure(&mut self) {
        if self.root.is_some() {
            self.nested.push(Facts::default());
        }
    }

    fn leave_closure(&mut self, closure_type: &Type) {
        let Some(inner) = self.nested.pop() else {
            return;
        };
        let Some(root) = self.root.as_deref_mut() else {
            return;
        };
        let number = root.closures.len();
        root.closures.push((closure_type.clone(), inner));
        if let Some(facts) = self.facts() {
            facts
                .values
                .push((Callable::Closure(number), closure_type.clone()));
        }
    }

    fn abandon_closure(&mut self) {
        if self.root.is_some() {
            self.nested.pop();
        }
    }

    /// The length of the call-of-a-value list now.
    fn mark(&mut self) -> usize {
        self.facts().map_or(0, |facts| facts.indirect.len())
    }

    /// Holds a place in the call-of-a-value list for a callee whose type
    /// is not known yet.
    fn reserve(&mut self) -> usize {
        self.facts().map_or(0, |facts| {
            facts.indirect.push(Type::Never);
            facts.indirect.len().saturating_sub(1)
        })
    }

    fn fill(&mut self, at: usize, callee: &Type) {
        if let Some(place) = self.facts().and_then(|facts| facts.indirect.get_mut(at)) {
            place.clone_from(callee);
        }
    }

    fn insert(&mut self, at: usize, callees: Vec<Type>) {
        if let Some(facts) = self.facts() {
            let at = at.min(facts.indirect.len());
            facts.indirect.splice(at..at, callees);
        }
    }
}

/// The type of the operand a frame resumes with.
fn arrived(input: Option<Type>) -> Result<Type, IrError> {
    input.ok_or_else(|| {
        IrError::InvalidExpression("a check resumed without its operand".to_owned())
    })
}

impl Checker<'_> {
    /// Checks `node`, gives its type, and adds its facts to `out`.
    pub(crate) fn check_gathering(
        &mut self,
        node: &Node,
        out: &mut Gathered,
    ) -> Result<Type, IrError> {
        self.walk(node, Some(out))
    }

    fn walk(
        &mut self,
        root: &Node,
        out: Option<&mut Gathered>,
    ) -> Result<Type, IrError> {
        let mut sink = Sink {
            root: out,
            nested: Vec::new(),
        };
        let mut frames = vec![Frame::new(root)];
        let mut input = None;
        loop {
            let Some(frame) = frames.last_mut() else {
                return arrived(input);
            };
            match self.resume(frame, input.take(), &mut sink) {
                Ok(Next::Operand(node)) => frames.push(Frame::new(node)),
                Ok(Next::Done(ty)) => {
                    frames.pop();
                    if let Err(error) = ty.check_depth() {
                        self.unwind(frames, &mut sink);
                        return Err(error);
                    }
                    if frames.is_empty() {
                        return Ok(ty);
                    }
                    input = Some(ty);
                }
                Err(error) => {
                    self.unwind(frames, &mut sink);
                    return Err(error);
                }
            }
        }
    }

    /// Gives back what each open frame bound or entered, innermost first.
    fn unwind(&mut self, mut frames: Vec<Frame<'_>>, sink: &mut Sink<'_>) {
        while let Some(mut frame) = frames.pop() {
            self.restore(mem::take(&mut frame.bound));
            if let Some(outer) = frame.outer.take() {
                self.leave(*outer);
                sink.abandon_closure();
            }
        }
    }

    fn leave(&mut self, outer: Activation) {
        self.slots = outer.slots;
        self.captures = outer.captures;
        self.result = outer.result;
    }

    fn enclosing_result(&self, what: &'static str) -> Result<Type, IrError> {
        self.result.clone().ok_or(IrError::MisplacedNode(what))
    }

    #[allow(clippy::too_many_lines)]
    fn resume<'n>(
        &mut self,
        frame: &mut Frame<'n>,
        input: Option<Type>,
        sink: &mut Sink<'_>,
    ) -> Result<Next<'n>, IrError> {
        let node = frame.node;
        let stage = frame.stage;
        frame.stage = stage.saturating_add(1);
        Ok(match &node.expr {
            Expr::Literal(constant) => {
                self.check_constant(constant)?;
                Next::Done(constant.value_type().clone())
            }
            Expr::Intrinsic {
                intrinsic,
                type_arguments,
                arguments,
            } => self.intrinsic(
                frame,
                stage,
                input,
                sink,
                *intrinsic,
                type_arguments,
                arguments,
            )?,
            Expr::Sequence(statements) => match statements.get(frame.index) {
                Some(statement) => {
                    frame.index = frame.index.saturating_add(1);
                    Next::Operand(statement)
                }
                None => Next::Done(input.ok_or_else(|| {
                    IrError::InvalidExpression("an empty sequence".to_owned())
                })?),
            },
            Expr::Variable { slot, value_type } => {
                self.check_type(value_type)?;
                let held = self.slots.get(*slot)?.ok_or(IrError::UnsetSlot(*slot))?;
                if held != value_type {
                    return Err(IrError::SlotTypeMismatch {
                        slot: *slot,
                        expected: Box::new(held.clone()),
                        actual: Box::new(value_type.clone()),
                    });
                }
                Next::Done(value_type.clone())
            }
            Expr::Global(index) => {
                let ty = self
                    .tables
                    .globals
                    .get(*index)
                    .map(|global| global.value_type.clone())
                    .ok_or(IrError::UnknownGlobal(*index))?;
                if let Some(facts) = sink.facts() {
                    facts.reads.insert(*index);
                }
                Next::Done(ty)
            }
            Expr::Function {
                function,
                type_arguments,
            } => {
                let ty = self.instantiated_function(*function, type_arguments)?;
                if let Some(facts) = sink.facts() {
                    facts
                        .values
                        .push((Callable::Function(*function), ty.clone()));
                }
                Next::Done(ty)
            }
            Expr::Closure { closure, captures } => {
                self.closure(frame, stage, input, sink, closure, captures)?
            }
            Expr::Captured { slot, value_type } => {
                self.check_type(value_type)?;
                let held = self.captures.get(*slot).ok_or(IrError::SlotOutOfRange {
                    slot: *slot,
                    count: self.captures.len(),
                })?;
                if held != value_type {
                    return Err(IrError::SlotTypeMismatch {
                        slot: *slot,
                        expected: Box::new(held.clone()),
                        actual: Box::new(value_type.clone()),
                    });
                }
                Next::Done(value_type.clone())
            }
            Expr::Let { slot, value, body } => match stage {
                0 => Next::Operand(value),
                1 => {
                    let value = arrived(input)?;
                    if let Some(slot) = slot {
                        let held = self.bind(*slot, value)?;
                        frame.bound.push((*slot, held));
                    }
                    Next::Operand(body)
                }
                _ => {
                    self.restore(mem::take(&mut frame.bound));
                    Next::Done(arrived(input)?)
                }
            },
            Expr::LetElse {
                pattern,
                value,
                fallback,
                body,
            } => match stage {
                0 => Next::Operand(value),
                1 => {
                    frame.kept.push(arrived(input)?);
                    Next::Operand(fallback)
                }
                2 => {
                    expect(&Type::Never, &arrived(input)?)?;
                    let value = arrived(frame.kept.pop())?;
                    self.check_pattern(pattern, &value, &mut frame.bound)?;
                    Next::Operand(body)
                }
                _ => {
                    self.restore(mem::take(&mut frame.bound));
                    Next::Done(arrived(input)?)
                }
            },
            Expr::Match {
                subject,
                arms,
                value_type,
            } => {
                match stage {
                    0 => {
                        self.check_type(value_type)?;
                        return Ok(Next::Operand(subject));
                    }
                    1 => {
                        if arms.is_empty() {
                            return Err(IrError::InvalidExpression(
                                "a match with no arm".to_owned(),
                            ));
                        }
                        frame.kept.push(arrived(input)?);
                    }
                    _ => {
                        let checked = expect(value_type, &arrived(input)?);
                        self.restore(mem::take(&mut frame.bound));
                        checked?;
                    }
                }
                match arms.get(frame.index) {
                    Some(arm) => {
                        frame.index = frame.index.saturating_add(1);
                        let subject = arrived(frame.kept.first().cloned())?;
                        self.check_pattern(&arm.pattern, &subject, &mut frame.bound)?;
                        Next::Operand(&arm.body)
                    }
                    None => Next::Done(value_type.clone()),
                }
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => match stage {
                0 => Next::Operand(condition),
                1 => {
                    expect(&self.bool_type()?, &arrived(input)?)?;
                    Next::Operand(then_branch)
                }
                2 => {
                    let then_type = arrived(input)?;
                    match else_branch {
                        None => {
                            expect(&Type::Void, &then_type)?;
                            Next::Done(Type::Void)
                        }
                        Some(else_branch) => {
                            frame.kept.push(then_type);
                            Next::Operand(else_branch)
                        }
                    }
                }
                _ => {
                    let else_type = arrived(input)?;
                    let then_type = arrived(frame.kept.pop())?;
                    if then_type == Type::Never {
                        Next::Done(else_type)
                    } else {
                        expect(&then_type, &else_type)?;
                        Next::Done(then_type)
                    }
                }
            },
            Expr::Construct {
                value_type,
                construction,
            } => {
                if stage == 0 {
                    let operands = self.construct_operands(value_type, construction)?;
                    frame.queue(operands);
                }
                match frame.next_operand(input)? {
                    Some(operand) => Next::Operand(operand),
                    None => Next::Done(self.constructed(value_type, sink)?),
                }
            }
            Expr::Field { record, field } => match stage {
                0 => Next::Operand(record),
                _ => {
                    let record = arrived(input)?;
                    match self.shape(&record)? {
                        Shape::Record(fields) => Next::Done(
                            fields
                                .into_iter()
                                .find_map(|(name, value)| {
                                    (name == *field).then_some(value)
                                })
                                .ok_or_else(|| {
                                    IrError::InvalidExpression(format!(
                                        "{record} has no field `{field}`"
                                    ))
                                })?,
                        ),
                        _ => {
                            return Err(IrError::InvalidExpression(format!(
                                "a field read of {record}, which is not a record"
                            )));
                        }
                    }
                }
            },
            Expr::Lookup { collection, key } => match stage {
                0 => Next::Operand(collection),
                1 => {
                    frame.kept.push(arrived(input)?);
                    Next::Operand(key)
                }
                _ => {
                    let key = arrived(input)?;
                    let collection = arrived(frame.kept.pop())?;
                    Next::Done(self.looked_up(&collection, &key, sink)?)
                }
            },
            Expr::Call { target, arguments } => {
                self.call(frame, stage, input, sink, target, arguments, false)?
            }
            Expr::TailCall { target, arguments } => {
                self.call(frame, stage, input, sink, target, arguments, true)?
            }
            Expr::Return(value) => match stage {
                0 => {
                    if self.in_guard {
                        return Err(IrError::MisplacedNode("a return inside a guard"));
                    }
                    self.enclosing_result("a return outside a callable")?;
                    Next::Operand(value)
                }
                _ => {
                    let result =
                        self.enclosing_result("a return outside a callable")?;
                    expect(&result, &arrived(input)?)?;
                    Next::Done(Type::Never)
                }
            },
            Expr::Try(value) => match stage {
                0 => {
                    if self.in_guard {
                        return Err(IrError::MisplacedNode("a try inside a guard"));
                    }
                    self.enclosing_result("a try outside a callable")?;
                    Next::Operand(value)
                }
                _ => {
                    let result = self.enclosing_result("a try outside a callable")?;
                    Next::Done(self.check_try(&arrived(input)?, &result)?)
                }
            },
            Expr::WidenToInterface { value, value_type } => match stage {
                0 => {
                    self.check_type(value_type)?;
                    Next::Operand(value)
                }
                _ => {
                    let value = arrived(input)?;
                    let widens = match value_type {
                        _ if value.is_interface_value() => false,
                        Type::Any => true,
                        _ if value == Type::Never => true,
                        Type::Interface(interface, arguments, choices) => {
                            self.tables.conforms_with(
                                self.evidence,
                                &value,
                                interface,
                                arguments,
                                choices,
                            )?
                        }
                        _ => false,
                    };
                    if !widens {
                        return Err(IrError::InvalidExpression(format!(
                            "{value} does not widen to {value_type}"
                        )));
                    }
                    Next::Done(value_type.clone())
                }
            },
            Expr::WidenAtom(value) => match stage {
                0 => Next::Operand(value),
                _ => match arrived(input)? {
                    Type::AtomSingleton(_) => Next::Done(Type::Atom),
                    other => {
                        return Err(IrError::InvalidExpression(format!(
                            "{other} is not an atom singleton"
                        )));
                    }
                },
            },
            Expr::Default(_) => {
                return Err(IrError::MisplacedNode(
                    "a default outside a labelled argument",
                ));
            }
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn intrinsic<'n>(
        &self,
        frame: &mut Frame<'n>,
        stage: u8,
        input: Option<Type>,
        sink: &mut Sink<'_>,
        intrinsic: CompilerIntrinsic,
        type_arguments: &[Type],
        arguments: &'n [Node],
    ) -> Result<Next<'n>, IrError> {
        let row = intrinsic.row();
        if stage == 0 {
            if row.generics.len() != type_arguments.len() {
                return Err(IrError::ArityMismatch {
                    what: "type arguments",
                    expected: row.generics.len(),
                    actual: type_arguments.len(),
                });
            }
            for argument in type_arguments {
                self.check_type(argument)?;
            }
            self.check_built_dict(intrinsic, type_arguments)?;
            if row.parameters.len() != arguments.len() {
                return Err(IrError::ArityMismatch {
                    what: "arguments",
                    expected: row.parameters.len(),
                    actual: arguments.len(),
                });
            }
            let mut operands = Vec::with_capacity(arguments.len());
            for (parameter, argument) in row.parameters.iter().zip(arguments) {
                let parameter =
                    parameter.instantiate(self.tables.library, type_arguments)?;
                operands.push((argument, parameter));
            }
            frame.queue(operands);
            frame.at = sink.mark();
        }
        if let Some(operand) = frame.next_operand(input)? {
            return Ok(Next::Operand(operand));
        }
        if intrinsic == CompilerIntrinsic::DictFromList
            && let Some(facts) = sink.facts()
        {
            facts.calls.extend(ordered_targets(&self.tables));
        }
        if sink.facts().is_some() {
            let mut callees = Vec::new();
            for parameter in row.parameters {
                if let RowType::Fn(..) = parameter {
                    callees.push(
                        parameter.instantiate(self.tables.library, type_arguments)?,
                    );
                }
            }
            sink.insert(frame.at, callees);
        }
        Ok(Next::Done(
            row.result
                .instantiate(self.tables.library, type_arguments)?,
        ))
    }

    fn closure<'n>(
        &mut self,
        frame: &mut Frame<'n>,
        stage: u8,
        input: Option<Type>,
        sink: &mut Sink<'_>,
        closure: &'n ClosureBody,
        captures: &'n [Node],
    ) -> Result<Next<'n>, IrError> {
        let not_callable = || {
            IrError::InvalidExpression(format!(
                "a closure of {}, which is not callable",
                closure.value_type
            ))
        };
        let result = || match &closure.value_type {
            Type::Fn(signature) => signature.result.clone(),
            Type::Proc(signature) => signature.result.clone(),
            _ => Type::Never,
        };
        if stage == 0 {
            self.check_type(&closure.value_type)?;
            call_shape(&closure.value_type).ok_or_else(not_callable)?;
            if captures.len() != closure.captures.len() {
                return Err(IrError::ArityMismatch {
                    what: "captures",
                    expected: closure.captures.len(),
                    actual: captures.len(),
                });
            }
            let mut operands = Vec::with_capacity(captures.len());
            for (expected, capture) in closure.captures.iter().zip(captures) {
                self.check_type(expected)?;
                operands.push((capture, expected.clone()));
            }
            frame.queue(operands);
        }
        if frame.outer.is_none() {
            if let Some(capture) = frame.next_operand(input)? {
                return Ok(Next::Operand(capture));
            }
            return self.enter_closure(frame, sink, closure, result());
        }
        expect(&result(), &arrived(input)?)?;
        if let Some(outer) = frame.outer.take() {
            self.leave(*outer);
            sink.leave_closure(&closure.value_type);
        }
        Ok(Next::Done(closure.value_type.clone()))
    }

    /// With every capture checked: checks the closure's slots and defaults
    /// and starts its body in its own activation.
    fn enter_closure<'n>(
        &mut self,
        frame: &mut Frame<'n>,
        sink: &mut Sink<'_>,
        closure: &'n ClosureBody,
        result: Type,
    ) -> Result<Next<'n>, IrError> {
        let (parameters, labels, _) =
            call_shape(&closure.value_type).ok_or_else(|| {
                IrError::InvalidExpression("a closure that is not callable".to_owned())
            })?;
        let slot_types = parameters
            .iter()
            .cloned()
            .chain(labels.entries().iter().map(|(_, value)| value.clone()))
            .collect::<Vec<_>>();
        if closure.slot_count < slot_types.len() {
            return Err(IrError::InvalidSlotCount {
                needed: slot_types.len(),
                declared: closure.slot_count,
            });
        }
        self.check_defaults(labels, &closure.defaults)?;
        let outer = Activation {
            slots: mem::replace(&mut self.slots, Slots::new(closure.slot_count)),
            captures: mem::replace(&mut self.captures, closure.captures.clone()),
            result: self.result.replace(result),
        };
        frame.outer = Some(Box::new(outer));
        sink.enter_closure();
        for (slot, ty) in slot_types.into_iter().enumerate() {
            self.bind(slot, ty)?;
        }
        Ok(Next::Operand(&closure.body))
    }

    /// The operands of a construction of `value_type`, each with the type it
    /// must have, in written order.
    fn construct_operands<'n>(
        &self,
        value_type: &Type,
        construction: &'n Construction,
    ) -> Result<Vec<(&'n Node, Type)>, IrError> {
        self.check_type(value_type)?;
        let library = self.tables.library;
        if let Some([key, value]) = library.arguments_of(Library::Dict, value_type) {
            let Construction::Dict(entries) = construction else {
                return Err(IrError::InvalidExpression(format!(
                    "a {value_type} is built from a dict literal"
                )));
            };
            self.check_dict_key(key)?;
            return Ok(entries
                .iter()
                .flat_map(|(entry_key, entry_value)| {
                    [(entry_key, key.clone()), (entry_value, value.clone())]
                })
                .collect());
        }
        Ok(match (self.shape(value_type)?, construction) {
            (Shape::Tuple(components), Construction::Tuple(operands)) => {
                if components.len() != operands.len() {
                    return Err(IrError::ArityMismatch {
                        what: "tuple components",
                        expected: components.len(),
                        actual: operands.len(),
                    });
                }
                operands.iter().zip(components).collect()
            }
            (Shape::Record(fields), Construction::Record(operands)) => {
                named_operands(&fields, operands)?
            }
            (Shape::Wrapper(representation), Construction::Wrapper(operand)) => {
                vec![(operand.as_ref(), representation)]
            }
            (Shape::Variant(cases, arguments), Construction::Case { case, fields }) => {
                let declared = cases
                    .iter()
                    .find(|declared| declared.name == *case)
                    .ok_or_else(|| {
                        IrError::InvalidExpression(format!("no case `{case}`"))
                    })?;
                let declared = declared
                    .fields
                    .iter()
                    .map(|(name, value)| (name.clone(), value.instantiate(&arguments)))
                    .collect::<Vec<_>>();
                named_operands(&declared, fields)?
            }
            (Shape::List(element), Construction::List(operands)) => operands
                .iter()
                .map(|operand| (operand, element.clone()))
                .collect(),
            _ => {
                return Err(IrError::InvalidExpression(format!(
                    "the construction does not fit {value_type}"
                )));
            }
        })
    }

    /// The type of a construction of `value_type` whose operands are
    /// checked, and the functions it enters.
    fn constructed(
        &self,
        value_type: &Type,
        sink: &mut Sink<'_>,
    ) -> Result<Type, IrError> {
        let mut ty = value_type.clone();
        if let Type::Declared(id, _) = value_type
            && let Some(invariant) = self.tables.definition(id)?.invariant()
            && let Some(consequence) = &invariant.consequence
        {
            ty = self.tables.library_type(
                Library::Result,
                vec![value_type.clone(), consequence.value_type().clone()],
            )?;
        }
        if let Type::Declared(id, _) = value_type
            && let Some(facts) = sink.facts()
        {
            if self.tables.library.get(Library::Dict) == Some(id) {
                facts.calls.extend(ordered_targets(&self.tables));
            } else if let Some(invariant) = self.tables.definition(id)?.invariant()
                && invariant.consequence.is_some()
            {
                facts.calls.insert(invariant.predicate);
            }
        }
        Ok(ty)
    }

    /// The type of a lookup in a `collection` by a `key`.
    fn looked_up(
        &self,
        collection: &Type,
        key: &Type,
        sink: &mut Sink<'_>,
    ) -> Result<Type, IrError> {
        let library = self.tables.library;
        let (key_type, item) = if let Type::List(element) = collection {
            (Type::U64, element.as_ref().clone())
        } else if library.arguments_of(Library::Str, collection).is_some() {
            (Type::U64, Type::Char)
        } else if library.arguments_of(Library::Bytes, collection).is_some() {
            (Type::U64, Type::U8)
        } else if let Some([key_type, value]) =
            library.arguments_of(Library::Dict, collection)
        {
            self.check_dict_key(key_type)?;
            if let Some(facts) = sink.facts() {
                facts.calls.extend(ordered_targets(&self.tables));
            }
            (key_type.clone(), value.clone())
        } else {
            return Err(IrError::InvalidExpression(format!(
                "a lookup on {collection}"
            )));
        };
        expect(&key_type, key)?;
        self.tables.library_type(Library::Option, vec![item])
    }

    #[allow(clippy::too_many_arguments)]
    fn call<'n>(
        &self,
        frame: &mut Frame<'n>,
        stage: u8,
        input: Option<Type>,
        sink: &mut Sink<'_>,
        target: &'n CallTarget,
        arguments: &'n Arguments,
        tail: bool,
    ) -> Result<Next<'n>, IrError> {
        let mut input = input;
        if stage == 0 {
            if tail {
                if self.in_guard {
                    return Err(IrError::MisplacedNode("a tail call inside a guard"));
                }
                self.enclosing_result("a tail call outside a callable")?;
            }
            let callee = match target {
                CallTarget::Direct {
                    function,
                    type_arguments,
                } => {
                    let callee =
                        self.instantiated_function(*function, type_arguments)?;
                    if let Some(facts) = sink.facts() {
                        facts.calls.insert(*function);
                    }
                    callee
                }
                CallTarget::Value(callee) => {
                    frame.at = sink.reserve();
                    return Ok(Next::Operand(callee));
                }
                CallTarget::Contract(call) => {
                    let callee = self.contract_callee(call)?;
                    if let Some(facts) = sink.facts() {
                        facts.calls.extend(contract_targets(
                            &self.tables,
                            &call.interface,
                            &call.member,
                        ));
                    }
                    callee
                }
            };
            self.callee_known(frame, &callee, arguments)?;
        } else if frame.labels.is_none() {
            let callee = arrived(input.take())?;
            sink.fill(frame.at, &callee);
            self.callee_known(frame, &callee, arguments)?;
        }
        if let Some(operand) = frame.next_operand(input)? {
            return Ok(Next::Operand(operand));
        }
        self.labelled(frame, arguments, tail)
    }

    /// With the callee's type known: checks the positional arity and queues
    /// the positional arguments.
    fn callee_known<'n>(
        &self,
        frame: &mut Frame<'n>,
        callee: &Type,
        arguments: &'n Arguments,
    ) -> Result<(), IrError> {
        let (parameters, labels, result) = call_shape(callee).ok_or_else(|| {
            IrError::InvalidExpression(format!(
                "a call of {callee}, which is not callable"
            ))
        })?;
        if parameters.len() != arguments.positional.len() {
            return Err(IrError::ArityMismatch {
                what: "arguments",
                expected: parameters.len(),
                actual: arguments.positional.len(),
            });
        }
        frame.queue(
            arguments
                .positional
                .iter()
                .zip(parameters.iter().cloned())
                .collect(),
        );
        frame.labels = Some(labels.clone());
        frame.kept.push(result);
        Ok(())
    }

    /// Checks the labelled arguments one at a time, and then the call's
    /// type.
    fn labelled<'n>(
        &self,
        frame: &mut Frame<'n>,
        arguments: &'n Arguments,
        tail: bool,
    ) -> Result<Next<'n>, IrError> {
        let labels = frame.labels.as_ref().ok_or_else(|| {
            IrError::InvalidExpression("a call resumed without its callee".to_owned())
        })?;
        if frame.index == 0
            && frame.seen.is_empty()
            && labels.len() != arguments.labelled.len()
        {
            return Err(IrError::ArityMismatch {
                what: "labelled arguments",
                expected: labels.len(),
                actual: arguments.labelled.len(),
            });
        }
        while let Some((label, argument)) = arguments.labelled.get(frame.index) {
            frame.index = frame.index.saturating_add(1);
            if !frame.seen.insert(label.as_str()) {
                return Err(IrError::Duplicate {
                    what: "labelled argument",
                    name: label.clone(),
                });
            }
            let expected = labels.get(label).ok_or_else(|| {
                IrError::InvalidExpression(format!("no label `{label}`"))
            })?;
            // Every callable a call can reach, a function or a closure, holds
            // a checked default for each of its labels, so a placeholder
            // always finds the callee's default.
            if let Expr::Default(value_type) = &argument.expr {
                self.check_type(value_type)?;
                expect(expected, value_type)?;
            } else {
                frame.expecting = Some(expected.clone());
                return Ok(Next::Operand(argument));
            }
        }
        let result = arrived(frame.kept.pop())?;
        if tail {
            if matches!(result, Type::Intent(_)) {
                return Err(IrError::InvalidExpression(
                    "a tail call of a procedure builds a plan".to_owned(),
                ));
            }
            expect(
                &self.enclosing_result("a tail call outside a callable")?,
                &result,
            )?;
        }
        Ok(Next::Done(result))
    }
}

/// The operands of a record or a case, each with its declared type, once
/// the written fields are exactly the declared ones.
fn named_operands<'n>(
    declared: &[(String, Type)],
    operands: &'n [(String, Node)],
) -> Result<Vec<(&'n Node, Type)>, IrError> {
    check_field_set(
        declared.iter().map(|(name, _)| name.as_str()),
        operands.iter().map(|(name, _)| name.as_str()),
        false,
    )?;
    operands
        .iter()
        .map(|(name, operand)| {
            declared
                .iter()
                .find_map(|(field, value)| {
                    (field == name).then(|| (operand, value.clone()))
                })
                .ok_or_else(|| IrError::InvalidExpression(format!("no field `{name}`")))
        })
        .collect()
}

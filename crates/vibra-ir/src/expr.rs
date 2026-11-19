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
    pub(crate) slots: Vec<Option<Type>>,
    pub(crate) captures: Vec<Type>,
    /// The enclosing callable's result, `None` in a module-value
    /// initializer.
    pub(crate) result: Option<Type>,
    /// Whether the expression is a guard's predicate or consequence, where
    /// nothing leaves the activation.
    pub(crate) in_guard: bool,
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
fn check_field_set<'n>(
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
            slots: vec![None; slot_count],
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

    fn bool_type(&self) -> Result<Type, IrError> {
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
    fn check_dict_key(&self, key: &Type) -> Result<(), IrError> {
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
    fn instantiated_function(
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
        let count = self.slots.len();
        let held = self
            .slots
            .get_mut(slot)
            .ok_or(IrError::SlotOutOfRange { slot, count })?;
        Ok(held.replace(ty))
    }

    /// Restores slots bound by [`Self::bind`], last first.
    pub(crate) fn restore(&mut self, bound: Vec<(usize, Option<Type>)>) {
        for (slot, held) in bound.into_iter().rev() {
            if let Some(place) = self.slots.get_mut(slot) {
                *place = held;
            }
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

    /// Checks `node` and gives its type.
    pub(crate) fn check(&mut self, node: &Node) -> Result<Type, IrError> {
        match &node.expr {
            Expr::Literal(constant) => {
                self.check_constant(constant)?;
                Ok(constant.value_type().clone())
            }
            Expr::Intrinsic {
                intrinsic,
                type_arguments,
                arguments,
            } => self.check_intrinsic(*intrinsic, type_arguments, arguments),
            Expr::Sequence(statements) => {
                let mut last = None;
                for statement in statements {
                    last = Some(self.check(statement)?);
                }
                last.ok_or_else(|| {
                    IrError::InvalidExpression("an empty sequence".to_owned())
                })
            }
            Expr::Variable { slot, value_type } => {
                self.check_type(value_type)?;
                let count = self.slots.len();
                let held = self
                    .slots
                    .get(*slot)
                    .ok_or(IrError::SlotOutOfRange { slot: *slot, count })?
                    .as_ref()
                    .ok_or(IrError::UnsetSlot(*slot))?;
                if held != value_type {
                    return Err(IrError::SlotTypeMismatch {
                        slot: *slot,
                        expected: Box::new(held.clone()),
                        actual: Box::new(value_type.clone()),
                    });
                }
                Ok(value_type.clone())
            }
            Expr::Global(index) => self
                .tables
                .globals
                .get(*index)
                .map(|global| global.value_type.clone())
                .ok_or(IrError::UnknownGlobal(*index)),
            Expr::Function {
                function,
                type_arguments,
            } => self.instantiated_function(*function, type_arguments),
            Expr::Closure { closure, captures } => {
                self.check_closure(closure, captures)
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
                Ok(value_type.clone())
            }
            Expr::Let { slot, value, body } => {
                let value = self.check(value)?;
                match slot {
                    Some(slot) => {
                        let held = self.bind(*slot, value)?;
                        let body = self.check(body);
                        self.restore(vec![(*slot, held)]);
                        body
                    }
                    None => self.check(body),
                }
            }
            Expr::LetElse {
                pattern,
                value,
                fallback,
                body,
            } => {
                let value = self.check(value)?;
                expect(&Type::Never, &self.check(fallback)?)?;
                let mut bound = Vec::new();
                let checked = self
                    .check_pattern(pattern, &value, &mut bound)
                    .and_then(|()| self.check(body));
                self.restore(bound);
                checked
            }
            Expr::Match {
                subject,
                arms,
                value_type,
            } => {
                self.check_type(value_type)?;
                let subject = self.check(subject)?;
                if arms.is_empty() {
                    return Err(IrError::InvalidExpression(
                        "a match with no arm".to_owned(),
                    ));
                }
                for arm in arms {
                    let mut bound = Vec::new();
                    let checked = self
                        .check_pattern(&arm.pattern, &subject, &mut bound)
                        .and_then(|()| self.check(&arm.body))
                        .and_then(|body| expect(value_type, &body));
                    self.restore(bound);
                    checked?;
                }
                Ok(value_type.clone())
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let condition = self.check(condition)?;
                expect(&self.bool_type()?, &condition)?;
                let then_type = self.check(then_branch)?;
                match else_branch {
                    None => {
                        expect(&Type::Void, &then_type)?;
                        Ok(Type::Void)
                    }
                    Some(else_branch) => {
                        let else_type = self.check(else_branch)?;
                        if then_type == Type::Never {
                            Ok(else_type)
                        } else {
                            expect(&then_type, &else_type)?;
                            Ok(then_type)
                        }
                    }
                }
            }
            Expr::Construct {
                value_type,
                construction,
            } => self.check_construct(value_type, construction),
            Expr::Field { record, field } => {
                let record = self.check(record)?;
                match self.shape(&record)? {
                    Shape::Record(fields) => fields
                        .into_iter()
                        .find_map(|(name, value)| (name == *field).then_some(value))
                        .ok_or_else(|| {
                            IrError::InvalidExpression(format!(
                                "{record} has no field `{field}`"
                            ))
                        }),
                    _ => Err(IrError::InvalidExpression(format!(
                        "a field read of {record}, which is not a record"
                    ))),
                }
            }
            Expr::Lookup { collection, key } => {
                let collection = self.check(collection)?;
                let key = self.check(key)?;
                let library = self.tables.library;
                let (key_type, item) = if let Type::List(element) = &collection {
                    (Type::U64, element.as_ref().clone())
                } else if library.arguments_of(Library::Str, &collection).is_some() {
                    (Type::U64, Type::Char)
                } else if library.arguments_of(Library::Bytes, &collection).is_some() {
                    (Type::U64, Type::U8)
                } else if let Some([key_type, value]) =
                    library.arguments_of(Library::Dict, &collection)
                {
                    self.check_dict_key(key_type)?;
                    (key_type.clone(), value.clone())
                } else {
                    return Err(IrError::InvalidExpression(format!(
                        "a lookup on {collection}"
                    )));
                };
                expect(&key_type, &key)?;
                self.tables.library_type(Library::Option, vec![item])
            }
            Expr::Call { target, arguments } => self.check_call(target, arguments),
            Expr::TailCall { target, arguments } => {
                if self.in_guard {
                    return Err(IrError::MisplacedNode("a tail call inside a guard"));
                }
                let result = self
                    .result
                    .clone()
                    .ok_or(IrError::MisplacedNode("a tail call outside a callable"))?;
                let callee = self.check_call(target, arguments)?;
                if matches!(callee, Type::Intent(_)) {
                    return Err(IrError::InvalidExpression(
                        "a tail call of a procedure builds a plan".to_owned(),
                    ));
                }
                expect(&result, &callee)?;
                Ok(callee)
            }
            Expr::Return(value) => {
                if self.in_guard {
                    return Err(IrError::MisplacedNode("a return inside a guard"));
                }
                let result = self
                    .result
                    .clone()
                    .ok_or(IrError::MisplacedNode("a return outside a callable"))?;
                expect(&result, &self.check(value)?)?;
                Ok(Type::Never)
            }
            Expr::Try(value) => {
                if self.in_guard {
                    return Err(IrError::MisplacedNode("a try inside a guard"));
                }
                let result = self
                    .result
                    .clone()
                    .ok_or(IrError::MisplacedNode("a try outside a callable"))?;
                let value = self.check(value)?;
                self.check_try(&value, &result)
            }
            Expr::WidenToInterface { value, value_type } => {
                self.check_type(value_type)?;
                let value = self.check(value)?;
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
                Ok(value_type.clone())
            }
            Expr::WidenAtom(value) => match self.check(value)? {
                Type::AtomSingleton(_) => Ok(Type::Atom),
                other => Err(IrError::InvalidExpression(format!(
                    "{other} is not an atom singleton"
                ))),
            },
            Expr::Default(_) => Err(IrError::MisplacedNode(
                "a default outside a labelled argument",
            )),
        }
    }

    fn check_try(&self, value: &Type, result: &Type) -> Result<Type, IrError> {
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

    fn check_intrinsic(
        &mut self,
        intrinsic: CompilerIntrinsic,
        type_arguments: &[Type],
        arguments: &[Node],
    ) -> Result<Type, IrError> {
        let row = intrinsic.row();
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
        if row.parameters.len() != arguments.len() {
            return Err(IrError::ArityMismatch {
                what: "arguments",
                expected: row.parameters.len(),
                actual: arguments.len(),
            });
        }
        for (parameter, argument) in row.parameters.iter().zip(arguments) {
            let parameter =
                parameter.instantiate(self.tables.library, type_arguments)?;
            expect(&parameter, &self.check(argument)?)?;
        }
        row.result.instantiate(self.tables.library, type_arguments)
    }

    fn check_closure(
        &mut self,
        closure: &ClosureBody,
        captures: &[Node],
    ) -> Result<Type, IrError> {
        self.check_type(&closure.value_type)?;
        let (parameters, labels, _) =
            call_shape(&closure.value_type).ok_or_else(|| {
                IrError::InvalidExpression(format!(
                    "a closure of {}, which is not callable",
                    closure.value_type
                ))
            })?;
        let result = match &closure.value_type {
            Type::Fn(signature) => signature.result.clone(),
            Type::Proc(signature) => signature.result.clone(),
            _ => Type::Never,
        };
        let slot_types = parameters
            .iter()
            .cloned()
            .chain(labels.entries().iter().map(|(_, value)| value.clone()))
            .collect::<Vec<_>>();
        if captures.len() != closure.captures.len() {
            return Err(IrError::ArityMismatch {
                what: "captures",
                expected: closure.captures.len(),
                actual: captures.len(),
            });
        }
        for (expected, capture) in closure.captures.iter().zip(captures) {
            self.check_type(expected)?;
            expect(expected, &self.check(capture)?)?;
        }
        if closure.slot_count < slot_types.len() {
            return Err(IrError::InvalidSlotCount {
                needed: slot_types.len(),
                declared: closure.slot_count,
            });
        }
        self.check_defaults(labels, &closure.defaults)?;
        let mut inner = Checker {
            tables: self.tables,
            scope: self.scope,
            evidence: self.evidence,
            slots: vec![None; closure.slot_count],
            captures: closure.captures.clone(),
            result: Some(result.clone()),
            in_guard: self.in_guard,
        };
        for (slot, ty) in slot_types.into_iter().enumerate() {
            inner.bind(slot, ty)?;
        }
        expect(&result, &inner.check(&closure.body)?)?;
        Ok(closure.value_type.clone())
    }

    fn check_construct(
        &mut self,
        value_type: &Type,
        construction: &Construction,
    ) -> Result<Type, IrError> {
        self.check_type(value_type)?;
        let library = self.tables.library;
        if let Some([key, value]) = library.arguments_of(Library::Dict, value_type) {
            let Construction::Dict(entries) = construction else {
                return Err(IrError::InvalidExpression(format!(
                    "a {value_type} is built from a dict literal"
                )));
            };
            self.check_dict_key(key)?;
            for (entry_key, entry_value) in entries {
                expect(key, &self.check(entry_key)?)?;
                expect(value, &self.check(entry_value)?)?;
            }
            return Ok(value_type.clone());
        }
        match (self.shape(value_type)?, construction) {
            (Shape::Tuple(components), Construction::Tuple(operands)) => {
                if components.len() != operands.len() {
                    return Err(IrError::ArityMismatch {
                        what: "tuple components",
                        expected: components.len(),
                        actual: operands.len(),
                    });
                }
                for (component, operand) in components.iter().zip(operands) {
                    expect(component, &self.check(operand)?)?;
                }
            }
            (Shape::Record(fields), Construction::Record(operands)) => {
                self.check_fields(&fields, operands)?;
            }
            (Shape::Wrapper(representation), Construction::Wrapper(operand)) => {
                expect(&representation, &self.check(operand)?)?;
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
                self.check_fields(&declared, fields)?;
            }
            (Shape::List(element), Construction::List(operands)) => {
                for operand in operands {
                    expect(&element, &self.check(operand)?)?;
                }
            }
            _ => {
                return Err(IrError::InvalidExpression(format!(
                    "the construction does not fit {value_type}"
                )));
            }
        }
        if let Type::Declared(id, _) = value_type
            && let Some(invariant) = self.tables.definition(id)?.invariant()
            && let Some(consequence) = &invariant.consequence
        {
            return self.tables.library_type(
                Library::Result,
                vec![value_type.clone(), consequence.value_type().clone()],
            );
        }
        Ok(value_type.clone())
    }

    fn check_fields(
        &mut self,
        declared: &[(String, Type)],
        operands: &[(String, Node)],
    ) -> Result<(), IrError> {
        check_field_set(
            declared.iter().map(|(name, _)| name.as_str()),
            operands.iter().map(|(name, _)| name.as_str()),
            false,
        )?;
        for (name, operand) in operands {
            let expected = declared
                .iter()
                .find_map(|(field, value)| (field == name).then_some(value))
                .ok_or_else(|| {
                    IrError::InvalidExpression(format!("no field `{name}`"))
                })?;
            expect(expected, &self.check(operand)?)?;
        }
        Ok(())
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

    fn callee_type(
        &mut self,
        target: &CallTarget,
    ) -> Result<(Type, Option<usize>), IrError> {
        match target {
            CallTarget::Direct {
                function,
                type_arguments,
            } => Ok((
                self.instantiated_function(*function, type_arguments)?,
                Some(*function),
            )),
            CallTarget::Value(callee) => Ok((self.check(callee)?, None)),
            CallTarget::Contract(call) => {
                let ContractCall {
                    interface,
                    member,
                    receiver,
                    arguments,
                    choices,
                    member_arguments,
                } = call.as_ref();
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
                Ok((
                    declared.signature.type_info().substitute(&substitution),
                    None,
                ))
            }
        }
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

    fn check_call(
        &mut self,
        target: &CallTarget,
        arguments: &Arguments,
    ) -> Result<Type, IrError> {
        let (callee, _) = self.callee_type(target)?;
        let (parameters, labels, result) = call_shape(&callee).ok_or_else(|| {
            IrError::InvalidExpression(format!(
                "a call of {callee}, which is not callable"
            ))
        })?;
        let (parameters, labels) = (parameters.to_vec(), labels.clone());
        if parameters.len() != arguments.positional.len() {
            return Err(IrError::ArityMismatch {
                what: "arguments",
                expected: parameters.len(),
                actual: arguments.positional.len(),
            });
        }
        for (parameter, argument) in parameters.iter().zip(&arguments.positional) {
            expect(parameter, &self.check(argument)?)?;
        }
        if labels.len() != arguments.labelled.len() {
            return Err(IrError::ArityMismatch {
                what: "labelled arguments",
                expected: labels.len(),
                actual: arguments.labelled.len(),
            });
        }
        let mut seen = BTreeSet::new();
        for (label, argument) in &arguments.labelled {
            if !seen.insert(label.as_str()) {
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
                expect(expected, &self.check(argument)?)?;
            }
        }
        Ok(result)
    }
}

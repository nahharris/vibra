//! What the emitter knows of types: the class a value sits in, the arena shape
//! of a closed type, and the static type of a checked node.
//!
//! A type's **class** says how a value of it sits in a cell: a scalar of one
//! of four widths, `Void` (a scalar `0`), or a reference to an arena value. A
//! type's **shape** lists the components of its values in the order the layout
//! stores them: declaration order for a declared record and for the fields of
//! a case, and name order for a structural record, so a case index or a field
//! position is computed here and nowhere else.
//!
//! The emitter specializes every callable at closed type arguments, so the
//! types it asks about are closed. A type the transition profile has no value
//! for, a procedure, a plan, or an interface value, is reported unsupported.

use vibra_ir::{
    CallTarget, CheckedModuleSet, ContractCall, Expr, FnType, Library, Node,
    SourceOrigin, Substitution, Type, TypeBody,
};

use crate::error::EmitError;
use crate::runtime::ValueClass;

/// The arena shape of a closed type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Shape {
    /// A scalar, held in a cell of this class; `Void` and `Never` are
    /// [`ValueClass::Void`].
    Scalar(ValueClass),
    /// An atom.
    Atom,
    /// `Str`.
    Str,
    /// `Bytes`.
    Bytes,
    /// `Dict(K V)`.
    Dict(Type, Type),
    /// A tuple of these components.
    Tuple(Vec<Type>),
    /// `List(T)`.
    List(Type),
    /// A record of these fields, in layout order.
    Record(Vec<(String, Type)>),
    /// A variant of these cases, each with its fields in declaration order.
    Variant(Vec<(String, Vec<(String, Type)>)>),
    /// A wrapper of this representation.
    Wrapper(Type),
    /// A function value.
    Function(FnType),
}

impl Shape {
    /// The class of a value of this shape.
    pub(crate) const fn class(&self) -> ValueClass {
        match self {
            Self::Scalar(class) => *class,
            _ => ValueClass::Ref,
        }
    }

    /// The position of the case `name` and its fields, for a variant.
    pub(crate) fn case(&self, name: &str) -> Option<(u32, &[(String, Type)])> {
        let Self::Variant(cases) = self else {
            return None;
        };
        cases
            .iter()
            .enumerate()
            .find(|(_, (case, _))| case == name)
            .and_then(|(position, (_, fields))| {
                Some((u32::try_from(position).ok()?, fields.as_slice()))
            })
    }
}

/// The position of `name` among `fields`.
pub(crate) fn field_position(fields: &[(String, Type)], name: &str) -> Option<u32> {
    fields
        .iter()
        .position(|(field, _)| field == name)
        .and_then(|position| u32::try_from(position).ok())
}

/// The emitter's view of the types of one module set.
#[derive(Clone, Copy)]
pub(crate) struct Types<'a> {
    set: &'a CheckedModuleSet,
}

impl<'a> Types<'a> {
    pub(crate) const fn new(set: &'a CheckedModuleSet) -> Self {
        Self { set }
    }

    /// The library declaration `ty` is an application of, if any.
    pub(crate) fn library_of(&self, ty: &Type) -> Option<Library> {
        match ty {
            Type::Declared(id, _) => self.set.library().library_of(id),
            _ => None,
        }
    }

    /// The library type `library` at `arguments`.
    pub(crate) fn library_type(
        &self,
        library: Library,
        arguments: Vec<Type>,
        origin: &SourceOrigin,
    ) -> Result<Type, EmitError> {
        self.set
            .library()
            .declared(library, arguments)
            .map_err(|error| EmitError::malformed(error.to_string(), origin))
    }

    /// The shape of the closed type `ty`.
    pub(crate) fn shape(
        &self,
        ty: &Type,
        origin: &SourceOrigin,
    ) -> Result<Shape, EmitError> {
        Ok(match ty {
            Type::I8
            | Type::I16
            | Type::I32
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::Char => Shape::Scalar(ValueClass::I32),
            Type::I64 | Type::U64 => Shape::Scalar(ValueClass::I64),
            Type::F32 => Shape::Scalar(ValueClass::F32),
            Type::F64 => Shape::Scalar(ValueClass::F64),
            Type::Void | Type::Never => Shape::Scalar(ValueClass::Void),
            Type::Atom | Type::AtomSingleton(_) => Shape::Atom,
            Type::List(element) => Shape::List(element.as_ref().clone()),
            Type::Tuple(components) => Shape::Tuple(components.clone()),
            Type::Record(fields) => Shape::Record(fields.entries().to_vec()),
            Type::Fn(signature) => Shape::Function(signature.as_ref().clone()),
            Type::Proc(_) => return Err(EmitError::unsupported("proc", origin)),
            Type::Intent(_) => return Err(EmitError::unsupported("plan", origin)),
            Type::Any | Type::Interface(..) => {
                return Err(EmitError::unsupported("interfaceValue", origin));
            }
            Type::Binder(_) | Type::SelfType | Type::Associated(_) => {
                return Err(EmitError::malformed(
                    format!("the open type {ty} where a closed one is needed"),
                    origin,
                ));
            }
            Type::Declared(id, arguments) => {
                match self.set.library().library_of(id) {
                    Some(Library::Str) => return Ok(Shape::Str),
                    Some(Library::Bytes) => return Ok(Shape::Bytes),
                    Some(Library::Dict) => {
                        return match arguments.as_slice() {
                            [key, value] => Ok(Shape::Dict(key.clone(), value.clone())),
                            _ => Err(EmitError::malformed(
                                format!("the dict type {ty}"),
                                origin,
                            )),
                        };
                    }
                    _ => {}
                }
                let definition = self.set.types().definition(id).ok_or_else(|| {
                    EmitError::malformed(format!("no declaration of {ty}"), origin)
                })?;
                let at = |field: &Type| field.instantiate(arguments);
                match definition.body() {
                    TypeBody::Record(fields) => Shape::Record(
                        fields
                            .iter()
                            .map(|(name, field)| (name.clone(), at(field)))
                            .collect(),
                    ),
                    TypeBody::Tuple(components) => {
                        Shape::Tuple(components.iter().map(at).collect())
                    }
                    TypeBody::Wrapper(representation) => {
                        Shape::Wrapper(at(representation))
                    }
                    TypeBody::Variant(cases) => Shape::Variant(
                        cases
                            .iter()
                            .map(|case| {
                                (
                                    case.name.clone(),
                                    case.fields
                                        .iter()
                                        .map(|(name, field)| (name.clone(), at(field)))
                                        .collect(),
                                )
                            })
                            .collect(),
                    ),
                }
            }
        })
    }

    /// The class of a value of the closed type `ty`.
    pub(crate) fn class(
        &self,
        ty: &Type,
        origin: &SourceOrigin,
    ) -> Result<ValueClass, EmitError> {
        Ok(self.shape(ty, origin)?.class())
    }

    /// The case index of `case` in the variant type `ty`.
    pub(crate) fn case_index(
        &self,
        ty: &Type,
        case: &str,
        origin: &SourceOrigin,
    ) -> Result<u32, EmitError> {
        self.shape(ty, origin)?
            .case(case)
            .map(|(position, _)| position)
            .ok_or_else(|| {
                EmitError::malformed(format!("{ty} has no case `{case}`"), origin)
            })
    }

    /// The type of the function `function` at `type_arguments`.
    pub(crate) fn function_type(
        &self,
        function: usize,
        type_arguments: &[Type],
        origin: &SourceOrigin,
    ) -> Result<Type, EmitError> {
        self.set
            .functions()
            .get(function)
            .map(|function| function.signature.type_info().instantiate(type_arguments))
            .ok_or_else(|| {
                EmitError::malformed(format!("no function {function}"), origin)
            })
    }

    fn member_type(
        &self,
        call: &ContractCall,
        origin: &SourceOrigin,
    ) -> Result<Type, EmitError> {
        let member = self
            .set
            .interfaces()
            .get(&call.interface)
            .and_then(|interface| interface.member(&call.member))
            .ok_or_else(|| {
                EmitError::malformed(format!("no member `{}`", call.member), origin)
            })?;
        let binders = call
            .arguments
            .iter()
            .chain(&call.member_arguments)
            .cloned()
            .collect::<Vec<_>>();
        Ok(member.signature.type_info().substitute(&Substitution {
            binders: &binders,
            self_type: Some(&call.receiver),
            associated: Some(&call.choices),
        }))
    }

    /// The type of a call's callee, open in the binders of its activation.
    pub(crate) fn callee_type(
        &self,
        target: &CallTarget,
        origin: &SourceOrigin,
    ) -> Result<Type, EmitError> {
        match target {
            CallTarget::Direct {
                function,
                type_arguments,
            } => self.function_type(*function, type_arguments, origin),
            CallTarget::Value(callee) => self.type_of(callee),
            CallTarget::Contract(call) => self.member_type(call, origin),
        }
    }

    /// The type of the field `field` of a record of type `record`.
    fn field_type(
        &self,
        record: &Type,
        field: &str,
        origin: &SourceOrigin,
    ) -> Result<Type, EmitError> {
        let found = match record {
            Type::Record(fields) => fields.get(field).cloned(),
            Type::Declared(id, arguments) => {
                match self.set.types().definition(id).map(|d| d.body()) {
                    Some(TypeBody::Record(fields)) => {
                        fields.iter().find_map(|(name, ty)| {
                            (name == field).then(|| ty.instantiate(arguments))
                        })
                    }
                    _ => None,
                }
            }
            _ => None,
        };
        found.ok_or_else(|| {
            EmitError::malformed(format!("{record} has no field `{field}`"), origin)
        })
    }

    /// Whether constructing `value_type` checks an invariant with a
    /// consequence, which makes the construction fallible.
    pub(crate) fn is_fallible(&self, value_type: &Type) -> bool {
        matches!(value_type, Type::Declared(id, _)
            if self
                .set
                .types()
                .definition(id)
                .and_then(|definition| definition.invariant())
                .is_some_and(|invariant| invariant.consequence.is_some()))
    }

    /// The static type of `node`, open in the binders of its activation.
    pub(crate) fn type_of(&self, node: &Node) -> Result<Type, EmitError> {
        let library = self.set.library();
        let mut node = node;
        // Follow the tail of sequences and `let`s without recursing.
        loop {
            match &node.expr {
                Expr::Sequence(statements) => {
                    node = statements.last().ok_or_else(|| {
                        EmitError::malformed("an empty sequence", &node.origin)
                    })?;
                }
                Expr::Let { body, .. } | Expr::LetElse { body, .. } => node = body,
                _ => break,
            }
        }
        let origin = &node.origin;
        Ok(match &node.expr {
            Expr::Literal(constant) => constant.value_type().clone(),
            Expr::Intrinsic {
                intrinsic,
                type_arguments,
                ..
            } => intrinsic
                .row()
                .result
                .instantiate(library, type_arguments)
                .map_err(|error| EmitError::malformed(error.to_string(), origin))?,
            Expr::Variable { value_type, .. } | Expr::Captured { value_type, .. } => {
                value_type.clone()
            }
            Expr::Global(index) => self
                .set
                .globals()
                .get(*index)
                .map(|global| global.value_type.clone())
                .ok_or_else(|| {
                    EmitError::malformed(format!("no module value {index}"), origin)
                })?,
            Expr::Function {
                function,
                type_arguments,
            } => self.function_type(*function, type_arguments, origin)?,
            Expr::Closure { closure, .. } => closure.value_type.clone(),
            Expr::Match { value_type, .. } => value_type.clone(),
            Expr::If {
                then_branch,
                else_branch,
                ..
            } => match else_branch {
                None => Type::Void,
                Some(else_branch) => {
                    let then_type = self.type_of(then_branch)?;
                    if then_type == Type::Never {
                        self.type_of(else_branch)?
                    } else {
                        then_type
                    }
                }
            },
            Expr::Construct { value_type, .. } => {
                if self.is_fallible(value_type) {
                    return Err(EmitError::unsupported("fallibleConstructor", origin));
                }
                value_type.clone()
            }
            Expr::Field { record, field } => {
                self.field_type(&self.type_of(record)?, field, origin)?
            }
            Expr::Lookup { collection, .. } => {
                let collection = self.type_of(collection)?;
                let item = if let Type::List(element) = &collection {
                    element.as_ref().clone()
                } else if library.arguments_of(Library::Str, &collection).is_some() {
                    Type::Char
                } else if library.arguments_of(Library::Bytes, &collection).is_some() {
                    Type::U8
                } else if let Some([_, value]) =
                    library.arguments_of(Library::Dict, &collection)
                {
                    value.clone()
                } else {
                    return Err(EmitError::malformed(
                        format!("a lookup on {collection}"),
                        origin,
                    ));
                };
                self.library_type(Library::Option, vec![item], origin)?
            }
            Expr::Call { target, .. } | Expr::TailCall { target, .. } => {
                match self.callee_type(target, origin)? {
                    Type::Fn(signature) => signature.result,
                    Type::Proc(_) => {
                        return Err(EmitError::unsupported("proc", origin));
                    }
                    other => {
                        return Err(EmitError::malformed(
                            format!("a call of {other}"),
                            origin,
                        ));
                    }
                }
            }
            Expr::Return(_) => Type::Never,
            Expr::Try(value) => {
                let carrier = self.type_of(value)?;
                library
                    .arguments_of(Library::Option, &carrier)
                    .or_else(|| library.arguments_of(Library::Result, &carrier))
                    .and_then(<[Type]>::first)
                    .cloned()
                    .ok_or_else(|| {
                        EmitError::malformed(format!("a try on {carrier}"), origin)
                    })?
            }
            Expr::WidenToInterface { value_type, .. } => value_type.clone(),
            Expr::WidenAtom(_) => Type::Atom,
            Expr::Default(value_type) => value_type.clone(),
            Expr::Sequence(_) | Expr::Let { .. } | Expr::LetElse { .. } => {
                return Err(EmitError::malformed("an unreachable node form", origin));
            }
        })
    }
}

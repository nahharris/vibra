//! The checked IR of Vibra: the types, values, and expressions a checked
//! module set is made of, the canonical encodings of types and values, the
//! closed `@compiler` intrinsic registry, the host registry, the boundary
//! every host call passes ([`authority`]), the source origins of nodes, and
//! the arena layout that toolchain-owned native code reads.
//!
//! The authority is `docs/spec`: the type chapter for [`Type`] and the
//! declarations, the effects chapter for where an `await` stands, rows and
//! their ceilings, and host bindings, the runtime chapter for evaluation,
//! the encodings, the registry, and the traps. Checking a program is not
//! done here; this
//! crate validates what a checker built, so a checker defect is an
//! [`IrError`] and never an execution.

use std::fmt;

pub mod arena;
pub mod authority;
mod check;
mod conformance;
mod constant;
mod effects;
mod encode;
mod expr;
pub mod external;
mod instantiation;
mod library;
mod module_set;
mod nominal;
mod origin;
mod pattern;
mod program;
mod signature;
mod trap;
mod types;
mod value;

pub use conformance::select_implementation;
pub use constant::Constant;
pub use effects::EffectFault;
pub use encode::{
    CANONICAL_NAN_F32, CANONICAL_NAN_F64, EncodeError, TypeTable, encode_open_type,
    encode_type, encode_value, type_document, value_document,
};
pub use expr::{
    Arguments, CallTarget, ClosureBody, Construction, ContractCall, ExitGuard, Expr,
    Guard, Node,
};
pub use library::{Library, LibraryTypes};
pub use module_set::{
    BodyContext, Callable, CheckedFunction, CheckedGlobal, CheckedModuleSet, Facts,
    Flow, FunctionBody, HostBinding, ModuleSetParts,
};
pub use nominal::{
    Case, Implementation, InterfaceDefinition, InterfaceMember, Invariant, TypeBody,
    TypeDefinition, Visibility,
};
pub use origin::SourceOrigin;
pub use pattern::{MatchArm, Pattern};
pub use program::CheckedProgram;
pub use signature::{
    Binder, Bound, ImplementationRecord, InterfaceKey, SignatureRecord,
};
pub use trap::Trap;
pub use types::{
    EntityRef, FnType, IntentType, NamedTypes, OpenScope, ProcType, Row, Substitution,
    Type, TypeId,
};
pub use value::{ObservedValue, Value};

/// A defect in what a checker built: the IR does not hold together.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IrError {
    /// An entity reference whose path or identity is malformed.
    InvalidIdentity(String),
    /// A name that is not an identifier.
    InvalidName {
        /// What was being named.
        what: &'static str,
        /// The name.
        name: String,
    },
    /// A name or identity given twice where it must be unique.
    Duplicate {
        /// What was given twice.
        what: &'static str,
        /// Its name.
        name: String,
    },
    /// A malformed type or declaration.
    InvalidType(String),
    /// A binder reference, `Self`, or an association slot where it is not
    /// valid.
    MisplacedOpenForm(String),
    /// A declared type the module set does not define.
    UnknownType(String),
    /// An interface the module set does not define.
    UnknownInterface(String),
    /// A function index past the module set's functions.
    UnknownFunction(usize),
    /// A module-value index past the module set's module values.
    UnknownGlobal(usize),
    /// A library declaration that no identity is bound to.
    UnboundLibrary(Library),
    /// A slot count smaller than the argument slots need.
    InvalidSlotCount {
        /// The slots needed.
        needed: usize,
        /// The slots declared.
        declared: usize,
    },
    /// A read of a slot that nothing bound.
    UnsetSlot(usize),
    /// A slot past the activation's slots.
    SlotOutOfRange {
        /// The slot.
        slot: usize,
        /// The number of slots.
        count: usize,
    },
    /// A read of a slot at a type other than the one it holds.
    SlotTypeMismatch {
        /// The slot.
        slot: usize,
        /// The type it holds.
        expected: Box<Type>,
        /// The type it was read at.
        actual: Box<Type>,
    },
    /// The wrong number of arguments, type arguments, fields, or
    /// components.
    ArityMismatch {
        /// What was counted.
        what: &'static str,
        /// The number expected.
        expected: usize,
        /// The number given.
        actual: usize,
    },
    /// A value of one type where another is expected.
    TypeMismatch {
        /// The type expected.
        expected: Box<Type>,
        /// The type given.
        actual: Box<Type>,
    },
    /// A node where it cannot be: a `return`, `try`, or tail call in a
    /// guard or outside a callable, or a default outside a labelled
    /// argument.
    MisplacedNode(&'static str),
    /// Any other malformed expression.
    InvalidExpression(String),
    /// An entry that cannot start a run.
    InvalidEntry(String),
    /// A module value whose initialization reads itself, through any chain
    /// of reads and calls.
    InitializerCycle {
        /// The module value's index.
        global: usize,
        /// Its name.
        name: String,
    },
    /// A generic function that reaches itself at a type built around one
    /// of its own type parameters, through any chain of instantiations.
    ExpandingInstantiation {
        /// The index of the function the instantiation is in.
        function: usize,
        /// Its name.
        name: String,
        /// The name of the function it instantiates.
        callee: String,
    },
    /// A rule of the effects chapter that the IR does not keep: an `await`
    /// out of place or outside its ceiling, a row entry that is no root, or
    /// a host binding that is not its registry row's.
    Effect {
        /// The function, or the module value whose initializer it is, that
        /// the failure is in; `None` for a type outside every body.
        owner: Option<String>,
        /// What is wrong, boxed so that every `Result` of the crate stays
        /// small on the stack.
        fault: Box<EffectFault>,
    },
    /// A type that has no encoding where one is required: it is, or holds,
    /// an operation interface value type, a `Proc`, or an `Intent`.
    NoEncoding {
        /// Where the encoding is required: a constant, or a dict key.
        what: &'static str,
        /// The type.
        ty: Box<Type>,
    },
}

impl IrError {
    /// This error, attributed to `owner` when it is an effect failure that
    /// names no owner yet.
    #[must_use]
    pub(crate) fn owned_by(self, owner: &str) -> Self {
        match self {
            Self::Effect { owner: None, fault } => Self::Effect {
                owner: Some(owner.to_owned()),
                fault,
            },
            other => other,
        }
    }
}

impl fmt::Display for IrError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidIdentity(message)
            | Self::InvalidType(message)
            | Self::MisplacedOpenForm(message)
            | Self::InvalidExpression(message)
            | Self::InvalidEntry(message) => formatter.write_str(message),
            Self::InvalidName { what, name } => {
                write!(formatter, "`{name}` is not a valid {what} name")
            }
            Self::Duplicate { what, name } => {
                write!(formatter, "the {what} `{name}` is given twice")
            }
            Self::UnknownType(path) => write!(formatter, "no type `@{path}`"),
            Self::UnknownInterface(path) => write!(formatter, "no interface `@{path}`"),
            Self::UnknownFunction(index) => write!(formatter, "no function {index}"),
            Self::UnknownGlobal(index) => write!(formatter, "no module value {index}"),
            Self::UnboundLibrary(library) => {
                write!(formatter, "no type is bound to `{}`", library.name())
            }
            Self::InvalidSlotCount { needed, declared } => {
                write!(formatter, "{declared} slots where {needed} are needed")
            }
            Self::UnsetSlot(slot) => {
                write!(formatter, "slot {slot} is read before it is set")
            }
            Self::SlotOutOfRange { slot, count } => {
                write!(formatter, "slot {slot} of an activation with {count}")
            }
            Self::SlotTypeMismatch {
                slot,
                expected,
                actual,
            } => write!(formatter, "slot {slot} holds {expected}, read as {actual}"),
            Self::ArityMismatch {
                what,
                expected,
                actual,
            } => write!(formatter, "{actual} {what} where {expected} are expected"),
            Self::TypeMismatch { expected, actual } => {
                write!(formatter, "expected {expected}, found {actual}")
            }
            Self::MisplacedNode(what) => formatter.write_str(what),
            Self::InitializerCycle { name, .. } => {
                write!(formatter, "the initializer of `{name}` reads itself")
            }
            Self::ExpandingInstantiation { name, callee, .. } => write!(
                formatter,
                "`{name}` instantiates `{callee}` at a type built around a type parameter that the instantiation reaches again"
            ),
            Self::Effect {
                owner: Some(owner),
                fault,
            } => write!(formatter, "in `{owner}`: {fault}"),
            Self::Effect { owner: None, fault } => write!(formatter, "{fault}"),
            Self::NoEncoding { what, ty } => write!(
                formatter,
                "{what} of type {ty}, which holds a grant, a procedure, or a plan and has no encoding"
            ),
        }
    }
}

impl std::error::Error for IrError {}

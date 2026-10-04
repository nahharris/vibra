//! The forms of the checked IR the emitter does not lower yet, and the typed
//! error that names them.

use std::fmt;

use vibra_ir::SourceOrigin;

/// One construct of a checked program that a module can be asked to contain.
///
/// A form is either lowered or named by [`NotLowered`]; the emitter never
/// approximates one. Each later step of Stage 4A moves forms from the second
/// kind to the first, and the order of the variants is the order of the
/// checked program's own structure, not an order of delivery.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Form {
    /// A program with more functions than the 32-bit indices of a module admit.
    ModuleSize,
    /// A module-level value.
    ModuleValue,
    /// A function with a fixed, labelled, or variadic parameter.
    Parameters,
    /// A function whose result is not `void`.
    NonVoidResult,
    /// One of the closed verified test assertion functions.
    TestAssertion,
    /// A function that implements a contract member.
    ContractImplementation,
    /// A literal other than the `void` literal.
    Literal,
    /// A call of a compiler registry operation.
    External,
    /// An omitted labelled argument, resolved to the callee's default.
    Default,
    /// A read of an activation slot.
    Variable,
    /// A read of a module-level value.
    Global,
    /// A module-level function used as a value.
    Function,
    /// A `lambda` with its closure environment.
    Closure,
    /// A read of a closure-environment slot.
    Captured,
    /// An immutable binding.
    Let,
    /// A `match`.
    Match,
    /// A widening to a union or `atom`.
    Widen,
    /// A `try`.
    Try,
    /// A `return`.
    Return,
    /// An `if`.
    If,
    /// A call.
    Call,
    /// A record constructor.
    Record,
    /// An enum variant constructor.
    Variant,
    /// A wrapper constructor.
    Wrap,
    /// A record projection.
    Project,
    /// A tuple constructor.
    Tuple,
    /// A tuple projection.
    TupleProject,
    /// An array built from element operands.
    Array,
    /// A dict built from entries.
    Dict,
    /// A checked lookup in an array, dict, `str`, or `bytes`.
    Lookup,
}

impl Form {
    /// The form's name as a diagnostic or a report spells it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::ModuleSize => "module-size",
            Self::ModuleValue => "module-value",
            Self::Parameters => "parameters",
            Self::NonVoidResult => "non-void-result",
            Self::TestAssertion => "test-assertion",
            Self::ContractImplementation => "contract-implementation",
            Self::Literal => "literal",
            Self::External => "external",
            Self::Default => "default",
            Self::Variable => "variable",
            Self::Global => "global",
            Self::Function => "function",
            Self::Closure => "closure",
            Self::Captured => "captured",
            Self::Let => "let",
            Self::Match => "match",
            Self::Widen => "widen",
            Self::Try => "try",
            Self::Return => "return",
            Self::If => "if",
            Self::Call => "call",
            Self::Record => "record",
            Self::Variant => "variant",
            Self::Wrap => "wrap",
            Self::Project => "project",
            Self::Tuple => "tuple",
            Self::TupleProject => "tuple-project",
            Self::Array => "array",
            Self::Dict => "dict",
            Self::Lookup => "lookup",
        }
    }
}

impl fmt::Display for Form {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// One use of a form the emitter does not lower, at its source origin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnloweredForm {
    form: Form,
    detail: Option<&'static str>,
    origin: Option<SourceOrigin>,
}

impl UnloweredForm {
    pub(crate) const fn new(form: Form, origin: Option<SourceOrigin>) -> Self {
        Self {
            form,
            detail: None,
            origin,
        }
    }

    /// The same use with the name of what further distinguishes it.
    pub(crate) const fn with_detail(mut self, detail: &'static str) -> Self {
        self.detail = Some(detail);
        self
    }

    /// The form.
    #[must_use]
    pub const fn form(&self) -> Form {
        self.form
    }

    /// What further distinguishes the use within its form, for the two forms
    /// whose lowering differs by case: the registry symbol of an
    /// [`Form::External`], whose rows are lowered by different steps, and the
    /// kind of a [`Form::Call`] (`direct`, `indirect`, or `contract`, each also
    /// `tail-` when it is an explicit tail transfer). `None` for every other
    /// form.
    #[must_use]
    pub const fn detail(&self) -> Option<&'static str> {
        self.detail
    }

    /// The first source origin at which the program uses the form, when the
    /// form has one.
    #[must_use]
    pub const fn origin(&self) -> Option<&SourceOrigin> {
        self.origin.as_ref()
    }
}

/// The program uses forms the emitter does not lower, so it produced no
/// module. It is an error value, never a partial or approximate module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotLowered {
    forms: Vec<UnloweredForm>,
}

impl NotLowered {
    /// Every distinct use not lowered, in [`Form`] order and then by detail,
    /// each with the first origin at which the program uses it. It is never
    /// empty.
    #[must_use]
    pub fn forms(&self) -> &[UnloweredForm] {
        &self.forms
    }

    pub(crate) fn single(form: Form) -> Self {
        Self {
            forms: vec![UnloweredForm::new(form, None)],
        }
    }

    pub(crate) fn from_uses(mut uses: Vec<UnloweredForm>) -> Option<Self> {
        if uses.is_empty() {
            return None;
        }
        // A stable sort keeps the first use of each form first, and the order
        // is independent of how the program was traversed.
        uses.sort_by_key(|used| (used.form, used.detail));
        uses.dedup_by_key(|used| (used.form, used.detail));
        Some(Self { forms: uses })
    }
}

impl fmt::Display for NotLowered {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("not lowered:")?;
        for (index, used) in self.forms.iter().enumerate() {
            let separator = if index == 0 { " " } else { ", " };
            write!(formatter, "{separator}{}", used.form)?;
            if let Some(detail) = used.detail {
                write!(formatter, " `{detail}`")?;
            }
            if let Some(origin) = &used.origin {
                write!(formatter, " at {}:{:?}", origin.source_id(), origin.span())?;
            }
        }
        Ok(())
    }
}

impl std::error::Error for NotLowered {}

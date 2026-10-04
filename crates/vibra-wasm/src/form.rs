//! The forms of the checked IR the emitter does not lower yet, and the typed
//! error that names them.

use std::fmt;

use vibra_ir::SourceOrigin;

/// One construct of a checked program that a module can be asked to contain.
///
/// A form is either lowered or named by [`NotLowered`]; the emitter never
/// approximates one. Each later step of Stage 4A moves forms from the second
/// kind to the first, and the order of the variants is the order of the checked
/// program's own structure, not an order of delivery. A form whose lowering
/// differs by case carries a detail naming the case: see
/// [`UnloweredForm::detail`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Form {
    /// A program with more functions than the 32-bit indices of a module admit.
    ModuleSize,
    /// A function with a variadic parameter.
    Parameters,
    /// A value of a type that no lowered value kind represents yet: an `array`
    /// or `dict`, or an interface value.
    Type,
    /// One of the closed verified test assertion functions.
    TestAssertion,
    /// A call of a compiler registry operation.
    External,
    /// A `match`.
    Match,
    /// A `try`.
    Try,
    /// A wrapper constructor over a builtin text type.
    Wrap,
    /// A call of a contract member, which dispatches from a run-time type.
    Call,
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
            Self::Parameters => "parameters",
            Self::Type => "type",
            Self::TestAssertion => "test-assertion",
            Self::External => "external",
            Self::Match => "match",
            Self::Try => "try",
            Self::Wrap => "wrap",
            Self::Call => "call",
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

    /// What further distinguishes the use within its form, for the forms whose
    /// lowering differs by case and is owned by different steps: the registry
    /// symbol of an [`Form::External`]; the kind of a [`Form::Call`]
    /// (`contract`, also `tail-contract` when it is an explicit tail
    /// transfer); the kind of type of a [`Form::Type`] (`array`, `dict`, or
    /// `interface`); and `variadic` for a [`Form::Parameters`]. `None` for
    /// every other form.
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

    /// A value of a type no lowered kind represents, named by its kind.
    pub(crate) fn type_of(kind: &'static str, origin: Option<SourceOrigin>) -> Self {
        Self {
            forms: vec![UnloweredForm::new(Form::Type, origin).with_detail(kind)],
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

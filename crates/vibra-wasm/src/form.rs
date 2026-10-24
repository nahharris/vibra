//! What the emitter reports when it cannot produce a module, and the typed
//! error that names it.
//!
//! Since Milestone 4 Step 11 the emitter lowers every form of a checked
//! program, so no valid construct is named here. The error remains for what the
//! checker cannot have produced: a program larger than the 32-bit indices of a
//! module, an entry that takes a parameter (a checked entry has none), and a
//! checked-IR shape that violates an invariant the checker guarantees, such as
//! a pattern over a type that has no shape, a contract call with no receiver,
//! or a registry row the closed tables do not know. A hand-built program can
//! contain those; a program the checker accepts never does, and the tests of
//! `tests/lowers_every_form_m4_step11.rs` hold the classifier to that.

use std::fmt;

use vibra_ir::SourceOrigin;

/// The kinds of program the emitter refuses to lower. The order of the variants
/// is the order a report lists them in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Form {
    /// A program with more functions than the 32-bit indices of a module admit.
    ModuleSize,
    /// An entry with parameters, which a checked entry never has (detail
    /// `entry`).
    Parameters,
    /// A checked-IR shape that breaks an invariant the checker guarantees. The
    /// detail names it: the kind of type or the registry symbol the lowering
    /// found no row for, `contract` for a contract call with no receiver,
    /// `conversion` for a result conversion between shapes that cannot differ,
    /// and `user-key` for a dict key with no order.
    Malformed,
}

impl Form {
    /// The form's name as a diagnostic or a report spells it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::ModuleSize => "module-size",
            Self::Parameters => "parameters",
            Self::Malformed => "malformed",
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

    /// What further distinguishes the use within its form: see the variants of
    /// [`Form`]. `None` for a [`Form::ModuleSize`].
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
            forms: vec![UnloweredForm::new(Form::Malformed, origin).with_detail(kind)],
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

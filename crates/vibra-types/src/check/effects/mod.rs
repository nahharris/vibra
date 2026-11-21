//! The checker's procedures, rows, and grants: every decision about a
//! `proc`, a `performs` row, a `Proc` or `Intent` type, an `await`, a grant
//! type, and the entry of a target.
//!
//! `docs/spec/03-effects.md` is the authority for every rule these files
//! check. The shared files of the checker (`declare.rs`, `interfaces.rs`,
//! `expr.rs`, `lower.rs`, `testing.rs`) take no such decision of their own:
//! each calls one hook here at the place the decision is taken, and the hook
//! answers. A hook whose step has not landed reports the `@tool.unavailable`
//! the form has, at the same span and with the same message, so the forms
//! stay unavailable until that step lifts them.
//!
//! Each file belongs to the step of `docs/roadmap/v0.0.2/m2/` that fills it;
//! a later step that fixes it says so in its guide.
//!
//! | File | Hooks | Step |
//! | --- | --- | --- |
//! | [`rows`] | the resolution of a written row | 08 |
//! | [`procs`] | [`procs::declare`], [`procs::module_headers`], [`procs::header`], [`procs::describe`], [`procs::apply`], [`procs::await_expr`], [`procs::lower`] | 05 |
//! | [`procs`] | [`procs::test_header`], [`procs::test_context`] | 07, 21 |
//! | [`members`] | [`members::contract_member`], [`members::impl_member`] | 08, 19 |
//! | [`analysis`] | [`analysis::check_bodies`] | 09 |
//! | [`values`] | [`values::written_type`], [`values::anonymous`] | 10 |
//! | [`grants`] | [`grants::check_dict_key`], [`grants::check_equatable`] | 22 |
//! | [`entry`] | the entry shape of a target | 25 |
//!
//! Step 08 also writes the rows the hooks of [`procs`] read, and steps 12
//! and 24 fix the files of their reviews.

pub(crate) mod analysis;
pub(crate) mod entry;
pub(crate) mod grants;
pub(crate) mod members;
pub(crate) mod procs;
pub(crate) mod rows;
pub(crate) mod values;

/// What a hook did with the form the shared checker handed it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Hook {
    /// Nothing: the shared checker reads the form as it reads any other.
    Continue,
    /// The hook reported the form, and the shared checker skips it.
    Reported,
}

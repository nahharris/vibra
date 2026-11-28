//! The checker's procedures, rows, and grants: every decision about a
//! `proc`, a `performs` row, a `Proc` or `Intent` type, an `await`, a grant
//! type, and the entry of a target.
//!
//! `docs/spec/03-effects.md` is the authority for every rule these files
//! check. The shared files of the checker (`declare.rs`, `interfaces.rs`,
//! `expr.rs`, `lower.rs`, `testing.rs`) take no such decision of their own:
//! each calls one hook here at the place the decision is taken, and the hook
//! answers. A form a hook does not support is the `@tool.unavailable` the
//! form has, at its span.
//!
//! | File | Hooks |
//! | --- | --- |
//! | [`rows`] | [`rows::resolve`], [`rows::section`] |
//! | [`procs`] | [`procs::declare`], [`procs::identity_kind`], [`procs::header`], [`procs::describe`], [`procs::callable_type`], [`procs::applied`], [`procs::apply`], [`procs::named_as_value`], [`procs::await_expr`], [`procs::finish_await`], [`procs::stub`], [`procs::lower`], [`procs::test_header`], [`procs::test_context`] |
//! | [`members`] | [`members::contract_member`], [`members::contract_row`], [`members::identity_kind`], [`members::member_type`], [`members::impl_member`], [`members::applied`], [`members::named_as_value`] |
//! | [`analysis`] | [`analysis::check_bodies`] |
//! | [`values`] | [`values::written_type`], [`values::anonymous`] |
//! | [`grants`] | [`grants::check_constant`], [`grants::is_encodable`] |
//! | [`entry`] | [`entry::check_targets`], [`entry::entry_signature`] |
//!
//! The hooks of [`procs`] read rows through [`rows`].

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

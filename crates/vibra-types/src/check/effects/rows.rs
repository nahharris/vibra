//! Rows as written: the entries of a `performs` section, of a `Proc` type,
//! and of an `Intent` type, resolved to the operation interfaces they name.
//!
//! `docs/spec/03-effects.md`, "Operation interfaces" and "Effect ceilings".
//! Step 08 fills this file: until then no hook reads a row, and every form
//! that writes a non-empty row is `@tool.unavailable` where the hook that
//! meets it reports it.

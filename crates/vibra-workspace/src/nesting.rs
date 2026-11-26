//! The source nesting limit of the published profile.
//!
//! `docs/spec/05-tooling.md`, "Limits": `m1Default` allows a token of a
//! source or data document to be nested 1,024 levels. Every document a
//! command reads past the reader, each captured module and each project
//! file, is measured once after it is read ([`vibra_syntax::nesting`]). A
//! document nested deeper is `@tool.limitExceeded` at its first token past
//! the limit, the observation is `unknown` and incomplete, and no later
//! stage reads the snapshot. The reader and the formatter read any depth.
//!
//! The limit is what bounds the host stack of the stages after the reader:
//! resolution, checking, lowering, and the interpreter recurse into the
//! nested forms of the tree, and a command runs them on a thread whose stack
//! is sized for it. They loop along a sequence, the statements of a body,
//! the elements of a list, or the arms of a `match`, so its length costs no
//! stack.

use vibra_diagnostics::{Diagnostic, DiagnosticCode};
use vibra_interp::LimitProfile;
use vibra_syntax::{Document, nesting};

/// The name `limits.reached` gives the source nesting limit.
pub const SOURCE_NESTING: &str = "sourceNesting";

/// The stack of the thread a command runs its stages on.
///
/// The stages after the reader (resolution, checking, lowering, the
/// interpreter) recurse into nested forms, so their host stack grows with
/// the nesting of what they read, and loop along sequences, so it does not
/// grow with their length (the host tests `sequences`). No document nested
/// past [`source_nesting_limit`] reaches them, so a stack sized for the
/// limit suffices for every input.
///
/// The size is at least four times the deepest stack any stage needs in a
/// debug build at the limit. The host test `stack::stack_per_level` of this
/// crate (ignored by default) finds that by bisection, for each stage and
/// each nesting shape, to 4 KiB. On Windows 11, x86-64, the deepest is
/// resolution and checking of 1,024 nested `do` blocks: 5,060 KiB, about
/// 4,900 bytes a level (nested `if`s, lists, and call arguments need 3,800
/// to 5,060 KiB). Four times that is 19.8 MiB; the stack is 32 MiB.
/// `stack::stages_fit_a_quarter_of_the_command_stack` runs every stage on
/// every shape at the limit on a quarter of this stack, so a stage that
/// grows past the margin fails a test before it overflows a command.
pub const COMMAND_STACK: usize = 32 * 1024 * 1024;

/// The most levels the published profile allows.
#[must_use]
pub const fn source_nesting_limit() -> usize {
    LimitProfile::M1_DEFAULT.source_nesting
}

/// `@tool.limitExceeded` at the first token of `document` nested more
/// levels than the profile allows, with the document's source identity;
/// `None` when the document is within the limit.
#[must_use]
pub fn nesting_fault(document: &Document) -> Option<Diagnostic> {
    let limit = source_nesting_limit();
    let span = nesting(document, limit).first_over?;
    let diagnostic = Diagnostic::new(
        DiagnosticCode::ToolLimitExceeded,
        span,
        format!(
            "this is nested more than {limit} levels, the source nesting limit of `{}`",
            LimitProfile::M1_DEFAULT.name
        ),
    )
    .with_note("no later stage reads a document nested past the limit");
    Some(match document.source_id() {
        Some(source_id) => diagnostic.with_source_id(source_id),
        None => diagnostic,
    })
}

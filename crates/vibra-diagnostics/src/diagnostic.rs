//! The structured diagnostic record.
//!
//! `docs/spec/07-diagnostics-and-conformance.md` fixes the contents: schema
//! version, code, level, message, primary source span, related spans, notes,
//! and zero or more fixes.
//!
//! Two of those are not stored here.
//!
//! The **level** is derived from the code rather than stored, because the
//! chapter requires every emitted diagnostic to carry the level its code is
//! registered with. Deriving it makes that true by construction; storing it
//! would create a second place for the two to disagree.
//!
//! The **schema version** belongs to the wire format and is applied by
//! `vibra-schema` when a diagnostic is serialized. It describes the JSON
//! contract, not the language fact, and storing a constant on every in-memory
//! record would put a tooling concern inside the language crates — which the
//! roadmap's dependency direction forbids.
//!
//! # Documents
//!
//! Spans here carry no document identity, because everything in milestone 1
//! diagnoses one document at a time. Diagnostics that relate spans across
//! modules arrive with the type system in milestone 3, and the workspace
//! service that owns document identity does not exist yet. Adding an
//! identifier now would mean guessing its representation before its only
//! consumer is designed.

use crate::{ByteSpan, DiagnosticCode, Level};

/// An additional span that helps explain a diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelatedSpan {
    /// Where the related construct is.
    pub span: ByteSpan,
    /// What that construct contributes to the diagnostic.
    pub message: String,
}

impl RelatedSpan {
    /// A related span with its explanation.
    #[must_use]
    pub fn new(span: ByteSpan, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
        }
    }
}

/// The revision of a document a fix was computed against.
///
/// Opaque to this crate. The workspace produces it, and the edit engine
/// rejects a fix whose expected revision no longer matches, which is what
/// makes an apply fail rather than write against a stale document.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocumentRevision(String);

impl DocumentRevision {
    /// A revision identifier.
    #[must_use]
    pub fn new(identifier: impl Into<String>) -> Self {
        Self(identifier.into())
    }

    /// The identifier as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A fix offered for a diagnostic.
///
/// The fix describes itself and the document state it was computed against; it
/// does not carry edits. Producing edits is the edit engine's job, and a fix
/// that carried them would be a second, unversioned edit format alongside the
/// transactional plans the tooling chapter defines.
///
/// A fix is only offered for a code whose registered capability is
/// [`FixCapability::Safe`](crate::FixCapability::Safe).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fix {
    description: String,
    is_safe: bool,
    expected_revision: DocumentRevision,
}

impl Fix {
    /// A fix that is safe to apply without review.
    #[must_use]
    pub fn safe(
        description: impl Into<String>,
        expected_revision: DocumentRevision,
    ) -> Self {
        Self {
            description: description.into(),
            is_safe: true,
            expected_revision,
        }
    }

    /// A fix that changes meaning or needs review before it is applied.
    ///
    /// V1 has no command that applies one. It exists so a fix can be offered
    /// and described without `vibra edit fix` treating it as applicable.
    #[must_use]
    pub fn unsafe_to_apply(
        description: impl Into<String>,
        expected_revision: DocumentRevision,
    ) -> Self {
        Self {
            description: description.into(),
            is_safe: false,
            expected_revision,
        }
    }

    /// What the fix does.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Whether the fix may be applied without review.
    #[must_use]
    pub const fn is_safe(&self) -> bool {
        self.is_safe
    }

    /// The document revision the fix was computed against.
    #[must_use]
    pub const fn expected_revision(&self) -> &DocumentRevision {
        &self.expected_revision
    }
}

/// One structured diagnostic.
///
/// Built with [`Diagnostic::new`] and refined with the `with_` methods, so the
/// code, primary span, and message — the three fields every diagnostic has —
/// cannot be omitted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    code: DiagnosticCode,
    message: String,
    primary_span: ByteSpan,
    related: Vec<RelatedSpan>,
    notes: Vec<String>,
    fixes: Vec<Fix>,
}

impl Diagnostic {
    /// A diagnostic for `code` at `primary_span`.
    ///
    /// The message helps a person. Tests assert codes, levels, spans, and
    /// related identities rather than its wording.
    #[must_use]
    pub fn new(
        code: DiagnosticCode,
        primary_span: ByteSpan,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            primary_span,
            related: Vec::new(),
            notes: Vec::new(),
            fixes: Vec::new(),
        }
    }

    /// Adds a related span.
    #[must_use]
    pub fn with_related(mut self, span: ByteSpan, message: impl Into<String>) -> Self {
        self.related.push(RelatedSpan::new(span, message));
        self
    }

    /// Adds a note.
    #[must_use]
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    /// Adds a fix.
    #[must_use]
    pub fn with_fix(mut self, fix: Fix) -> Self {
        self.fixes.push(fix);
        self
    }

    /// The diagnostic's code.
    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        self.code
    }

    /// The level registered for this diagnostic's code.
    #[must_use]
    pub const fn level(&self) -> Level {
        self.code.level()
    }

    /// The human-facing message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Where the diagnostic is reported.
    #[must_use]
    pub const fn primary_span(&self) -> ByteSpan {
        self.primary_span
    }

    /// Additional spans that explain the diagnostic.
    #[must_use]
    pub fn related(&self) -> &[RelatedSpan] {
        &self.related
    }

    /// Additional explanation carrying no span.
    #[must_use]
    pub fn notes(&self) -> &[String] {
        &self.notes
    }

    /// Fixes offered for this diagnostic.
    #[must_use]
    pub fn fixes(&self) -> &[Fix] {
        &self.fixes
    }

    /// Whether this diagnostic rejects the construct it reports.
    #[must_use]
    pub fn is_error(&self) -> bool {
        self.level() == Level::Error
    }
}

#[cfg(test)]
mod tests {
    use super::{Diagnostic, DocumentRevision, Fix};
    use crate::{ByteSpan, DiagnosticCode, FixCapability, Level};

    #[test]
    fn a_diagnostic_keeps_its_code_span_and_message() {
        let diagnostic = Diagnostic::new(
            DiagnosticCode::SyntaxUnmatchedDelimiter,
            ByteSpan::new(4, 5),
            "this list is never closed",
        );

        assert_eq!(diagnostic.code(), DiagnosticCode::SyntaxUnmatchedDelimiter);
        assert_eq!(diagnostic.primary_span(), ByteSpan::new(4, 5));
        assert_eq!(diagnostic.message(), "this list is never closed");
        assert!(diagnostic.related().is_empty());
        assert!(diagnostic.notes().is_empty());
        assert!(diagnostic.fixes().is_empty());
    }

    #[test]
    fn the_level_comes_from_the_registry_and_cannot_be_set() {
        // The chapter requires every emitted diagnostic to carry its code's
        // registered level. Deriving it is what makes that unconditional.
        let error = Diagnostic::new(
            DiagnosticCode::SyntaxRetiredForm,
            ByteSpan::empty_at(0),
            "`while` was retired",
        );
        assert_eq!(error.level(), Level::Error);
        assert!(error.is_error());

        let warning = Diagnostic::new(
            DiagnosticCode::StyleArgumentOrder,
            ByteSpan::empty_at(0),
            "labelled operand precedes a fixed one",
        );
        assert_eq!(warning.level(), Level::Warning);
        assert!(!warning.is_error());
    }

    #[test]
    fn related_spans_and_notes_accumulate_in_order() {
        let diagnostic = Diagnostic::new(
            DiagnosticCode::NameMemberCollision,
            ByteSpan::new(10, 14),
            "two members share a name",
        )
        .with_related(ByteSpan::new(2, 6), "first declared here")
        .with_related(ByteSpan::new(20, 24), "and again here")
        .with_note("member names are flat within one owner");

        assert_eq!(diagnostic.related().len(), 2);
        assert_eq!(diagnostic.related()[0].span, ByteSpan::new(2, 6));
        assert_eq!(diagnostic.related()[0].message, "first declared here");
        assert_eq!(diagnostic.related()[1].span, ByteSpan::new(20, 24));
        assert_eq!(
            diagnostic.notes(),
            ["member names are flat within one owner"]
        );
    }

    #[test]
    fn a_safe_fix_records_its_expected_revision() {
        let revision = DocumentRevision::new("sha256:abc");
        let diagnostic = Diagnostic::new(
            DiagnosticCode::StyleArgumentOrder,
            ByteSpan::new(0, 8),
            "operands are in a noncanonical order",
        )
        .with_fix(Fix::safe("reorder operands canonically", revision.clone()));

        let fix = &diagnostic.fixes()[0];
        assert!(fix.is_safe());
        assert_eq!(fix.description(), "reorder operands canonically");
        assert_eq!(fix.expected_revision(), &revision);
        assert_eq!(revision.as_str(), "sha256:abc");
    }

    #[test]
    fn a_fix_can_declare_itself_unsafe_to_apply() {
        let fix = Fix::unsafe_to_apply(
            "rewrite the call",
            DocumentRevision::new("sha256:def"),
        );
        assert!(!fix.is_safe());
    }

    #[test]
    fn only_a_code_registered_as_fixable_is_given_a_fix() {
        // The invariant the emitting code must respect: a diagnostic offers a
        // fix only where the registry says the compiler can produce one.
        let fixable: Vec<DiagnosticCode> = DiagnosticCode::ALL
            .iter()
            .copied()
            .filter(|code| code.fix_capability() == FixCapability::Safe)
            .collect();

        assert_eq!(fixable, [DiagnosticCode::StyleArgumentOrder]);

        for code in fixable {
            let diagnostic = Diagnostic::new(code, ByteSpan::empty_at(0), "message")
                .with_fix(Fix::safe("fix", DocumentRevision::new("sha256:0")));
            assert_eq!(diagnostic.fixes().len(), 1);
        }
    }

    #[test]
    fn diagnostics_compare_by_value() {
        let build = || {
            Diagnostic::new(
                DiagnosticCode::TypeNotApplicable,
                ByteSpan::new(1, 2),
                "not applicable",
            )
            .with_note("only closed categories are applicable")
        };
        assert_eq!(build(), build());
        assert_ne!(
            build(),
            Diagnostic::new(
                DiagnosticCode::TypeNotApplicable,
                ByteSpan::new(1, 3),
                "not applicable",
            )
        );
    }
}

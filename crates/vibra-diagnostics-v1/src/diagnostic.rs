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
//! wire crate when a diagnostic is serialized. It describes the JSON
//! contract, not the language fact, and storing a constant on every in-memory
//! record would put a tooling concern inside the language crates — which the
//! roadmap's dependency direction forbids.
//!
//! # Documents
//!
//! A span may carry the case-relative source identity that owns it. The
//! identity is optional for the syntax-only APIs, which historically diagnose
//! one document at a time, but conformance and workspace adapters attach it
//! before comparing observations from more than one document. Keeping the
//! identity beside the byte span prevents equal offsets in different files
//! from being flattened into one location.

use crate::{ByteSpan, DiagnosticCode, Level};

/// An additional span that helps explain a diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelatedSpan {
    /// The source identity that owns the span, when known.
    pub source_id: Option<String>,
    /// Where the related construct is.
    pub span: ByteSpan,
    /// What that construct contributes to the diagnostic.
    pub message: String,
}

/// The revision of a document a fix was computed against.
///
/// Opaque to this crate. The workspace produces it, and the edit engine
/// rejects a fix whose expected revision no longer matches, which is what
/// makes an apply fail rather than write against a stale document.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocumentRevision(String);

impl DocumentRevision {
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
    source_id: Option<String>,
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
            source_id: None,
            primary_span,
            related: Vec::new(),
            notes: Vec::new(),
            fixes: Vec::new(),
        }
    }

    /// Attaches the source identity that owns this diagnostic.
    ///
    /// Related spans created before this call inherit the same identity when
    /// they do not already have one. A later workspace phase can attach a
    /// different identity to an individual related span if it crosses a
    /// document boundary.
    #[must_use]
    pub fn with_source_id(mut self, source_id: impl Into<String>) -> Self {
        let source_id = source_id.into();
        self.source_id = Some(source_id.clone());
        for related in &mut self.related {
            if related.source_id.is_none() {
                related.source_id = Some(source_id.clone());
            }
        }
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

    /// The source identity that owns the primary span, when known.
    #[must_use]
    pub fn source_id(&self) -> Option<&str> {
        self.source_id.as_deref()
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
}

#[cfg(test)]
mod tests {
    use super::Diagnostic;
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

        let warning = Diagnostic::new(
            DiagnosticCode::StyleArgumentOrder,
            ByteSpan::empty_at(0),
            "labelled operand precedes a fixed one",
        );
        assert_eq!(warning.level(), Level::Warning);
    }

    #[test]
    fn only_style_argument_order_is_registered_as_safely_fixable() {
        let fixable: Vec<DiagnosticCode> = DiagnosticCode::ALL
            .iter()
            .copied()
            .filter(|code| code.fix_capability() == FixCapability::Safe)
            .collect();

        assert_eq!(fixable, [DiagnosticCode::StyleArgumentOrder]);
    }
    #[test]
    fn diagnostics_compare_by_value() {
        let build = || {
            Diagnostic::new(
                DiagnosticCode::TypeNotApplicable,
                ByteSpan::new(1, 2),
                "not applicable",
            )
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

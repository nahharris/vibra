//! Source origins carried by checked nodes.

use std::sync::Arc;

use vibra_diagnostics::ByteSpan;

/// A source identity and a span: where a checked node came from.
///
/// A trap keeps the origin of the node that raised it, as the runtime
/// chapter's "Traps" requires.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceOrigin {
    /// Shared, so that cloning an origin, which every node carries, never
    /// copies the source identity.
    source_id: Arc<str>,
    span: ByteSpan,
}

impl SourceOrigin {
    /// The origin of `span` in the document `source_id`.
    #[must_use]
    pub fn new(source_id: impl Into<Arc<str>>, span: ByteSpan) -> Self {
        Self {
            source_id: source_id.into(),
            span,
        }
    }

    /// The source document identity.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// The half-open byte span in that document.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

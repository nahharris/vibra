//! The result of resolution and its `@resolved.v2` observation.
//!
//! `docs/spec/07-diagnostics-and-conformance.md`, "Persistent observations":
//! `@resolved.v2` is the closed record of `format`, `package`, `modules`
//! sorted by `path`, `declarations` sorted by `identity` (each with
//! `identity`, `anchor`, `access`, and `span`, and a member of an `impl`
//! block with `member` after its block's anchor), `imports` sorted by span
//! (each with `span`, `path`, `name`, and `to` when the import resolved), and
//! `references` sorted by span (each with `span` and `to`). A span is the
//! record `{sourceId start end}`; an absent optional field is omitted.

use std::fmt::Write as _;

use vibra_diagnostics::{ByteSpan, Diagnostic};
use vibra_syntax::canonical_string;

use crate::graph::Access;
use crate::identity::{Anchor, DeclarationId};
use crate::input::PackageId;

/// The format atom of the observation.
pub const RESOLVED_FORMAT: &str = "@resolved.v2";

/// One module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedModule {
    pub(crate) path: String,
    pub(crate) unit: String,
    pub(crate) source_id: String,
}

impl ResolvedModule {
    /// The module path, `@` included.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The unit, `@` included.
    #[must_use]
    pub fn unit(&self) -> &str {
        &self.unit
    }

    /// The source identity.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }
}

/// One declaration with its anchor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedDeclaration {
    pub(crate) identity: DeclarationId,
    pub(crate) anchor: Anchor,
    /// For a member of an `impl` block, the name of the block, which its
    /// anchor has in place of the member's, with the kind `@impl`.
    pub(crate) implementation: Option<String>,
    pub(crate) access: Access,
    pub(crate) source_id: String,
    pub(crate) span: ByteSpan,
}

impl ResolvedDeclaration {
    /// The identity.
    #[must_use]
    pub const fn identity(&self) -> &DeclarationId {
        &self.identity
    }

    /// The anchor the identity hashes; for a member of an `impl` block, the
    /// package, module, and owner of its block, with the member's kind and
    /// name.
    #[must_use]
    pub const fn anchor(&self) -> &Anchor {
        &self.anchor
    }

    /// For a member of an `impl` block, the block's name: its target, as
    /// [`crate::implementation_name`] writes it.
    #[must_use]
    pub fn implementation(&self) -> Option<&str> {
        self.implementation.as_deref()
    }

    /// Who may name the declaration from another module.
    #[must_use]
    pub const fn access(&self) -> Access {
        self.access
    }

    /// The source identity of the module that declares it.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// The declaration's bytes, opener to closer.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

/// One `use`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedImport {
    pub(crate) source_id: String,
    pub(crate) span: ByteSpan,
    pub(crate) path: String,
    pub(crate) name: String,
    pub(crate) to: Option<DeclarationId>,
}

impl ResolvedImport {
    /// The source identity of the importing module.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// The bytes of the `use`, opener to closer.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    /// The locator, `@` included.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The local name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The identity of the module or declaration it names; `None` when
    /// resolution failed or the target has no identity yet.
    #[must_use]
    pub const fn to(&self) -> Option<&DeclarationId> {
        self.to.as_ref()
    }
}

/// One resolved reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedReference {
    pub(crate) source_id: String,
    pub(crate) span: ByteSpan,
    pub(crate) to: DeclarationId,
}

impl ResolvedReference {
    /// The source identity of the referring module.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// The bytes of the path.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    /// The identity of the declaration the path names.
    #[must_use]
    pub const fn to(&self) -> &DeclarationId {
        &self.to
    }
}

/// The local bindings visible at one statement of a body. It is not part of
/// the `@resolved.v2` observation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisibleLocals {
    pub(crate) source_id: String,
    pub(crate) span: ByteSpan,
    pub(crate) names: Vec<String>,
}

impl VisibleLocals {
    /// The source identity of the module.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// The bytes of the statement.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    /// The names visible at the start of the statement, outermost scope
    /// first, each scope in binding order.
    #[must_use]
    pub fn names(&self) -> &[String] {
        &self.names
    }
}

/// Everything resolution established about one input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolution {
    pub(crate) package: PackageId,
    pub(crate) modules: Vec<ResolvedModule>,
    pub(crate) declarations: Vec<ResolvedDeclaration>,
    pub(crate) imports: Vec<ResolvedImport>,
    pub(crate) references: Vec<ResolvedReference>,
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) visible: Vec<VisibleLocals>,
}

impl Resolution {
    /// The locals visible at each statement of the local modules, in the
    /// order the statements are walked: each module in input order, its
    /// declarations in written order.
    #[must_use]
    pub fn visible_locals(&self) -> &[VisibleLocals] {
        &self.visible
    }

    /// The modules, sorted by path.
    #[must_use]
    pub fn modules(&self) -> &[ResolvedModule] {
        &self.modules
    }

    /// The declarations, members included, sorted by identity.
    #[must_use]
    pub fn declarations(&self) -> &[ResolvedDeclaration] {
        &self.declarations
    }

    /// The imports, sorted by span.
    #[must_use]
    pub fn imports(&self) -> &[ResolvedImport] {
        &self.imports
    }

    /// The resolved references, sorted by span.
    #[must_use]
    pub fn references(&self) -> &[ResolvedReference] {
        &self.references
    }

    /// The reader's and resolution's diagnostics, sorted by source, start,
    /// end, and registry order.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Whether any diagnostic is an error.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(Diagnostic::is_error)
    }

    /// Sorts every list in its observation order.
    pub(crate) fn sort(&mut self) {
        self.modules
            .sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));
        self.declarations.sort_by(|left, right| {
            (
                &left.identity,
                &left.source_id,
                left.span.start(),
                left.span.end(),
            )
                .cmp(&(
                    &right.identity,
                    &right.source_id,
                    right.span.start(),
                    right.span.end(),
                ))
        });
        self.imports.sort_by(|left, right| {
            (&left.source_id, left.span.start(), left.span.end()).cmp(&(
                &right.source_id,
                right.span.start(),
                right.span.end(),
            ))
        });
        self.references.sort_by(|left, right| {
            (
                &left.source_id,
                left.span.start(),
                left.span.end(),
                &left.to,
            )
                .cmp(&(
                    &right.source_id,
                    right.span.start(),
                    right.span.end(),
                    &right.to,
                ))
        });
        self.diagnostics.sort_by(|left, right| {
            (
                left.source_id(),
                left.primary_span().start(),
                left.primary_span().end(),
                left.code(),
            )
                .cmp(&(
                    right.source_id(),
                    right.primary_span().start(),
                    right.primary_span().end(),
                    right.code(),
                ))
        });
    }

    /// The canonical `@resolved.v2` text: one line, no trailing newline.
    #[must_use]
    pub fn canonical_text(&self) -> String {
        let mut out = format!(
            "{{format: {RESOLVED_FORMAT} package: {{name: {} version: {}}} modules: [",
            canonical_string(self.package.name()),
            canonical_string(self.package.version())
        );
        for (index, module) in self.modules.iter().enumerate() {
            separate(&mut out, index);
            let _ = write!(
                out,
                "{{path: {} unit: {} sourceId: {}}}",
                module.path,
                module.unit,
                canonical_string(&module.source_id)
            );
        }
        out.push_str("] declarations: [");
        for (index, declaration) in self.declarations.iter().enumerate() {
            separate(&mut out, index);
            let anchor = &declaration.anchor;
            let _ = write!(
                out,
                "{{identity: {} anchor: {{package: {} version: {} module: {}",
                canonical_string(declaration.identity.as_str()),
                canonical_string(anchor.package()),
                canonical_string(anchor.version()),
                anchor.module()
            );
            if !anchor.owner().is_empty() {
                let _ = write!(out, " owner: {}", canonical_string(anchor.owner()));
            }
            match &declaration.implementation {
                Some(target) => {
                    let _ = write!(
                        out,
                        " kind: @impl name: {}}} member: {}",
                        canonical_string(target),
                        canonical_string(anchor.name()),
                    );
                }
                None => {
                    let _ = write!(
                        out,
                        " kind: {} name: {}}}",
                        anchor.kind().as_atom(),
                        canonical_string(anchor.name()),
                    );
                }
            }
            let _ = write!(
                out,
                " access: {} span: {}}}",
                declaration.access.as_atom(),
                span_record(&declaration.source_id, declaration.span)
            );
        }
        out.push_str("] imports: [");
        for (index, import) in self.imports.iter().enumerate() {
            separate(&mut out, index);
            let _ = write!(
                out,
                "{{span: {} path: {} name: {}",
                span_record(&import.source_id, import.span),
                import.path,
                canonical_string(&import.name)
            );
            if let Some(to) = &import.to {
                let _ = write!(out, " to: {}", canonical_string(to.as_str()));
            }
            out.push('}');
        }
        out.push_str("] references: [");
        for (index, reference) in self.references.iter().enumerate() {
            separate(&mut out, index);
            let _ = write!(
                out,
                "{{span: {} to: {}}}",
                span_record(&reference.source_id, reference.span),
                canonical_string(reference.to.as_str())
            );
        }
        out.push_str("]}");
        out
    }
}

fn separate(out: &mut String, index: usize) {
    if index > 0 {
        out.push(' ');
    }
}

fn span_record(source_id: &str, span: ByteSpan) -> String {
    format!(
        "{{sourceId: {} start: {}u64 end: {}u64}}",
        canonical_string(source_id),
        span.start(),
        span.end()
    )
}

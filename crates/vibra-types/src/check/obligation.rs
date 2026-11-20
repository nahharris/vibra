//! Obligation records.
//!
//! `docs/spec/07-diagnostics-and-conformance.md`, "Persistent observations":
//! one obligation for each site where a bare guard must be discharged, with
//! the declaration that contains the site, the guarded declaration, the
//! site's span, the status, the discharge rules used, the declarations the
//! discharge read, and the refuting values.

use vibra_diagnostics::ByteSpan;

/// The guard an obligation discharges.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ObligationKind {
    /// A bare `requires`, at a call.
    Requires,
    /// A bare `ensures`, at its predicate.
    Ensures,
    /// A bare `where`, at a construction.
    Where,
}

impl ObligationKind {
    /// The atom a record writes, without its `@`.
    #[must_use]
    pub const fn atom(self) -> &'static str {
        match self {
            Self::Requires => "requires",
            Self::Ensures => "ensures",
            Self::Where => "where",
        }
    }
}

/// What the discharge established.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ObligationStatus {
    /// The rules establish the predicate.
    Passed,
    /// The rules refute it.
    Refuted,
    /// The rules establish nothing, or a tool limit stopped them.
    Unknown,
    /// The predicate is outside the certified fragment.
    Unsupported,
}

impl ObligationStatus {
    /// The atom a record writes, without its `@`.
    #[must_use]
    pub const fn atom(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Refuted => "refuted",
            Self::Unknown => "unknown",
            Self::Unsupported => "unsupported",
        }
    }
}

/// One obligation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Obligation {
    pub(crate) kind: ObligationKind,
    pub(crate) declaration: String,
    pub(crate) predicate: String,
    pub(crate) source_id: String,
    pub(crate) span: ByteSpan,
    pub(crate) status: ObligationStatus,
    pub(crate) rules: Vec<String>,
    pub(crate) dependencies: Vec<String>,
    pub(crate) counterexample: Option<String>,
}

impl Obligation {
    /// The guard discharged.
    #[must_use]
    pub const fn kind(&self) -> ObligationKind {
        self.kind
    }

    /// The identity of the declaration that contains the site.
    #[must_use]
    pub fn declaration(&self) -> &str {
        &self.declaration
    }

    /// The identity of the guarded declaration.
    #[must_use]
    pub fn predicate(&self) -> &str {
        &self.predicate
    }

    /// The source of the site.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// The site: the call or the construction, or an `ensures` predicate.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    /// The status.
    #[must_use]
    pub const fn status(&self) -> ObligationStatus {
        self.status
    }

    /// The discharge rules used, in order, each `GUARD-6` followed by the
    /// integer facts it used.
    #[must_use]
    pub fn rules(&self) -> &[String] {
        &self.rules
    }

    /// The identities of the guarded declaration and of every declaration
    /// whose checked body or value the discharge read, sorted, without
    /// [`declaration`](Self::declaration).
    #[must_use]
    pub fn dependencies(&self) -> &[String] {
        &self.dependencies
    }

    /// The canonical VIBON text of the refuting values, when refuted.
    #[must_use]
    pub fn counterexample(&self) -> Option<&str> {
        self.counterexample.as_deref()
    }

    /// The record the `@types.v2` document writes.
    pub(crate) fn record(&self) -> String {
        let strings = |values: &[String]| {
            values
                .iter()
                .map(|value| quoted(value))
                .collect::<Vec<_>>()
                .join(" ")
        };
        let counterexample = self
            .counterexample
            .as_ref()
            .map(|text| format!(" counterexample: {}", quoted(text)))
            .unwrap_or_default();
        format!(
            "{{declaration: {} predicate: {} kind: @{} span: {{sourceId: {} start: {}u64 \
             end: {}u64}} status: @{} rules: [{}] dependencies: [{}]{counterexample}}}",
            quoted(&self.declaration),
            quoted(&self.predicate),
            self.kind.atom(),
            quoted(&self.source_id),
            self.span.start(),
            self.span.end(),
            self.status.atom(),
            strings(&self.rules),
            strings(&self.dependencies),
        )
    }
}

/// `text` as a VIBON string.
fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            character if u32::from(character) < 0x20 => {
                out.push_str(&format!("\\u{{{:x}}}", u32::from(character)));
            }
            character => out.push(character),
        }
    }
    out.push('"');
    out
}

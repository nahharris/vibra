//! Lexical classification for the Step 6 name surface.
//!
//! This module deliberately stops at spelling and lexical category. It does
//! not resolve a name, decide its contextual role, or build field-access or
//! reference nodes.

/// The lexical category selected by a name's spelling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NameKind {
    /// An unqualified or qualified symbol.
    Symbol,
    /// A symbol followed by `:`.
    Label,
    /// An atom name prefixed by `@`.
    Atom,
    /// One of the three equivalent discard spellings.
    Discard,
}

impl NameKind {
    /// The stable category spelling used by diagnostics and tooling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Symbol => "symbol",
            Self::Label => "label",
            Self::Atom => "atom",
            Self::Discard => "discard",
        }
    }

    /// Whether this category creates no lexical identity.
    #[must_use]
    pub const fn is_discard(self) -> bool {
        matches!(self, Self::Discard)
    }
}

/// One valid name with its exact spelling and dot-separated value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Name {
    raw: String,
    value: String,
    segments: Vec<String>,
    kind: NameKind,
}

impl Name {
    /// The exact source spelling, including `@` or `:` when present.
    #[must_use]
    pub fn raw(&self) -> &str {
        &self.raw
    }

    /// The name value without its atom or label marker.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    /// The dot-separated name segments, excluding lexical markers.
    #[must_use]
    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    /// The lexical category.
    #[must_use]
    pub const fn kind(&self) -> NameKind {
        self.kind
    }

    /// Alias for [`Self::kind`] for callers that describe categories directly.
    #[must_use]
    pub const fn category(&self) -> NameKind {
        self.kind
    }

    /// Whether this name is one of `-`, `@-`, or `-:`.
    #[must_use]
    pub const fn is_discard(&self) -> bool {
        self.kind.is_discard()
    }
}

/// The result of classifying a nonliteral leaf as a Step 6 name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NameClassification {
    /// The leaf is a valid symbol, label, atom, or discard.
    Name(Name),
    /// The leaf is not a valid name spelling.
    Invalid,
}

impl NameClassification {
    /// The valid name, if classification succeeded.
    #[must_use]
    pub fn as_name(&self) -> Option<&Name> {
        match self {
            Self::Name(name) => Some(name),
            Self::Invalid => None,
        }
    }

    /// Whether this leaf is an invalid name candidate.
    #[must_use]
    pub const fn is_invalid(&self) -> bool {
        matches!(self, Self::Invalid)
    }
}

/// Classifies one nonliteral leaf using the shared name grammar.
#[must_use]
pub fn classify_name(text: &str) -> NameClassification {
    if matches!(text, "-" | "@-" | "-:") {
        return NameClassification::Name(Name {
            raw: text.to_owned(),
            value: text.to_owned(),
            segments: Vec::new(),
            kind: NameKind::Discard,
        });
    }

    let (kind, value) = if let Some(value) = text.strip_prefix('@') {
        (NameKind::Atom, value)
    } else if let Some(value) = text.strip_suffix(':') {
        (NameKind::Label, value)
    } else {
        (NameKind::Symbol, text)
    };

    let Some(segments) = valid_segments(value) else {
        return NameClassification::Invalid;
    };

    NameClassification::Name(Name {
        raw: text.to_owned(),
        value: value.to_owned(),
        segments: segments.into_iter().map(str::to_owned).collect(),
        kind,
    })
}

fn valid_segments(value: &str) -> Option<Vec<&str>> {
    if value.is_empty() {
        return None;
    }

    let segments: Vec<&str> = value.split('.').collect();
    if segments.iter().all(|segment| valid_segment(segment)) {
        Some(segments)
    } else {
        None
    }
}

fn valid_segment(segment: &str) -> bool {
    let mut characters = segment.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    characters.all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
    })
}

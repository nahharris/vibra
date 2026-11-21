//! Rule identities and the rule index, `conformance/rules.tsv`.
//!
//! `docs/spec/07-diagnostics-and-conformance.md`, "Rule identities and
//! example coverage", defines a rule identity as `V1-` or `V2-`, one of twenty
//! section prefixes, and a descriptive suffix in lower case with hyphens. An
//! identity is never reused: a removed rule stays in the index as retired.
//!
//! The index is a tab-separated file with the header
//! `identity<TAB>status<TAB>anchor` and one row per rule identity. A case may
//! address only a rule whose row is `active`.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

/// The file name of the rule index, beside the corpus root.
pub const RULES_FILE_NAME: &str = "rules.tsv";

/// The header line of the rule index.
pub const RULES_HEADER: &str = "identity\tstatus\tanchor";

/// The twenty section prefixes of a rule identity, in specification order.
pub const RULE_PREFIXES: &[&str] = &[
    "CHARTER",
    "SRC-READER",
    "SRC-CALLS",
    "SRC-DECL",
    "SRC-EXPR",
    "SRC-FMT",
    "TYPE-NOMINAL",
    "TYPE-NAMES",
    "TYPE-INFER",
    "TYPE-GENERIC",
    "TYPE-INTERFACE",
    "TYPE-CONVERT",
    "TYPE-CONTROL",
    "EFFECT",
    "PROJECT",
    "TOOL",
    "RUNTIME",
    "DIAG",
    "CONTRACT",
    "GRANT",
];

/// The specification versions an identity may carry.
const VERSIONS: &[&str] = &["V1", "V2"];

/// A well-formed rule identity, split into its parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdentityParts<'a> {
    /// `V1` or `V2`.
    pub version: &'a str,
    /// One of [`RULE_PREFIXES`].
    pub prefix: &'a str,
    /// The descriptive suffix.
    pub suffix: &'a str,
}

/// Why a spelling is not a rule identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvalidIdentity {
    identity: String,
    reason: &'static str,
}

impl InvalidIdentity {
    /// The rejected spelling.
    #[must_use]
    pub fn identity(&self) -> &str {
        &self.identity
    }
}

impl fmt::Display for InvalidIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "`{}` is not a rule identity: {}",
            self.identity, self.reason
        )
    }
}

impl std::error::Error for InvalidIdentity {}

/// Whether `suffix` is a non-empty run of lower-case letters, digits, and
/// hyphens.
pub(crate) fn is_suffix(suffix: &str) -> bool {
    !suffix.is_empty()
        && suffix.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '-'
        })
}

/// Splits a rule identity into its version, prefix, and suffix.
///
/// The longest prefix that is followed by `-` wins, because several prefixes
/// share the `SRC-` and `TYPE-` stems.
pub fn parse_identity(identity: &str) -> Result<IdentityParts<'_>, InvalidIdentity> {
    let invalid = |reason| InvalidIdentity {
        identity: identity.to_owned(),
        reason,
    };
    let Some((version, rest)) = identity.split_once('-') else {
        return Err(invalid("it has no version"));
    };
    if !VERSIONS.contains(&version) {
        return Err(invalid("the version is not `V1` or `V2`"));
    }
    let Some((prefix, suffix)) = RULE_PREFIXES
        .iter()
        .filter_map(|prefix| {
            rest.strip_prefix(prefix)
                .and_then(|after| after.strip_prefix('-'))
                .map(|suffix| (*prefix, suffix))
        })
        .max_by_key(|(prefix, _)| prefix.len())
    else {
        return Err(invalid("the section prefix is not one of the twenty"));
    };
    if !is_suffix(suffix) {
        return Err(invalid(
            "the suffix is not lower-case letters, digits, and hyphens",
        ));
    }
    Ok(IdentityParts {
        version,
        prefix,
        suffix,
    })
}

/// The status of a rule in the index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleStatus {
    /// Cases may address the rule.
    Active,
    /// The rule was removed; its identity is never reused.
    Retired,
}

impl RuleStatus {
    /// The spelling in the index.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Retired => "retired",
        }
    }
}

impl fmt::Display for RuleStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// One row of the rule index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleRow {
    /// The rule identity.
    pub identity: String,
    /// Whether cases may address it.
    pub status: RuleStatus,
    /// The specification anchor of the rule.
    pub anchor: String,
}

/// A rule index decoding failure.
#[derive(Debug)]
pub enum RulesError {
    /// The index could not be read.
    Io {
        /// The path that was read.
        path: PathBuf,
        /// The underlying failure.
        source: std::io::Error,
    },
    /// The index violates its format; `line` is one-based.
    Invalid {
        /// The offending line.
        line: usize,
        /// What is wrong with it.
        message: String,
    },
}

impl fmt::Display for RulesError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(formatter, "cannot read {}: {source}", path.display())
            }
            Self::Invalid { line, message } => {
                write!(formatter, "{RULES_FILE_NAME} line {line}: {message}")
            }
        }
    }
}

impl std::error::Error for RulesError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Invalid { .. } => None,
        }
    }
}

/// The decoded rule index.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuleIndex {
    rows: BTreeMap<String, RuleRow>,
}

impl RuleIndex {
    /// Decodes the text of a rule index.
    ///
    /// The first line is exactly [`RULES_HEADER`]. Every other line is three
    /// tab-separated fields: a rule identity, `active` or `retired`, and a
    /// non-empty anchor. An identity appears once. The text may end with one
    /// line feed.
    pub fn parse(text: &str) -> Result<Self, RulesError> {
        let body = text.strip_suffix('\n').unwrap_or(text);
        let mut lines = body.split('\n').enumerate();
        let header = lines.next().map(|(_, line)| line);
        if header != Some(RULES_HEADER) {
            return Err(RulesError::Invalid {
                line: 1,
                message: format!(
                    "the header must be `{}`",
                    RULES_HEADER.escape_debug()
                ),
            });
        }
        let mut rows = BTreeMap::new();
        for (index, line) in lines {
            let number = index + 1;
            let invalid = |message: String| RulesError::Invalid {
                line: number,
                message,
            };
            let fields: Vec<&str> = line.split('\t').collect();
            let [identity, status, anchor] = fields.as_slice() else {
                return Err(invalid(format!(
                    "a row has three tab-separated fields, not {}",
                    fields.len()
                )));
            };
            parse_identity(identity).map_err(|error| invalid(error.to_string()))?;
            let status = match *status {
                "active" => RuleStatus::Active,
                "retired" => RuleStatus::Retired,
                other => {
                    return Err(invalid(format!(
                        "the status `{other}` is not `active` or `retired`"
                    )));
                }
            };
            if anchor.is_empty() {
                return Err(invalid(format!("rule `{identity}` has no anchor")));
            }
            let row = RuleRow {
                identity: (*identity).to_owned(),
                status,
                anchor: (*anchor).to_owned(),
            };
            if rows.insert(row.identity.clone(), row).is_some() {
                return Err(invalid(format!("rule `{identity}` appears twice")));
            }
        }
        Ok(Self { rows })
    }

    /// Reads and decodes a rule index file.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, RulesError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|source| RulesError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse(&text)
    }

    /// The status of `identity`, or `None` when the index has no row for it.
    #[must_use]
    pub fn status(&self, identity: &str) -> Option<RuleStatus> {
        self.rows.get(identity).map(|row| row.status)
    }

    /// Every row, ordered by identity.
    pub fn rows(&self) -> impl Iterator<Item = &RuleRow> {
        self.rows.values()
    }

    /// The number of rows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether the index has no row.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

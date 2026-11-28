//! Rule identities and the rule index: `conformance/rules.tsv` and the files
//! of `conformance/rules/`.
//!
//! `docs/spec/07-diagnostics-and-conformance.md`, "Rule identities and
//! example coverage", defines a rule identity as `V1-` or `V2-`, one of twenty
//! section prefixes, and a descriptive suffix in lower case with hyphens. An
//! identity is never reused: a removed rule stays in the index as retired.
//!
//! Each rules file is tab-separated, with the header
//! `identity<TAB>status<TAB>anchor` and one row per rule identity. The index
//! is `rules.tsv` followed by every `rules/*.tsv`, so that each step of a
//! milestone adds its rules in a file of its own. A rule identity is a row of
//! exactly one file. A case may address only a rule whose row is `active`.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};

/// The file name of the first rules file, in the conformance root beside the
/// corpus root.
pub const RULES_FILE_NAME: &str = "rules.tsv";

/// The directory of the further rules files, in the conformance root. Only
/// its files whose extension is `tsv` are rules files.
pub(crate) const RULES_DIRECTORY_NAME: &str = "rules";

/// The header line of every rules file.
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
    /// A rules file violates its format; `line` is one-based.
    Invalid {
        /// The rules file, as [`RuleIndex::from_root`] names it: `rules.tsv`
        /// or `rules/<name>.tsv`.
        file: String,
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
            Self::Invalid {
                file,
                line,
                message,
            } => {
                write!(formatter, "{file} line {line}: {message}")
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

/// One row of the index and the place it was read from.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    row: RuleRow,
    /// The rules file that holds the row.
    file: String,
    /// The one-based line of the row.
    line: usize,
}

/// The decoded rule index.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuleIndex {
    rows: BTreeMap<String, Entry>,
}

impl RuleIndex {
    /// Decodes the text of one rules file, named `rules.tsv` in errors.
    ///
    /// The first line is exactly [`RULES_HEADER`]. Every other line is three
    /// tab-separated fields: a rule identity, `active` or `retired`, and a
    /// non-empty anchor. An identity appears once. The text may end with one
    /// line feed.
    pub fn parse(text: &str) -> Result<Self, RulesError> {
        let mut index = Self::default();
        index.add_file(RULES_FILE_NAME, text)?;
        Ok(index)
    }

    /// Reads and decodes the rule index of a conformance root: the file
    /// [`RULES_FILE_NAME`], then every file of the directory `rules` whose
    /// extension is `tsv`, in the byte order of their file names.
    ///
    /// Each file is decoded as [`RuleIndex::parse`] decodes one. A rule
    /// identity is a row of exactly one file: a second row for an identity,
    /// in the same file or in a later one, is the error a duplicate inside one
    /// file is, naming the file of the second row. A root without the
    /// directory has the rows of `rules.tsv` alone; any other entry of the
    /// directory is not a rules file and is ignored.
    pub fn from_root(root: impl AsRef<Path>) -> Result<Self, RulesError> {
        let root = root.as_ref();
        let mut index = Self::default();
        index.add_path(&root.join(RULES_FILE_NAME), RULES_FILE_NAME)?;

        let directory = root.join(RULES_DIRECTORY_NAME);
        let io = |source| RulesError::Io {
            path: directory.clone(),
            source,
        };
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(index);
            }
            Err(source) => return Err(io(source)),
        };
        let mut names: Vec<OsString> = Vec::new();
        for entry in entries {
            let name = entry.map_err(io)?.file_name();
            if Path::new(&name)
                .extension()
                .is_some_and(|extension| extension == "tsv")
            {
                names.push(name);
            }
        }
        names.sort_by(|left, right| {
            left.as_encoded_bytes().cmp(right.as_encoded_bytes())
        });
        for name in names {
            let file = format!("{RULES_DIRECTORY_NAME}/{}", name.to_string_lossy());
            index.add_path(&directory.join(&name), &file)?;
        }
        Ok(index)
    }

    /// Reads the rules file at `path` and adds its rows, naming it `file`.
    fn add_path(&mut self, path: &Path, file: &str) -> Result<(), RulesError> {
        let text = std::fs::read_to_string(path).map_err(|source| RulesError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        self.add_file(file, &text)
    }

    /// Decodes the text of the rules file `file` and adds its rows.
    fn add_file(&mut self, file: &str, text: &str) -> Result<(), RulesError> {
        let body = text.strip_suffix('\n').unwrap_or(text);
        let mut lines = body.split('\n').enumerate();
        let header = lines.next().map(|(_, line)| line);
        if header != Some(RULES_HEADER) {
            return Err(RulesError::Invalid {
                file: file.to_owned(),
                line: 1,
                message: format!(
                    "the header must be `{}`",
                    RULES_HEADER.escape_debug()
                ),
            });
        }
        for (index, line) in lines {
            let number = index + 1;
            let invalid = |message: String| RulesError::Invalid {
                file: file.to_owned(),
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
            if let Some(first) = self.rows.get(*identity) {
                return Err(invalid(format!(
                    "rule `{identity}` appears twice: its first row is {} line {}",
                    first.file, first.line
                )));
            }
            let row = RuleRow {
                identity: (*identity).to_owned(),
                status,
                anchor: (*anchor).to_owned(),
            };
            self.rows.insert(
                row.identity.clone(),
                Entry {
                    row,
                    file: file.to_owned(),
                    line: number,
                },
            );
        }
        Ok(())
    }

    /// The status of `identity`, or `None` when the index has no row for it.
    #[must_use]
    pub fn status(&self, identity: &str) -> Option<RuleStatus> {
        self.rows.get(identity).map(|entry| entry.row.status)
    }

    /// Every row, ordered by identity.
    pub fn rows(&self) -> impl Iterator<Item = &RuleRow> {
        self.rows.values().map(|entry| &entry.row)
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

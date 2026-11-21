//! Path names and the reserved-name set of the scope policies.
//!
//! `docs/spec/03-effects.md`, "Filesystem policies", fixes the name profile
//! of a `RelativePath`. A name is non-empty and is rejected if it is `.` or
//! `..`, contains `/`, `\`, `:`, NUL, or a control character, ends in a dot
//! or a space, or is a reserved device name. A path is a list of names,
//! possibly empty: the empty path names the directory a grant holds.
//!
//! Names are portable: the same closed set of device names is rejected on
//! every platform. A name's **stem** is the part before its first dot, with
//! trailing spaces removed; the name is a device name when its stem, after
//! each character is mapped to its Unicode simple uppercase, is one of the
//! closed set [`RESERVED_STEMS`].
//!
//! The vectors of `tests/it/names.tsv` hold this function to the chapter.
//! The file-scope adapter of `vibra-fs-scope` checks the same profile with a
//! function of its own, and is run over the same vectors.

use std::fmt;

/// The closed set of reserved device stems, each in upper case: `CON`,
/// `PRN`, `AUX`, `NUL`, `CONIN$`, `CONOUT$`, `COM0` through `COM9`, `LPT0`
/// through `LPT9`, and the superscript-digit forms `COM¹`, `COM²`, `COM³`,
/// `LPT¹`, `LPT²`, `LPT³`.
pub const RESERVED_STEMS: [&str; 32] = [
    "CON",
    "PRN",
    "AUX",
    "NUL",
    "CONIN$",
    "CONOUT$",
    "COM0",
    "COM1",
    "COM2",
    "COM3",
    "COM4",
    "COM5",
    "COM6",
    "COM7",
    "COM8",
    "COM9",
    "COM\u{b9}",
    "COM\u{b2}",
    "COM\u{b3}",
    "LPT0",
    "LPT1",
    "LPT2",
    "LPT3",
    "LPT4",
    "LPT5",
    "LPT6",
    "LPT7",
    "LPT8",
    "LPT9",
    "LPT\u{b9}",
    "LPT\u{b2}",
    "LPT\u{b3}",
];

/// Why a text is not a name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NameError {
    /// The empty text.
    Empty,
    /// `.` or `..`.
    DotName,
    /// A `/`, `\`, `:`, NUL, or another control character.
    ForbiddenCharacter(char),
    /// The name ends in a dot or a space.
    TrailingDotOrSpace,
    /// The stem is a reserved device name.
    ReservedDevice,
}

impl fmt::Display for NameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("a name is non-empty"),
            Self::DotName => formatter.write_str("a name is not `.` or `..`"),
            Self::ForbiddenCharacter(character) => write!(
                formatter,
                "a name contains no `/`, `\\`, `:`, NUL, or control character, \
                 and this one contains U+{:04X}",
                u32::from(*character)
            ),
            Self::TrailingDotOrSpace => {
                formatter.write_str("a name does not end in a dot or a space")
            }
            Self::ReservedDevice => {
                formatter.write_str("the name is a reserved device name")
            }
        }
    }
}

/// Checks `name` against the name profile of "Filesystem policies".
///
/// The checks run in the order of the chapter's sentence: empty, `.` and
/// `..`, the forbidden characters, the trailing dot or space, and the
/// reserved device names.
///
/// # Errors
///
/// The first rule the name breaks.
pub fn check_name(name: &str) -> Result<(), NameError> {
    if name.is_empty() {
        return Err(NameError::Empty);
    }
    if name == "." || name == ".." {
        return Err(NameError::DotName);
    }
    if let Some(character) = name.chars().find(|character| {
        matches!(character, '/' | '\\' | ':') || character.is_control()
    }) {
        return Err(NameError::ForbiddenCharacter(character));
    }
    if name.ends_with(['.', ' ']) {
        return Err(NameError::TrailingDotOrSpace);
    }
    if is_reserved_device(name) {
        return Err(NameError::ReservedDevice);
    }
    Ok(())
}

/// Whether the stem of `name`, mapped to its simple uppercase, is a reserved
/// device stem.
fn is_reserved_device(name: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end_matches(' ');
    let upper = stem.chars().map(simple_uppercase).collect::<String>();
    RESERVED_STEMS.contains(&upper.as_str())
}

/// The Unicode simple uppercase of `character`.
///
/// Rust's `char::to_uppercase` is the full mapping. Where the full mapping is
/// one character it is the simple mapping, because `SpecialCasing.txt` has
/// unconditional entries only for mappings to several characters. Where it is
/// several, the simple mapping is the character itself or another non-ASCII
/// letter, and neither can occur in a device stem, whose characters are ASCII
/// capitals, digits, `$`, and `¹²³`; the character is kept, which gives the
/// verdict of the simple mapping.
fn simple_uppercase(character: char) -> char {
    let mut upper = character.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(single), None) => single,
        _ => character,
    }
}

/// One name that satisfies the name profile.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Name(String);

impl Name {
    /// The name `text`.
    ///
    /// # Errors
    ///
    /// The first rule of the name profile that `text` breaks.
    pub fn new(text: impl Into<String>) -> Result<Self, NameError> {
        let text = text.into();
        check_name(&text)?;
        Ok(Self(text))
    }

    /// The name's scalars.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Name {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A list of names, each satisfying the name profile: the representation of
/// a `RelativePath`. The empty list is valid and names the directory a grant
/// holds.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RelativeNames(Vec<Name>);

/// A name of a [`RelativeNames`] that failed the profile: its position in the
/// list and the rule it broke.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct InvalidName {
    /// The position of the name in the list, from zero.
    pub index: usize,
    /// The rule it broke.
    pub error: NameError,
}

impl RelativeNames {
    /// The empty path.
    #[must_use]
    pub const fn empty() -> Self {
        Self(Vec::new())
    }

    /// The path of `names`, in order.
    ///
    /// # Errors
    ///
    /// The first name that breaks the profile.
    pub fn new<I, S>(names: I) -> Result<Self, InvalidName>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        names
            .into_iter()
            .enumerate()
            .map(|(index, name)| {
                Name::new(name).map_err(|error| InvalidName { index, error })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Self)
    }

    /// The names, in order.
    #[must_use]
    pub fn names(&self) -> &[Name] {
        &self.0
    }

    /// The number of names.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether this is the empty path.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether `names` begins with this path, compared name by name, never
    /// as strings: `reports` does not cover `reportsOld`.
    #[must_use]
    pub fn is_prefix_of(&self, names: &[String]) -> bool {
        self.0.len() <= names.len()
            && self
                .0
                .iter()
                .zip(names)
                .all(|(own, other)| own.as_str() == other.as_str())
    }
}

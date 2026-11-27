//! The name profile of the scope policies: the one definition the broker,
//! the world driver, and the compiler row `fs.validNames` share.
//!
//! `docs/spec/03-effects.md`, "Filesystem policies", fixes the profile of a
//! name of a `RelativePath`. A name is non-empty and is rejected if it is `.`
//! or `..`, contains `/`, `\`, `:`, NUL, or a control character, ends in a
//! dot or a space, or is a reserved device name. A path is a list of names,
//! possibly empty: the empty path names the directory a grant holds.
//!
//! Names are portable: the same closed set of device names is rejected on
//! every platform. A name's **stem** is the part before its first dot, with
//! trailing spaces removed; the name is a device name when its stem, after
//! each character is mapped to its Unicode simple uppercase, is one of the
//! closed set [`RESERVED_STEMS`].
//!
//! The file-scope adapter of `vibra-fs-scope`, which knows no language and
//! depends on no crate of the workspace, checks the same profile again at the
//! filesystem with a function of its own; both are held to the vectors of
//! `crates/vibra-host/tests/it/names.tsv`.

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
/// reserved device names. A control character is one of the general
/// category `Cc`: U+0000 to U+001F and U+007F to U+009F.
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

/// Whether every name of `names` satisfies the profile: the meaning of the
/// compiler row `fs.validNames`. The empty list is valid.
#[must_use]
pub fn valid_names<S: AsRef<str>>(names: &[S]) -> bool {
    names.iter().all(|name| check_name(name.as_ref()).is_ok())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_rule_names_the_first_it_breaks() {
        assert_eq!(check_name(""), Err(NameError::Empty));
        assert_eq!(check_name(".."), Err(NameError::DotName));
        assert_eq!(check_name("a:b"), Err(NameError::ForbiddenCharacter(':')));
        assert_eq!(
            check_name("a\u{9f}b"),
            Err(NameError::ForbiddenCharacter('\u{9f}'))
        );
        assert_eq!(check_name("a."), Err(NameError::TrailingDotOrSpace));
        assert_eq!(check_name("con\u{131}n$"), Err(NameError::ReservedDevice));
        assert_eq!(check_name("nul2"), Ok(()));
        assert_eq!(check_name("a\u{a0}b"), Ok(()));
    }

    #[test]
    fn a_list_is_valid_when_every_name_is() {
        assert!(valid_names::<&str>(&[]));
        assert!(valid_names(&["reports", "a.txt"]));
        assert!(!valid_names(&["a", ".."]));
        assert!(!valid_names(&["NUL.txt"]));
    }
}

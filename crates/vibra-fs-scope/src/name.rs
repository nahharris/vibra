//! The name profile: the lexical rules for one requested name.
//!
//! They are the rules of `RelativePath` in the effects chapter: a name is
//! non-empty; it is not `.` or `..`; it contains no `/`, `\`, `:`, NUL, or
//! other control character; it does not end in a dot or a space; and it is not
//! a reserved device name. These rules make absolute, drive-relative, UNC,
//! `\\?\`, and alternate-stream inputs unrepresentable, because each needs a
//! separator or a colon. The function is the same on every platform.

use crate::ScopeError;

/// The closed set of reserved device stems, in upper case: `CON`, `PRN`,
/// `AUX`, `NUL`, `CONIN$`, `CONOUT$`, `COM0` to `COM9`, `LPT0` to `LPT9`, and
/// the six superscript-digit forms.
const DEVICE_STEMS: &[&str] = &[
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

/// Checks every name of a request before anything is opened. The empty
/// request is valid: it names the root.
pub(crate) fn check_each(names: &[&str]) -> Result<(), ScopeError> {
    names.iter().try_for_each(|name| check_name(name))
}

/// Checks one name against the name profile of the effects chapter.
///
/// A name is rejected with [`ScopeError::DeniedName`] when it is empty, `.`
/// or `..`; when it contains `/`, `\`, `:`, NUL, or another control
/// character; when it ends in a dot or a space; or when its stem, the part
/// before its first dot with trailing spaces removed, is a reserved device
/// name after each of its characters is mapped to its Unicode simple
/// uppercase. The result is the same on every platform.
pub fn check_name(name: &str) -> Result<(), ScopeError> {
    let plain = !name.is_empty()
        && name != "."
        && name != ".."
        && !name
            .chars()
            .any(|c| matches!(c, '/' | '\\' | ':') || c.is_control())
        && !name.ends_with(['.', ' '])
        && !is_device_name(name);
    if plain {
        Ok(())
    } else {
        Err(ScopeError::DeniedName)
    }
}

/// Whether the stem of `name`, the part before its first dot with trailing
/// spaces removed, is a reserved device stem once each of its characters is
/// mapped to its simple uppercase.
fn is_device_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).trim_end_matches(' ');
    let upper = stem.chars().map(simple_uppercase).collect::<String>();
    DEVICE_STEMS.contains(&upper.as_str())
}

/// The Unicode simple uppercase of `c`, for the device test.
///
/// Where the full uppercase of `c` is one character, it is the simple
/// uppercase. Where it is several (`ß`, the ligatures, the Greek letters of
/// `SpecialCasing.txt`), `c` is kept: the simple uppercase of each such
/// character is itself or another non-ASCII letter, so neither result can
/// spell a device stem, whose characters are ASCII capitals, digits, `$`,
/// and `¹²³`. The verdict is therefore that of the simple mapping.
fn simple_uppercase(c: char) -> char {
    let mut upper = c.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(single), None) => single,
        _ => c,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_names_pass() {
        for name in [
            "notes.txt",
            ".txt",
            "a",
            "LONGFI~1.TXT",
            "console",
            "conx.txt",
            "COM10",
            "com10",
            "nul2",
            "lpt",
            "nul_file",
            "r\u{e9}sum\u{e9}",
        ] {
            assert!(check_name(name).is_ok(), "{name:?} is a plain name");
        }
    }

    #[test]
    fn every_rejected_shape_is_denied() {
        for name in [
            "",
            ".",
            "..",
            "a/b",
            "a\\b",
            "a:b",
            "a.",
            "a ",
            "a\u{7}",
            "C:foo",
            "\\\\server\\share",
            "file.txt::$DATA",
            "nul\u{0}",
            "NUL",
            "nul",
            "NUL.txt",
            "con .log",
            "COM0",
            "LPT9.x",
            "CONIN$",
            "conout$",
            "COM\u{b9}",
            "lpt\u{b3}.txt",
            "aux.tar.gz",
            "CON\u{131}N$",
            "con\u{131}n$.txt",
        ] {
            assert!(
                matches!(check_name(name), Err(ScopeError::DeniedName)),
                "{name:?} is denied"
            );
        }
    }

    #[test]
    fn the_empty_request_is_valid() {
        assert!(check_each(&[]).is_ok());
        assert!(matches!(
            check_each(&["a", "..", "b"]),
            Err(ScopeError::DeniedName)
        ));
        assert!(check_each(&["a", "b", "c.txt"]).is_ok());
    }

    #[test]
    fn a_character_with_a_long_uppercase_is_kept() {
        // `ß` uppercases to `SS` in full and to itself in the simple mapping.
        assert_eq!(simple_uppercase('\u{df}'), '\u{df}');
        assert_eq!(simple_uppercase('\u{131}'), 'I');
        assert_eq!(simple_uppercase('\u{b9}'), '\u{b9}');
    }
}

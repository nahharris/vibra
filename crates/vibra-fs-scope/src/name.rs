//! The lexical rules for one requested name.
//!
//! They are the rules of `RelativePath` in the effects chapter: a name is
//! non-empty; it is not `.` or `..`; it contains no `/`, `\`, `:`, NUL, or
//! other control character; it does not end in a dot or a space; and it is not
//! a reserved Windows device name, with or without an extension. These rules
//! make absolute, drive-relative, UNC, `\\?\`, and alternate-stream inputs
//! unrepresentable, because each needs a separator or a colon.

use crate::ScopeError;

/// The reserved Windows device names, in upper case. `COM` and `LPT` with a
/// superscript digit are devices too.
const DEVICE_NAMES: &[&str] = &[
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

/// Checks every name of a request before anything is opened.
pub(crate) fn check_all(names: &[&str]) -> Result<(), ScopeError> {
    if names.is_empty() {
        return Err(ScopeError::DeniedName);
    }
    names.iter().try_for_each(|name| check(name))
}

/// Checks one name.
pub(crate) fn check(name: &str) -> Result<(), ScopeError> {
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

/// Whether Windows treats `name` as a device: its stem, the part before the
/// first dot with trailing spaces removed, is a device name in any ASCII case.
fn is_device_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).trim_end_matches(' ');
    let upper = stem.to_ascii_uppercase();
    DEVICE_NAMES.contains(&upper.as_str())
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
            "nul_file",
            "r\u{e9}sum\u{e9}",
        ] {
            assert!(check(name).is_ok(), "{name:?} is a plain name");
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
            "C:foo",
            "C:\\Windows",
            "\\\\server\\share",
            "\\\\?\\C:\\x",
            "\\??\\C:\\x",
            "file.txt::$DATA",
            "file.txt:stream",
            "nul\u{0}",
            "tab\there",
            "trailing.",
            "trailing ",
            "NUL",
            "nul",
            "CON",
            "COM1",
            "com\u{b9}",
            "aux.txt",
            "AUX .txt",
            "lpt9.tar.gz",
            "CONIN$",
        ] {
            assert!(
                matches!(check(name), Err(ScopeError::DeniedName)),
                "{name:?} is denied"
            );
        }
    }

    #[test]
    fn an_empty_request_is_denied() {
        assert!(matches!(check_all(&[]), Err(ScopeError::DeniedName)));
        assert!(matches!(
            check_all(&["a", "..", "b"]),
            Err(ScopeError::DeniedName)
        ));
        assert!(check_all(&["a", "b", "c.txt"]).is_ok());
    }
}

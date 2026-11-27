//! The environment adapters: `env.readVar` and `env.readArgs`.
//!
//! `docs/spec/06-runtime.md`, "Environment": a permitted name that is
//! missing is `notPresent`, and so is a name no environment can hold (an
//! empty one, or one containing `=` or NUL); a value that is not Unicode is
//! `notUnicode`. The arguments are the program's, in order, without the
//! executable's path; one that is not Unicode is `notUnicode`. The size a
//! read reserves is the UTF-8 length of the value, or the sum of the UTF-8
//! lengths of the arguments.
//!
//! The broker decided in step 3 that the name is one the grant permits; the
//! process environment is consulted only after that. A name is compared
//! exactly, on every platform, so a name that differs only in case is
//! another name.

use std::ffi::{OsStr, OsString};

use vibra_ir::authority::HostOk;

use super::{Answer, Variables, length, moved};
use crate::driver::{Found, FoundAction, Gate, Operand, Progress};

/// The value of `name` in `variables`, exactly as it is spelled.
fn lookup(variables: &Variables, name: &str) -> Option<OsString> {
    let name = OsStr::new(name);
    match variables {
        Variables::Process => std::env::vars_os()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value),
        Variables::Fixed(entries) => entries
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone()),
    }
}

/// Reserves `size` input bytes at the gate.
///
/// # Errors
///
/// The answer of the call when the gate refused or reserved less.
fn reserve(gate: &mut dyn Gate, size: u64) -> Result<(), Answer> {
    let found = Found {
        real_names: &[],
        action: FoundAction::Read,
        size: Some(size),
        links: None,
    };
    match gate.admit(&found) {
        Err(_) => Err(Answer::Refused),
        Ok(reserved) if reserved.input < size => {
            Err(Answer::NotSubmitted("unsupported"))
        }
        Ok(_) => Ok(()),
    }
}

/// `env.readVar` of the name in `operand`.
pub(super) fn read_var(
    variables: &Variables,
    operand: Operand<'_>,
    gate: &mut dyn Gate,
) -> Answer {
    let Operand::Text(name) = operand else {
        return Answer::NotSubmitted("unsupported");
    };
    // No environment can hold such a name, so none is looked up.
    if name.is_empty() || name.contains(['=', '\0']) {
        return Answer::Err("notPresent", Progress::NONE);
    }
    let Some(value) = lookup(variables, name) else {
        return Answer::Err("notPresent", Progress::NONE);
    };
    let Ok(value) = value.into_string() else {
        return Answer::Err("notUnicode", Progress::NONE);
    };
    let size = length(value.len());
    match reserve(gate, size) {
        Ok(()) => Answer::Ok(HostOk::Text(value), moved(size, 0)),
        Err(answer) => answer,
    }
}

/// `env.readArgs`.
pub(super) fn read_args(arguments: &[OsString], gate: &mut dyn Gate) -> Answer {
    let mut size: u64 = 0;
    for argument in arguments {
        let Some(text) = argument.to_str() else {
            return Answer::Err("notUnicode", Progress::NONE);
        };
        size = size.saturating_add(length(text.len()));
    }
    // The copy for the program is made only once its size is reserved.
    if let Err(answer) = reserve(gate, size) {
        return answer;
    }
    let texts = arguments
        .iter()
        .filter_map(|argument| argument.to_str().map(str::to_owned))
        .collect();
    Answer::Ok(HostOk::Names(texts), moved(size, 0))
}

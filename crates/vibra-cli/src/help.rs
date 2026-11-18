//! The usage text: the command hierarchy with the support inventory.
//!
//! `docs/spec/05-tooling.md`, "Command surface": help shows the hierarchy with
//! the supported inventory and MUST NOT present an unavailable command as
//! working.

use crate::inventory::LINES;

/// The prefix of every command line in the usage text.
pub const COMMAND_PREFIX: &str = "  vibra [GLOBAL...] ";

/// The mark that follows a command line this tool does not support.
pub const UNAVAILABLE_MARK: &str = "(unavailable)";

/// The usage text, built from the inventory.
#[must_use]
pub fn usage() -> String {
    let mut text = String::from(
        "usage: vibra [GLOBAL...] COMMAND\n\
         \n\
         GLOBAL, each at most once and before the command:\n\
         \x20 --format human|json   the output format (default human)\n\
         \x20 --workspace PATH      the workspace root (default: the current directory)\n\
         \n\
         commands:\n",
    );
    for line in &LINES {
        text.push_str(COMMAND_PREFIX);
        text.push_str(line.synopsis);
        if !line.supported {
            text.push(' ');
            text.push_str(UNAVAILABLE_MARK);
        }
        text.push('\n');
    }
    text.push_str(
        "\n\
         `help`, `--help`, and `-h` are equivalent and take no arguments.\n\
         A command marked (unavailable) is known but not supported by this tool.\n",
    );
    text
}

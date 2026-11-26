//! The support inventory: one row per command line of the command surface.
//!
//! `docs/spec/05-tooling.md`, "Command surface". This is the single table the
//! parser, `help`, and the unavailable outcome read, so a command the parser
//! accepts always has a row, and a command the tool does not support is named
//! as such wherever it is shown.

use vibra_schema::{COMMAND_RESULT_SCHEMA_ID, CommandCapability, DIAGNOSTIC_SCHEMA_ID};

/// An option of a command line.
#[derive(Clone, Copy, Debug)]
pub struct OptionSpec {
    /// The spelling, such as `--write`.
    pub name: &'static str,
    /// Whether the option takes the next token as its value.
    pub takes_value: bool,
    /// Whether the option may appear more than once.
    pub repeatable: bool,
}

/// One command line of the surface, with its grammar and its support.
#[derive(Clone, Copy, Debug)]
pub struct Line {
    /// The words that select the command line, such as `["project", "init"]`.
    pub words: &'static [&'static str],
    /// The line as the command surface writes it after `vibra [GLOBAL...] `.
    pub synopsis: &'static str,
    /// The spellings the first operand must be one of; empty when any text is
    /// allowed.
    pub first_operand: &'static [&'static str],
    /// How many operands the line requires.
    pub required: usize,
    /// How many further operands the line accepts.
    pub optional: usize,
    /// Whether any number of operands follows the required ones.
    pub variadic: bool,
    /// Whether every token after the command is an operand, whatever it
    /// begins with.
    pub raw_operands: bool,
    /// The options of the line.
    pub options: &'static [OptionSpec],
    /// Whether the line accepts `--` and takes every later token as a program
    /// argument.
    pub passthrough: bool,
    /// Whether this tool supports the command.
    pub supported: bool,
    /// The operations a supported command offers. Its profiles are derived
    /// from them by [`OPERATION_PROFILES`].
    pub operations: &'static [&'static str],
    /// The schemas a supported command's documents use.
    pub schema_ids: &'static [&'static str],
}

impl Line {
    /// The command name the envelope carries, such as `project init`.
    #[must_use]
    pub fn name(&self) -> String {
        self.words.join(" ")
    }
}

const WRITE: OptionSpec = OptionSpec {
    name: "--write",
    takes_value: false,
    repeatable: false,
};

const UNSUPPORTED: Line = Line {
    words: &[],
    synopsis: "",
    first_operand: &[],
    required: 0,
    optional: 0,
    variadic: false,
    raw_operands: false,
    options: &[],
    passthrough: false,
    supported: false,
    operations: &[],
    schema_ids: &[],
};

/// The eighteen command lines, in the order of the command surface.
pub static LINES: [Line; 18] = [
    Line {
        words: &["project", "init"],
        synopsis: "project init [DEST]",
        optional: 1,
        ..UNSUPPORTED
    },
    Line {
        words: &["project", "inspect"],
        synopsis: "project inspect",
        ..UNSUPPORTED
    },
    Line {
        words: &["project", "add"],
        synopsis: "project add target|dependency DATA [--write]",
        first_operand: &["target", "dependency"],
        required: 2,
        options: &[WRITE],
        ..UNSUPPORTED
    },
    Line {
        words: &["project", "remove"],
        synopsis: "project remove target|dependency NAME [--write]",
        first_operand: &["target", "dependency"],
        required: 2,
        options: &[WRITE],
        ..UNSUPPORTED
    },
    Line {
        words: &["project", "sync"],
        synopsis: "project sync",
        ..UNSUPPORTED
    },
    Line {
        words: &["fmt"],
        synopsis: "fmt PATH [--write]",
        required: 1,
        options: &[WRITE],
        supported: true,
        operations: &["format"],
        schema_ids: &[COMMAND_RESULT_SCHEMA_ID, DIAGNOSTIC_SCHEMA_ID],
        ..UNSUPPORTED
    },
    Line {
        words: &["check"],
        synopsis: "check [TARGET]",
        optional: 1,
        supported: true,
        operations: &["workspaceCheck"],
        schema_ids: &[COMMAND_RESULT_SCHEMA_ID, DIAGNOSTIC_SCHEMA_ID],
        ..UNSUPPORTED
    },
    Line {
        words: &["lint"],
        synopsis: "lint [TARGET] [--write]",
        optional: 1,
        options: &[WRITE],
        ..UNSUPPORTED
    },
    Line {
        words: &["test"],
        synopsis: "test [TEST]",
        optional: 1,
        supported: true,
        operations: &["workspaceTest"],
        schema_ids: &[COMMAND_RESULT_SCHEMA_ID, DIAGNOSTIC_SCHEMA_ID],
        ..UNSUPPORTED
    },
    Line {
        words: &["run"],
        synopsis: "run [TARGET] [-- ARG...]",
        optional: 1,
        passthrough: true,
        supported: true,
        operations: &["workspaceRun"],
        schema_ids: &[COMMAND_RESULT_SCHEMA_ID, DIAGNOSTIC_SCHEMA_ID],
        ..UNSUPPORTED
    },
    Line {
        words: &["eval"],
        synopsis: "eval SUBJECT [--arg VIBON]...",
        required: 1,
        options: &[OptionSpec {
            name: "--arg",
            takes_value: true,
            repeatable: true,
        }],
        ..UNSUPPORTED
    },
    Line {
        words: &["build"],
        synopsis: "build [TARGET] [--target TRIPLE]",
        optional: 1,
        options: &[OptionSpec {
            name: "--target",
            takes_value: true,
            repeatable: false,
        }],
        ..UNSUPPORTED
    },
    Line {
        words: &["query"],
        synopsis: "query SUBJECT [--include METADATA] [--expand RELATIONS]",
        required: 1,
        options: &[
            OptionSpec {
                name: "--include",
                takes_value: true,
                repeatable: false,
            },
            OptionSpec {
                name: "--expand",
                takes_value: true,
                repeatable: false,
            },
        ],
        ..UNSUPPORTED
    },
    Line {
        words: &["edit", "fix"],
        synopsis: "edit fix SUBJECT [--write]",
        required: 1,
        options: &[WRITE],
        ..UNSUPPORTED
    },
    Line {
        words: &["edit", "rename"],
        synopsis: "edit rename SUBJECT NEWNAME [--write]",
        required: 2,
        options: &[WRITE],
        ..UNSUPPORTED
    },
    Line {
        words: &["edit", "imports"],
        synopsis: "edit imports PATH [--write]",
        required: 1,
        options: &[WRITE],
        ..UNSUPPORTED
    },
    Line {
        words: &["mcp"],
        synopsis: "mcp [SERVER-PERMISSION...]",
        variadic: true,
        raw_operands: true,
        ..UNSUPPORTED
    },
    Line {
        words: &["help"],
        synopsis: "help",
        supported: true,
        operations: &["help"],
        schema_ids: &[COMMAND_RESULT_SCHEMA_ID],
        ..UNSUPPORTED
    },
];

/// The row of `help`.
#[must_use]
pub fn help_line() -> &'static Line {
    &LINES[17]
}

/// The one table from an operation to the conformance profile it belongs to.
///
/// `docs/spec/07-diagnostics-and-conformance.md`, "Conformance profiles":
/// each case operation is the own operation of exactly one profile, and a
/// command's `profiles` are the profiles of the operations it offers. `help`
/// offers its own name, which `docs/spec/05-tooling.md`, `CommandCapability`,
/// gives `toolingV2`, whose CLI contract covers it. No row of [`LINES`]
/// names a profile.
pub static OPERATION_PROFILES: [(&str, &str); 11] = [
    ("reader", "readerV2"),
    ("projectDecode", "staticV2"),
    ("sourceGraph", "staticV2"),
    ("resolve", "staticV2"),
    ("workspaceCheck", "staticV2"),
    ("interpret", "interpreterV2"),
    ("workspaceRun", "interpreterV2"),
    ("workspaceTest", "interpreterV2"),
    ("format", "toolingV2"),
    ("query", "toolingV2"),
    ("help", "toolingV2"),
];

/// The profile of `operation`; `None` for a name the table does not hold.
#[must_use]
pub fn profile_of(operation: &str) -> Option<&'static str> {
    OPERATION_PROFILES
        .iter()
        .find(|(name, _)| *name == operation)
        .map(|(_, profile)| *profile)
}

/// The profiles of `line`'s operations, each once, in the order the
/// operations first name them.
#[must_use]
pub fn profiles(line: &Line) -> Vec<String> {
    let mut profiles: Vec<String> = Vec::new();
    for profile in line.operations.iter().filter_map(|name| profile_of(name)) {
        if !profiles.iter().any(|known| known == profile) {
            profiles.push(profile.to_owned());
        }
    }
    profiles
}

/// The `CommandCapability` of every row, in order.
#[must_use]
pub fn capabilities() -> Vec<CommandCapability> {
    LINES
        .iter()
        .map(|line| CommandCapability {
            command: line.name(),
            operations: line
                .operations
                .iter()
                .map(|text| (*text).to_owned())
                .collect(),
            profiles: profiles(line),
            supported: line.supported,
            schema_ids: line
                .schema_ids
                .iter()
                .map(|text| (*text).to_owned())
                .collect(),
        })
        .collect()
}

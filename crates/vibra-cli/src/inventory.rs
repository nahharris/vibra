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
    /// The operations a supported command offers.
    pub operations: &'static [&'static str],
    /// The conformance profiles that cover a supported command.
    pub profiles: &'static [&'static str],
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
    profiles: &[],
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
        profiles: &["readerV2"],
        schema_ids: &[COMMAND_RESULT_SCHEMA_ID, DIAGNOSTIC_SCHEMA_ID],
        ..UNSUPPORTED
    },
    Line {
        words: &["check"],
        synopsis: "check [TARGET]",
        optional: 1,
        supported: true,
        operations: &["workspaceCheck"],
        profiles: &["staticV2"],
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
        profiles: &["staticV2", "interpreterV2"],
        schema_ids: &[COMMAND_RESULT_SCHEMA_ID, DIAGNOSTIC_SCHEMA_ID],
        ..UNSUPPORTED
    },
    Line {
        words: &["run"],
        synopsis: "run [TARGET] [-- ARG...]",
        optional: 1,
        passthrough: true,
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
        profiles: &["toolingV2"],
        schema_ids: &[COMMAND_RESULT_SCHEMA_ID],
        ..UNSUPPORTED
    },
];

/// The row of `help`.
#[must_use]
pub fn help_line() -> &'static Line {
    &LINES[17]
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
            profiles: line
                .profiles
                .iter()
                .map(|text| (*text).to_owned())
                .collect(),
            supported: line.supported,
            schema_ids: line
                .schema_ids
                .iter()
                .map(|text| (*text).to_owned())
                .collect(),
        })
        .collect()
}

//! The command-line grammar.
//!
//! `docs/spec/05-tooling.md`, "Command surface": `GLOBAL` options, each at
//! most once and before the command; an unknown command or option, a repeated
//! single option, and an extra positional are invalid input; `help`, `--help`,
//! and `-h` in the command position are equivalent and take no arguments; and
//! there are no aliases. The grammar of each command line comes from the
//! support inventory, which is the one table all readers share.

use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::inventory::{LINES, Line, help_line};

/// The global output mode accepted before a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    /// Human-readable terminal output.
    Human,
    /// One versioned JSON envelope on standard output.
    Json,
}

/// The global options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Globals {
    /// `--format`, `human` by default.
    pub format: OutputFormat,
    /// `--workspace`, the current directory by default.
    pub workspace: PathBuf,
}

/// A well-formed command line.
#[derive(Clone, Debug)]
pub struct Invocation {
    /// The global options.
    pub globals: Globals,
    /// The row of the support inventory the command line selected.
    pub line: &'static Line,
    /// The operands, in order.
    pub operands: Vec<OsString>,
    /// The options after the command, in order, with their values.
    pub options: Vec<(&'static str, Option<OsString>)>,
    /// The program arguments after `--`; only `run` accepts them.
    pub program_arguments: Vec<OsString>,
}

/// Why a command line is invalid input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reason {
    /// No command followed the global options.
    MissingCommand,
    /// The first word names no command.
    UnknownCommand(String),
    /// A group of commands was given no known subcommand.
    UnknownSubcommand(String),
    /// A word that begins with `-` names no option here.
    UnknownOption(String),
    /// An option that may appear once appeared twice.
    RepeatedOption(&'static str),
    /// An option that takes a value is last.
    MissingOptionValue(String),
    /// `--format` was given something other than `human` or `json`.
    UnknownFormat,
    /// The command takes fewer operands than were given.
    ExtraOperand(String),
    /// The command needs more operands than were given.
    MissingOperand,
    /// The first operand is not one of the spellings the command allows.
    UnknownKind(String),
}

impl fmt::Display for Reason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCommand => formatter.write_str("no command was given"),
            Self::UnknownCommand(word) => write!(formatter, "unknown command `{word}`"),
            Self::UnknownSubcommand(word) => {
                write!(formatter, "unknown subcommand `{word}`")
            }
            Self::UnknownOption(word) => write!(formatter, "unknown option `{word}`"),
            Self::RepeatedOption(name) => {
                write!(formatter, "the option `{name}` may be given only once")
            }
            Self::MissingOptionValue(name) => {
                write!(formatter, "the option `{name}` needs a value")
            }
            Self::UnknownFormat => {
                formatter.write_str("`--format` is `human` or `json`")
            }
            Self::ExtraOperand(word) => {
                write!(formatter, "unexpected argument `{word}`")
            }
            Self::MissingOperand => {
                formatter.write_str("a required argument is missing")
            }
            Self::UnknownKind(word) => write!(formatter, "unexpected `{word}`"),
        }
    }
}

/// A command line that is invalid input.
///
/// `format` is the output format recognized before the fault, so the refusal
/// is reported in the format the caller asked for when it was asked for
/// validly. `command` is the recognized command name, or empty.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rejection {
    /// The format recognized so far.
    pub format: OutputFormat,
    /// The recognized command name, or the empty string.
    pub command: String,
    /// What was wrong.
    pub reason: Reason,
}

fn text(token: &OsString) -> String {
    token.to_string_lossy().into_owned()
}

/// Reads a command line.
///
/// `current_directory` is the workspace when no `--workspace` is given, and
/// the base of a relative `--workspace`.
///
/// # Errors
///
/// A [`Rejection`] when the command line is invalid input.
pub fn parse(
    arguments: &[OsString],
    current_directory: &Path,
) -> Result<Invocation, Rejection> {
    let mut format = OutputFormat::Human;
    let mut format_given = false;
    let mut workspace: Option<PathBuf> = None;
    let mut index = 0;
    let reject = |format: OutputFormat, command: &str, reason: Reason| Rejection {
        format,
        command: command.to_owned(),
        reason,
    };

    while let Some(token) = arguments.get(index) {
        match token.to_str() {
            Some("--format") => {
                if format_given {
                    return Err(reject(format, "", Reason::RepeatedOption("--format")));
                }
                let value = arguments.get(index + 1).ok_or_else(|| {
                    reject(
                        format,
                        "",
                        Reason::MissingOptionValue("--format".to_owned()),
                    )
                })?;
                format = match value.to_str() {
                    Some("human") => OutputFormat::Human,
                    Some("json") => OutputFormat::Json,
                    _ => return Err(reject(format, "", Reason::UnknownFormat)),
                };
                format_given = true;
                index += 2;
            }
            Some("--workspace") => {
                if workspace.is_some() {
                    return Err(reject(
                        format,
                        "",
                        Reason::RepeatedOption("--workspace"),
                    ));
                }
                let value = arguments.get(index + 1).ok_or_else(|| {
                    reject(
                        format,
                        "",
                        Reason::MissingOptionValue("--workspace".to_owned()),
                    )
                })?;
                workspace = Some(current_directory.join(value));
                index += 2;
            }
            _ => break,
        }
    }
    let globals = Globals {
        format,
        workspace: workspace.unwrap_or_else(|| current_directory.to_path_buf()),
    };

    let Some(first) = arguments.get(index) else {
        return Err(reject(format, "", Reason::MissingCommand));
    };
    let first_text = first.to_str();
    let (line, consumed) = if matches!(first_text, Some("--help" | "-h")) {
        (help_line(), 1)
    } else {
        let word = first_text.unwrap_or_default();
        let group = LINES
            .iter()
            .filter(|line| line.words.first() == Some(&word))
            .collect::<Vec<_>>();
        if group.is_empty() {
            return Err(reject(format, "", Reason::UnknownCommand(text(first))));
        }
        match group.iter().find(|line| line.words.len() == 1) {
            Some(line) => (*line, 1),
            None => {
                let second = arguments.get(index + 1);
                let found = second.and_then(|second| {
                    group
                        .iter()
                        .find(|line| line.words.get(1).copied() == second.to_str())
                });
                match (found, second) {
                    (Some(line), _) => (*line, 2),
                    (None, Some(second)) => {
                        return Err(reject(
                            format,
                            "",
                            Reason::UnknownSubcommand(text(second)),
                        ));
                    }
                    (None, None) => {
                        return Err(reject(format, "", Reason::MissingCommand));
                    }
                }
            }
        }
    };

    let name = line.name();
    let rest = arguments.get(index + consumed..).unwrap_or_default();
    let mut operands: Vec<OsString> = Vec::new();
    let mut options: Vec<(&'static str, Option<OsString>)> = Vec::new();
    let mut program_arguments = Vec::new();
    let mut tokens = rest.iter();
    while let Some(token) = tokens.next() {
        let word = token.to_str();
        if line.raw_operands {
            operands.push(token.clone());
        } else if word == Some("--") && line.passthrough {
            program_arguments.extend(tokens.by_ref().cloned());
        } else if let Some(word) =
            word.filter(|word| word.len() > 1 && word.starts_with('-'))
        {
            let spec = line
                .options
                .iter()
                .find(|spec| spec.name == word)
                .ok_or_else(|| {
                    reject(format, &name, Reason::UnknownOption(word.to_owned()))
                })?;
            if !spec.repeatable && options.iter().any(|(given, _)| *given == spec.name)
            {
                return Err(reject(format, &name, Reason::RepeatedOption(spec.name)));
            }
            let value = if spec.takes_value {
                Some(tokens.next().cloned().ok_or_else(|| {
                    reject(format, &name, Reason::MissingOptionValue(word.to_owned()))
                })?)
            } else {
                None
            };
            options.push((spec.name, value));
        } else {
            operands.push(token.clone());
        }
    }

    if !line.variadic
        && let Some(extra) = operands.get(line.required + line.optional)
    {
        return Err(reject(format, &name, Reason::ExtraOperand(text(extra))));
    }
    if operands.len() < line.required {
        return Err(reject(format, &name, Reason::MissingOperand));
    }
    if !line.first_operand.is_empty()
        && let Some(kind) = operands.first()
        && !line
            .first_operand
            .iter()
            .any(|allowed| kind.to_str() == Some(allowed))
    {
        return Err(reject(format, &name, Reason::UnknownKind(text(kind))));
    }

    Ok(Invocation {
        globals,
        line,
        operands,
        options,
        program_arguments,
    })
}

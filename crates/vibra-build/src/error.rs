//! Why a build produces no executable.

use std::fmt;
use std::path::PathBuf;

use crate::link::LinkCommand;

/// Why [`crate::build_executable`] or one of its steps produced no
/// executable. No variant leaves an executable at the output path.
#[derive(Debug)]
pub enum BuildError {
    /// The host or the target is not one the driver implements: the driver
    /// discovers and runs the MSVC linker, so it builds `x86_64-pc-windows-msvc`
    /// executables on a Windows host and nothing else.
    Unsupported {
        /// The operating system of the host, as `std::env::consts::OS` names it.
        host: &'static str,
        /// The triple that was asked for.
        target: &'static str,
    },
    /// No linker, or no library the linker needs, was found.
    ///
    /// The diagnostic this maps to is `@tool.missing-link-tool`, named by the
    /// specification package X3-S1; the registry does not hold it yet, so the
    /// binding is owed after that package merges.
    MissingLinkTool {
        /// What was looked for and where, in the order the discovery rule
        /// looks, each as one line of text.
        searched: Vec<String>,
    },
    /// The linker ran and failed, or could not be started.
    ///
    /// The diagnostic this maps to is `@tool.link-failed`, named by the
    /// specification package X3-S1; the binding is owed after that package
    /// merges.
    LinkFailed {
        /// The command that was run.
        command: LinkCommand,
        /// The exit code, or `None` when the process did not start or did not
        /// exit with a code.
        status: Option<i32>,
        /// The linker's standard output and standard error, in that order, as
        /// lossy UTF-8; or the operating system's reason the process did not
        /// start.
        output: String,
    },
    /// The lowering refused the checked program.
    Lower(vibra_lower::LowerError),
    /// The lowered program did not verify.
    Verify(vibra_exec::VerifyError),
    /// The code generator refused the program.
    Emit(vibra_codegen::EmitError),
    /// A file or directory of the build could not be read or written.
    Io {
        /// What the driver was doing.
        action: &'static str,
        /// The path it was doing it to.
        path: PathBuf,
        /// The operating system's message.
        message: String,
    },
}

impl fmt::Display for BuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported { host, target } => write!(
                formatter,
                "cannot build for `{target}` on a `{host}` host: only \
                 x86_64-pc-windows-msvc on Windows is implemented"
            ),
            Self::MissingLinkTool { searched } => {
                write!(formatter, "no usable MSVC linker was found; searched:")?;
                for line in searched {
                    write!(formatter, "\n  {line}")?;
                }
                Ok(())
            }
            Self::LinkFailed {
                command,
                status,
                output,
            } => {
                match status {
                    Some(code) => write!(formatter, "the linker exited with {code}")?,
                    None => write!(formatter, "the linker did not complete")?,
                }
                write!(formatter, "\ncommand: {command}\n{output}")
            }
            Self::Lower(error) => write!(formatter, "lowering failed: {error}"),
            Self::Verify(error) => {
                write!(formatter, "the lowered program does not verify: {error}")
            }
            Self::Emit(error) => write!(formatter, "{error}"),
            Self::Io {
                action,
                path,
                message,
            } => write!(formatter, "cannot {action} `{}`: {message}", path.display()),
        }
    }
}

impl std::error::Error for BuildError {}

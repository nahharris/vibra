//! The link command, as a value, and its execution.
//!
//! The argument list is the one the feasibility record
//! (`docs/roadmap/v0.0.3/m1/feasibility.md`, "The link and run") observed to
//! link: the same options, the same libraries, in the same order. The library
//! directories are `/LIBPATH:` arguments; the `LIB` variable is never used.

use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::error::BuildError;
use crate::system;
use crate::toolchain::Toolchain;

/// The system libraries the runtime archive needs, as
/// `rustc --print native-static-libs` printed them for the archive, with the
/// default C runtime last.
pub const SYSTEM_LIBRARIES: [&str; 6] = [
    "kernel32.lib",
    "ntdll.lib",
    "userenv.lib",
    "ws2_32.lib",
    "dbghelp.lib",
    "/defaultlib:msvcrt",
];

/// A process to run: a program and its arguments. It is run without a shell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkCommand {
    /// The absolute path of `link.exe`.
    pub program: PathBuf,
    /// The arguments, one element each, unquoted.
    pub args: Vec<OsString>,
}

impl fmt::Display for LinkCommand {
    /// The command as a line of text, with quotes around the program and each
    /// argument that has a space, to show where the arguments begin and end.
    /// The process is started with an argument list, not with this text.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = vec![self.program.as_os_str()];
        parts.extend(self.args.iter().map(OsString::as_os_str));
        for (position, part) in parts.into_iter().enumerate() {
            let text = part.to_string_lossy();
            if position > 0 {
                formatter.write_str(" ")?;
            }
            if text.contains(' ') {
                write!(formatter, "\"{text}\"")?;
            } else {
                formatter.write_str(&text)?;
            }
        }
        Ok(())
    }
}

/// The link command for `object` and `archive` into `output`:
///
/// ```text
/// <link.exe> /NOLOGO /MACHINE:X64 /SUBSYSTEM:CONSOLE /OUT:<output> <object> <archive>
///     /LIBPATH:<msvc lib\x64> /LIBPATH:<sdk ucrt\x64> /LIBPATH:<sdk um\x64>
///     kernel32.lib ntdll.lib userenv.lib ws2_32.lib dbghelp.lib /defaultlib:msvcrt
/// ```
///
/// A pure function of its arguments.
#[must_use]
pub fn link_command(
    toolchain: &Toolchain,
    object: &Path,
    archive: &Path,
    output: &Path,
) -> LinkCommand {
    let mut args: Vec<OsString> = vec![
        "/NOLOGO".into(),
        "/MACHINE:X64".into(),
        "/SUBSYSTEM:CONSOLE".into(),
    ];
    args.push(prefixed("/OUT:", output));
    args.push(object.into());
    args.push(archive.into());
    args.extend(
        toolchain
            .library_dirs
            .iter()
            .map(|directory| prefixed("/LIBPATH:", directory)),
    );
    args.extend(SYSTEM_LIBRARIES.iter().map(OsString::from));
    LinkCommand {
        program: toolchain.link.clone(),
        args,
    }
}

fn prefixed(option: &str, path: &Path) -> OsString {
    let mut text = OsString::from(option);
    text.push(path);
    text
}

/// The environment `link.exe` runs in: `TMP`, set to the work directory, and
/// nothing else. `PATH`, `LIB`, `SystemRoot`, and the rest are absent.
///
/// `TMP` is what the linker demonstrably needs: it writes `lnk*.tmp` to the
/// temporary directory, and with no temporary directory in its environment it
/// falls back to the Windows directory and fails with `LNK1104`. Observed by
/// linking with an empty environment (`LNK1104`), with `SystemRoot` alone
/// (`LNK1104`), and with `TMP` alone or `TEMP` alone (success). Setting it to
/// the work directory keeps the linker's temporary files there.
#[must_use]
pub fn link_environment(work_dir: &Path) -> Vec<(OsString, OsString)> {
    vec![(OsString::from("TMP"), work_dir.into())]
}

/// Runs `command` in `environment` and waits.
///
/// # Errors
///
/// [`BuildError::LinkFailed`] when the process did not start or did not exit
/// with zero. The output path is not touched here; [`crate::build_program`]
/// removes it on failure.
pub fn run_link(
    command: &LinkCommand,
    environment: &[(OsString, OsString)],
) -> Result<String, BuildError> {
    let ran = system::run(&command.program, &command.args, environment);
    if ran.success {
        Ok(ran.output)
    } else {
        Err(BuildError::LinkFailed {
            command: command.clone(),
            status: ran.code,
            output: ran.output,
        })
    }
}

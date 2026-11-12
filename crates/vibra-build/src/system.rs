//! The machine: the one module of this crate that reads the environment,
//! starts processes, and touches the filesystem.
//!
//! Everything else in the crate is a function of values (`Sources`,
//! `Toolchain`, `LinkCommand`) and of the answers of the functions here. The
//! architecture tests scan the crate's other modules for these APIs
//! (`the_build_driver_reads_the_machine_in_one_module`).

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::error::BuildError;

/// How a process ended: its exit code if it had one, and its output.
pub(crate) struct Ran {
    /// The exit code; `None` when the process did not start or was ended by a
    /// signal.
    pub(crate) code: Option<i32>,
    /// Standard output, then standard error, as lossy UTF-8; or the reason the
    /// process did not start.
    pub(crate) output: String,
    /// Whether the process exited with code zero.
    pub(crate) success: bool,
}

/// The operating system of the host.
pub(crate) const fn host_os() -> &'static str {
    std::env::consts::OS
}

/// Whether the host is Windows.
pub(crate) const fn host_is_windows() -> bool {
    cfg!(windows)
}

/// The value of an environment variable when it is set and not empty.
pub(crate) fn env_value(name: &str) -> Option<OsString> {
    std::env::var_os(name).filter(|value| !value.is_empty())
}

pub(crate) fn is_file(path: &Path) -> bool {
    path.is_file()
}

pub(crate) fn is_dir(path: &Path) -> bool {
    path.is_dir()
}

/// The text of a file, or `None` when it cannot be read.
pub(crate) fn read_text(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// The names of the entries of a directory, sorted; empty when it cannot be
/// read.
pub(crate) fn entry_names(path: &Path) -> Vec<String> {
    let mut names = std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect::<Vec<_>>();
    names.sort();
    names
}

fn io_error(action: &'static str, path: &Path, error: &std::io::Error) -> BuildError {
    BuildError::Io {
        action,
        path: path.to_owned(),
        message: error.to_string(),
    }
}

pub(crate) fn create_dir_all(path: &Path) -> Result<(), BuildError> {
    std::fs::create_dir_all(path)
        .map_err(|error| io_error("create the directory", path, &error))
}

pub(crate) fn write_file(path: &Path, bytes: &[u8]) -> Result<(), BuildError> {
    std::fs::write(path, bytes)
        .map_err(|error| io_error("write the file", path, &error))
}

/// Removes a file; a file that is not there is not an error.
pub(crate) fn remove_file(path: &Path) -> Result<(), BuildError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error("remove the file", path, &error)),
    }
}

/// Runs `program` with exactly `environment` and no standard input, and waits.
/// No shell is involved and `PATH` is not consulted: `program` is a path.
pub(crate) fn run(
    program: &Path,
    args: &[OsString],
    environment: &[(OsString, OsString)],
) -> Ran {
    let started = Command::new(program)
        .args(args)
        .env_clear()
        .envs(environment.iter().map(|(name, value)| (name, value)))
        .stdin(Stdio::null())
        .output();
    match started {
        Ok(output) => {
            let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
            text.push_str(&String::from_utf8_lossy(&output.stderr));
            Ran {
                code: output.status.code(),
                success: output.status.success(),
                output: text,
            }
        }
        Err(error) => Ran {
            code: None,
            success: false,
            output: format!("the process did not start: {error}"),
        },
    }
}

/// The environment of the tools the driver asks about the machine: the system
/// directory and the program-data directory, and nothing else. `vswhere.exe`
/// answers with nothing when `ProgramData` is missing: it reads the
/// installer's state from there.
fn query_environment() -> Vec<(OsString, OsString)> {
    ["SystemRoot", "ProgramData"]
        .into_iter()
        .filter_map(|name| env_value(name).map(|value| (OsString::from(name), value)))
        .collect()
}

/// The standard output of `vswhere.exe` when it exits with zero.
pub(crate) fn query_vswhere(vswhere: &Path) -> Option<String> {
    let args = [
        "-latest",
        "-products",
        "*",
        "-requires",
        "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
        "-property",
        "installationPath",
    ]
    .map(OsString::from);
    let ran = run(vswhere, &args, &query_environment());
    ran.success.then_some(ran.output)
}

/// The standard output of `reg.exe query` for the Windows SDK root, in the
/// 32-bit registry view, when it exits with zero. `reg.exe` is addressed by
/// its absolute path under the system directory, never through `PATH`.
pub(crate) fn query_kits_root() -> Option<String> {
    let system_root = env_value("SystemRoot")?;
    let reg = PathBuf::from(system_root).join("System32").join("reg.exe");
    let args = [
        OsStr::new("query"),
        OsStr::new(r"HKLM\SOFTWARE\Microsoft\Windows Kits\Installed Roots"),
        OsStr::new("/v"),
        OsStr::new("KitsRoot10"),
        OsStr::new("/reg:32"),
    ]
    .map(OsStr::to_os_string);
    let ran = run(&reg, &args, &query_environment());
    ran.success.then_some(ran.output)
}

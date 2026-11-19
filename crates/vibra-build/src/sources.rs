//! The ambient facts toolchain discovery starts from, as a value.

use std::path::PathBuf;

use crate::error::BuildError;
use crate::system;

/// Where Windows keeps the 32-bit program directory when the environment does
/// not say.
const DEFAULT_PROGRAM_FILES_X86: &str = r"C:\Program Files (x86)";

/// The ambient facts discovery needs, read once by [`Sources::from_machine`]
/// and handed to [`crate::Toolchain::discover`], which reads nothing but the
/// filesystem below them.
///
/// A `None` is a fact that was not found. Discovery never replaces it by
/// looking at `PATH`, `VCINSTALLDIR`, `LIB`, or `INCLUDE`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sources {
    /// The 32-bit program directory (`ProgramFiles(x86)`): the standard
    /// location of the Windows SDK when the registry names none.
    pub program_files_x86: Option<PathBuf>,
    /// The installation path of the newest Visual Studio instance that has the
    /// component `Microsoft.VisualStudio.Component.VC.Tools.x86.x64`, as
    /// `vswhere.exe` reports it.
    pub visual_studio: Option<PathBuf>,
    /// The Windows SDK root, the value `KitsRoot10` of the registry key
    /// `HKLM\SOFTWARE\Microsoft\Windows Kits\Installed Roots`.
    pub kits_root: Option<PathBuf>,
}

impl Sources {
    /// Reads the sources from this machine: the `ProgramFiles(x86)` and
    /// `SystemRoot` variables, `vswhere.exe` at its documented location under
    /// the 32-bit program directory, and `reg.exe` under the system directory.
    /// Both programs are run by absolute path with an explicit environment.
    ///
    /// A source that cannot be read is `None` in the result.
    ///
    /// # Errors
    ///
    /// [`BuildError::Unsupported`] on a host other than Windows.
    pub fn from_machine() -> Result<Self, BuildError> {
        if !system::host_is_windows() {
            return Err(BuildError::Unsupported {
                host: system::host_os(),
                target: vibra_codegen::Target::WINDOWS_X64.triple(),
            });
        }
        let program_files_x86 = system::env_value("ProgramFiles(x86)")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_PROGRAM_FILES_X86));
        let vswhere = program_files_x86
            .join("Microsoft Visual Studio")
            .join("Installer")
            .join("vswhere.exe");
        let visual_studio = system::is_file(&vswhere)
            .then(|| system::query_vswhere(&vswhere))
            .flatten()
            .and_then(|text| parse_installation_path(&text));
        let kits_root =
            system::query_kits_root().and_then(|text| parse_kits_root(&text));
        Ok(Self {
            program_files_x86: Some(program_files_x86),
            visual_studio,
            kits_root,
        })
    }
}

/// The installation path `vswhere.exe -property installationPath` printed: its
/// first non-empty line.
#[must_use]
pub fn parse_installation_path(output: &str) -> Option<PathBuf> {
    output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(PathBuf::from)
}

/// The value `reg.exe query ... /v KitsRoot10` printed: the text after
/// `REG_SZ` on the line that names the value.
#[must_use]
pub fn parse_kits_root(output: &str) -> Option<PathBuf> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("KitsRoot10"))
        .find_map(|line| line.split_once("REG_SZ"))
        .map(|(_, value)| value.trim())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

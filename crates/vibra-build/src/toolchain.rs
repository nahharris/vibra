//! Toolchain discovery: a pure function of [`Sources`] and the filesystem. The
//! rule is documented on [`Toolchain::discover`].

use std::path::{Path, PathBuf};

use crate::error::BuildError;
use crate::sources::Sources;
use crate::system;

/// The MSVC linker and the directories of the libraries it links against, as
/// absolute paths found by [`Toolchain::discover`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Toolchain {
    /// `link.exe`, which exists.
    pub link: PathBuf,
    /// The library directories, in the order they are passed to the linker: the
    /// MSVC `lib\x64`, the SDK's `ucrt\x64`, and the SDK's `um\x64`.
    pub library_dirs: Vec<PathBuf>,
    /// The MSVC toolset version that was chosen, such as `14.43.34808`.
    pub msvc_version: String,
    /// The Windows SDK version that was chosen, such as `10.0.22621.0`.
    pub sdk_version: String,
}

impl Toolchain {
    /// Finds the linker and the library directories under `sources`.
    ///
    /// # The rule
    ///
    /// Nothing here consults `PATH`, and nothing relies on a developer command
    /// prompt: `VCINSTALLDIR`, `LIB`, and `INCLUDE` are never read. Every path
    /// is built from a source and checked on disk.
    ///
    /// 1. The Visual Studio installation is `Sources::visual_studio` (what
    ///    `vswhere.exe -latest -products * -requires
    ///    Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property
    ///    installationPath` reported). The MSVC toolset is the single line of
    ///    `<install>\VC\Auxiliary\Build\Microsoft.VCToolsVersion.default.txt`,
    ///    trimmed, the toolset the installation marks as default; if that file
    ///    is missing or names no directory, the highest directory name under
    ///    `<install>\VC\Tools\MSVC`, compared as dot-separated integers.
    /// 2. The linker is
    ///    `<install>\VC\Tools\MSVC\<toolset>\bin\Hostx64\x64\link.exe` and the
    ///    MSVC library directory is `<install>\VC\Tools\MSVC\<toolset>\lib\x64`,
    ///    which must hold `msvcrt.lib`.
    /// 3. The Windows SDK root is `Sources::kits_root`, or
    ///    `<program_files_x86>\Windows Kits\10` when the registry named none.
    ///    Its version is the highest directory name of `<root>\Lib`, compared
    ///    as dot-separated integers, that holds both `um\x64\kernel32.lib` and
    ///    `ucrt\x64\ucrt.lib`; a name that is not dot-separated integers is
    ///    never chosen.
    /// 4. The library directories, in order, are the MSVC `lib\x64`, then the
    ///    SDK's `ucrt\x64`, then its `um\x64`.
    ///
    /// # Errors
    ///
    /// [`BuildError::MissingLinkTool`], listing everything that was looked for
    /// and where, when the linker or any library directory is missing.
    pub fn discover(sources: &Sources) -> Result<Self, BuildError> {
        let mut searched = Vec::new();
        let msvc = find_msvc(sources, &mut searched);
        let sdk = find_sdk(sources, &mut searched);
        match (msvc, sdk) {
            (Some(msvc), Some(sdk)) => Ok(Self {
                link: msvc.link,
                library_dirs: vec![msvc.library_dir, sdk.ucrt, sdk.um],
                msvc_version: msvc.version,
                sdk_version: sdk.version,
            }),
            _ => Err(BuildError::MissingLinkTool { searched }),
        }
    }

    /// Discovers the toolchain of this machine: [`Sources::from_machine`] and
    /// then [`Toolchain::discover`].
    ///
    /// # Errors
    ///
    /// [`BuildError::Unsupported`] off Windows, otherwise as
    /// [`Toolchain::discover`].
    pub fn from_machine() -> Result<Self, BuildError> {
        Self::discover(&Sources::from_machine()?)
    }
}

struct Msvc {
    link: PathBuf,
    library_dir: PathBuf,
    version: String,
}

struct Sdk {
    ucrt: PathBuf,
    um: PathBuf,
    version: String,
}

/// `name` as dot-separated integers; `None` when it is anything else.
fn version_key(name: &str) -> Option<Vec<u64>> {
    name.split('.')
        .map(|part| part.parse::<u64>().ok())
        .collect()
}

/// The entry of `names` with the highest version, compared numerically, among
/// those `keep` accepts.
fn highest(names: &[String], keep: impl Fn(&str) -> bool) -> Option<String> {
    names
        .iter()
        .filter(|name| keep(name))
        .filter_map(|name| version_key(name).map(|key| (key, name)))
        .max()
        .map(|(_, name)| name.clone())
}

fn find_msvc(sources: &Sources, searched: &mut Vec<String>) -> Option<Msvc> {
    let Some(install) = &sources.visual_studio else {
        searched.push(
            "a Visual Studio installation with the component \
             Microsoft.VisualStudio.Component.VC.Tools.x86.x64: vswhere.exe reported none"
                .to_owned(),
        );
        return None;
    };
    let tools = install.join("VC").join("Tools").join("MSVC");
    let default_file = install
        .join("VC")
        .join("Auxiliary")
        .join("Build")
        .join("Microsoft.VCToolsVersion.default.txt");
    let named = system::read_text(&default_file)
        .map(|text| text.trim().to_owned())
        .filter(|version| !version.is_empty() && system::is_dir(&tools.join(version)));
    let version = named.or_else(|| {
        highest(&system::entry_names(&tools), |name| {
            system::is_dir(&tools.join(name))
        })
    });
    let Some(version) = version else {
        searched.push(format!(
            "the default MSVC toolset: {} (absent or naming no directory), then the highest \
             version directory under {}",
            default_file.display(),
            tools.display()
        ));
        return None;
    };
    let toolset = tools.join(&version);
    let link = toolset
        .join("bin")
        .join("Hostx64")
        .join("x64")
        .join("link.exe");
    let library_dir = toolset.join("lib").join("x64");
    let mut complete = true;
    if !system::is_file(&link) {
        searched.push(format!("link.exe: {}", link.display()));
        complete = false;
    }
    let msvcrt = library_dir.join("msvcrt.lib");
    if !system::is_file(&msvcrt) {
        searched.push(format!("msvcrt.lib: {}", msvcrt.display()));
        complete = false;
    }
    complete.then_some(Msvc {
        link,
        library_dir,
        version,
    })
}

fn find_sdk(sources: &Sources, searched: &mut Vec<String>) -> Option<Sdk> {
    let root = sources.kits_root.clone().or_else(|| {
        sources
            .program_files_x86
            .as_ref()
            .map(|files| files.join("Windows Kits").join("10"))
    });
    let Some(root) = root else {
        searched.push(
            "the Windows SDK root: the registry value KitsRoot10 is absent and no \
             ProgramFiles(x86) directory is known"
                .to_owned(),
        );
        return None;
    };
    let lib = root.join("Lib");
    let holds_libraries = |version: &str| sdk_libraries(&lib.join(version)).is_some();
    let version = highest(&system::entry_names(&lib), holds_libraries);
    let Some(version) = version else {
        searched.push(format!(
            "a Windows SDK version under {} holding um\\x64\\kernel32.lib and ucrt\\x64\\ucrt.lib",
            lib.display()
        ));
        return None;
    };
    let (ucrt, um) = sdk_libraries(&lib.join(&version))?;
    Some(Sdk { ucrt, um, version })
}

/// The `ucrt\x64` and `um\x64` directories of an SDK version when each holds
/// the library that proves it.
fn sdk_libraries(version_dir: &Path) -> Option<(PathBuf, PathBuf)> {
    let ucrt = version_dir.join("ucrt").join("x64");
    let um = version_dir.join("um").join("x64");
    (system::is_file(&ucrt.join("ucrt.lib"))
        && system::is_file(&um.join("kernel32.lib")))
    .then_some((ucrt, um))
}

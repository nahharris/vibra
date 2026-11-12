//! Toolchain discovery: a function of `Sources` and the filesystem, never of
//! `PATH` (ledger N13: on the development machine `link.exe` on `PATH` is
//! BusyBox).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use vibra_build::{BuildError, Sources, Toolchain};

/// A directory under cargo's per-target scratch directory, removed with the
/// value.
struct Tree {
    root: PathBuf,
}

impl Tree {
    fn new(label: &str) -> Self {
        static SERIAL: AtomicUsize = AtomicUsize::new(0);
        let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
            "discovery-{label}-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a scratch directory");
        Self { root }
    }

    fn file(&self, relative: &str) {
        let path = self.root.join(relative);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("directories");
        std::fs::write(path, b"x").expect("a file");
    }

    fn text(&self, relative: &str, text: &str) {
        let path = self.root.join(relative);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("directories");
        std::fs::write(path, text).expect("a file");
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A toolset directory that holds everything discovery checks for.
fn toolset(tree: &Tree, install: &str, version: &str) {
    let base = format!("{install}/VC/Tools/MSVC/{version}");
    tree.file(&format!("{base}/bin/Hostx64/x64/link.exe"));
    tree.file(&format!("{base}/lib/x64/msvcrt.lib"));
}

/// An SDK version that holds both libraries.
fn sdk(tree: &Tree, root: &str, version: &str) {
    tree.file(&format!("{root}/Lib/{version}/ucrt/x64/ucrt.lib"));
    tree.file(&format!("{root}/Lib/{version}/um/x64/kernel32.lib"));
}

fn sources(tree: &Tree, install: &str, kits: &str) -> Sources {
    Sources {
        program_files_x86: None,
        visual_studio: Some(tree.path(install)),
        kits_root: Some(tree.path(kits)),
    }
}

#[test]
fn the_default_toolset_wins_and_the_highest_sdk_is_chosen_numerically() {
    let tree = Tree::new("rule");
    // 14.9 is the installation's default; 14.10 is newer by number and, as
    // text, older.
    toolset(&tree, "vs", "14.9.1");
    toolset(&tree, "vs", "14.10.1");
    tree.text(
        "vs/VC/Auxiliary/Build/Microsoft.VCToolsVersion.default.txt",
        "14.9.1\r\n",
    );
    sdk(&tree, "kits", "10.0.9.0");
    sdk(&tree, "kits", "10.0.22621.0");
    // Newer than both, and not a usable SDK: it holds no `ucrt`.
    tree.file("kits/Lib/10.0.26100.0/um/x64/kernel32.lib");
    // And a name that is not a version.
    sdk(&tree, "kits", "wdf");
    let found = Toolchain::discover(&sources(&tree, "vs", "kits")).expect("found");
    assert_eq!(found.msvc_version, "14.9.1");
    assert_eq!(found.sdk_version, "10.0.22621.0");
    assert_eq!(
        found.link,
        tree.path("vs/VC/Tools/MSVC/14.9.1/bin/Hostx64/x64/link.exe")
    );
    assert_eq!(
        found.library_dirs,
        [
            tree.path("vs/VC/Tools/MSVC/14.9.1/lib/x64"),
            tree.path("kits/Lib/10.0.22621.0/ucrt/x64"),
            tree.path("kits/Lib/10.0.22621.0/um/x64"),
        ]
    );
}

#[test]
fn without_a_default_file_the_highest_toolset_is_chosen_numerically() {
    let tree = Tree::new("highest");
    toolset(&tree, "vs", "14.9.1");
    toolset(&tree, "vs", "14.10.1");
    sdk(&tree, "kits", "10.0.1.0");
    let found = Toolchain::discover(&sources(&tree, "vs", "kits")).expect("found");
    assert_eq!(found.msvc_version, "14.10.1");
    // A default file that names a directory that is not there is not trusted.
    tree.text(
        "vs/VC/Auxiliary/Build/Microsoft.VCToolsVersion.default.txt",
        "14.99.0\n",
    );
    let found = Toolchain::discover(&sources(&tree, "vs", "kits")).expect("found");
    assert_eq!(found.msvc_version, "14.10.1");
}

#[test]
fn the_sdk_falls_back_to_the_standard_place_under_the_program_directory() {
    let tree = Tree::new("fallback");
    toolset(&tree, "vs", "14.1.0");
    sdk(&tree, "pf/Windows Kits/10", "10.0.1.0");
    let found = Toolchain::discover(&Sources {
        program_files_x86: Some(tree.path("pf")),
        visual_studio: Some(tree.path("vs")),
        kits_root: None,
    })
    .expect("found");
    assert_eq!(found.sdk_version, "10.0.1.0");
}

#[test]
fn the_missing_tool_error_says_what_was_looked_for_and_where() {
    let tree = Tree::new("missing");
    // Nothing known at all.
    let error = Toolchain::discover(&Sources::default()).expect_err("nothing");
    let BuildError::MissingLinkTool { searched } = &error else {
        panic!("not a missing tool: {error}");
    };
    assert_eq!(searched.len(), 2, "{searched:#?}");
    assert!(
        searched[0].contains("vswhere.exe reported none"),
        "{searched:#?}"
    );
    assert!(searched[1].contains("Windows SDK root"), "{searched:#?}");

    // Everything named and nothing there: the same lines every time.
    let nowhere = Sources {
        program_files_x86: Some(tree.path("pf")),
        visual_studio: Some(tree.path("vs")),
        kits_root: Some(tree.path("kits")),
    };
    let first = Toolchain::discover(&nowhere).expect_err("nothing there");
    let second = Toolchain::discover(&nowhere).expect_err("nothing there");
    assert_eq!(first.to_string(), second.to_string());
    let BuildError::MissingLinkTool { searched } = &first else {
        panic!("not a missing tool: {first}");
    };
    assert_eq!(searched.len(), 2, "{searched:#?}");
    assert!(searched[0].contains("Microsoft.VCToolsVersion.default.txt"));
    assert!(searched[1].contains("ucrt\\x64\\ucrt.lib"));
    assert!(
        first
            .to_string()
            .starts_with("no usable MSVC linker was found; searched:\n  ")
    );

    // The toolset exists but the linker does not: the path is named.
    tree.file("vs/VC/Tools/MSVC/14.1.0/lib/x64/msvcrt.lib");
    sdk(&tree, "kits", "10.0.1.0");
    let error = Toolchain::discover(&nowhere).expect_err("no link.exe");
    let BuildError::MissingLinkTool { searched } = &error else {
        panic!("not a missing tool: {error}");
    };
    assert_eq!(searched.len(), 1, "{searched:#?}");
    assert!(searched[0].starts_with("link.exe: "), "{searched:#?}");
    assert!(searched[0].ends_with("link.exe"), "{searched:#?}");
}

/// The marker that makes the test below run as its own child.
const CHILD: &str = "VIBRA_BUILD_DISCOVERY_CHILD";

#[cfg(windows)]
#[test]
fn the_linker_is_found_without_path() {
    if std::env::var_os(CHILD).is_some() {
        // The child: `PATH` is empty. Report what discovery finds.
        let path = std::env::var_os("PATH").unwrap_or_default();
        println!("DISCOVERY-PATH-EMPTY={}", path.is_empty());
        match Toolchain::from_machine() {
            Ok(toolchain) => println!("DISCOVERY-LINK={}", toolchain.link.display()),
            Err(error) => println!("DISCOVERY-ERROR={error}"),
        }
        return;
    }
    let mut command =
        std::process::Command::new(std::env::current_exe().expect("the test binary"));
    command
        .args([
            "--exact",
            "discovery::the_linker_is_found_without_path",
            "--nocapture",
        ])
        .env_clear()
        .env(CHILD, "1")
        .env("PATH", "");
    // The variables discovery documents: the system directory, the program-data
    // directory for `vswhere.exe`, and the 32-bit program directory.
    for name in ["SystemRoot", "ProgramData", "ProgramFiles(x86)"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    let output = command.output().expect("a child process");
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{text}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        text.contains("DISCOVERY-PATH-EMPTY=true"),
        "the child's PATH was not empty:\n{text}"
    );
    let link = text
        .lines()
        .find_map(|line| line.strip_prefix("DISCOVERY-LINK="))
        .unwrap_or_else(|| {
            panic!("discovery found no linker with an empty PATH:\n{text}")
        });
    let link = PathBuf::from(link);
    assert!(link.is_absolute() && link.is_file(), "{}", link.display());
    assert_eq!(
        link.file_name().and_then(|name| name.to_str()),
        Some("link.exe")
    );
    assert!(
        link.components().any(|c| c.as_os_str() == "MSVC"),
        "the linker is the MSVC one: {}",
        link.display()
    );

    // With every source removed, the missing-tool error of X3-S1 names what was
    // looked for.
    let error = Toolchain::discover(&Sources::default()).expect_err("no sources");
    assert!(
        matches!(error, BuildError::MissingLinkTool { .. }),
        "{error}"
    );
    // And with the sources named but nothing at them.
    let tree = Tree::new("removed");
    let error = Toolchain::discover(&Sources {
        program_files_x86: Some(tree.path("pf")),
        visual_studio: Some(tree.path("vs")),
        kits_root: Some(tree.path("kits")),
    })
    .expect_err("nothing there");
    assert!(
        matches!(error, BuildError::MissingLinkTool { .. }),
        "{error}"
    );
}

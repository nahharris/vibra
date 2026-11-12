//! Another host or target is a typed error, not a crash and not a guess.

use std::path::PathBuf;

use vibra_build::{BuildError, BuildOptions, Inputs, Target, Toolchain, build_program};
use vibra_exec::{FuncId, Program};

fn options(target: Target) -> BuildOptions {
    BuildOptions {
        target,
        toolchain: Toolchain {
            link: PathBuf::from("link.exe"),
            library_dirs: Vec::new(),
            msvc_version: String::new(),
            sdk_version: String::new(),
        },
        archive: PathBuf::from("no-such.lib"),
        work_dir: PathBuf::from("no-such-work"),
        output: PathBuf::from("no-such-output.exe"),
    }
}

fn empty() -> Program {
    Program {
        functions: Vec::new(),
        entry: FuncId(0),
        globals: Vec::new(),
        constants: Vec::new(),
        origins: Vec::new(),
    }
}

const INPUTS: Inputs = Inputs {
    entry_outcome: 0,
    registry_minor: 0,
};

#[test]
fn the_linux_target_is_unsupported_and_nothing_is_written() {
    let options = options(Target::LINUX_X64);
    let error = build_program(&empty(), INPUTS, &options).expect_err("unsupported");
    assert!(
        matches!(
            error,
            BuildError::Unsupported {
                target: "x86_64-unknown-linux-gnu",
                ..
            }
        ),
        "{error}"
    );
    assert!(!options.work_dir.exists());
    assert!(!options.output.exists());
}

#[cfg(not(windows))]
#[test]
fn another_host_cannot_discover_or_build() {
    let error = Toolchain::from_machine().expect_err("unsupported");
    assert!(matches!(error, BuildError::Unsupported { .. }), "{error}");
    let error = build_program(&empty(), INPUTS, &options(Target::WINDOWS_X64))
        .expect_err("unsupported");
    assert!(matches!(error, BuildError::Unsupported { .. }), "{error}");
}

#[cfg(windows)]
#[test]
fn a_missing_archive_is_named_before_the_linker_runs() {
    let options = options(Target::WINDOWS_X64);
    let error = build_program(&empty(), INPUTS, &options).expect_err("no archive");
    assert!(
        matches!(&error, BuildError::Io { path, .. } if *path == options.archive),
        "{error}"
    );
    assert!(!options.work_dir.exists());
}

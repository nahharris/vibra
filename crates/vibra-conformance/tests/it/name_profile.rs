//! One definition of the name profile of the scope policies
//! (`docs/spec/03-effects.md`, "Filesystem policies").
//!
//! The broker, the world driver, and the compiler row `fs.validNames` that
//! `RelativePath`'s `where` calls share `vibra_ir::names`. The only other
//! definition is the file-scope adapter's own check in `vibra-fs-scope`,
//! which knows no language and depends on no crate of the workspace; both are
//! held to the vectors of `crates/vibra-host/tests/it/names.tsv`. The
//! standard library defines no profile of its own.
//!
//! The marker of a definition is the reserved device stem `CONOUT$` written
//! as a string literal: every definition lists the closed set of stems.

use std::path::{Path, PathBuf};

use crate::support::{rust_sources, workspace_root};

/// A string literal no definition of the profile can do without.
const MARKER: &str = "\"CONOUT$\"";

/// Every file with the extension `extension` below `directory`.
fn files(directory: &Path, extension: &str, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("readable directory") {
        let path = entry.expect("readable directory entry").path();
        if path.is_dir() {
            files(&path, extension, found);
        } else if path.extension().is_some_and(|found| found == extension) {
            found.push(path);
        }
    }
}

/// The files of the workspace's crate sources and of the standard library
/// that define the profile, relative to the root, with `/` separators.
fn definitions(root: &Path) -> Vec<String> {
    let mut sources = Vec::new();
    for entry in std::fs::read_dir(root.join("crates")).expect("a crates directory") {
        let source = entry.expect("readable directory entry").path().join("src");
        if source.is_dir() {
            rust_sources(&source, &mut sources);
        }
    }
    files(&root.join("stdlib"), "vib", &mut sources);
    let mut found = sources
        .iter()
        .filter(|path| {
            std::fs::read_to_string(path)
                .expect("readable source")
                .contains(MARKER)
        })
        .map(|path| {
            path.strip_prefix(root)
                .expect("below the root")
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect::<Vec<_>>();
    found.sort();
    found
}

#[test]
fn the_name_profile_has_one_definition_beside_the_adapter_s_own() {
    assert_eq!(
        definitions(&workspace_root()),
        [
            "crates/vibra-fs-scope/src/name.rs",
            "crates/vibra-ir/src/names.rs"
        ]
    );
}

/// The check sees a second definition: a copy of the stems anywhere else.
#[test]
fn a_second_definition_is_seen() {
    let root = crate::support::scratch_directory("name-profile");
    for (relative, text) in [
        (
            "crates/vibra-ir/src/names.rs",
            "const S: &[&str] = &[\"CONOUT$\"];\n",
        ),
        (
            "crates/vibra-host/src/path.rs",
            "const S: &[&str] = &[\"CONOUT$\"];\n",
        ),
        (
            "stdlib/src/std/fs.vib",
            "let s: List(Str) be [\"CONOUT$\"] endlet\n",
        ),
    ] {
        crate::support::write(&root, relative, text);
    }
    assert_eq!(
        definitions(&root),
        [
            "crates/vibra-host/src/path.rs",
            "crates/vibra-ir/src/names.rs",
            "stdlib/src/std/fs.vib"
        ]
    );
}

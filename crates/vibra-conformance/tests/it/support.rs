//! Helpers shared by the test modules.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) use vibra_conformance::workspace_root;

/// Every `.rs` file below `directory`, appended to `found`.
pub(crate) fn rust_sources(directory: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("readable source directory") {
        let path = entry.expect("readable directory entry").path();
        if path.is_dir() {
            rust_sources(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

/// The attribute that gates host-language test code.
const TEST_GATE: &str = "#[cfg(test)]";

/// Whether `text`, trimmed, opens a module: `mod name`, with or without a
/// visibility.
fn opens_module(text: &str) -> bool {
    let text = text.trim_start();
    let text = ["pub(crate) ", "pub(super) ", "pub "]
        .iter()
        .find_map(|visibility| text.strip_prefix(visibility))
        .unwrap_or(text);
    text.starts_with("mod ")
}

/// The lines of `source`, numbered from one, that are not in a
/// `#[cfg(test)]` module: the attribute, the `mod` it gates, and that
/// module's block, matched brace by brace, are left out. A `#[cfg(test)]`
/// field, function, or other item is kept, and so is every line after the
/// module.
pub(crate) fn outside_test_modules(source: &str) -> Vec<(usize, &str)> {
    let lines = source.lines().collect::<Vec<_>>();
    let mut kept = Vec::new();
    let mut index = 0;
    while let Some(line) = lines.get(index) {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix(TEST_GATE) else {
            kept.push((index + 1, *line));
            index += 1;
            continue;
        };
        // The gated item: on the attribute's line, or on the next line that
        // is neither an attribute nor a comment.
        let mut item = index;
        if rest.trim().is_empty() {
            item += 1;
            while lines.get(item).is_some_and(|text| {
                let text = text.trim_start();
                text.starts_with("#[") || text.starts_with("//")
            }) {
                item += 1;
            }
        }
        let opens = lines.get(item).is_some_and(|text| {
            let text = if item == index { rest } else { text };
            opens_module(text)
        });
        if !opens {
            kept.push((index + 1, *line));
            index += 1;
            continue;
        }
        // Skip through the line that closes the module's block, or through
        // `mod name;`.
        let mut depth: i64 = 0;
        let mut opened = false;
        let mut end = item;
        while let Some(text) = lines.get(end) {
            let text = if end == index { rest } else { text };
            let code = text.split("//").next().unwrap_or(text);
            for character in code.chars() {
                match character {
                    '{' => {
                        depth += 1;
                        opened = true;
                    }
                    '}' => depth -= 1,
                    _ => {}
                }
            }
            end += 1;
            if (opened && depth <= 0) || (!opened && code.trim_end().ends_with(';')) {
                break;
            }
        }
        index = end;
    }
    kept
}

/// A fresh, empty directory under the target's scratch directory, named after
/// `name` and unique within this run.
pub(crate) fn scratch_directory(name: &str) -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "{name}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    if directory.exists() {
        std::fs::remove_dir_all(&directory).expect("remove a stale scratch directory");
    }
    std::fs::create_dir_all(&directory).expect("create a scratch directory");
    directory
}

/// Writes `contents` at `relative` below `root`, creating directories.
pub(crate) fn write(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create a fixture directory");
    }
    std::fs::write(&path, contents).expect("write a fixture file");
}

/// The header of the rule index followed by one row per `(identity, status)`.
pub(crate) fn rules(rows: &[(&str, &str)]) -> String {
    let mut text = format!("{}\n", vibra_conformance::RULES_HEADER);
    for (identity, status) in rows {
        text.push_str(&format!(
            "{identity}\t{status}\t07-diagnostics-and-conformance.md#conformance-corpus\n"
        ));
    }
    text
}

/// A valid `reader` case manifest for `id` addressing `rule`, whose input is
/// `input.vibon`.
pub(crate) fn reader_case(id: &str, rule: &str) -> String {
    format!(
        "id = \"{id}\"\n\
         rule = \"{rule}\"\n\
         profile = \"readerV2\"\n\
         operation = \"reader\"\n\
         description = \"07-diagnostics-and-conformance.md#conformance-corpus\"\n\
         \n\
         [inputs]\n\
         data = [\"input.vibon\"]\n\
         \n\
         [expect]\n\
         verdict = \"accepted\"\n"
    )
}

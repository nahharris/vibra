//! The test hooks of `vibra-host` are an authority surface, enabled only by
//! the crate's own tests.
//!
//! `vibra-host` has a cargo feature `testing` whose hooks
//! (`Context::root_for_tests` and its siblings) issue root grants, release
//! resources, and read or set the counters. `root_for_tests` mints a root
//! grant, which `docs/spec/06-runtime.md`, "Host world", allows only the
//! runner's world and startup to do. So no manifest of the workspace but
//! `vibra-host`'s own `[dev-dependencies]` names the feature, and every hook
//! is compiled only under it.
//!
//! A hook is a function whose name ends in `_for_tests`. The inventory is
//! found by scanning `vibra-host/src`, never listed by hand: every function
//! under the gate is a hook, and every hook is under the gate.

use crate::support::{rust_sources, workspace_root};

use std::collections::BTreeSet;
use std::path::Path;

/// The feature that compiles the hooks.
const FEATURE: &str = "testing";

/// The attribute that gates a hook.
const GATE: &str = "#[cfg(feature = \"testing\")]";

/// The one place that may enable the feature: the crate's own
/// dev-dependency on itself.
const ALLOWED: (&str, &str) = ("crates/vibra-host/Cargo.toml", "dev-dependencies");

/// The suffix of a hook's name.
const HOOK_SUFFIX: &str = "_for_tests";

/// The dependency tables of a manifest, by their dotted names: the three
/// kinds at the top, under each `target`, and the workspace's.
fn dependency_tables(manifest: &toml::Table) -> Vec<(String, &toml::Table)> {
    let kinds = ["dependencies", "dev-dependencies", "build-dependencies"];
    let mut tables = Vec::new();
    for kind in kinds {
        if let Some(table) = manifest.get(kind).and_then(toml::Value::as_table) {
            tables.push((kind.to_owned(), table));
        }
    }
    if let Some(targets) = manifest.get("target").and_then(toml::Value::as_table) {
        for (target, value) in targets {
            for kind in kinds {
                if let Some(table) = value.get(kind).and_then(toml::Value::as_table) {
                    tables.push((format!("target.{target}.{kind}"), table));
                }
            }
        }
    }
    if let Some(table) = manifest
        .get("workspace")
        .and_then(|workspace| workspace.get("dependencies"))
        .and_then(toml::Value::as_table)
    {
        tables.push(("workspace.dependencies".to_owned(), table));
    }
    tables
}

/// Whether the dependency `name = value` is `vibra-host`, under its own name
/// or renamed with `package`.
fn is_host(name: &str, value: &toml::Value) -> bool {
    name == "vibra-host"
        || value.get("package").and_then(toml::Value::as_str) == Some("vibra-host")
}

/// Every place `manifest`, at `path` below the workspace root, enables the
/// feature `testing` of `vibra-host`: a dependency that lists it in its
/// `features`, and a feature of the manifest that names `<dependency>/testing`
/// or `<dependency>?/testing`. Each is `(path, table)`.
fn enablers(path: &str, manifest: &toml::Table) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut names = vec!["vibra-host".to_owned()];
    for (table_name, table) in dependency_tables(manifest) {
        for (name, value) in table {
            if !is_host(name, value) {
                continue;
            }
            names.push(name.clone());
            let enables = value
                .get("features")
                .and_then(toml::Value::as_array)
                .is_some_and(|features| {
                    features
                        .iter()
                        .any(|feature| feature.as_str() == Some(FEATURE))
                });
            if enables {
                found.push((path.to_owned(), table_name.clone()));
            }
        }
    }
    if let Some(features) = manifest.get("features").and_then(toml::Value::as_table) {
        for (feature, enabled) in features {
            let enables = enabled.as_array().is_some_and(|enabled| {
                enabled.iter().filter_map(toml::Value::as_str).any(|item| {
                    names.iter().any(|name| {
                        item == format!("{name}/{FEATURE}")
                            || item == format!("{name}?/{FEATURE}")
                    })
                })
            });
            if enables {
                found.push((path.to_owned(), format!("features.{feature}")));
            }
        }
    }
    found
}

/// Every manifest of the workspace: the root's and each crate's, by its path
/// below the root with `/` separators.
fn manifests(root: &Path) -> Vec<(String, toml::Table)> {
    let mut paths = vec![("Cargo.toml".to_owned(), root.join("Cargo.toml"))];
    for entry in std::fs::read_dir(root.join("crates")).expect("a crates directory") {
        let directory = entry.expect("a readable entry").path();
        let manifest = directory.join("Cargo.toml");
        if manifest.is_file() {
            let name = directory
                .file_name()
                .and_then(|name| name.to_str())
                .expect("a Unicode crate directory")
                .to_owned();
            paths.push((format!("crates/{name}/Cargo.toml"), manifest));
        }
    }
    paths
        .into_iter()
        .map(|(relative, path)| {
            let text = std::fs::read_to_string(&path).expect("a readable manifest");
            let manifest = text
                .parse::<toml::Table>()
                .unwrap_or_else(|error| panic!("cannot parse {relative}: {error}"));
            (relative, manifest)
        })
        .collect()
}

/// The enablers of every manifest but the one allowed.
fn unexpected(manifests: &[(String, toml::Table)]) -> Vec<(String, String)> {
    manifests
        .iter()
        .flat_map(|(path, manifest)| enablers(path, manifest))
        .filter(|(path, table)| (path.as_str(), table.as_str()) != ALLOWED)
        .collect()
}

/// The name of the function `line` defines, if it defines one.
fn defined(line: &str) -> Option<&str> {
    if line.trim_start().starts_with("//") {
        return None;
    }
    let (before, after) = line.split_once("fn ")?;
    // `fn` is a word of its own: at the start, or after a space.
    if !(before.is_empty() || before.ends_with(' ')) {
        return None;
    }
    let end = after
        .find(|character: char| !(character.is_alphanumeric() || character == '_'))
        .unwrap_or(after.len());
    let name = after.get(..end)?;
    (!name.is_empty() && after.get(end..)?.starts_with(['(', '<'])).then_some(name)
}

/// The index one past the last line of the item that starts at `start`:
/// its block, matched brace by brace, or the line that ends it with `;`.
fn item_end(lines: &[&str], start: usize) -> usize {
    let mut depth: i64 = 0;
    let mut opened = false;
    let mut end = start;
    while let Some(text) = lines.get(end) {
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
    end
}

/// The line ranges of `source` that the feature gates: each item whose
/// attributes and doc comments, directly above it, hold the gate.
fn gated_ranges(lines: &[&str]) -> Vec<std::ops::Range<usize>> {
    let mut ranges = Vec::new();
    let mut index = 0;
    while let Some(line) = lines.get(index) {
        if line.trim() != GATE {
            index += 1;
            continue;
        }
        let mut item = index + 1;
        while lines.get(item).is_some_and(|text| {
            let text = text.trim_start();
            text.starts_with("#[") || text.starts_with("//")
        }) {
            item += 1;
        }
        let end = item_end(lines, item);
        ranges.push(item..end);
        index = end.max(index + 1);
    }
    ranges
}

/// The functions `source` defines under the gate, by line number from one.
fn gated(source: &str) -> Vec<(usize, String)> {
    let lines = source.lines().collect::<Vec<_>>();
    gated_ranges(&lines)
        .into_iter()
        .flatten()
        .filter_map(|index| {
            let name = defined(lines.get(index)?)?;
            Some((index + 1, name.to_owned()))
        })
        .collect()
}

/// The hooks `source` defines outside an item the feature gates. A hook is
/// a function whose name ends in `_for_tests`, wherever it is defined.
fn ungated(source: &str) -> Vec<String> {
    let lines = source.lines().collect::<Vec<_>>();
    let ranges = gated_ranges(&lines);
    lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            let hook = defined(line).filter(|name| name.ends_with(HOOK_SUFFIX))?;
            (!ranges.iter().any(|range| range.contains(&index)))
                .then(|| format!("{}: `{hook}` is not gated", index + 1))
        })
        .collect()
}

#[test]
fn only_the_hosts_own_tests_enable_its_testing_feature() {
    let manifests = manifests(&workspace_root());
    assert!(
        manifests.len() > 10,
        "the scan reads the workspace's manifests"
    );
    let all = manifests
        .iter()
        .flat_map(|(path, manifest)| enablers(path, manifest))
        .collect::<Vec<_>>();
    assert!(
        all.iter()
            .any(|(path, table)| (path.as_str(), table.as_str()) == ALLOWED),
        "the audit is not vacuous: `vibra-host`'s own tests enable the feature"
    );
    let unexpected = unexpected(&manifests);
    assert!(
        unexpected.is_empty(),
        "manifests other than `vibra-host`'s dev-dependency enable `vibra-host/testing`: \
         {unexpected:?}"
    );
}

#[test]
fn the_test_hooks_are_compiled_only_under_the_feature() {
    let source_root = workspace_root()
        .join("crates")
        .join("vibra-host")
        .join("src");
    let mut sources = Vec::new();
    rust_sources(&source_root, &mut sources);
    let mut inventory = Vec::new();
    let mut found = Vec::new();
    for path in sources {
        let source = std::fs::read_to_string(&path).expect("readable source");
        for (line, name) in gated(&source) {
            if !name.ends_with(HOOK_SUFFIX) {
                found.push(format!(
                    "{}:{line}: `{name}` is gated but not named as a hook",
                    path.display()
                ));
            }
            inventory.push(name);
        }
        for fault in ungated(&source) {
            found.push(format!("{}:{fault}", path.display()));
        }
    }
    let distinct = inventory.iter().collect::<BTreeSet<_>>();
    assert_eq!(
        distinct.len(),
        inventory.len(),
        "every hook is defined once"
    );
    assert!(
        inventory
            .iter()
            .any(|name| name == "startup_event_for_tests"),
        "the scan finds the gated hooks: {inventory:?}"
    );
    assert!(
        found.is_empty(),
        "hooks outside the feature:\n{}",
        found.join("\n")
    );
}

#[test]
fn atk_second_testing_enabler() {
    let parse = |text: &str| text.parse::<toml::Table>().expect("a manifest");
    let own = parse(
        "[package]\nname = \"vibra-host\"\n\
         [dev-dependencies]\nvibra-host = { path = \".\", features = [\"testing\"] }\n",
    );
    let workspace = parse(
        "[package]\nname = \"vibra-workspace\"\n\
         [dev-dependencies]\nvibra-host = { workspace = true, features = [\"testing\"] }\n",
    );
    let renamed = parse(
        "[package]\nname = \"vibra-cli\"\n\
         [dependencies]\nhost = { package = \"vibra-host\", path = \"../vibra-host\" }\n\
         [features]\ndebug = [\"host?/testing\"]\n",
    );
    let target = parse(
        "[package]\nname = \"vibra-cli\"\n\
         [target.'cfg(windows)'.dependencies]\nvibra-host = { workspace = true, features = [\"testing\"] }\n",
    );
    let root = parse(
        "[workspace]\nmembers = []\n\
         [workspace.dependencies]\nvibra-host = { path = \"crates/vibra-host\", features = [\"testing\"] }\n",
    );
    let only_own = [(ALLOWED.0.to_owned(), own.clone())];
    assert!(unexpected(&only_own).is_empty());
    for (path, manifest) in [
        ("crates/vibra-workspace/Cargo.toml", workspace),
        ("crates/vibra-cli/Cargo.toml", renamed),
        ("crates/vibra-cli/Cargo.toml", target),
        ("Cargo.toml", root),
    ] {
        let manifests = [
            (ALLOWED.0.to_owned(), own.clone()),
            (path.to_owned(), manifest),
        ];
        assert_eq!(unexpected(&manifests).len(), 1, "{path}");
    }
    // A hook outside the gated block is reported; one inside it is not.
    let gated = "/// Hooks.\n#[cfg(feature = \"testing\")]\nimpl Context<'_> {\n    \
                 /// Doc.\n    pub fn root_for_tests(&mut self) {}\n}\n";
    assert!(ungated(gated).is_empty());
    let open = "impl Context<'_> {\n    pub fn root_for_tests(&mut self) {}\n}\n";
    assert_eq!(ungated(open), ["2: `root_for_tests` is not gated"]);
}

/// F2 of the review of grants: a hook added beside the six, outside the
/// gate, is reported, whatever its name.
#[test]
fn atk_seventh_hook_outside_the_gate() {
    let source = "#[cfg(feature = \"testing\")]\nimpl Context<'_> {\n    \
                  pub fn root_for_tests(&mut self) {}\n}\n\n\
                  impl Context<'_> {\n    pub fn seventh_for_tests(&mut self) {}\n}\n";
    assert_eq!(ungated(source), ["7: `seventh_for_tests` is not gated"]);
    let gated_names = gated(source)
        .into_iter()
        .map(|(_, name)| name)
        .collect::<Vec<_>>();
    assert_eq!(gated_names, ["root_for_tests"]);
    // A gated function directly under the attribute, and one nested in a
    // gated module, are both found.
    let nested = "#[cfg(feature = \"testing\")]\npub fn a_for_tests() {}\n\
                  /// Doc.\n#[cfg(feature = \"testing\")]\nmod hooks {\n    \
                  pub fn b_for_tests() {\n        let _ = 1;\n    }\n}\n";
    assert!(ungated(nested).is_empty());
    assert_eq!(
        gated(nested)
            .into_iter()
            .map(|(_, name)| name)
            .collect::<Vec<_>>(),
        ["a_for_tests", "b_for_tests"]
    );
}

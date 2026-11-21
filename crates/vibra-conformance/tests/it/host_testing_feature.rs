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

use crate::support::{rust_sources, workspace_root};

use std::path::Path;

/// The feature that compiles the hooks.
const FEATURE: &str = "testing";

/// The attribute that gates a hook.
const GATE: &str = "#[cfg(feature = \"testing\")]";

/// The one place that may enable the feature: the crate's own
/// dev-dependency on itself.
const ALLOWED: (&str, &str) = ("crates/vibra-host/Cargo.toml", "dev-dependencies");

/// The hooks the feature compiles.
const HOOKS: [&str; 5] = [
    "root_for_tests",
    "release_for_tests",
    "counters_for_tests",
    "preset_for_tests",
    "slot_for_tests",
];

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

/// The hooks `source` defines outside an item gated by the feature: a hook
/// is gated when the attribute is among the attributes and doc comments
/// directly above its `fn`, or above the top-level item that holds it.
fn ungated(source: &str) -> Vec<String> {
    let lines = source.lines().collect::<Vec<_>>();
    let gated_above = |index: usize| {
        lines
            .get(..index)
            .unwrap_or_default()
            .iter()
            .rev()
            .map(|line| line.trim())
            .take_while(|line| line.starts_with("#[") || line.starts_with("///"))
            .any(|line| line == GATE)
    };
    let mut found = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let Some(hook) = HOOKS
            .iter()
            .find(|hook| line.contains(&format!("fn {hook}(")))
        else {
            continue;
        };
        // The top-level item holding the hook: the nearest line above it
        // that starts in the first column and opens an item.
        let item = (0..index).rev().find(|&above| {
            lines.get(above).is_some_and(|text| {
                !text.is_empty()
                    && !text.starts_with([' ', '\t', '}', '#', '/'])
                    && !text.starts_with("//")
            })
        });
        let gated = gated_above(index) || item.is_some_and(gated_above);
        if !gated {
            found.push(format!("{}: `{hook}` is not gated", index + 1));
        }
    }
    found
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
    let mut defined = 0;
    let mut found = Vec::new();
    for path in sources {
        let source = std::fs::read_to_string(&path).expect("readable source");
        defined += HOOKS
            .iter()
            .filter(|hook| source.contains(&format!("fn {hook}(")))
            .count();
        for fault in ungated(&source) {
            found.push(format!("{}:{fault}", path.display()));
        }
    }
    assert_eq!(defined, HOOKS.len(), "every hook is defined once");
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

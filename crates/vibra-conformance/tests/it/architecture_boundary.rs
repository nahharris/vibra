//! Enforces the roadmap's architecture boundary as a test rather than a
//! convention.
//!
//! `docs/roadmap/README.md` states that crate boundaries may evolve but that
//! dependency arrows may not point from language semantics into CLI, MCP,
//! filesystem UI, or a backend. This test reads every workspace manifest and
//! fails when the real dependency graph disagrees with the declared
//! architecture, so adding a forbidden arrow cannot pass CI unaccompanied by
//! a deliberate change to the table below.

// A test asserts by panicking, and indexes fixtures it just built.
use crate::support::{rust_sources, workspace_root};

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The permitted intra-workspace dependencies of each crate.
///
/// A crate may depend on exactly the crates listed for it, and every crate in
/// `crates/` must appear here. Widening a row is a deliberate architecture
/// change: record it in the active milestone's step plan.
const ARCHITECTURE: &[(&str, &[&str])] = &[
    // The leaf. Diagnostics are a language surface and know nothing else.
    ("vibra-diagnostics", &[]),
    // The reader. Emits diagnostics; must not reach the formatter or schemas.
    ("vibra-syntax", &["vibra-diagnostics"]),
    // Checked semantic IR is independent of syntax and wire formats.
    ("vibra-ir", &["vibra-diagnostics"]),
    // The checker lowers syntax and resolved identities to checked IR.
    (
        "vibra-types",
        &[
            "vibra-diagnostics",
            "vibra-ir",
            "vibra-resolve",
            "vibra-syntax",
        ],
    ),
    // The interpreter consumes checked IR and has no frontend dependency.
    // It calls the toolchain-owned native code directly, over its own values.
    (
        "vibra-interp",
        &[
            "vibra-diagnostics",
            "vibra-host",
            "vibra-ir",
            "vibra-native",
        ],
    ),
    // Consumes the reader's tree; nothing in the language depends on it.
    ("vibra-fmt", &["vibra-diagnostics", "vibra-syntax"]),
    // The wire format. The Step 10 adapter consumes syntax and workspace
    // facts; no phase depends on the wire crate.
    (
        "vibra-schema",
        &["vibra-diagnostics", "vibra-syntax", "vibra-workspace"],
    ),
    // The workspace hands the reference interpreter a provider: a world for a
    // test and a conformance case, and the system provider for `run`, which the
    // command line builds (Milestone 4, Step 16b).
    // Project schema decoding sits above syntax and diagnostics. Later
    // workspace phases may widen this row when their inputs exist.
    (
        "vibra-workspace",
        &[
            "vibra-diagnostics",
            "vibra-fmt",
            "vibra-host",
            "vibra-interp",
            "vibra-ir",
            "vibra-resolve",
            "vibra-syntax",
            "vibra-types",
        ],
    ),
    // Command services consume the workspace plan and render schema facts;
    // no language phase depends on the process interface.
    (
        "vibra-cli",
        &[
            "vibra-diagnostics",
            "vibra-fmt",
            "vibra-host",
            "vibra-schema",
            "vibra-syntax",
            "vibra-types",
            "vibra-workspace",
        ],
    ),
    // Resolution owns its neutral graph input and depends only on language
    // structure; workspace and conformance adapt filesystem snapshots into it.
    ("vibra-resolve", &["vibra-diagnostics", "vibra-syntax"]),
    // The WebAssembly emitter is the second consumer of checked IR, beside the
    // interpreter. It reaches no engine, and it does not reach the native-code
    // crate either: a module names a native only by the import names recorded
    // in `vibra-ir`.
    ("vibra-wasm", &["vibra-diagnostics", "vibra-ir"]),
    // The toolchain-owned native code, written once against a value-access
    // interface that the interpreter and the runner each implement. It reaches
    // no engine and no emitter: it reads the closed table of native imports and
    // the shape of an arena object recorded in `vibra-ir`.
    ("vibra-native", &["vibra-ir"]),
    // The host provider: the provider trait, the system provider, the in-memory
    // host world, and the audit event (Milestone 4, Step 16a). It reads the
    // closed host registry of `vibra-ir`, and the reader's VIBON data for the
    // two documents it owns, the world and the event. It reaches no engine, no
    // emitter, and no checker. The reference interpreter and the runner call it;
    // the harness decodes a case's world and hands each backend a fresh provider.
    // The emitter never depends on it: a module names a host operation only by
    // the symbol and signature recorded in `vibra-ir`.
    (
        "vibra-host",
        &["vibra-diagnostics", "vibra-ir", "vibra-syntax"],
    ),
    // The runner is the only crate that depends on the engine. It reads the
    // boundary names and codes recorded in `vibra-ir`, and it supplies the
    // imports of the native-code crate over the instance memory. It
    // does not depend on the emitter: the harness joins them.
    (
        "vibra-wasm-run",
        &["vibra-host", "vibra-ir", "vibra-native"],
    ),
    // The actor kernel is generic over an executor and knows no language, no
    // value layout, and no backend: it needs no other crate (N19).
    ("vibra-actors", &[]),
    // The execution IR sits downstream of checked IR and knows no runtime, no
    // value layout, and no target (docs/roadmap/nexus/decisions.md, N4). Its
    // tests build source origins, which need a span.
    ("vibra-exec", &["vibra-diagnostics", "vibra-ir"]),
    // The lowering is the one crate that knows both the checked IR and the
    // execution IR (docs/roadmap/nexus/design.md, section 1). It reaches no
    // runtime and no checker; its differential tests live in the harness.
    ("vibra-lower", &["vibra-exec", "vibra-ir"]),
    // Nexus owns execution state and reaches the host provider. It does not
    // depend on the execution IR or on a code generator: whatever executes a
    // program's blocks drives it through its transfers (N5).
    ("vibra-nexus", &["vibra-host", "vibra-ir", "vibra-native"]),
    // The reference stepper joins the two: verified execution IR, run on
    // Nexus with no generated code (N6). It is a test instrument, and its
    // tests supply a host world and spans.
    (
        "vibra-nexus-eval",
        &[
            "vibra-diagnostics",
            "vibra-exec",
            "vibra-host",
            "vibra-ir",
            "vibra-nexus",
        ],
    ),
    // The bridge joins generated code to Nexus: the C ABI, the launcher, and the
    // static archive, and the only crate with `unsafe` (N10). Its normal
    // dependencies are exactly `vibra-host`, `vibra-ir`, and `vibra-nexus`
    // (`the_bridge_carries_no_compiler_ir`): the archive linked into user
    // programs carries no compiler IR. Its tests drive hand-written step
    // functions against the reference stepper, which needs the execution IR
    // and the stepper, and spans for the origin table.
    (
        "vibra-nexus-bridge",
        &[
            "vibra-diagnostics",
            "vibra-exec",
            "vibra-host",
            "vibra-ir",
            "vibra-nexus",
            "vibra-nexus-eval",
        ],
    ),
    // The code generator: verified execution IR in, a native object out. It
    // is written against the bridge's contract (the ABI constants, the
    // descriptor types, and the routine table) and is the only crate that
    // names Cranelift (N12). Its tests build spans for the origin table.
    (
        "vibra-codegen",
        &[
            "vibra-diagnostics",
            "vibra-exec",
            "vibra-ir",
            "vibra-nexus-bridge",
        ],
    ),
    // The build driver: toolchain discovery and the link driver (X3-07). It
    // lowers, verifies, generates, and links, so it names the lowering, the
    // execution IR, the generator, and the checked IR for the entry's result
    // type. It is not a semantic crate: it reads the environment, starts the
    // linker, and writes files, in one module (`system`). No command depends on
    // it yet (`vibra-cli` is not changed until the specification is approved).
    (
        "vibra-build",
        &["vibra-codegen", "vibra-exec", "vibra-ir", "vibra-lower"],
    ),
    // The harness. Legitimately sits above every node.
    (
        "vibra-conformance",
        &[
            "vibra-build",
            "vibra-diagnostics",
            "vibra-fmt",
            "vibra-schema",
            "vibra-resolve",
            "vibra-syntax",
            "vibra-wasm",
            "vibra-wasm-run",
            "vibra-workspace",
            "vibra-exec",
            "vibra-host",
            "vibra-interp",
            "vibra-ir",
            "vibra-lower",
            "vibra-nexus",
            "vibra-nexus-eval",
            "vibra-types",
        ],
    ),
];

/// Every dependency section a manifest may use to create an arrow.
const DEPENDENCY_SECTIONS: &[&str] =
    &["dependencies", "dev-dependencies", "build-dependencies"];

fn read_manifest(path: &Path) -> toml::Table {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    text.parse::<toml::Table>()
        .unwrap_or_else(|error| panic!("cannot parse {}: {error}", path.display()))
}

/// Reads every manifest under `crates/`, paired with its Vibra dependencies.
fn crate_manifests(root: &Path) -> Vec<(PathBuf, toml::Table)> {
    let entries = std::fs::read_dir(root.join("crates"))
        .expect("the workspace has a crates directory");

    let mut manifests = Vec::new();
    for entry in entries {
        let manifest_path = entry
            .expect("readable directory entry")
            .path()
            .join("Cargo.toml");
        if manifest_path.is_file() {
            let manifest = read_manifest(&manifest_path);
            manifests.push((manifest_path, manifest));
        }
    }

    assert!(!manifests.is_empty(), "the workspace defines no crates");
    manifests
}

/// The Vibra crates named in any dependency section of `manifest`.
fn vibra_dependencies(manifest: &toml::Table) -> BTreeSet<String> {
    let mut edges = BTreeSet::new();
    for section in DEPENDENCY_SECTIONS {
        let Some(table) = manifest.get(*section).and_then(toml::Value::as_table) else {
            continue;
        };
        for dependency in table.keys() {
            if dependency.starts_with("vibra-") {
                edges.insert(dependency.clone());
            }
        }
    }
    edges
}

fn workspace_graph(root: &Path) -> BTreeMap<String, BTreeSet<String>> {
    crate_manifests(root)
        .into_iter()
        .map(|(path, manifest)| {
            let name = manifest
                .get("package")
                .and_then(|package| package.get("name"))
                .and_then(toml::Value::as_str)
                .unwrap_or_else(|| panic!("{} has no package name", path.display()))
                .to_owned();
            let edges = vibra_dependencies(&manifest);
            (name, edges)
        })
        .collect()
}

fn declared_architecture() -> BTreeMap<String, BTreeSet<String>> {
    ARCHITECTURE
        .iter()
        .map(|(crate_name, allowed)| {
            (
                (*crate_name).to_owned(),
                allowed.iter().map(|name| (*name).to_owned()).collect(),
            )
        })
        .collect()
}

/// Describes every way `graph` disagrees with `allowed`.
///
/// Kept pure so the checker itself is testable against synthetic graphs
/// rather than only against a workspace that is expected to pass.
fn violations(
    graph: &BTreeMap<String, BTreeSet<String>>,
    allowed: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<String> {
    let mut found = Vec::new();

    for (crate_name, edges) in graph {
        let Some(permitted) = allowed.get(crate_name) else {
            found.push(format!(
                "crate `{crate_name}` exists but the architecture table does not \
                 describe it"
            ));
            continue;
        };
        for edge in edges.difference(permitted) {
            found.push(format!(
                "crate `{crate_name}` depends on `{edge}`, which the architecture \
                 boundary does not permit"
            ));
        }
    }

    for crate_name in allowed.keys() {
        if !graph.contains_key(crate_name) {
            found.push(format!(
                "the architecture table describes `{crate_name}`, which no manifest \
                 defines"
            ));
        }
    }

    found
}

#[test]
fn workspace_matches_the_declared_architecture() {
    let found = violations(
        &workspace_graph(&workspace_root()),
        &declared_architecture(),
    );
    assert!(
        found.is_empty(),
        "architecture boundary violated:\n{}",
        found.join("\n")
    );
}

#[test]
fn a_forbidden_arrow_is_reported() {
    let graph = BTreeMap::from([
        (
            "vibra-diagnostics".to_owned(),
            BTreeSet::from(["vibra-fmt".to_owned()]),
        ),
        ("vibra-fmt".to_owned(), BTreeSet::new()),
    ]);
    let allowed = BTreeMap::from([
        ("vibra-diagnostics".to_owned(), BTreeSet::new()),
        ("vibra-fmt".to_owned(), BTreeSet::new()),
    ]);

    let found = violations(&graph, &allowed);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("vibra-diagnostics"), "{}", found[0]);
    assert!(found[0].contains("vibra-fmt"), "{}", found[0]);
}

#[test]
fn an_undescribed_crate_is_reported() {
    let graph = BTreeMap::from([("vibra-newcomer".to_owned(), BTreeSet::new())]);
    let found = violations(&graph, &BTreeMap::new());
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("does not describe it"), "{}", found[0]);
}

#[test]
fn a_missing_crate_is_reported() {
    let allowed = BTreeMap::from([("vibra-departed".to_owned(), BTreeSet::new())]);
    let found = violations(&BTreeMap::new(), &allowed);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("no manifest defines"), "{}", found[0]);
}

#[test]
fn no_crate_depends_on_the_archived_tree() {
    for (manifest_path, manifest) in crate_manifests(&workspace_root()) {
        for section in DEPENDENCY_SECTIONS {
            let Some(table) = manifest.get(*section).and_then(toml::Value::as_table)
            else {
                continue;
            };
            for (dependency, value) in table {
                let path = value
                    .get("path")
                    .and_then(toml::Value::as_str)
                    .unwrap_or_default();
                assert!(
                    !path.contains("archive"),
                    "{} depends on `{dependency}` at `{path}`; `archive/pre-v1` is \
                     evidence, never a build input",
                    manifest_path.display()
                );
            }
        }
    }
}

#[test]
fn the_workspace_excludes_the_archived_tree() {
    let manifest = read_manifest(&workspace_root().join("Cargo.toml"));
    let excluded = manifest
        .get("workspace")
        .and_then(|workspace| workspace.get("exclude"))
        .and_then(toml::Value::as_array)
        .expect("the workspace declares an exclude list");

    assert!(
        excluded
            .iter()
            .filter_map(toml::Value::as_str)
            .any(|path| path == "archive"),
        "`archive/pre-v1/Cargo.toml` must stay outside the workspace"
    );
}

/// Crates that implement language semantics. The M2 plan says they accept
/// explicit inputs and never search disk; filesystem acquisition belongs to
/// `vibra-workspace`.
const SEMANTIC_CRATES: &[&str] = &[
    "vibra-diagnostics",
    "vibra-syntax",
    "vibra-resolve",
    "vibra-types",
    "vibra-ir",
    "vibra-interp",
    "vibra-fmt",
    "vibra-wasm",
    "vibra-native",
    "vibra-exec",
    "vibra-actors",
    "vibra-lower",
    // It maps the execution IR to machine code from explicit inputs (the
    // verified program, a target, and two numbers) and writes no file: the
    // caller receives the object's bytes.
    "vibra-codegen",
    "vibra-nexus",
    "vibra-nexus-bridge",
    "vibra-nexus-eval",
];

/// The one crate that may depend on the WebAssembly engine, and the engine's
/// package name. Every other crate, the emitter included, reaches a module only
/// through bytes.
const ENGINE_CRATE: &str = "vibra-wasm-run";
const ENGINE: &str = "wasmtime";

/// The crates only the harness may depend on, because they run modules.
const RUNNER_CRATE: &str = "vibra-wasm-run";

/// Every dependency, of any package, named in any dependency section.
fn all_dependencies(manifest: &toml::Table) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for section in DEPENDENCY_SECTIONS {
        if let Some(table) = manifest.get(*section).and_then(toml::Value::as_table) {
            found.extend(table.keys().cloned());
        }
    }
    found
}

#[test]
fn only_the_runner_depends_on_the_engine() {
    for (path, manifest) in crate_manifests(&workspace_root()) {
        let name = manifest
            .get("package")
            .and_then(|package| package.get("name"))
            .and_then(toml::Value::as_str)
            .expect("a package name");
        let dependencies = all_dependencies(&manifest);
        assert_eq!(
            dependencies.contains(ENGINE),
            name == ENGINE_CRATE,
            "{}: `{ENGINE}` is a dependency of `{ENGINE_CRATE}` and of no other crate",
            path.display()
        );
    }
}

#[test]
fn only_the_harness_depends_on_the_runner() {
    for (name, edges) in workspace_graph(&workspace_root()) {
        assert_eq!(
            edges.contains(RUNNER_CRATE),
            name == "vibra-conformance",
            "`{name}`: only `vibra-conformance` depends on `{RUNNER_CRATE}`"
        );
    }
}

#[test]
fn the_emitter_has_no_engine_and_no_native_code_crate() {
    let mut checked = false;
    for (_, manifest) in crate_manifests(&workspace_root()) {
        let is_emitter = manifest
            .get("package")
            .and_then(|package| package.get("name"))
            .and_then(toml::Value::as_str)
            == Some("vibra-wasm");
        if !is_emitter {
            continue;
        }
        checked = true;
        let section = |name: &str| -> BTreeSet<String> {
            manifest
                .get(name)
                .and_then(toml::Value::as_table)
                .map(|table| table.keys().cloned().collect())
                .unwrap_or_default()
        };
        assert_eq!(
            section("dependencies"),
            BTreeSet::from(["vibra-ir".to_owned(), "wasm-encoder".to_owned()]),
            "the emitter depends on `vibra-ir` and the encoder only (`vibra-diagnostics` \
             is permitted but unused)"
        );
        assert!(
            section("dev-dependencies")
                .is_subset(&BTreeSet::from(["vibra-diagnostics".to_owned()])),
            "the emitter's tests may use `vibra-diagnostics` and nothing else"
        );
        assert!(section("build-dependencies").is_empty());
    }
    assert!(checked, "the emitter crate exists");
}

/// The toolchain-owned native code crate. It is written once and called by both
/// backends: the interpreter directly, and the runner for the imports of a
/// module. The emitter never depends on it.
const NATIVE_CRATE: &str = "vibra-native";

#[test]
fn only_the_two_backends_run_the_native_code_and_it_reaches_neither_the_engine_nor_the_emitter()
 {
    let graph = workspace_graph(&workspace_root());
    let dependents = graph
        .iter()
        .filter(|(_, edges)| edges.contains(NATIVE_CRATE))
        .map(|(name, _)| name.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        dependents,
        // Nexus hands the native code a view of its value store (N7).
        BTreeSet::from(["vibra-interp", "vibra-nexus", "vibra-wasm-run"]),
        "the reference interpreter, the runner, and Nexus call the native code, and nothing else does"
    );
    let own = graph.get(NATIVE_CRATE).expect("the native crate exists");
    assert_eq!(
        own,
        &BTreeSet::from(["vibra-ir".to_owned()]),
        "the native code depends on the closed tables of `vibra-ir` and on no engine or emitter"
    );
    for (_, manifest) in crate_manifests(&workspace_root()) {
        let is_native = manifest
            .get("package")
            .and_then(|package| package.get("name"))
            .and_then(toml::Value::as_str)
            == Some(NATIVE_CRATE);
        if is_native {
            assert!(
                !all_dependencies(&manifest).contains(ENGINE),
                "the native code has no engine dependency"
            );
        }
    }
}

/// The host provider crate. The reference interpreter and the runner may call it,
/// and the harness depends on it to decode a case's host world and hand each
/// backend a fresh provider. The emitter and the language phases never do: a
/// module names a host operation only by the registry symbol and signature that
/// `vibra-ir` records.
const HOST_CRATE: &str = "vibra-host";

#[test]
fn only_the_backends_and_the_harness_depend_on_the_host_provider() {
    let graph = workspace_graph(&workspace_root());
    let dependents = graph
        .iter()
        .filter(|(_, edges)| edges.contains(HOST_CRATE))
        .map(|(name, _)| name.as_str())
        .collect::<BTreeSet<_>>();
    let permitted = BTreeSet::from([
        "vibra-cli",
        "vibra-conformance",
        "vibra-interp",
        // Nexus performs a native program's host operations, and the
        // reference stepper's tests hand it a host world.
        "vibra-nexus",
        // The launcher's `system` profile builds the system provider, and
        // its `harness` profile a world provider.
        "vibra-nexus-bridge",
        "vibra-nexus-eval",
        "vibra-wasm-run",
        "vibra-workspace",
    ]);
    assert!(
        dependents.is_subset(&permitted),
        "{dependents:?} depend on `{HOST_CRATE}`; only {permitted:?} may"
    );
    let own = graph.get(HOST_CRATE).expect("the host crate exists");
    assert_eq!(
        own,
        &BTreeSet::from([
            "vibra-diagnostics".to_owned(),
            "vibra-ir".to_owned(),
            "vibra-syntax".to_owned()
        ]),
        "the host provider reads the registry and the reader's data and nothing above them"
    );
    for (_, manifest) in crate_manifests(&workspace_root()) {
        let is_host = manifest
            .get("package")
            .and_then(|package| package.get("name"))
            .and_then(toml::Value::as_str)
            == Some(HOST_CRATE);
        if is_host {
            assert!(
                !all_dependencies(&manifest).contains(ENGINE),
                "the host provider has no engine dependency"
            );
        }
    }
}

#[test]
fn a_dependency_on_the_engine_is_reported() {
    let manifest = "[package]\nname = \"vibra-other\"\n[dependencies]\nwasmtime.workspace = true\n"
        .parse::<toml::Table>()
        .expect("a manifest");
    assert!(all_dependencies(&manifest).contains(ENGINE));
}

/// Filesystem entry points a semantic crate must not name.
const FILESYSTEM_PATTERNS: &[&str] = &["std::fs", "fs::", "File::open", "read_dir("];

fn filesystem_references(source: &str) -> Vec<&'static str> {
    FILESYSTEM_PATTERNS
        .iter()
        .copied()
        .filter(|pattern| source.contains(pattern))
        .collect()
}

#[test]
fn semantic_crates_do_not_touch_the_filesystem() {
    let root = workspace_root();
    let mut found = Vec::new();
    for crate_name in SEMANTIC_CRATES {
        let mut sources = Vec::new();
        rust_sources(
            &root.join("crates").join(crate_name).join("src"),
            &mut sources,
        );
        assert!(!sources.is_empty(), "{crate_name} has no sources");
        for path in sources {
            let source = std::fs::read_to_string(&path).expect("readable source");
            for pattern in filesystem_references(&source) {
                found.push(format!("{} names `{pattern}`", path.display()));
            }
        }
    }
    assert!(
        found.is_empty(),
        "semantic crates must accept explicit inputs:\n{}",
        found.join("\n")
    );
}

/// What the actor kernel must not name: it is deterministic by construction.
const KERNEL_FORBIDDEN: &[&str] = &[
    "unsafe",
    "std::thread",
    "thread::",
    "std::time",
    "Instant",
    "SystemTime",
    "std::sync",
    "std::process",
    "std::env",
];

fn kernel_forbidden_references(source: &str) -> Vec<&'static str> {
    KERNEL_FORBIDDEN
        .iter()
        .copied()
        .filter(|pattern| source.contains(pattern))
        .collect()
}

#[test]
fn the_actor_kernel_has_no_unsafe_no_thread_and_no_clock() {
    let root = workspace_root();
    let crate_root = root.join("crates").join("vibra-actors");
    let mut sources = Vec::new();
    rust_sources(&crate_root.join("src"), &mut sources);
    assert!(!sources.is_empty(), "vibra-actors has no sources");
    let mut found = Vec::new();
    for path in sources {
        let source = std::fs::read_to_string(&path).expect("readable source");
        for pattern in kernel_forbidden_references(&source) {
            found.push(format!("{} names `{pattern}`", path.display()));
        }
    }
    assert!(
        found.is_empty(),
        "the actor kernel is single-threaded, clockless, and safe:\n{}",
        found.join("\n")
    );

    let manifest = read_manifest(&crate_root.join("Cargo.toml"));
    assert!(
        all_dependencies(&manifest).is_empty(),
        "the actor kernel has no dependency of any kind"
    );
}

#[test]
fn a_thread_or_clock_reference_in_the_actor_kernel_is_reported() {
    assert_eq!(
        kernel_forbidden_references("use std::time::Instant;"),
        ["std::time", "Instant"]
    );
    assert!(kernel_forbidden_references("let t = Time(0);").is_empty());
}

#[test]
fn a_filesystem_reference_is_reported() {
    assert_eq!(
        filesystem_references("use std::fs;\nfs::read(path)"),
        ["std::fs", "fs::"]
    );
    assert!(filesystem_references("let bytes = include_bytes!(\"x\");").is_empty());
}

/// Crates compiled into the shipped `vibra` binary.
const SHIPPED_CRATES: &[&str] = &[
    "vibra-diagnostics",
    "vibra-syntax",
    "vibra-resolve",
    "vibra-types",
    "vibra-ir",
    "vibra-interp",
    "vibra-native",
    "vibra-fmt",
    "vibra-schema",
    "vibra-workspace",
    "vibra-cli",
];

#[test]
fn shipped_crates_do_not_depend_on_their_build_checkout() {
    // `env!("CARGO_MANIFEST_DIR")` is an absolute build-host path; a binary
    // that reads through it breaks once installed or relocated.
    let root = workspace_root();
    let mut found = Vec::new();
    for crate_name in SHIPPED_CRATES {
        let mut sources = Vec::new();
        rust_sources(
            &root.join("crates").join(crate_name).join("src"),
            &mut sources,
        );
        for path in sources {
            let source = std::fs::read_to_string(&path).expect("readable source");
            if source.contains("CARGO_MANIFEST_DIR") {
                found.push(path.display().to_string());
            }
        }
    }
    assert!(
        found.is_empty(),
        "build-checkout paths in:\n{}",
        found.join("\n")
    );
}

/// The one crate that contains `unsafe` code and `extern "C"` items (decision
/// N10). Its lint tables restate the workspace's.
const BRIDGE_CRATE: &str = "vibra-nexus-bridge";

/// `source` with comments, string literals, and character literals blanked
/// out, so a scan sees code only.
fn code_only(source: &str) -> String {
    let chars = source.chars().collect::<Vec<_>>();
    let mut out = String::with_capacity(source.len());
    let mut at = 0;
    while at < chars.len() {
        let current = chars[at];
        let next = chars.get(at + 1).copied();
        if current == '/' && next == Some('/') {
            while at < chars.len() && chars[at] != '\n' {
                at += 1;
            }
        } else if current == '/' && next == Some('*') {
            let mut depth = 0;
            while at < chars.len() {
                if chars[at] == '/' && chars.get(at + 1) == Some(&'*') {
                    depth += 1;
                    at += 2;
                } else if chars[at] == '*' && chars.get(at + 1) == Some(&'/') {
                    depth -= 1;
                    at += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    at += 1;
                }
            }
            out.push(' ');
        } else if current == 'r' && matches!(next, Some('"' | '#')) && {
            let mut hashes = at + 1;
            while chars.get(hashes) == Some(&'#') {
                hashes += 1;
            }
            chars.get(hashes) == Some(&'"')
        } {
            let mut hashes = 0;
            at += 1;
            while chars.get(at) == Some(&'#') {
                hashes += 1;
                at += 1;
            }
            at += 1;
            while at < chars.len() {
                if chars[at] == '"'
                    && (0..hashes)
                        .all(|offset| chars.get(at + 1 + offset) == Some(&'#'))
                {
                    at += 1 + hashes;
                    break;
                }
                at += 1;
            }
            out.push(' ');
        } else if current == '"' {
            at += 1;
            while at < chars.len() && chars[at] != '"' {
                at += if chars[at] == '\\' { 2 } else { 1 };
            }
            at += 1;
            out.push(' ');
        } else if current == '\''
            && (next == Some('\\') || chars.get(at + 2) == Some(&'\''))
        {
            at += 1;
            while at < chars.len() && chars[at] != '\'' {
                at += if chars[at] == '\\' { 2 } else { 1 };
            }
            at += 1;
            out.push(' ');
        } else {
            out.push(current);
            at += 1;
        }
    }
    out
}

/// Whether `code` contains `word` as a whole identifier.
fn names_token(code: &str, word: &str) -> bool {
    let is_identifier = |c: char| c.is_alphanumeric() || c == '_';
    code.match_indices(word).any(|(start, _)| {
        !code[..start].chars().next_back().is_some_and(is_identifier)
            && !code[start + word.len()..]
                .chars()
                .next()
                .is_some_and(is_identifier)
    })
}

fn package_name(manifest: &toml::Table) -> &str {
    manifest
        .get("package")
        .and_then(|package| package.get("name"))
        .and_then(toml::Value::as_str)
        .expect("a package name")
}

#[test]
fn only_the_bridge_contains_unsafe() {
    let root = workspace_root();
    let mut checked = 0;
    let mut found = Vec::new();
    for (path, manifest) in crate_manifests(&root) {
        let name = package_name(&manifest).to_owned();
        if name == BRIDGE_CRATE {
            continue;
        }
        checked += 1;
        let lints = manifest.get("lints").and_then(toml::Value::as_table);
        let inherits = lints.is_some_and(|lints| {
            lints.len() == 1
                && lints.get("workspace").and_then(toml::Value::as_bool) == Some(true)
        });
        if !inherits {
            found.push(format!(
                "{} does not inherit the workspace lints (`[lints] workspace = true`)",
                path.display()
            ));
        }
        let mut sources = Vec::new();
        rust_sources(path.parent().expect("a crate directory"), &mut sources);
        for source_path in sources {
            let source =
                std::fs::read_to_string(&source_path).expect("readable source");
            if names_token(&code_only(&source), "unsafe") {
                found.push(format!("{} contains `unsafe`", source_path.display()));
            }
        }
    }
    assert!(
        checked >= 15,
        "the scan covers the workspace ({checked} crates)"
    );
    assert!(
        found.is_empty(),
        "only `{BRIDGE_CRATE}` may contain `unsafe` or its own lint table:\n{}",
        found.join("\n")
    );
}

#[test]
fn the_unsafe_scan_reads_code_only() {
    assert!(names_token(&code_only("let x = unsafe { f() };"), "unsafe"));
    assert!(names_token(
        &code_only("unsafe extern \"C\" fn f() {}"),
        "unsafe"
    ));
    for harmless in [
        "// unsafe is not used",
        "/* unsafe /* nested */ still a comment */",
        "let s = \"unsafe\";",
        "let s = r#\"unsafe \"quoted\"\"#;",
        "let c = 'u'; let t: &'static str = \"\";",
        "#![forbid(unsafe_code)]",
        "let unsafe_count = 0;",
    ] {
        assert!(
            !names_token(&code_only(harmless), "unsafe"),
            "`{harmless}` is not unsafe code"
        );
    }
}

#[test]
fn the_bridge_lints_are_the_workspace_lints_except_for_unsafe() {
    let root = workspace_root();
    let workspace = read_manifest(&root.join("Cargo.toml"));
    let bridge =
        read_manifest(&root.join("crates").join(BRIDGE_CRATE).join("Cargo.toml"));
    let table = |manifest: &toml::Table, outer: &str, tool: &str| -> toml::Table {
        manifest
            .get(outer)
            .and_then(|lints| lints.get(tool))
            .and_then(toml::Value::as_table)
            .cloned()
            .unwrap_or_else(|| panic!("`[{outer}.{tool}]` is missing"))
    };
    let string = |value: &str| toml::Value::String(value.to_owned());
    for (tool, replaced, added) in [
        (
            "rust",
            ("unsafe_code", "allow"),
            ("unsafe_op_in_unsafe_fn", "deny"),
        ),
        (
            "clippy",
            // No workspace clippy lint is replaced; the pair below is a no-op
            // placeholder that keeps the shape of the check uniform.
            ("", ""),
            ("undocumented_unsafe_blocks", "deny"),
        ),
    ] {
        let mut expected = table(&workspace, "workspace", "lints")
            .get(tool)
            .and_then(toml::Value::as_table)
            .cloned()
            .expect("workspace lints");
        if !replaced.0.is_empty() {
            assert!(
                expected.contains_key(replaced.0),
                "the workspace sets `{}`",
                replaced.0
            );
            expected.insert(replaced.0.to_owned(), string(replaced.1));
        }
        expected.insert(added.0.to_owned(), string(added.1));
        assert_eq!(
            table(&bridge, "lints", tool),
            expected,
            "the bridge's `[lints.{tool}]` must be the workspace's, except for `unsafe`"
        );
    }
    let workspace_rust = table(&workspace, "workspace", "lints");
    assert_eq!(
        workspace_rust
            .get("rust")
            .and_then(|rust| rust.get("unsafe_code"))
            .and_then(toml::Value::as_str),
        Some("forbid"),
        "the workspace keeps `unsafe_code = \"forbid\"`"
    );
    assert!(
        bridge
            .get("lints")
            .and_then(|lints| lints.get("workspace"))
            .is_none(),
        "the bridge states its own lints"
    );
}

#[test]
fn the_bridge_carries_no_compiler_ir() {
    let root = workspace_root();
    let manifest =
        read_manifest(&root.join("crates").join(BRIDGE_CRATE).join("Cargo.toml"));
    let section = |name: &str| -> BTreeSet<String> {
        manifest
            .get(name)
            .and_then(toml::Value::as_table)
            .map(|table| table.keys().cloned().collect())
            .unwrap_or_default()
    };
    assert_eq!(
        section("dependencies"),
        BTreeSet::from([
            "vibra-host".to_owned(),
            "vibra-ir".to_owned(),
            "vibra-nexus".to_owned()
        ]),
        "the archive's dependencies are the runtime, the host provider, and the closed registries"
    );
    assert!(section("build-dependencies").is_empty());
    assert!(
        section("dev-dependencies").is_subset(&BTreeSet::from([
            "vibra-diagnostics".to_owned(),
            "vibra-exec".to_owned(),
            "vibra-nexus-eval".to_owned()
        ])),
        "tests may use the stepper, the execution IR, and spans"
    );
    let compiler = ["vibra-exec", "vibra-lower", "vibra-types", "vibra-syntax"];
    for (name, edges) in workspace_graph(&root) {
        if name == "vibra-nexus" {
            assert!(
                compiler
                    .iter()
                    .all(|crate_name| !edges.contains(*crate_name)),
                "the runtime depends on no compiler crate"
            );
        }
    }
}

#[test]
fn the_runtime_and_the_execution_ir_agree_on_the_frame_bound() {
    assert_eq!(vibra_nexus::MAX_FRAME_SLOTS, vibra_exec::MAX_SLOTS);
}

/// The one crate that may depend on Cranelift (decision N12). The Wasm runner
/// reaches it only through Wasmtime, which is not a `cranelift-*` dependency
/// of any manifest here.
const CODEGEN_CRATE: &str = "vibra-codegen";

/// The Cranelift packages the generator names, all pinned to one version.
const CRANELIFT_PACKAGES: &[&str] = &[
    "cranelift-codegen",
    "cranelift-frontend",
    "cranelift-module",
    "cranelift-object",
];

#[test]
fn only_codegen_depends_on_cranelift() {
    let mut checked = 0;
    for (path, manifest) in crate_manifests(&workspace_root()) {
        let name = package_name(&manifest).to_owned();
        let cranelift = all_dependencies(&manifest)
            .into_iter()
            .filter(|dependency| dependency.starts_with("cranelift"))
            .collect::<BTreeSet<_>>();
        if name == CODEGEN_CRATE {
            assert_eq!(
                cranelift,
                CRANELIFT_PACKAGES
                    .iter()
                    .map(|package| (*package).to_owned())
                    .collect::<BTreeSet<_>>(),
                "{}: the generator names the four Cranelift packages",
                path.display()
            );
        } else {
            assert!(
                cranelift.is_empty(),
                "{}: `{name}` names {cranelift:?}; only `{CODEGEN_CRATE}` depends on Cranelift",
                path.display()
            );
        }
        checked += 1;
    }
    assert!(
        checked >= 15,
        "the scan covers the workspace ({checked} crates)"
    );
}

/// The locked versions of every package called `name`.
fn locked_versions(lock: &toml::Table, name: &str) -> Vec<String> {
    lock.get("package")
        .and_then(toml::Value::as_array)
        .expect("a lock file with packages")
        .iter()
        .filter(|package| {
            package.get("name").and_then(toml::Value::as_str) == Some(name)
        })
        .filter_map(|package| package.get("version").and_then(toml::Value::as_str))
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_lock_file_holds_one_cranelift_and_the_generator_pins_it() {
    let root = workspace_root();
    let lock = read_manifest(&root.join("Cargo.lock"));
    let manifest =
        read_manifest(&root.join("crates").join(CODEGEN_CRATE).join("Cargo.toml"));
    let dependencies = manifest
        .get("dependencies")
        .and_then(toml::Value::as_table)
        .expect("dependencies");
    let locked = locked_versions(&lock, "cranelift-codegen");
    assert_eq!(
        locked.len(),
        1,
        "one version of `cranelift-codegen`: {locked:?}"
    );
    for package in CRANELIFT_PACKAGES {
        let requirement = dependencies
            .get(*package)
            .and_then(toml::Value::as_str)
            .unwrap_or_else(|| panic!("`{package}` is a plain version string"));
        assert_eq!(
            requirement.strip_prefix('='),
            Some(locked[0].as_str()),
            "`{package}` is pinned with `=` to the locked `cranelift-codegen`"
        );
        assert_eq!(
            locked_versions(&lock, package),
            locked,
            "`{package}` is locked once, at the version of `cranelift-codegen`"
        );
    }
    // The packages the object writer shares with Wasmtime's graph are not
    // duplicated by the generator.
    let all = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .expect("packages")
        .iter()
        .filter_map(|package| package.get("name").and_then(toml::Value::as_str))
        .filter(|name| name.starts_with("cranelift-"))
        .collect::<BTreeSet<_>>();
    for name in all {
        assert_eq!(
            locked_versions(&lock, name).len(),
            1,
            "`{name}` is locked once"
        );
    }
    for shared in ["object", "gimli", "target-lexicon"] {
        assert_eq!(
            locked_versions(&lock, shared).len(),
            1,
            "`{shared}` is locked once"
        );
    }
}

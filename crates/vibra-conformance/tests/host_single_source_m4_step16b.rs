//! One source for the host operations (milestone 4 Step 16b-1).
//!
//! `docs/spec/06-runtime.md`, "Host calls and the builders": each host operation
//! is written once, in `vibra-host`, against an interface that reads arguments
//! and builds results. The reference interpreter supplies the interface over its
//! own values, the runner supplies it through the exported accessors and builders
//! and never through `vibra_v1_memory`, and both build the audit event with the
//! same code. This audit holds the interpreter, the emitter, and the runner to
//! that: none implements an operation, none calls a provider method itself,
//! none builds an event except by `AuditEvent::completed`, and the runner's host
//! path does not read the instance's memory. That the emitter does not know the
//! provider is the dependency rule of `architecture_boundary`.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs
)]

use std::path::{Path, PathBuf};

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
}

fn rust_sources(directory: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("readable source directory") {
        let path = entry.expect("readable directory entry").path();
        if path.is_dir() {
            rust_sources(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

/// The production code of one source file: without its test module, and
/// without comment lines.
fn production(path: &Path) -> String {
    let text = std::fs::read_to_string(path).expect("readable source");
    let end = text.find("\n#[cfg(test)]").unwrap_or(text.len());
    text[..end]
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn crate_sources(name: &str) -> Vec<(String, String)> {
    let mut paths = Vec::new();
    rust_sources(
        &workspace_root().join("crates").join(name).join("src"),
        &mut paths,
    );
    paths
        .into_iter()
        .map(|path| {
            let relative = path
                .strip_prefix(workspace_root())
                .expect("inside the workspace")
                .to_string_lossy()
                .replace('\\', "/");
            (relative, production(&path))
        })
        .collect()
}

/// Every method of the provider: the 17 operations.
const PROVIDER_METHODS: &[&str] = &[
    "stdin_text",
    "stdin_bytes",
    "stdout_text",
    "stdout_bytes",
    "stderr_text",
    "stderr_bytes",
    "read_file",
    "read_text",
    "read_entries",
    "write_file",
    "write_text",
    "stat",
    "env_var",
    "env_args",
    "now_wall",
    "now_monotonic",
    "random_bytes",
];

#[test]
fn no_backend_calls_a_provider_method_or_implements_an_operation() {
    for krate in ["vibra-interp", "vibra-wasm", "vibra-wasm-run"] {
        for (path, code) in crate_sources(krate) {
            for method in PROVIDER_METHODS {
                // `read_text` is also a method of the value-access trait, which
                // a backend implements and calls on itself.
                let call = if *method == "read_text" {
                    "provider.read_text(".to_owned()
                } else {
                    format!(".{method}(")
                };
                assert!(
                    !code.contains(&call),
                    "{path} calls the provider method `{method}`; only `vibra-host::perform` does"
                );
            }
            // The ambient filesystem, environment, and clock are the audit of
            // `ambient_reads_m4_step16`; the random algorithm is named here.
            assert!(
                !code.contains("SplitMix64"),
                "{path} names `SplitMix64`, which only `vibra-host` may"
            );
        }
    }
}

#[test]
fn every_event_is_built_by_the_one_function() {
    for krate in [
        "vibra-interp",
        "vibra-wasm-run",
        "vibra-workspace",
        "vibra-cli",
    ] {
        for (path, code) in crate_sources(krate) {
            for forbidden in ["HostEvent {", "AuditEvent::Host(", "AuditEvent::Opaque("]
            {
                assert!(
                    !code.contains(forbidden),
                    "{path} builds an event with `{forbidden}`; only `AuditEvent::completed` does"
                );
            }
        }
    }
    let interpreter = crate_sources("vibra-interp")
        .into_iter()
        .any(|(_, code)| code.contains("AuditEvent::completed("));
    let runner = crate_sources("vibra-wasm-run")
        .into_iter()
        .any(|(_, code)| code.contains("AuditEvent::completed("));
    assert!(interpreter && runner, "both backends build the event");
}

#[test]
fn the_runners_host_path_never_reads_the_instances_memory() {
    let path = workspace_root().join("crates/vibra-wasm-run/src/host.rs");
    let code = production(&path);
    for forbidden in [
        "MEMORY_EXPORT",
        "vibra_v1_memory",
        "get_memory",
        "Memory",
        "memory.data",
        "memory.read",
        "memory.write",
        "natives",
        "vibra_native",
    ] {
        assert!(
            !code.contains(forbidden),
            "the runner's host module mentions `{forbidden}`"
        );
    }
    // What it uses instead: the accessors and the builders.
    for required in [
        "LENGTH_EXPORT",
        "READ_I32_EXPORT",
        "READ_ID_EXPORT",
        "NEW_EXPORT",
        "WRITE_I32_EXPORT",
        "WRITE_ID_EXPORT",
        "SEAL_EXPORT",
    ] {
        assert!(
            code.contains(required),
            "{required} is how the runner reads and builds"
        );
    }
}

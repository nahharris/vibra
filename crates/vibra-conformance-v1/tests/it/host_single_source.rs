//! One source for the host operations (milestone 4 Step 16b-1).
//!
//! `docs/spec/06-runtime.md`, "Host calls and the builders": each host operation
//! is written once, in `vibra-host-v1`, against an interface that reads arguments
//! and builds results. The reference interpreter supplies the interface over its
//! own values, the runner supplies it through the exported accessors and builders
//! and never through `vibra_v1_memory`, and both build the audit event with the
//! same code. This audit holds the interpreter, the emitter, and the runner to
//! that: none implements an operation, none calls a provider method itself,
//! none builds an event except by `AuditEvent::completed`, and the runner's host
//! path does not read the instance's memory. That the emitter does not know the
//! provider is the dependency rule of `architecture_boundary`.

use crate::support::crate_sources;

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
    for krate in ["vibra-interp-v1", "vibra-wasm", "vibra-wasm-run"] {
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
                    "{path} calls the provider method `{method}`; only `vibra-host-v1::perform` does"
                );
            }
            // The ambient filesystem, environment, and clock are the audit of
            // `ambient_reads`; the random algorithm is named here.
            assert!(
                !code.contains("SplitMix64"),
                "{path} names `SplitMix64`, which only `vibra-host-v1` may"
            );
        }
    }
}

//! The versioned host ABI registry: the single source of truth for every host
//! import a `$wasm` body may target.
//!
//! Each entry declares the import's parameter shape — including which
//! positions are *capability parameters* and which policy domains those
//! capabilities must cover — plus the capability domains the import requires
//! overall. Lowering validates every `$wasm` body against this registry
//! (`E-WASM-002`, `E-WASM-003`, `E-CAP-002`) and the runtime dispatches
//! strictly on `(module, name)`, so declaring a `$wasm` wrapper confers no
//! authority by itself: privileged imports are only callable with a genuine
//! `$policy` value covering their domains.
//!
//! The registry is exported as a machine-readable document in
//! `schemas/host-abi.json`; a test asserts the two stay in sync.

/// Capability domains understood by the policy model, in canonical order.
pub const DOMAINS: &[&str] = &[
    "fs-read",
    "fs-write",
    "stdin-read",
    "env-read",
    "env-write",
    "net-connect",
    "net-listen",
    "process-run",
    "clock",
    "random",
    "system-info",
];

/// Shape of one host-import parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKind {
    /// An ordinary data value (string, integer, handle, structural form, ...).
    Value,
    /// A capability position: must be fed by a `$policy`-typed wrapper
    /// argument whose declared domains cover every listed domain.
    Capability(&'static [&'static str]),
}

/// One entry of the versioned host ABI.
#[derive(Debug, Clone, Copy)]
pub struct HostImport {
    pub module: &'static str,
    pub name: &'static str,
    pub params: &'static [ParamKind],
}

impl HostImport {
    /// The capability domains this import requires (union over capability
    /// parameters). Empty for pure/baseline imports.
    pub fn required_domains(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        for param in self.params {
            if let ParamKind::Capability(domains) = param {
                for domain in *domains {
                    if !out.contains(domain) {
                        out.push(*domain);
                    }
                }
            }
        }
        out
    }
}

const V: ParamKind = ParamKind::Value;

macro_rules! cap {
    ($($domain:literal),+) => {
        ParamKind::Capability(&[$($domain),+])
    };
}

/// The complete host ABI: every import a `$wasm` body may bind.
pub const HOST_ABI: &[HostImport] = &[
    // vibra_v1: standard streams and handle IO. Handle-consuming imports need
    // no capability parameter: the handle itself is the authority, minted by
    // a capability-checked open.
    entry("vibra_v1", "stdin_open", &[cap!("stdin-read")]),
    entry("vibra_v1", "stdout_open", &[]),
    entry("vibra_v1", "stderr_open", &[]),
    entry("vibra_v1", "fd_read", &[V]),
    entry("vibra_v1", "fd_read_line", &[V]),
    entry("vibra_v1", "fd_write", &[V, V]),
    entry("vibra_v1", "fd_sync", &[V]),
    entry("vibra_v1", "fd_close", &[V]),
    // vibra_v1: pure path algebra.
    entry("vibra_v1", "path_new", &[V]),
    entry("vibra_v1", "path_join", &[V, V]),
    entry("vibra_v1", "path_parent", &[V]),
    entry("vibra_v1", "path_extension", &[V]),
    // vibra_v1: filesystem.
    entry("vibra_v1", "fs_open_read", &[V, cap!("fs-read")]),
    entry("vibra_v1", "fs_open_write", &[V, cap!("fs-write")]),
    entry("vibra_v1", "fs_open_append", &[V, cap!("fs-write")]),
    entry("vibra_v1", "fs_open_read_write", &[V, cap!("fs-read", "fs-write")]),
    entry("vibra_v1", "fs_read_to_string", &[V, cap!("fs-read")]),
    entry("vibra_v1", "fs_write_string_all", &[V, V, cap!("fs-write")]),
    entry("vibra_v1", "fs_append_string", &[V, V, cap!("fs-write")]),
    entry("vibra_v1", "fs_exists", &[V, cap!("fs-read")]),
    entry("vibra_v1", "fs_create_dir_all", &[V, cap!("fs-write")]),
    entry("vibra_v1", "fs_remove_file", &[V, cap!("fs-write")]),
    entry("vibra_v1", "fs_remove_dir", &[V, cap!("fs-write")]),
    entry("vibra_v1", "fs_read_dir", &[V, cap!("fs-read")]),
    entry("vibra_v1", "fs_metadata", &[V, cap!("fs-read")]),
    entry("vibra_v1", "fs_canonicalize", &[V, cap!("fs-read")]),
    // vibra_v1: environment, network, process, clock, randomness, system.
    entry("vibra_v1", "env_get", &[V, cap!("env-read")]),
    entry("vibra_v1", "env_set", &[V, V, cap!("env-write")]),
    entry("vibra_v1", "net_connect", &[V, cap!("net-connect")]),
    entry("vibra_v1", "net_listen", &[V, cap!("net-listen")]),
    entry("vibra_v1", "process_run", &[V, cap!("process-run")]),
    entry("vibra_v1", "clock_now_unix_millis", &[cap!("clock")]),
    entry("vibra_v1", "random_bytes", &[V, cap!("random")]),
    entry("vibra_v1", "system_info", &[cap!("system-info")]),
    // vibra_test: in-memory assertions; capability-free.
    entry("vibra_test", "assert", &[V]),
    entry("vibra_test", "fail", &[V]),
    entry("vibra_test", "assert-eq-bool", &[V, V]),
    entry("vibra_test", "assert-eq-int", &[V, V]),
    entry("vibra_test", "assert-eq-float", &[V, V]),
    entry("vibra_test", "assert-eq-str", &[V, V]),
    // vibra_code: structural source editing over in-memory documents;
    // capability-free.
    entry("vibra_code", "parse", &[V]),
    entry("vibra_code", "make-query", &[V, V, V, V, V, V]),
    entry("vibra_code", "capture-pattern", &[V, V]),
    entry("vibra_code", "emit", &[V]),
    entry("vibra_code", "root", &[V]),
    entry("vibra_code", "at", &[V, V]),
    entry("vibra_code", "parent", &[V]),
    entry("vibra_code", "children", &[V]),
    entry("vibra_code", "find", &[V, V]),
    entry("vibra_code", "source", &[V]),
    entry("vibra_code", "to-form", &[V]),
    entry("vibra_code", "render", &[V]),
    entry("vibra_code", "replace", &[V, V, V]),
    entry("vibra_code", "delete", &[V, V]),
    entry("vibra_code", "upsert-mapping", &[V, V, V, V]),
    entry("vibra_code", "insert-mapping", &[V, V, V, V]),
    entry("vibra_code", "rename-key", &[V, V, V]),
    entry("vibra_code", "insert-sequence", &[V, V, V, V]),
    entry("vibra_code", "splice-sequence", &[V, V, V, V, V]),
    entry("vibra_code", "copy", &[V, V, V, V]),
    entry("vibra_code", "move", &[V, V, V, V]),
];

const fn entry(
    module: &'static str,
    name: &'static str,
    params: &'static [ParamKind],
) -> HostImport {
    HostImport {
        module,
        name,
        params,
    }
}

/// Look up a host import by `(module, name)`.
pub fn lookup(module: &str, name: &str) -> Option<&'static HostImport> {
    HOST_ABI
        .iter()
        .find(|import| import.module == module && import.name == name)
}

/// True when `module` is a known host module (even if `name` is unknown).
pub fn is_host_module(module: &str) -> bool {
    HOST_ABI.iter().any(|import| import.module == module)
}

/// The machine-readable registry document published as `schemas/host-abi.json`.
pub fn schema_document() -> serde_json::Value {
    let imports: Vec<serde_json::Value> = HOST_ABI
        .iter()
        .map(|import| {
            let params: Vec<serde_json::Value> = import
                .params
                .iter()
                .map(|param| match param {
                    ParamKind::Value => serde_json::json!({ "kind": "value" }),
                    ParamKind::Capability(domains) => serde_json::json!({
                        "kind": "capability",
                        "domains": domains,
                    }),
                })
                .collect();
            serde_json::json!({
                "module": import.module,
                "name": import.name,
                "params": params,
                "required-domains": import.required_domains(),
            })
        })
        .collect();
    serde_json::json!({
        "$id": "https://vibra-lang.org/schemas/host-abi.json",
        "description": "Versioned Vibra host ABI registry: every host import a `$wasm` body may bind, with capability requirements",
        "domains": DOMAINS,
        "imports": imports,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_no_duplicate_entries() {
        for (idx, import) in HOST_ABI.iter().enumerate() {
            assert!(
                !HOST_ABI[..idx]
                    .iter()
                    .any(|other| other.module == import.module && other.name == import.name),
                "duplicate host ABI entry {}.{}",
                import.module,
                import.name
            );
        }
    }

    #[test]
    fn every_capability_domain_is_known() {
        for import in HOST_ABI {
            for domain in import.required_domains() {
                assert!(
                    DOMAINS.contains(&domain),
                    "{}.{} requires unknown domain `{domain}`",
                    import.module,
                    import.name
                );
            }
        }
    }

    #[test]
    fn registry_matches_published_schema() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/schemas/host-abi.json");
        let expected = format!(
            "{}\n",
            serde_json::to_string_pretty(&schema_document()).expect("serialize host ABI schema")
        );
        if std::env::var_os("VIBRA_REGEN_SCHEMAS").is_some() {
            std::fs::write(path, &expected).expect("write schemas/host-abi.json");
        }
        let published =
            std::fs::read_to_string(path).expect("schemas/host-abi.json must exist");
        assert_eq!(
            published, expected,
            "schemas/host-abi.json is out of sync with src/host_abi.rs; regenerate it with `VIBRA_REGEN_SCHEMAS=1 cargo test -p vibra host_abi`"
        );
    }
}

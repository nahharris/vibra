//! Closed compiler-known host transitions used by native `deffect` operations.
//!
//! Intrinsics are intentionally a small naming layer over the versioned host
//! ABI. Source uses lowercase kebab-case atoms (`@fs-open-read`); the ABI keeps
//! its stable underscore spelling. No arbitrary module or symbol can be named
//! through this form.

use crate::host_abi::HostImport;

pub fn module_name(name: &str) -> &'static str {
    if name.starts_with("test-") {
        "vibra_test"
    } else {
        "vibra_v1"
    }
}

pub fn import_name(name: &str) -> String {
    if let Some(name) = name.strip_prefix("test-") {
        name.to_string()
    } else {
        name.replace('-', "_")
    }
}

pub fn lookup(name: &str) -> Option<&'static HostImport> {
    let import = import_name(name);
    crate::host_abi::lookup(module_name(name), &import)
}

pub fn is_known(name: &str) -> bool {
    lookup(name).is_some()
}

pub type Effect = (&'static str, &'static str);

/// The native root inventory. Effect names are not arbitrary user-defined type
/// aliases: they are the compiler/stdlib contract. Keeping the inventory next
/// to the intrinsic registry lets both lowering paths reject misspelled roots
/// consistently.
const ROOTS: &[(&str, &[&str])] = &[
    ("stream", &["read", "write", "manage"]),
    ("fs", &["read", "write", "metadata"]),
    ("io", &["stdin", "stdout", "stderr"]),
    ("net", &["connect", "bind"]),
    ("process", &["spawn", "wait", "signal"]),
    ("env", &["read", "write"]),
    ("time", &["now", "sleep"]),
    ("random", &["generate"]),
    ("sys", &["info"]),
];

pub fn is_known_root(domain: &str, action: &str) -> bool {
    ROOTS
        .iter()
        .any(|(known_domain, actions)| *known_domain == domain && actions.contains(&action))
}

/// Native effect roots for intrinsic transitions. This table uses the same
/// nominal root vocabulary exposed by the stdlib and effect report.
pub fn effects_for(name: &str) -> &'static [Effect] {
    intrinsic_effects_for_import(&import_name(name))
}

/// Whether an intrinsic crosses the ambient-authority boundary. Pure value
/// intrinsics (path/bytes/text/collection algebra) may be used by ordinary
/// functions; typed host transitions must be owned by a `deffect` operation.
pub fn requires_effect(name: &str) -> bool {
    !effects_for(name).is_empty()
}

/// Host results whose legacy ABI only carries a generic handle/result shape may
/// specialize only the concrete endpoint type that the transition is known to
/// acquire. Every other nominal host-backed result is rejected by lowering.
/// The canonical names (possibly prefixed by an internal module namespace)
/// are part of the nominal endpoint contract; a matching access mode alone is
/// not sufficient.
pub fn endpoint_result_suffixes(name: &str) -> &'static [&'static str] {
    match import_name(name).as_str() {
        "fs_open_read" => &["fs.reader"],
        "fs_open_write" | "fs_open_write_options" => &["fs.writer"],
        "fs_open_append" => &["fs.appender"],
        "fs_open_read_write" => &["fs.file"],
        "stdin_open" => &["io.input"],
        "stdout_open" => &["io.output"],
        "stderr_open" => &["io.error-output"],
        "net_connect" | "net_accept" => &["net.tcp-stream"],
        "net_listen" => &["net.tcp-listener", "net.listener"],
        "net_udp_bind" => &["net.udp-socket"],
        "process_spawn" => &["process.child"],
        "process_child_stdin" => &["process.child-stdin"],
        "process_child_stdout" => &["process.child-stdout"],
        "process_child_stderr" => &["process.child-stderr"],
        _ => &[],
    }
}

fn intrinsic_effects_for_import(name: &str) -> &'static [Effect] {
    match name {
        "fs_open_read" => &[("fs", "read")],
        "fs_open_write"
        | "fs_open_write_options"
        | "fs_open_append"
        | "fs_create_dir_all"
        | "fs_remove_file"
        | "fs_remove_dir"
        | "fs_rename" => &[("fs", "write")],
        "fs_open_read_write" => &[("fs", "write"), ("fs", "read")],
        "fs_exists" | "fs_metadata" | "fs_canonicalize" => &[("fs", "metadata")],
        "fd_read" | "fd_read_bytes" | "fd_read_line" | "fd_read_bytes_up_to" => {
            &[("stream", "read")]
        }
        "fd_write" | "fd_write_bytes" | "fd_write_bytes_some" => &[("stream", "write")],
        "fd_sync" | "fd_close" | "net_set_deadline" | "net_local_address" => {
            &[("stream", "manage")]
        }
        "stdin_open" => &[("io", "stdin")],
        "stdout_open" => &[("io", "stdout")],
        "stderr_open" => &[("io", "stderr")],
        "net_resolve" | "net_connect" | "net_udp_connect" => &[("net", "connect")],
        "net_listen" | "net_accept" | "net_udp_bind" => &[("net", "bind")],
        "net_udp_send_to" => &[("stream", "write")],
        "net_shutdown" => &[("stream", "manage")],
        "net_udp_recv_from" => &[("stream", "read")],
        "process_spawn"
        | "process_child_stdin"
        | "process_child_stdout"
        | "process_child_stderr" => &[("process", "spawn")],
        "process_wait" => &[("process", "wait")],
        "process_kill" => &[("process", "signal")],
        "env_get" | "env_list" => &[("env", "read")],
        "env_set" | "env_remove" => &[("env", "write")],
        "clock_now_unix_millis" | "clock_monotonic_millis" => &[("time", "now")],
        "clock_sleep_millis" => &[("time", "sleep")],
        "random_bytes" => &[("random", "generate")],
        "system_info" | "system_args" | "system_current_dir" | "system_executable"
        | "system_temp_dir" => &[("sys", "info")],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intrinsic_names_are_closed_and_kebab_mapped() {
        let entry = lookup("clock-now-unix-millis").expect("clock intrinsic");
        assert_eq!(entry.name, "clock_now_unix_millis");
        let test = lookup("test-assert").expect("test intrinsic");
        assert_eq!(test.module, "vibra_test");
        assert_eq!(test.name, "assert");
        assert!(lookup("not-a-host-transition").is_none());
        assert!(lookup("fs-read-to-string").is_none());
        assert!(lookup("fs-read-dir").is_none());
    }

    #[test]
    fn native_root_inventory_is_closed() {
        assert!(is_known_root("stream", "read"));
        assert!(is_known_root("process", "signal"));
        assert!(!is_known_root("stream", "unknown"));
        assert!(!is_known_root("database", "read"));
    }
}

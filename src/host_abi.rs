//! The versioned host ABI registry: the single source of truth for every host
//! import a `$wasm` body may target.
//!
//! Each entry declares the import's parameter shape. Lowering validates every
//! `$wasm` body against this registry (`E-WASM-002`, `E-WASM-003`) and the
//! runtime dispatches strictly on `(module, name)`.
//!
//! The registry is exported as a machine-readable document in
//! `schemas/host-abi.json`; a test asserts the two stay in sync.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    Void,
    Bool,
    Atom,
    Int64,
    UInt64,
    Duration,
    Instant,
    NetAddress,
    TcpStream,
    TcpListener,
    UdpSocket,
    Float64,
    Str,
    Bytes,
    Path,
    ReadHandle,
    WriteHandle,
    ProcessHandle,
    AnyHandle,
    ResultVoid,
    ResultStr,
    ResultBytes,
    ResultReadHandle,
    ResultWriteHandle,
    ResultProcessHandle,
    ResultPaths,
    ResultPath,
    OptionPath,
    OptionStr,
    OptionAny,
    ResultAny,
    Any,
    Array,
    StringMap,
    Record,
}

/// One entry of the versioned host ABI.
#[derive(Debug, Clone, Copy)]
pub struct HostImport {
    pub module: &'static str,
    pub name: &'static str,
    pub params: &'static [ValueKind],
    pub result: ValueKind,
}

impl ValueKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Void => "void",
            Self::Bool => "bool",
            Self::Atom => "atom",
            Self::Int64 => "int64",
            Self::UInt64 => "uint64",
            Self::Duration => "duration",
            Self::Instant => "instant",
            Self::NetAddress => "net-address",
            Self::TcpStream => "tcp-stream",
            Self::TcpListener => "tcp-listener",
            Self::UdpSocket => "udp-socket",
            Self::Float64 => "float64",
            Self::Str => "str",
            Self::Bytes => "bytes",
            Self::Path => "path",
            Self::ReadHandle => "read-handle",
            Self::WriteHandle => "write-handle",
            Self::ProcessHandle => "process-handle",
            Self::AnyHandle => "any-handle",
            Self::ResultVoid => "result-void",
            Self::ResultStr => "result-str",
            Self::ResultBytes => "result-bytes",
            Self::ResultReadHandle => "result-read-handle",
            Self::ResultWriteHandle => "result-write-handle",
            Self::ResultProcessHandle => "result-process-handle",
            Self::ResultPaths => "result-paths",
            Self::ResultPath => "result-path",
            Self::OptionPath => "option-path",
            Self::OptionStr => "option-str",
            Self::OptionAny => "option-any",
            Self::ResultAny => "result-any",
            Self::Any => "any",
            Self::Array => "array",
            Self::StringMap => "string-map",
            Self::Record => "record",
        }
    }
}

const BOOL: ValueKind = ValueKind::Bool;
const ATOM: ValueKind = ValueKind::Atom;
const I64: ValueKind = ValueKind::Int64;
const U64: ValueKind = ValueKind::UInt64;
const DURATION: ValueKind = ValueKind::Duration;
const INSTANT: ValueKind = ValueKind::Instant;
const NET_ADDRESS: ValueKind = ValueKind::NetAddress;
const TCP_STREAM: ValueKind = ValueKind::TcpStream;
const TCP_LISTENER: ValueKind = ValueKind::TcpListener;
const UDP_SOCKET: ValueKind = ValueKind::UdpSocket;
const F64: ValueKind = ValueKind::Float64;
const STR: ValueKind = ValueKind::Str;
const BYTES: ValueKind = ValueKind::Bytes;
const PATH: ValueKind = ValueKind::Path;
const READ_HANDLE: ValueKind = ValueKind::ReadHandle;
const WRITE_HANDLE: ValueKind = ValueKind::WriteHandle;
const PROCESS_HANDLE: ValueKind = ValueKind::ProcessHandle;
const ANY_HANDLE: ValueKind = ValueKind::AnyHandle;

/// The complete host ABI: every import a `$wasm` body may bind.
pub const HOST_ABI: &[HostImport] = &[
    // vibra_v1: standard streams and handle IO.
    entry("vibra_v1", "stdin_open", &[], ValueKind::ReadHandle),
    entry("vibra_v1", "stdout_open", &[], ValueKind::WriteHandle),
    entry("vibra_v1", "stderr_open", &[], ValueKind::WriteHandle),
    entry("vibra_v1", "fd_read", &[READ_HANDLE], ValueKind::ResultStr),
    entry(
        "vibra_v1",
        "fd_read_bytes",
        &[READ_HANDLE],
        ValueKind::ResultBytes,
    ),
    entry(
        "vibra_v1",
        "fd_read_line",
        &[READ_HANDLE],
        ValueKind::ResultStr,
    ),
    entry(
        "vibra_v1",
        "fd_read_bytes_up_to",
        &[READ_HANDLE, U64],
        ValueKind::ResultBytes,
    ),
    entry(
        "vibra_v1",
        "fd_write",
        &[WRITE_HANDLE, STR],
        ValueKind::ResultVoid,
    ),
    entry(
        "vibra_v1",
        "fd_write_bytes",
        &[WRITE_HANDLE, BYTES],
        ValueKind::ResultVoid,
    ),
    entry(
        "vibra_v1",
        "fd_write_bytes_some",
        &[WRITE_HANDLE, BYTES],
        ValueKind::ResultAny,
    ),
    entry(
        "vibra_v1",
        "fd_sync",
        &[WRITE_HANDLE],
        ValueKind::ResultVoid,
    ),
    entry("vibra_v1", "fd_close", &[ANY_HANDLE], ValueKind::ResultVoid),
    // vibra_v1: pure path algebra.
    entry("vibra_v1", "path_new", &[STR], ValueKind::Path),
    entry("vibra_v1", "path_join", &[PATH, STR], ValueKind::Path),
    entry("vibra_v1", "path_parent", &[PATH], ValueKind::OptionPath),
    entry("vibra_v1", "path_extension", &[PATH], ValueKind::OptionStr),
    entry("vibra_v1", "path_file_name", &[PATH], ValueKind::OptionStr),
    entry("vibra_v1", "path_components", &[PATH], ValueKind::ResultAny),
    // vibra_v1: pure byte-array helpers.
    entry("vibra_v1", "bytes_len", &[BYTES], ValueKind::UInt64),
    entry(
        "vibra_v1",
        "bytes_slice",
        &[BYTES, U64, U64],
        ValueKind::Bytes,
    ),
    entry("vibra_v1", "bytes_from_str", &[STR], ValueKind::Bytes),
    entry("vibra_v1", "bytes_get", &[BYTES, U64], ValueKind::OptionAny),
    entry(
        "vibra_v1",
        "bytes_concat",
        &[BYTES, BYTES],
        ValueKind::ResultAny,
    ),
    entry(
        "vibra_v1",
        "bytes_find",
        &[BYTES, BYTES],
        ValueKind::OptionAny,
    ),
    entry(
        "vibra_v1",
        "bytes_from_array",
        &[ValueKind::Array],
        ValueKind::ResultAny,
    ),
    entry(
        "vibra_v1",
        "bytes_decode_utf8",
        &[BYTES],
        ValueKind::ResultAny,
    ),
    // UTF-8 text. Text offsets are Unicode-scalar offsets; byte length is
    // named explicitly. Every allocating operation is bounded by RunConfig.
    entry("vibra_v1", "str_scalar_len", &[STR], ValueKind::UInt64),
    entry("vibra_v1", "str_byte_len", &[STR], ValueKind::UInt64),
    entry("vibra_v1", "str_is_empty", &[STR], ValueKind::Bool),
    entry("vibra_v1", "str_contains", &[STR, STR], ValueKind::Bool),
    entry("vibra_v1", "str_starts_with", &[STR, STR], ValueKind::Bool),
    entry("vibra_v1", "str_ends_with", &[STR, STR], ValueKind::Bool),
    entry("vibra_v1", "str_find", &[STR, STR], ValueKind::OptionAny),
    entry(
        "vibra_v1",
        "str_scalar_at",
        &[STR, U64],
        ValueKind::OptionStr,
    ),
    entry("vibra_v1", "str_split", &[STR, STR], ValueKind::ResultAny),
    entry("vibra_v1", "str_trim", &[STR], ValueKind::ResultAny),
    entry(
        "vibra_v1",
        "str_replace",
        &[STR, STR, STR],
        ValueKind::ResultAny,
    ),
    entry(
        "vibra_v1",
        "str_join",
        &[ValueKind::Array, STR],
        ValueKind::ResultAny,
    ),
    entry("vibra_v1", "str_lowercase", &[STR], ValueKind::ResultAny),
    entry("vibra_v1", "str_uppercase", &[STR], ValueKind::ResultAny),
    // Locale-free primitive parsing and canonical formatting.
    entry("vibra_v1", "parse_int64", &[STR], ValueKind::ResultAny),
    entry("vibra_v1", "parse_uint64", &[STR], ValueKind::ResultAny),
    entry("vibra_v1", "parse_float64", &[STR], ValueKind::ResultAny),
    entry("vibra_v1", "parse_bool", &[STR], ValueKind::ResultAny),
    entry("vibra_v1", "format_int64", &[I64], ValueKind::Str),
    entry("vibra_v1", "format_uint64", &[U64], ValueKind::Str),
    entry("vibra_v1", "format_float64", &[F64], ValueKind::Str),
    entry("vibra_v1", "format_bool", &[BOOL], ValueKind::Str),
    entry("vibra_v1", "format_atom", &[ATOM], ValueKind::Str),
    // Pure, deterministic collection operations. `Any` is intentional: the
    // generic wrapper signatures are statically checked by Vibra lowering.
    entry(
        "vibra_v1",
        "array_len",
        &[ValueKind::Array],
        ValueKind::UInt64,
    ),
    entry(
        "vibra_v1",
        "array_get",
        &[ValueKind::Array, U64],
        ValueKind::OptionAny,
    ),
    entry(
        "vibra_v1",
        "array_set",
        &[ValueKind::Array, U64, ValueKind::Any],
        ValueKind::ResultAny,
    ),
    entry(
        "vibra_v1",
        "array_slice",
        &[ValueKind::Array, U64, U64],
        ValueKind::ResultAny,
    ),
    entry(
        "vibra_v1",
        "array_append",
        &[ValueKind::Array, ValueKind::Any],
        ValueKind::ResultAny,
    ),
    entry(
        "vibra_v1",
        "array_insert",
        &[ValueKind::Array, U64, ValueKind::Any],
        ValueKind::ResultAny,
    ),
    entry(
        "vibra_v1",
        "array_remove",
        &[ValueKind::Array, U64],
        ValueKind::ResultAny,
    ),
    entry(
        "vibra_v1",
        "array_contains",
        &[ValueKind::Array, ValueKind::Any],
        ValueKind::Bool,
    ),
    entry(
        "vibra_v1",
        "map_len",
        &[ValueKind::StringMap],
        ValueKind::UInt64,
    ),
    entry(
        "vibra_v1",
        "map_get",
        &[ValueKind::StringMap, STR],
        ValueKind::OptionAny,
    ),
    entry(
        "vibra_v1",
        "map_insert",
        &[ValueKind::StringMap, STR, ValueKind::Any],
        ValueKind::ResultAny,
    ),
    entry(
        "vibra_v1",
        "map_remove",
        &[ValueKind::StringMap, STR],
        ValueKind::OptionAny,
    ),
    entry(
        "vibra_v1",
        "map_contains_key",
        &[ValueKind::StringMap, STR],
        ValueKind::Bool,
    ),
    // vibra_v1: filesystem.
    entry(
        "vibra_v1",
        "fs_open_read",
        &[PATH],
        ValueKind::ResultReadHandle,
    ),
    entry(
        "vibra_v1",
        "fs_open_write",
        &[PATH],
        ValueKind::ResultWriteHandle,
    ),
    entry(
        "vibra_v1",
        "fs_open_write_options",
        &[PATH, BOOL, BOOL, BOOL],
        ValueKind::ResultWriteHandle,
    ),
    entry(
        "vibra_v1",
        "fs_open_append",
        &[PATH],
        ValueKind::ResultWriteHandle,
    ),
    entry(
        "vibra_v1",
        "fs_open_read_write",
        &[PATH],
        ValueKind::ResultReadHandle,
    ),
    entry(
        "vibra_v1",
        "fs_read_to_string",
        &[PATH],
        ValueKind::ResultStr,
    ),
    entry(
        "vibra_v1",
        "fs_write_string_all",
        &[PATH, STR],
        ValueKind::ResultVoid,
    ),
    entry(
        "vibra_v1",
        "fs_append_string",
        &[PATH, STR],
        ValueKind::ResultVoid,
    ),
    entry("vibra_v1", "fs_exists", &[PATH], ValueKind::Bool),
    entry(
        "vibra_v1",
        "fs_create_dir_all",
        &[PATH],
        ValueKind::ResultVoid,
    ),
    entry("vibra_v1", "fs_remove_file", &[PATH], ValueKind::ResultVoid),
    entry("vibra_v1", "fs_remove_dir", &[PATH], ValueKind::ResultVoid),
    entry("vibra_v1", "fs_read_dir", &[PATH], ValueKind::ResultPaths),
    entry(
        "vibra_v1",
        "fs_read_dir_entries",
        &[PATH],
        ValueKind::ResultAny,
    ),
    entry("vibra_v1", "fs_metadata", &[PATH], ValueKind::ResultStr),
    entry(
        "vibra_v1",
        "fs_canonicalize",
        &[PATH],
        ValueKind::ResultPath,
    ),
    entry(
        "vibra_v1",
        "fs_rename",
        &[PATH, PATH],
        ValueKind::ResultVoid,
    ),
    entry("vibra_v1", "fs_copy", &[PATH, PATH], ValueKind::ResultVoid),
    // vibra_v1: environment, network, process, clock, randomness, system.
    entry("vibra_v1", "env_get", &[STR], ValueKind::ResultStr),
    entry("vibra_v1", "env_set", &[STR, STR], ValueKind::ResultVoid),
    entry("vibra_v1", "env_remove", &[STR], ValueKind::ResultVoid),
    entry("vibra_v1", "env_list", &[], ValueKind::Array),
    entry(
        "vibra_v1",
        "net_address_parse",
        &[STR],
        ValueKind::ResultAny,
    ),
    entry("vibra_v1", "net_resolve", &[STR], ValueKind::ResultAny),
    entry(
        "vibra_v1",
        "net_connect",
        &[NET_ADDRESS],
        ValueKind::ResultAny,
    ),
    entry(
        "vibra_v1",
        "net_listen",
        &[NET_ADDRESS],
        ValueKind::ResultAny,
    ),
    entry(
        "vibra_v1",
        "net_accept",
        &[TCP_LISTENER],
        ValueKind::ResultAny,
    ),
    entry(
        "vibra_v1",
        "net_local_address",
        &[ANY_HANDLE],
        ValueKind::ResultAny,
    ),
    entry(
        "vibra_v1",
        "net_set_deadline",
        &[TCP_STREAM, DURATION],
        ValueKind::ResultVoid,
    ),
    entry(
        "vibra_v1",
        "net_shutdown",
        &[TCP_STREAM, STR],
        ValueKind::ResultVoid,
    ),
    entry(
        "vibra_v1",
        "net_udp_bind",
        &[NET_ADDRESS],
        ValueKind::ResultAny,
    ),
    entry(
        "vibra_v1",
        "net_udp_connect",
        &[UDP_SOCKET, NET_ADDRESS],
        ValueKind::ResultVoid,
    ),
    entry(
        "vibra_v1",
        "net_udp_send_to",
        &[UDP_SOCKET, BYTES, NET_ADDRESS],
        ValueKind::ResultAny,
    ),
    entry(
        "vibra_v1",
        "net_udp_recv_from",
        &[UDP_SOCKET, U64],
        ValueKind::ResultAny,
    ),
    entry(
        "vibra_v1",
        "process_run",
        &[ValueKind::Record],
        ValueKind::ResultAny,
    ),
    entry(
        "vibra_v1",
        "process_spawn",
        &[ValueKind::Record],
        ValueKind::ResultProcessHandle,
    ),
    entry(
        "vibra_v1",
        "process_wait",
        &[PROCESS_HANDLE],
        ValueKind::ResultAny,
    ),
    entry(
        "vibra_v1",
        "process_kill",
        &[PROCESS_HANDLE],
        ValueKind::ResultVoid,
    ),
    entry(
        "vibra_v1",
        "process_child_stdin",
        &[PROCESS_HANDLE],
        ValueKind::ResultWriteHandle,
    ),
    entry(
        "vibra_v1",
        "process_child_stdout",
        &[PROCESS_HANDLE],
        ValueKind::ResultReadHandle,
    ),
    entry(
        "vibra_v1",
        "process_child_stderr",
        &[PROCESS_HANDLE],
        ValueKind::ResultReadHandle,
    ),
    entry("vibra_v1", "clock_now_unix_millis", &[], ValueKind::UInt64),
    entry(
        "vibra_v1",
        "clock_monotonic_millis",
        &[],
        ValueKind::Instant,
    ),
    entry(
        "vibra_v1",
        "clock_duration_from_millis",
        &[U64],
        ValueKind::Duration,
    ),
    entry(
        "vibra_v1",
        "clock_duration_add",
        &[DURATION, DURATION],
        ValueKind::ResultAny,
    ),
    entry(
        "vibra_v1",
        "clock_duration_between",
        &[INSTANT, INSTANT],
        ValueKind::ResultAny,
    ),
    entry(
        "vibra_v1",
        "clock_sleep_millis",
        &[DURATION],
        ValueKind::Void,
    ),
    entry("vibra_v1", "random_bytes", &[U64], ValueKind::Bytes),
    entry("vibra_v1", "system_info", &[], ValueKind::Record),
    entry("vibra_v1", "system_args", &[], ValueKind::Array),
    entry("vibra_v1", "system_current_dir", &[], ValueKind::ResultStr),
    entry("vibra_v1", "system_executable", &[], ValueKind::ResultStr),
    entry("vibra_v1", "system_temp_dir", &[], ValueKind::Str),
    // vibra_test: in-memory assertions.
    entry("vibra_test", "assert", &[BOOL], ValueKind::Void),
    entry("vibra_test", "fail", &[STR], ValueKind::Void),
    entry(
        "vibra_test",
        "assert-eq-bool",
        &[BOOL, BOOL],
        ValueKind::Void,
    ),
    entry("vibra_test", "assert-eq-int", &[I64, I64], ValueKind::Void),
    entry(
        "vibra_test",
        "assert-eq-float",
        &[F64, F64],
        ValueKind::Void,
    ),
    entry("vibra_test", "assert-eq-str", &[STR, STR], ValueKind::Void),
];

const fn entry(
    module: &'static str,
    name: &'static str,
    params: &'static [ValueKind],
    result: ValueKind,
) -> HostImport {
    HostImport {
        module,
        name,
        params,
        result,
    }
}

/// One effect, as its structural `(domain, action)` identity.
pub type Effect = (&'static str, &'static str);

/// The effects each host import performs.
///
/// This is the ground truth a `(wasm ...)` body is checked against, so a user module
/// cannot launder an effect by declaring `effects: ()` on a raw host import. The
/// registry declares these in good faith: the compiler treats the pairs as opaque
/// data and holds no opinion about what any particular effect means.
///
/// Kept as one table rather than a field on all ~130 `HostImport` rows so the
/// complete host effect surface is reviewable in a single place. Any import absent
/// from this table is pure -- which covers the `str`, `bytes`, `array`, `map`,
/// `path`, `format`, and `parse` algebra, and the `vibra_test` harness intrinsics.
/// A unit test asserts every key names a real import.
///
/// Domains follow stdlib module names rather than host ABI spellings: `@time @now`
/// rather than `@clock @now`, `@sys @info` rather than `@system @info`.
///
/// The split between `fs` and `io` is by *what* is addressed, not by direction:
/// `fs.*` covers the filesystem namespace (opening, removing, renaming, metadata,
/// whole-file convenience calls) while `io.*` covers stream reads and writes on an
/// already-open handle, whichever kind it is.
const HOST_EFFECTS: &[(&str, &str, &[Effect])] = &[
    // Filesystem namespace.
    ("vibra_v1", "fs_open_read", &[("fs", "read")]),
    ("vibra_v1", "fs_read_to_string", &[("fs", "read")]),
    ("vibra_v1", "fs_read_dir", &[("fs", "read")]),
    ("vibra_v1", "fs_read_dir_entries", &[("fs", "read")]),
    ("vibra_v1", "fs_open_write", &[("fs", "write")]),
    ("vibra_v1", "fs_open_write_options", &[("fs", "write")]),
    ("vibra_v1", "fs_open_append", &[("fs", "write")]),
    ("vibra_v1", "fs_write_string_all", &[("fs", "write")]),
    ("vibra_v1", "fs_append_string", &[("fs", "write")]),
    ("vibra_v1", "fs_create_dir_all", &[("fs", "write")]),
    ("vibra_v1", "fs_remove_file", &[("fs", "write")]),
    ("vibra_v1", "fs_remove_dir", &[("fs", "write")]),
    ("vibra_v1", "fs_rename", &[("fs", "write")]),
    (
        "vibra_v1",
        "fs_open_read_write",
        &[("fs", "read"), ("fs", "write")],
    ),
    ("vibra_v1", "fs_copy", &[("fs", "read"), ("fs", "write")]),
    ("vibra_v1", "fs_metadata", &[("fs", "metadata")]),
    ("vibra_v1", "fs_exists", &[("fs", "metadata")]),
    ("vibra_v1", "fs_canonicalize", &[("fs", "metadata")]),
    // Stream reads and writes on an open handle of any kind.
    ("vibra_v1", "fd_read", &[("io", "read")]),
    ("vibra_v1", "fd_read_line", &[("io", "read")]),
    ("vibra_v1", "fd_read_bytes", &[("io", "read")]),
    ("vibra_v1", "fd_read_bytes_up_to", &[("io", "read")]),
    ("vibra_v1", "fd_write", &[("io", "write")]),
    ("vibra_v1", "fd_write_bytes", &[("io", "write")]),
    ("vibra_v1", "fd_write_bytes_some", &[("io", "write")]),
    ("vibra_v1", "fd_sync", &[("io", "write")]),
    ("vibra_v1", "fd_close", &[("io", "write")]),
    ("vibra_v1", "stdin_open", &[("io", "read")]),
    ("vibra_v1", "stdout_open", &[("io", "write")]),
    ("vibra_v1", "stderr_open", &[("io", "write")]),
    // Network. `net_address_parse` is pure string work; `net_local_address` and
    // `net_set_deadline` only inspect or configure a socket already held.
    ("vibra_v1", "net_connect", &[("net", "connect")]),
    ("vibra_v1", "net_udp_connect", &[("net", "connect")]),
    ("vibra_v1", "net_resolve", &[("net", "connect")]),
    ("vibra_v1", "net_listen", &[("net", "listen")]),
    ("vibra_v1", "net_accept", &[("net", "listen")]),
    ("vibra_v1", "net_udp_bind", &[("net", "listen")]),
    ("vibra_v1", "net_udp_send_to", &[("net", "send")]),
    ("vibra_v1", "net_shutdown", &[("net", "send")]),
    ("vibra_v1", "net_udp_recv_from", &[("net", "receive")]),
    // Processes. `process_run` both creates and awaits. Killing is its own effect
    // rather than a flavour of waiting: waiting observes a child, killing terminates
    // it, and a program may legitimately be allowed to do the former without the
    // latter. The `process_child_std*` accessors only return handles, and reads or
    // writes through them are `io.*`.
    ("vibra_v1", "process_spawn", &[("process", "spawn")]),
    (
        "vibra_v1",
        "process_run",
        &[("process", "spawn"), ("process", "wait")],
    ),
    ("vibra_v1", "process_wait", &[("process", "wait")]),
    ("vibra_v1", "process_kill", &[("process", "kill")]),
    // Environment.
    ("vibra_v1", "env_get", &[("env", "read")]),
    ("vibra_v1", "env_list", &[("env", "read")]),
    ("vibra_v1", "env_set", &[("env", "write")]),
    ("vibra_v1", "env_remove", &[("env", "write")]),
    // Clock. Duration arithmetic is pure. Reading the clock and sleeping are
    // distinct effects: sleeping does not observe the time, it yields wall-clock
    // duration, which is separately reviewable (a program may legitimately read the
    // clock without being allowed to block).
    ("vibra_v1", "clock_now_unix_millis", &[("time", "now")]),
    ("vibra_v1", "clock_monotonic_millis", &[("time", "now")]),
    ("vibra_v1", "clock_sleep_millis", &[("time", "sleep")]),
    // Host introspection.
    ("vibra_v1", "system_info", &[("sys", "info")]),
    ("vibra_v1", "system_args", &[("sys", "info")]),
    ("vibra_v1", "system_current_dir", &[("sys", "info")]),
    ("vibra_v1", "system_executable", &[("sys", "info")]),
    ("vibra_v1", "system_temp_dir", &[("sys", "info")]),
    ("vibra_v1", "random_bytes", &[("random", "generate")]),
];

/// The effects a host import performs, or an empty slice if it is pure.
pub fn effects_for(module: &str, name: &str) -> &'static [Effect] {
    HOST_EFFECTS
        .iter()
        .find(|(entry_module, entry_name, _)| *entry_module == module && *entry_name == name)
        .map_or(&[], |(_, _, effects)| *effects)
}

impl HostImport {
    /// The effects this import performs, declared by the registry.
    pub fn effects(&self) -> &'static [Effect] {
        effects_for(self.module, self.name)
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
                .map(|kind| {
                    serde_json::json!({
                        "kind": "value",
                        "type": kind.as_str(),
                    })
                })
                .collect();
            let effects: Vec<serde_json::Value> = import
                .effects()
                .iter()
                .map(|(domain, action)| serde_json::json!({ "domain": domain, "action": action }))
                .collect();
            let intrinsic_effects: Vec<serde_json::Value> = crate::intrinsics::effects_for(
                import.name,
            )
            .iter()
            .map(|(root, operation)| serde_json::json!({ "root": root, "operation": operation }))
            .collect();
            serde_json::json!({
                "module": import.module,
                "name": import.name,
                "params": params,
                "return": import.result.as_str(),
                "effects": effects,
                "intrinsic-effects": intrinsic_effects,
            })
        })
        .collect();
    serde_json::json!({
        "$id": "https://vibra-lang.org/schemas/host-abi.json",
        "description": "Versioned Vibra host ABI registry: every host import a `$wasm` body may bind",
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

    /// The cost of keeping effects in their own table instead of on every row: a key
    /// could name an import that does not exist, or two, and nothing would notice.
    #[test]
    fn every_effect_table_key_names_exactly_one_real_import() {
        for (idx, (module, name, effects)) in HOST_EFFECTS.iter().enumerate() {
            assert!(
                lookup(module, name).is_some(),
                "effect table names unknown host import {module}.{name}"
            );
            assert!(
                !effects.is_empty(),
                "{module}.{name} is listed with no effects; omit it instead, absence means pure"
            );
            assert!(
                !HOST_EFFECTS[..idx]
                    .iter()
                    .any(|(other_module, other_name, _)| other_module == module
                        && other_name == name),
                "duplicate effect table entry {module}.{name}"
            );
        }
    }

    /// Pins the two halves of the split that is easiest to get wrong: reaching the
    /// host is an effect, and pure algebra over values already in hand is not.
    #[test]
    fn effects_distinguish_host_access_from_pure_algebra() {
        assert_eq!(
            lookup("vibra_v1", "fs_open_read").unwrap().effects(),
            &[("fs", "read")]
        );
        assert_eq!(
            lookup("vibra_v1", "fs_copy").unwrap().effects(),
            &[("fs", "read"), ("fs", "write")]
        );
        for pure in [
            "path_join",
            "bytes_len",
            "array_len",
            "str_trim",
            "clock_duration_from_millis",
            "net_address_parse",
        ] {
            assert!(
                lookup("vibra_v1", pure).unwrap().effects().is_empty(),
                "{pure} computes over values already in hand and must stay pure"
            );
        }
        for intrinsic in ["assert", "fail"] {
            assert!(
                lookup("vibra_test", intrinsic)
                    .unwrap()
                    .effects()
                    .is_empty(),
                "{intrinsic} is a harness intrinsic with no host authority"
            );
        }
    }

    #[test]
    fn registry_exposes_exact_value_and_return_shapes() {
        assert!(lookup("vibra_code", "parse").is_none());
        assert!(!is_host_module("vibra_code"));

        let import = lookup("vibra_v1", "random_bytes").unwrap();
        assert_eq!(import.params[0], ValueKind::UInt64);
        assert_eq!(import.result, ValueKind::Bytes);

        let open = lookup("vibra_v1", "fs_open_read").unwrap();
        assert_eq!(open.params[0], ValueKind::Path);
        assert_eq!(open.result, ValueKind::ResultReadHandle);

        let bounded = lookup("vibra_v1", "fd_read_bytes_up_to").unwrap();
        assert_eq!(bounded.params, &[READ_HANDLE, U64]);
        assert_eq!(bounded.result, ValueKind::ResultBytes);

        let rename = lookup("vibra_v1", "fs_rename").unwrap();
        assert_eq!(rename.params, &[PATH, PATH]);

        let copy = lookup("vibra_v1", "fs_copy").unwrap();
        assert_eq!(copy.params, &[PATH, PATH]);

        let monotonic = lookup("vibra_v1", "clock_monotonic_millis").unwrap();
        assert_eq!(monotonic.result, ValueKind::Instant);

        let list = lookup("vibra_v1", "env_list").unwrap();
        assert_eq!(list.result, ValueKind::Array);

        let info = lookup("vibra_v1", "system_info").unwrap();
        assert_eq!(info.result, ValueKind::Record);

        let connect = lookup("vibra_v1", "net_connect").unwrap();
        assert_eq!(connect.params[0], NET_ADDRESS);

        let bind = lookup("vibra_v1", "net_udp_bind").unwrap();
        assert_eq!(bind.params, &[NET_ADDRESS]);

        let accept = lookup("vibra_v1", "net_accept").unwrap();
        assert_eq!(accept.params, &[TCP_LISTENER]);
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
        let published = std::fs::read_to_string(path).expect("schemas/host-abi.json must exist");
        assert_eq!(
            published.replace("\r\n", "\n"),
            expected,
            "schemas/host-abi.json is out of sync with src/host_abi.rs; regenerate it with `VIBRA_REGEN_SCHEMAS=1 cargo test -p vibra host_abi`"
        );
    }
}

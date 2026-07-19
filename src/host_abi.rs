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

use crate::lower::CapabilityDomain;

/// Shape of one host-import parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKind {
    /// An ordinary data value (string, integer, handle, structural form, ...).
    Value(ValueKind),
    /// A capability position: must be fed by a `$policy`-typed wrapper
    /// argument whose declared domains cover every listed domain.
    Capability(&'static [CapabilityDomain]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    Void,
    Bool,
    Int64,
    UInt64,
    Float64,
    Str,
    Bytes,
    Path,
    ReadHandle,
    WriteHandle,
    AnyHandle,
    ResultVoid,
    ResultStr,
    ResultBytes,
    ResultReadHandle,
    ResultWriteHandle,
    ResultPaths,
    ResultPath,
    OptionPath,
    OptionStr,
    OptionAny,
    ResultAny,
    Any,
    Array,
    StringMap,
    CodeDocument,
    CodeQuery,
    CodeNode,
    CodeForm,
    CodeChangeSet,
    CodePath,
    CodePattern,
    CodeSegment,
    CodeForms,
    CodeDocumentResult,
    CodeNodeResult,
    CodeNodesResult,
    CodeMatchesResult,
    CodeFormResult,
}

/// One entry of the versioned host ABI.
#[derive(Debug, Clone, Copy)]
pub struct HostImport {
    pub module: &'static str,
    pub name: &'static str,
    pub params: &'static [ParamKind],
    pub result: ValueKind,
}

impl HostImport {
    /// The capability domains this import requires (union over capability
    /// parameters). Empty for pure/baseline imports.
    pub fn required_domains(&self) -> Vec<CapabilityDomain> {
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

impl ValueKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Void => "void",
            Self::Bool => "bool",
            Self::Int64 => "int64",
            Self::UInt64 => "uint64",
            Self::Float64 => "float64",
            Self::Str => "str",
            Self::Bytes => "bytes",
            Self::Path => "path",
            Self::ReadHandle => "read-handle",
            Self::WriteHandle => "write-handle",
            Self::AnyHandle => "any-handle",
            Self::ResultVoid => "result-void",
            Self::ResultStr => "result-str",
            Self::ResultBytes => "result-bytes",
            Self::ResultReadHandle => "result-read-handle",
            Self::ResultWriteHandle => "result-write-handle",
            Self::ResultPaths => "result-paths",
            Self::ResultPath => "result-path",
            Self::OptionPath => "option-path",
            Self::OptionStr => "option-str",
            Self::OptionAny => "option-any",
            Self::ResultAny => "result-any",
            Self::Any => "any",
            Self::Array => "array",
            Self::StringMap => "string-map",
            Self::CodeDocument => "code-document",
            Self::CodeQuery => "code-query",
            Self::CodeNode => "code-node",
            Self::CodeForm => "code-form",
            Self::CodeChangeSet => "code-change-set",
            Self::CodePath => "code-path",
            Self::CodePattern => "code-pattern",
            Self::CodeSegment => "code-segment",
            Self::CodeForms => "code-forms",
            Self::CodeDocumentResult => "code-document-result",
            Self::CodeNodeResult => "code-node-result",
            Self::CodeNodesResult => "code-nodes-result",
            Self::CodeMatchesResult => "code-matches-result",
            Self::CodeFormResult => "code-form-result",
        }
    }
}

const BOOL: ParamKind = ParamKind::Value(ValueKind::Bool);
const I64: ParamKind = ParamKind::Value(ValueKind::Int64);
const U64: ParamKind = ParamKind::Value(ValueKind::UInt64);
const F64: ParamKind = ParamKind::Value(ValueKind::Float64);
const STR: ParamKind = ParamKind::Value(ValueKind::Str);
const BYTES: ParamKind = ParamKind::Value(ValueKind::Bytes);
const PATH: ParamKind = ParamKind::Value(ValueKind::Path);
const READ_HANDLE: ParamKind = ParamKind::Value(ValueKind::ReadHandle);
const WRITE_HANDLE: ParamKind = ParamKind::Value(ValueKind::WriteHandle);
const ANY_HANDLE: ParamKind = ParamKind::Value(ValueKind::AnyHandle);

macro_rules! cap {
    ($($domain:ident),+) => {
        ParamKind::Capability(&[$(CapabilityDomain::$domain),+])
    };
}

/// The complete host ABI: every import a `$wasm` body may bind.
pub const HOST_ABI: &[HostImport] = &[
    // vibra_v1: standard streams and handle IO. Handle-consuming imports need
    // no capability parameter: the handle itself is the authority, minted by
    // a capability-checked open.
    entry(
        "vibra_v1",
        "stdin_open",
        &[cap!(StdinRead)],
        ValueKind::ReadHandle,
    ),
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
        &[ParamKind::Value(ValueKind::Array)],
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
        &[ParamKind::Value(ValueKind::Array), STR],
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
    // Pure, deterministic collection operations. `Any` is intentional: the
    // generic wrapper signatures are statically checked by Vibra lowering.
    entry(
        "vibra_v1",
        "array_len",
        &[ParamKind::Value(ValueKind::Array)],
        ValueKind::UInt64,
    ),
    entry(
        "vibra_v1",
        "array_get",
        &[ParamKind::Value(ValueKind::Array), U64],
        ValueKind::OptionAny,
    ),
    entry(
        "vibra_v1",
        "array_set",
        &[
            ParamKind::Value(ValueKind::Array),
            U64,
            ParamKind::Value(ValueKind::Any),
        ],
        ValueKind::ResultAny,
    ),
    entry(
        "vibra_v1",
        "array_slice",
        &[ParamKind::Value(ValueKind::Array), U64, U64],
        ValueKind::ResultAny,
    ),
    entry(
        "vibra_v1",
        "array_append",
        &[
            ParamKind::Value(ValueKind::Array),
            ParamKind::Value(ValueKind::Any),
        ],
        ValueKind::ResultAny,
    ),
    entry(
        "vibra_v1",
        "array_insert",
        &[
            ParamKind::Value(ValueKind::Array),
            U64,
            ParamKind::Value(ValueKind::Any),
        ],
        ValueKind::ResultAny,
    ),
    entry(
        "vibra_v1",
        "array_remove",
        &[ParamKind::Value(ValueKind::Array), U64],
        ValueKind::ResultAny,
    ),
    entry(
        "vibra_v1",
        "array_contains",
        &[
            ParamKind::Value(ValueKind::Array),
            ParamKind::Value(ValueKind::Any),
        ],
        ValueKind::Bool,
    ),
    entry(
        "vibra_v1",
        "map_len",
        &[ParamKind::Value(ValueKind::StringMap)],
        ValueKind::UInt64,
    ),
    entry(
        "vibra_v1",
        "map_get",
        &[ParamKind::Value(ValueKind::StringMap), STR],
        ValueKind::OptionAny,
    ),
    entry(
        "vibra_v1",
        "map_insert",
        &[
            ParamKind::Value(ValueKind::StringMap),
            STR,
            ParamKind::Value(ValueKind::Any),
        ],
        ValueKind::ResultAny,
    ),
    entry(
        "vibra_v1",
        "map_remove",
        &[ParamKind::Value(ValueKind::StringMap), STR],
        ValueKind::OptionAny,
    ),
    entry(
        "vibra_v1",
        "map_contains_key",
        &[ParamKind::Value(ValueKind::StringMap), STR],
        ValueKind::Bool,
    ),
    // vibra_v1: filesystem.
    entry(
        "vibra_v1",
        "fs_open_read",
        &[PATH, cap!(FsRead)],
        ValueKind::ResultReadHandle,
    ),
    entry(
        "vibra_v1",
        "fs_open_write",
        &[PATH, cap!(FsWrite)],
        ValueKind::ResultWriteHandle,
    ),
    entry(
        "vibra_v1",
        "fs_open_append",
        &[PATH, cap!(FsWrite)],
        ValueKind::ResultWriteHandle,
    ),
    entry(
        "vibra_v1",
        "fs_open_read_write",
        &[PATH, cap!(FsRead), cap!(FsWrite)],
        ValueKind::ResultReadHandle,
    ),
    entry(
        "vibra_v1",
        "fs_read_to_string",
        &[PATH, cap!(FsRead)],
        ValueKind::ResultStr,
    ),
    entry(
        "vibra_v1",
        "fs_write_string_all",
        &[PATH, STR, cap!(FsWrite)],
        ValueKind::ResultVoid,
    ),
    entry(
        "vibra_v1",
        "fs_append_string",
        &[PATH, STR, cap!(FsWrite)],
        ValueKind::ResultVoid,
    ),
    entry(
        "vibra_v1",
        "fs_exists",
        &[PATH, cap!(FsRead)],
        ValueKind::Bool,
    ),
    entry(
        "vibra_v1",
        "fs_create_dir_all",
        &[PATH, cap!(FsWrite)],
        ValueKind::ResultVoid,
    ),
    entry(
        "vibra_v1",
        "fs_remove_file",
        &[PATH, cap!(FsWrite)],
        ValueKind::ResultVoid,
    ),
    entry(
        "vibra_v1",
        "fs_remove_dir",
        &[PATH, cap!(FsWrite)],
        ValueKind::ResultVoid,
    ),
    entry(
        "vibra_v1",
        "fs_read_dir",
        &[PATH, cap!(FsRead)],
        ValueKind::ResultPaths,
    ),
    entry(
        "vibra_v1",
        "fs_metadata",
        &[PATH, cap!(FsRead)],
        ValueKind::ResultStr,
    ),
    entry(
        "vibra_v1",
        "fs_canonicalize",
        &[PATH, cap!(FsRead)],
        ValueKind::ResultPath,
    ),
    // vibra_v1: environment, network, process, clock, randomness, system.
    entry(
        "vibra_v1",
        "env_get",
        &[STR, cap!(EnvRead)],
        ValueKind::ResultStr,
    ),
    entry(
        "vibra_v1",
        "env_set",
        &[STR, STR, cap!(EnvWrite)],
        ValueKind::ResultVoid,
    ),
    entry(
        "vibra_v1",
        "net_connect",
        &[STR, cap!(NetConnect)],
        ValueKind::ResultVoid,
    ),
    entry(
        "vibra_v1",
        "net_listen",
        &[STR, cap!(NetListen)],
        ValueKind::ResultVoid,
    ),
    entry(
        "vibra_v1",
        "process_run",
        &[STR, cap!(ProcessRun)],
        ValueKind::ResultVoid,
    ),
    entry(
        "vibra_v1",
        "clock_now_unix_millis",
        &[cap!(Clock)],
        ValueKind::UInt64,
    ),
    entry(
        "vibra_v1",
        "random_bytes",
        &[U64, cap!(Random)],
        ValueKind::Bytes,
    ),
    entry(
        "vibra_v1",
        "system_info",
        &[cap!(SystemInfo)],
        ValueKind::Str,
    ),
    // vibra_test: in-memory assertions; capability-free.
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
    // vibra_code: structural source editing over in-memory documents;
    // capability-free.
    entry("vibra_code", "parse", &[STR], ValueKind::CodeDocumentResult),
    entry(
        "vibra_code",
        "make-query",
        &[CodeParam::PATH, BOOL, STR, STR, CodeParam::PATTERN, U64],
        ValueKind::CodeQuery,
    ),
    entry(
        "vibra_code",
        "capture-pattern",
        &[CodeParam::PATTERN, STR],
        ValueKind::CodePattern,
    ),
    entry("vibra_code", "emit", &[CodeParam::DOCUMENT], ValueKind::Str),
    entry(
        "vibra_code",
        "root",
        &[CodeParam::DOCUMENT],
        ValueKind::CodeNodeResult,
    ),
    entry(
        "vibra_code",
        "at",
        &[CodeParam::DOCUMENT, CodeParam::PATH],
        ValueKind::CodeNodeResult,
    ),
    entry(
        "vibra_code",
        "parent",
        &[CodeParam::NODE],
        ValueKind::CodeNodeResult,
    ),
    entry(
        "vibra_code",
        "children",
        &[CodeParam::NODE],
        ValueKind::CodeNodesResult,
    ),
    entry(
        "vibra_code",
        "find",
        &[CodeParam::DOCUMENT, CodeParam::QUERY],
        ValueKind::CodeMatchesResult,
    ),
    entry("vibra_code", "source", &[CodeParam::NODE], ValueKind::Str),
    entry(
        "vibra_code",
        "to-form",
        &[CodeParam::NODE],
        ValueKind::CodeFormResult,
    ),
    entry("vibra_code", "render", &[CodeParam::FORM], ValueKind::Str),
    entry(
        "vibra_code",
        "replace",
        &[CodeParam::DOCUMENT, CodeParam::NODE, CodeParam::FORM],
        ValueKind::CodeDocumentResult,
    ),
    entry(
        "vibra_code",
        "delete",
        &[CodeParam::DOCUMENT, CodeParam::NODE],
        ValueKind::CodeDocumentResult,
    ),
    entry(
        "vibra_code",
        "upsert-mapping",
        &[CodeParam::DOCUMENT, CodeParam::NODE, STR, CodeParam::FORM],
        ValueKind::CodeDocumentResult,
    ),
    entry(
        "vibra_code",
        "insert-mapping",
        &[CodeParam::DOCUMENT, CodeParam::NODE, STR, CodeParam::FORM],
        ValueKind::CodeDocumentResult,
    ),
    entry(
        "vibra_code",
        "rename-key",
        &[CodeParam::DOCUMENT, CodeParam::NODE, STR],
        ValueKind::CodeDocumentResult,
    ),
    entry(
        "vibra_code",
        "insert-sequence",
        &[CodeParam::DOCUMENT, CodeParam::NODE, U64, CodeParam::FORM],
        ValueKind::CodeDocumentResult,
    ),
    entry(
        "vibra_code",
        "splice-sequence",
        &[
            CodeParam::DOCUMENT,
            CodeParam::NODE,
            U64,
            U64,
            CodeParam::FORMS,
        ],
        ValueKind::CodeDocumentResult,
    ),
    entry(
        "vibra_code",
        "copy",
        &[
            CodeParam::DOCUMENT,
            CodeParam::NODE,
            CodeParam::NODE,
            CodeParam::SEGMENT,
        ],
        ValueKind::CodeDocumentResult,
    ),
    entry(
        "vibra_code",
        "move",
        &[
            CodeParam::DOCUMENT,
            CodeParam::NODE,
            CodeParam::NODE,
            CodeParam::SEGMENT,
        ],
        ValueKind::CodeDocumentResult,
    ),
];

struct CodeParam;
impl CodeParam {
    const DOCUMENT: ParamKind = ParamKind::Value(ValueKind::CodeDocument);
    const QUERY: ParamKind = ParamKind::Value(ValueKind::CodeQuery);
    const NODE: ParamKind = ParamKind::Value(ValueKind::CodeNode);
    const FORM: ParamKind = ParamKind::Value(ValueKind::CodeForm);
    const PATH: ParamKind = ParamKind::Value(ValueKind::CodePath);
    const PATTERN: ParamKind = ParamKind::Value(ValueKind::CodePattern);
    const SEGMENT: ParamKind = ParamKind::Value(ValueKind::CodeSegment);
    const FORMS: ParamKind = ParamKind::Value(ValueKind::CodeForms);
}

const fn entry(
    module: &'static str,
    name: &'static str,
    params: &'static [ParamKind],
    result: ValueKind,
) -> HostImport {
    HostImport {
        module,
        name,
        params,
        result,
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
                    ParamKind::Value(kind) => serde_json::json!({
                        "kind": "value",
                        "type": kind.as_str(),
                    }),
                    ParamKind::Capability(domains) => serde_json::json!({
                        "kind": "capability",
                        "domains": domains.iter().map(|domain| domain.as_str()).collect::<Vec<_>>(),
                    }),
                })
                .collect();
            serde_json::json!({
                "module": import.module,
                "name": import.name,
                "params": params,
                "return": import.result.as_str(),
                "required-domains": import.required_domains().iter().map(|domain| domain.as_str()).collect::<Vec<_>>(),
            })
        })
        .collect();
    serde_json::json!({
        "$id": "https://vibra-lang.org/schemas/host-abi.json",
        "description": "Versioned Vibra host ABI registry: every host import a `$wasm` body may bind, with capability requirements",
        "domains": CapabilityDomain::ALL.map(CapabilityDomain::as_str),
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
                    CapabilityDomain::ALL.contains(&domain),
                    "{}.{} requires unknown domain `{domain}`",
                    import.module,
                    import.name
                );
            }
        }
    }

    #[test]
    fn registry_exposes_exact_value_and_return_shapes() {
        let import = lookup("vibra_v1", "random_bytes").unwrap();
        assert_eq!(import.params[0], ParamKind::Value(ValueKind::UInt64));
        assert_eq!(import.result, ValueKind::Bytes);

        let open = lookup("vibra_v1", "fs_open_read").unwrap();
        assert_eq!(open.params[0], ParamKind::Value(ValueKind::Path));
        assert_eq!(open.result, ValueKind::ResultReadHandle);
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

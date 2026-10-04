//! Validation of a module under exactly the v1 feature baseline.
//!
//! `docs/spec/06-runtime.md`, "WebAssembly boundary", fixes what a v1 module
//! may be: the core instruction set and function tables, multiple results,
//! bulk memory, sign-extension operators, and non-trapping float-to-integer
//! conversions, and none of the optional proposals; imports from the native
//! import module only; one defined 32-bit memory exported as
//! `vibra_v1_memory` and no other export outside the versioned table; and no
//! custom section. The check is wasmparser's validator under a feature set
//! that names exactly the baseline, then those structural rules.

use std::fmt;

use vibra_ir::boundary::{FUNCTION_EXPORTS, MEMORY_EXPORT, NATIVE_IMPORT_MODULE};
use wasmparser::{
    ExternalKind, FuncValidatorAllocations, Parser, Payload, TypeRef, ValidPayload,
    Validator, WasmFeatures,
};

/// The feature set a v1 module validates under, and no more.
///
/// WebAssembly 1.0 with mutable globals, plus the four named extensions. The
/// tail-call, garbage-collection, exception, SIMD, relaxed SIMD, threads,
/// reference-type, memory64, multi-memory, extended-constant, and
/// custom-page-size features are all off.
#[must_use]
pub fn baseline_features() -> WasmFeatures {
    WasmFeatures::WASM1
        | WasmFeatures::MULTI_VALUE
        | WasmFeatures::BULK_MEMORY
        | WasmFeatures::SIGN_EXTENSION
        | WasmFeatures::SATURATING_FLOAT_TO_INT
}

/// Why a module is not a v1 module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValidationError {
    /// The binary is malformed or invalid under the baseline feature set,
    /// which includes a use of an instruction or type of another feature.
    Invalid(String),
    /// The module has a custom section, which a module emitted before
    /// `vibra build` exists must not.
    CustomSection(String),
    /// An import from a module other than the native import module, or an
    /// import that is not a function.
    ForeignImport {
        /// The import's module.
        module: String,
        /// The import's name.
        name: String,
    },
    /// An export outside the versioned table: a name that is not in it, or a
    /// name exported as the wrong kind, such as a memory under another name.
    ForeignExport {
        /// The export's name.
        name: String,
    },
    /// The module does not define exactly one memory, or does not export it
    /// exactly once as `vibra_v1_memory`.
    Memory(String),
}

impl fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => {
                write!(formatter, "not a v1 module: {message}")
            }
            Self::CustomSection(name) => {
                write!(
                    formatter,
                    "a v1 module has no custom section, found `{name}`"
                )
            }
            Self::ForeignImport { module, name } => write!(
                formatter,
                "import `{module}`.`{name}` is outside the `{NATIVE_IMPORT_MODULE}` functions"
            ),
            Self::ForeignExport { name } => {
                write!(formatter, "export `{name}` is outside the versioned table")
            }
            Self::Memory(message) => write!(formatter, "memory: {message}"),
        }
    }
}

impl std::error::Error for ValidationError {}

/// What a v1 module declares at its boundary.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModuleSummary {
    /// Each native import as its `(module, name)` pair, in module order. Every
    /// module is `vibra_native_v1`.
    pub imports: Vec<(String, String)>,
    /// Every export name, in module order, the memory included.
    pub exports: Vec<String>,
    /// The pages of the one memory the module defines at instantiation.
    pub initial_pages: u64,
    /// The functions the module defines, imports excluded.
    pub functions: usize,
    /// The passive and active data segments the module carries.
    pub data_segments: usize,
}

/// Validates `bytes` as a v1 module and reports what it declares.
///
/// # Errors
///
/// The first way the module departs from the baseline.
pub fn validate(bytes: &[u8]) -> Result<ModuleSummary, ValidationError> {
    let invalid = |error: wasmparser::BinaryReaderError| {
        ValidationError::Invalid(error.to_string())
    };
    let mut validator = Validator::new_with_features(baseline_features());
    let mut summary = ModuleSummary::default();
    let mut defined_memories = 0_usize;
    let mut memory_exports = 0_usize;
    for payload in Parser::new(0).parse_all(bytes) {
        let payload = payload.map_err(invalid)?;
        // A function body is validated when the payload hands it back.
        if let ValidPayload::Func(function, body) =
            validator.payload(&payload).map_err(invalid)?
        {
            function
                .into_validator(FuncValidatorAllocations::default())
                .validate(&body)
                .map_err(invalid)?;
        }
        match payload {
            Payload::CustomSection(section) => {
                return Err(ValidationError::CustomSection(section.name().to_owned()));
            }
            Payload::ImportSection(reader) => {
                for import in reader.into_imports() {
                    let import = import.map_err(invalid)?;
                    if import.module != NATIVE_IMPORT_MODULE
                        || !matches!(
                            import.ty,
                            TypeRef::Func(_) | TypeRef::FuncExact(_)
                        )
                    {
                        return Err(ValidationError::ForeignImport {
                            module: import.module.to_owned(),
                            name: import.name.to_owned(),
                        });
                    }
                    summary
                        .imports
                        .push((import.module.to_owned(), import.name.to_owned()));
                }
            }
            Payload::FunctionSection(reader) => {
                summary.functions =
                    usize::try_from(reader.count()).unwrap_or(usize::MAX);
            }
            Payload::DataSection(reader) => {
                summary.data_segments =
                    usize::try_from(reader.count()).unwrap_or(usize::MAX);
            }
            Payload::MemorySection(reader) => {
                for memory in reader {
                    let memory = memory.map_err(invalid)?;
                    defined_memories += 1;
                    summary.initial_pages = memory.initial;
                }
            }
            Payload::ExportSection(reader) => {
                for export in reader {
                    let export = export.map_err(invalid)?;
                    let allowed = match export.kind {
                        ExternalKind::Memory => export.name == MEMORY_EXPORT,
                        ExternalKind::Func => FUNCTION_EXPORTS.contains(&export.name),
                        _ => false,
                    };
                    if !allowed {
                        return Err(ValidationError::ForeignExport {
                            name: export.name.to_owned(),
                        });
                    }
                    if export.kind == ExternalKind::Memory {
                        memory_exports += 1;
                    }
                    summary.exports.push(export.name.to_owned());
                }
            }
            _ => {}
        }
    }
    if defined_memories != 1 {
        return Err(ValidationError::Memory(format!(
            "a v1 module defines exactly one memory, found {defined_memories}"
        )));
    }
    if memory_exports != 1 {
        return Err(ValidationError::Memory(format!(
            "the memory is exported exactly once as `{MEMORY_EXPORT}`, found {memory_exports} exports"
        )));
    }
    Ok(summary)
}

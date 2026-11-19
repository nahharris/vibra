//! Deterministic WebAssembly emission for checked Vibra IR.
//!
//! This is the second consumer of [`vibra_ir::CheckedProgram`], beside the
//! reference interpreter: it has no frontend of its own and lowers the same
//! checked IR the interpreter runs. It depends on `vibra-ir` and the encoder
//! only. It reaches no engine and no native code: a module names a native
//! routine only by an import from the pure module `vibra_native_v1`, whose
//! rows `vibra_ir::external::NATIVE_ROWS` fixes.
//!
//! # What is lowered
//!
//! The module is the transition module of `docs/spec/06-runtime.md`, "Wasm
//! transition": it runs the pure part of the language. Every module has a
//! memory layer: the value arena, precise reference counting with a worklist
//! release, the handle table, the instance state, and the accessors
//! ([`layout`] states the representation). A program lowers through its
//! literals, intrinsic rows, tuples, records, variants, wrappers, lists,
//! `match` and `let` patterns, `if`, sequences, `return`, `try`, calls of
//! functions, function values, and closures, tail calls, the rows of `Str`,
//! `Bytes`, `List`, and `Dict` that the native imports implement, and the
//! assertions of a test module. Each generic callable is specialized at the
//! type arguments it is used at. Activations live in the arena and a call does
//! not nest a WebAssembly call ([`layout`], "Activations and the dispatcher"),
//! so a tail call runs in constant space.
//!
//! The backend is fail-closed: a node the transition profile does not run is
//! [`EmitError::Unsupported`], naming the node and its origin, and no module
//! is produced, so a program is never partly executed.
//!
//! # The module
//!
//! A module defines one 32-bit linear memory exported as `vibra_v1_memory`,
//! one function table holding its lowered instances, imports only native
//! routines, exports `vibra_v1_eval` or `vibra_v1_test` and the accessors of
//! the chapter's closed table, and has no custom section. Emission is
//! deterministic: the bytes depend only on the checked program.

mod encode;
mod error;
pub mod layout;
mod lower;
mod primitive;
mod runtime;
pub mod support;
mod types;

use vibra_ir::{CheckedModuleSet, CheckedProgram, SourceOrigin, Type};

pub use error::EmitError;

use crate::lower::{Callable, Lowered, Lowering};
use crate::runtime::TestEntry;

/// The origin table the toolchain produces beside a module.
///
/// A module carries no source map, so a trap or a failed assertion names its
/// origin by an ordinal into this table, which maps each ordinal to one source
/// span (`docs/spec/06-runtime.md`, "Traps"). Ordinal `0` means no origin, so
/// the first entry has ordinal `1`. The table is not part of the module's
/// bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OriginTable {
    origins: Vec<(SourceOrigin, Option<Type>)>,
}

impl OriginTable {
    /// The number of origins the table maps.
    #[must_use]
    pub fn len(&self) -> usize {
        self.origins.len()
    }

    /// Whether the table maps no origin.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.origins.is_empty()
    }

    /// The origin an ordinal names, or `None` for ordinal `0` and for an
    /// ordinal outside the table.
    #[must_use]
    pub fn origin(&self, ordinal: u32) -> Option<&SourceOrigin> {
        self.row(ordinal).map(|(origin, _)| origin)
    }

    /// The closed type at which the host reads the operands of a failed
    /// `assert.equal` at an ordinal. `None` for every other origin.
    #[must_use]
    pub fn operand_type(&self, ordinal: u32) -> Option<&Type> {
        self.row(ordinal).and_then(|(_, ty)| ty.as_ref())
    }

    fn row(&self, ordinal: u32) -> Option<&(SourceOrigin, Option<Type>)> {
        let index = usize::try_from(ordinal).ok()?.checked_sub(1)?;
        self.origins.get(index)
    }
}

/// A module and the table that maps its origin ordinals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmittedModule {
    bytes: Vec<u8>,
    origins: OriginTable,
}

impl EmittedModule {
    /// The module's binary encoding.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The origin table produced beside the module.
    #[must_use]
    pub const fn origins(&self) -> &OriginTable {
        &self.origins
    }

    /// The module's binary encoding, by value.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

/// Lowers a checked program to an evaluation module, whose `vibra_v1_eval`
/// runs the entry and leaves its result.
///
/// The same program always yields the same bytes.
///
/// # Errors
///
/// [`EmitError::Unsupported`] for a node the transition profile does not run;
/// [`EmitError::ModuleSize`] and [`EmitError::Malformed`] for a program larger
/// than a module or one the checker could not have produced. No module is
/// produced then.
pub fn emit(program: &CheckedProgram) -> Result<EmittedModule, EmitError> {
    let set = program.module_set();
    let entry = program.entry();
    let function = set.functions().get(entry).ok_or(EmitError::Malformed {
        what: format!("no entry function {entry}"),
        origin: None,
    })?;
    let result = match function.signature.type_info() {
        Type::Fn(signature) => signature.result.clone(),
        Type::Proc(_) => return Err(EmitError::unsupported("proc", &function.origin)),
        other => {
            return Err(EmitError::malformed(
                format!("an entry of type {other}"),
                &function.origin,
            ));
        }
    };
    let class = types::Types::new(set).class(&result, &function.origin)?;
    let mut lowering = Lowering::new(set, false);
    let root =
        lowering.instance(Callable::Function(entry), Vec::new(), &function.origin)?;
    lowering.measure()?;
    let lowered = lowering.emit()?;
    let slots = lowered.slots.get(root as usize).copied().unwrap_or(0);
    assemble(
        lowered,
        encode::Entry::Program {
            function: root,
            slots,
            class,
        },
    )
}

/// Lowers tests of a checked module set to a test module.
///
/// `tests` are the function indices of the tests, in canonical discovery
/// order: the module's `vibra_v1_test` runs the test at a position of this
/// list, each in a fresh instance. A function that returns a value is not a
/// test, and running it is the trap `@runtime.invalidCheckedProgram`, as the
/// reference interpreter refuses it. The same input always yields the same
/// bytes.
///
/// # Errors
///
/// [`EmitError::Unsupported`] for a test with `takes` and for a node the
/// transition profile does not run; otherwise as for [`emit`].
pub fn emit_tests(
    set: &CheckedModuleSet,
    tests: &[usize],
) -> Result<EmittedModule, EmitError> {
    let mut lowering = Lowering::new(set, true);
    let mut roots = Vec::with_capacity(tests.len());
    for index in tests {
        let function = set.functions().get(*index).ok_or(EmitError::Malformed {
            what: format!("no test function {index}"),
            origin: None,
        })?;
        if !function.signature.binders().is_empty() {
            return Err(EmitError::malformed("a generic test", &function.origin));
        }
        let signature = match function.signature.type_info() {
            Type::Fn(signature) => signature,
            Type::Proc(_) => {
                return Err(EmitError::unsupported("proc", &function.origin));
            }
            other => {
                return Err(EmitError::malformed(
                    format!("a test of type {other}"),
                    &function.origin,
                ));
            }
        };
        if signature.slot_count() != 0 {
            return Err(EmitError::unsupported("testTakes", &function.origin));
        }
        roots.push(if signature.result == Type::Void {
            Some(lowering.instance(
                Callable::Function(*index),
                Vec::new(),
                &function.origin,
            )?)
        } else {
            None
        });
    }
    lowering.measure()?;
    let lowered = lowering.emit()?;
    let entries: Vec<TestEntry> = roots
        .into_iter()
        .map(|root| {
            root.map(|root| {
                (root, lowered.slots.get(root as usize).copied().unwrap_or(0))
            })
        })
        .collect();
    assemble(lowered, encode::Entry::Tests { entries })
}

/// Encodes the lowered instances under `entry`.
fn assemble(
    lowered: Lowered,
    entry: encode::Entry,
) -> Result<EmittedModule, EmitError> {
    let import_count =
        u32::try_from(lowered.imports.len()).map_err(|_| EmitError::ModuleSize)?;
    let instance_count =
        u32::try_from(lowered.functions.len()).map_err(|_| EmitError::ModuleSize)?;
    let end = import_count
        .checked_add(instance_count)
        .ok_or(EmitError::ModuleSize)?;
    let imports = lowered
        .imports
        .iter()
        .map(|intrinsic| {
            intrinsic
                .native_import()
                .ok_or_else(|| EmitError::Malformed {
                    what: format!(
                        "the row `{}` is not a native import",
                        intrinsic.symbol()
                    ),
                    origin: None,
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let parts = encode::Parts {
        functions: lowered.functions,
        // The table holds function indices, which the imports come before.
        table: (import_count..end).collect(),
        entry,
        segments: lowered.segments,
        imports,
        extra: lowered.extra,
    };
    let bytes = encode::module(&parts).ok_or(EmitError::ModuleSize)?;
    Ok(EmittedModule {
        bytes,
        origins: OriginTable {
            origins: lowered.origins,
        },
    })
}

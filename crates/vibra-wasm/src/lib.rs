//! Deterministic WebAssembly emission for checked Vibra IR.
//!
//! This is the second consumer of [`vibra_ir::CheckedProgram`], beside the
//! reference interpreter: it has no frontend of its own and lowers the same
//! typed IR the interpreter runs. It depends on `vibra-ir` and the encoder
//! only. It reaches no engine, and it does not depend on the native-code
//! crate: a module names a native only by an import from the pure module whose
//! name `vibra_ir::boundary` records.
//!
//! # Status
//!
//! Milestone 4 Step 4 is the skeleton. It lowers one program shape, the empty
//! `void` entry, which is what the differential harness needs to prove the
//! pipeline end to end. Every other form returns [`NotLowered`], naming each
//! form, and never a module that omits part of the program. Steps 5a onward
//! move forms out of [`NotLowered`] one step at a time.
//!
//! # The module
//!
//! A module defines one 32-bit linear memory exported as `vibra_v1_memory`,
//! imports nothing (the empty entry uses no native), exports the accessors of
//! `docs/spec/06-runtime.md`, "WebAssembly boundary", that do not depend on a
//! value kind or on tests, and has no custom section. Emission is
//! deterministic: the bytes depend only on the checked program.

mod classify;
mod encode;
mod form;

use vibra_ir::{CheckedProgram, SourceOrigin};

pub use form::{Form, NotLowered, UnloweredForm};

/// The origin table the toolchain produces beside a module.
///
/// A module carries no source map before Milestone 7, so a trap or a failed
/// assertion names its origin by an ordinal into this table, which maps each
/// ordinal to one source span (`docs/spec/06-runtime.md`, "Traps"). Ordinal `0`
/// means no origin, so the first entry has ordinal `1`. The table is not part
/// of the module's bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OriginTable {
    origins: Vec<SourceOrigin>,
}

impl OriginTable {
    /// The number of origins the table maps.
    #[must_use]
    pub fn len(&self) -> usize {
        self.origins.len()
    }

    /// Whether the table maps no origin. The skeleton's modules never trap or
    /// assert, so theirs is always empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.origins.is_empty()
    }

    /// The origin an ordinal names, or `None` for ordinal `0` and for an
    /// ordinal outside the table.
    #[must_use]
    pub fn origin(&self, ordinal: u32) -> Option<&SourceOrigin> {
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

/// Lowers a checked program to a v1 module.
///
/// The same program always yields the same bytes.
///
/// # Errors
///
/// [`NotLowered`] names every form the program uses that this step does not
/// lower. No module is produced for such a program.
pub fn emit(program: &CheckedProgram) -> Result<EmittedModule, NotLowered> {
    // A module indexes its functions with 32 bits. A count that does not fit is
    // reported, never wrapped.
    let sizes = u32::try_from(program.functions().len())
        .ok()
        .zip(u32::try_from(program.entry_index()).ok());
    let Some((functions, entry)) = sizes else {
        return Err(NotLowered::single(Form::ModuleSize));
    };
    if let Some(not_lowered) = NotLowered::from_uses(classify::unlowered(program)) {
        return Err(not_lowered);
    }
    Ok(EmittedModule {
        bytes: encode::module(functions, entry),
        origins: OriginTable::default(),
    })
}

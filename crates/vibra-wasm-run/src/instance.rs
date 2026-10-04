//! An instantiated module and the host's side of its accessors.
//!
//! The host reaches a value only through an ID the instance issued
//! (`docs/spec/06-runtime.md`, "The value arena"), and never an offset or an
//! address. A [`ValueId`] is that ID tagged with the instance that issued it, so
//! the host rejects another instance's ID itself, as the boundary requires of
//! the host; the module rejects a zero, a released, and a never-issued ID, an
//! index outside the value, and a kind the accessor does not admit, each by
//! recording `@runtime.invalid-host-value`. An accessor that stops returns the
//! [`Outcome`] the stop recorded, and the instance answers the next call.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

use vibra_ir::boundary::{
    ENTRY_EXPORT, LENGTH_EXPORT, LIVE_SIZE_EXPORT, READ_F32_EXPORT, READ_F64_EXPORT,
    READ_I32_EXPORT, READ_I64_EXPORT, READ_ID_EXPORT, RELEASE_EXPORT, TrapCode,
    VARIANT_EXPORT,
};
use wasmtime::{Instance as EngineInstance, Store};

use crate::{Host, Outcome, ResultSlot, RunnerError, export_error, interpret};

/// The result of `Runner::start`.
#[derive(Debug)]
pub enum Started {
    /// The module is instantiated and has not run.
    Ready(Instance),
    /// The module's own memory exceeds the runner's limit: the host event.
    MemoryExhausted,
}

/// A value ID, tagged with the instance that issued it.
///
/// The number is not part of the type's interface and does not appear in its
/// `Debug` output. A host holds an ID until it passes it to
/// [`Instance::release`].
#[derive(Clone, PartialEq, Eq)]
pub struct ValueId {
    instance: u64,
    id: u64,
}

impl ValueId {
    /// The ID's number, for a test of the rules the specification states about
    /// IDs: strictly increasing, never reused, never zero. No other code reads
    /// it, and it appears in no result, encoding, or snapshot.
    #[doc(hidden)]
    #[must_use]
    pub const fn number_for_tests(&self) -> u64 {
        self.id
    }
}

impl fmt::Debug for ValueId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ValueId(..)")
    }
}

/// The tags of instances, so two instances never share one.
static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);

/// An instantiated v1 module: its store, its engine instance, and the tag that
/// marks the IDs it issues.
pub struct Instance {
    store: Store<Host>,
    instance: EngineInstance,
    tag: u64,
}

impl fmt::Debug for Instance {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Instance").finish_non_exhaustive()
    }
}

impl Instance {
    pub(crate) fn new(store: Store<Host>, instance: EngineInstance) -> Self {
        Self {
            store,
            instance,
            tag: NEXT_INSTANCE.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// Calls `vibra_v1_entry` and reads what the module recorded.
    ///
    /// # Errors
    ///
    /// [`RunnerError::Export`] when the module lacks the export the protocol
    /// needs.
    pub fn call_entry(&mut self) -> Result<Outcome, RunnerError> {
        let entry = self
            .instance
            .get_typed_func::<(), ()>(&mut self.store, ENTRY_EXPORT)
            .map_err(|error| export_error(ENTRY_EXPORT, &error))?;
        let call = entry.call(&mut self.store, ());
        let stop = call.err().map(|error| format!("{error:#}"));
        interpret(&mut self.store, &self.instance, stop)
    }

    /// The ID an entry's result slot holds, for a result that is an arena
    /// value, which the host holds until it releases it. A slot that holds a
    /// scalar or `void` gives an ID the module rejects.
    #[must_use]
    pub const fn result_id(&self, slot: ResultSlot) -> ValueId {
        ValueId {
            instance: self.tag,
            id: slot.0,
        }
    }

    /// An ID this instance did not issue, tagged as its own, for a test of how
    /// the module validates an ID. `0` is invalid, and so is any number the
    /// instance has not handed out.
    #[must_use]
    pub const fn forge_id(&self, raw: u64) -> ValueId {
        ValueId {
            instance: self.tag,
            id: raw,
        }
    }

    /// The live arena size in bytes: allocated minus released, the handle
    /// table included.
    ///
    /// # Errors
    ///
    /// The [`Outcome`] of a stop, which `vibra_v1_live_size` does not raise.
    pub fn live_size(&mut self) -> Result<u64, Outcome> {
        let size: i64 = self.call(LIVE_SIZE_EXPORT, ())?;
        Ok(u64::from_ne_bytes(size.to_ne_bytes()))
    }

    /// The variant index of an enum or the member index of a union.
    ///
    /// # Errors
    ///
    /// `@runtime.invalid-host-value` for an ID the instance or the module
    /// rejects, or a kind with no variant.
    pub fn variant(&mut self, id: &ValueId) -> Result<u32, Outcome> {
        let raw = self.raw(id)?;
        let variant: i32 = self.call(VARIANT_EXPORT, (raw,))?;
        Ok(variant.cast_unsigned())
    }

    /// The scalar, byte, element, entry, or component count of a value.
    ///
    /// # Errors
    ///
    /// `@runtime.invalid-host-value` for an ID or kind it rejects.
    pub fn length(&mut self, id: &ValueId) -> Result<u64, Outcome> {
        let raw = self.raw(id)?;
        let length: i64 = self.call(LENGTH_EXPORT, (raw,))?;
        Ok(length.cast_unsigned())
    }

    /// An `i32`-slot component: a character, a byte, or a narrow integer.
    ///
    /// # Errors
    ///
    /// `@runtime.invalid-host-value` for an ID, index, or kind it rejects.
    pub fn read_i32(&mut self, id: &ValueId, index: u64) -> Result<i32, Outcome> {
        let raw = self.raw(id)?;
        self.call(READ_I32_EXPORT, (raw, index.cast_signed()))
    }

    /// An `i64` component.
    ///
    /// # Errors
    ///
    /// `@runtime.invalid-host-value` for an ID, index, or kind it rejects.
    pub fn read_i64(&mut self, id: &ValueId, index: u64) -> Result<i64, Outcome> {
        let raw = self.raw(id)?;
        self.call(READ_I64_EXPORT, (raw, index.cast_signed()))
    }

    /// An `f32` component.
    ///
    /// # Errors
    ///
    /// `@runtime.invalid-host-value` for an ID, index, or kind it rejects.
    pub fn read_f32(&mut self, id: &ValueId, index: u64) -> Result<f32, Outcome> {
        let raw = self.raw(id)?;
        self.call(READ_F32_EXPORT, (raw, index.cast_signed()))
    }

    /// An `f64` component.
    ///
    /// # Errors
    ///
    /// `@runtime.invalid-host-value` for an ID, index, or kind it rejects.
    pub fn read_f64(&mut self, id: &ValueId, index: u64) -> Result<f64, Outcome> {
        let raw = self.raw(id)?;
        self.call(READ_F64_EXPORT, (raw, index.cast_signed()))
    }

    /// A compound component, as a new ID the host releases.
    ///
    /// # Errors
    ///
    /// `@runtime.invalid-host-value` for an ID, index, or kind it rejects, and
    /// the memory host event when the instance has no ID or memory left.
    pub fn read_id(&mut self, id: &ValueId, index: u64) -> Result<ValueId, Outcome> {
        let raw = self.raw(id)?;
        let issued: i64 = self.call(READ_ID_EXPORT, (raw, index.cast_signed()))?;
        Ok(ValueId {
            instance: self.tag,
            id: issued.cast_unsigned(),
        })
    }

    /// Reads `length` bytes of the module's linear memory at `offset`, for the
    /// host tests of the layout (`vibra_wasm::layout`): the depth the frame
    /// stack reached, and which blocks a module-value table holds. Only
    /// toolchain-owned code reads the memory, and no result, encoding, or
    /// snapshot holds what it reads.
    ///
    /// # Errors
    ///
    /// A defect when the module exports no memory or the range is outside it.
    #[doc(hidden)]
    pub fn read_memory_for_tests(
        &mut self,
        offset: u32,
        length: usize,
    ) -> Result<Vec<u8>, Outcome> {
        let defect = |cause: &str| Outcome::Defect {
            cause: cause.to_owned(),
        };
        let memory = self
            .instance
            .get_memory(&mut self.store, vibra_ir::boundary::MEMORY_EXPORT)
            .ok_or_else(|| defect("the module exports no memory"))?;
        let mut bytes = vec![0_u8; length];
        memory
            .read(&self.store, offset as usize, &mut bytes)
            .map_err(|_| defect("the range is outside the memory"))?;
        Ok(bytes)
    }

    /// Drops the host's hold on an ID. The ID is invalid from then on.
    ///
    /// # Errors
    ///
    /// `@runtime.invalid-host-value` for an ID the instance or the module
    /// rejects.
    pub fn release(&mut self, id: ValueId) -> Result<(), Outcome> {
        let raw = self.raw(&id)?;
        self.call(RELEASE_EXPORT, (raw,))
    }

    /// The ID as the module's `i64`, after the host's own check that this
    /// instance issued it.
    fn raw(&self, id: &ValueId) -> Result<i64, Outcome> {
        if id.instance != self.tag {
            return Err(Outcome::Trapped {
                code: TrapCode::InvalidHostValue,
                origin: None,
            });
        }
        Ok(id.id.cast_signed())
    }

    /// Calls an export. A call that stops is read by the protocol.
    fn call<P, R>(&mut self, name: &'static str, params: P) -> Result<R, Outcome>
    where
        P: wasmtime::WasmParams,
        R: wasmtime::WasmResults,
    {
        let function = self
            .instance
            .get_typed_func::<P, R>(&mut self.store, name)
            .map_err(|error| Outcome::Defect {
                cause: export_error(name, &error).to_string(),
            })?;
        match function.call(&mut self.store, params) {
            Ok(value) => Ok(value),
            Err(error) => {
                let report = format!("{error:#}");
                match interpret(&mut self.store, &self.instance, Some(report)) {
                    Ok(stop) => Err(stop),
                    Err(error) => Err(Outcome::Defect {
                        cause: error.to_string(),
                    }),
                }
            }
        }
    }
}

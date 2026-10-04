//! The host-side canonical result observation.
//!
//! A run that completes leaves its result in the slot `vibra_v1_result`
//! reports. The host reads the value through the accessors, by the entry's
//! result type, and builds the [`ObservedValue`] whose canonical encoding the
//! reference interpreter produces for the same program, so the harness compares
//! the two backends byte for byte. No ID, offset, or index appears in the
//! value: the host holds an ID only while it reads, and releases it.
//!
//! The reader is a table over the result type, and a later step adds a row for
//! each kind it lowers. A type the reader has no row for is a defect of the
//! toolchain: the emitter lowered a program the host cannot observe.

use vibra_ir::{ObservedValue, Type, Value};

use crate::{Instance, Outcome, ResultSlot, Runner, RunnerError, Started, ValueId};

/// The live arena size in bytes at the three moments a test cares about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiveSizes {
    /// Before the entry ran.
    pub start: u64,
    /// After the entry completed, while the host still held its result.
    pub with_result: u64,
    /// After the host read the result and released it.
    pub end: u64,
}

/// A run, observed.
#[derive(Debug, PartialEq, Eq)]
pub enum Observed {
    /// The entry completed and its result was read.
    Completed {
        /// The result, ready for its canonical encoding.
        value: ObservedValue,
        /// The live sizes around the run.
        live: LiveSizes,
    },
    /// The run did not complete, or the result could not be read: the trap,
    /// host event, failed assertion, or defect, as [`Outcome`] reports it.
    /// Never [`Outcome::Completed`].
    Stopped(Outcome),
}

impl Runner {
    /// Validates and instantiates `bytes`, runs its entry, and reads the
    /// result as a value of type `result`.
    ///
    /// # Errors
    ///
    /// A [`RunnerError`] when the module is not a v1 module or lacks an export.
    pub fn run_observed(
        &self,
        bytes: &[u8],
        result: &Type,
    ) -> Result<Observed, RunnerError> {
        let mut instance = match self.start(bytes)? {
            Started::MemoryExhausted => {
                return Ok(Observed::Stopped(Outcome::MemoryExhausted));
            }
            Started::Ready(instance) => instance,
        };
        let start = match instance.live_size() {
            Ok(start) => start,
            Err(stop) => return Ok(Observed::Stopped(stop)),
        };
        let (slot, with_result) = match instance.call_entry()? {
            Outcome::Completed { result, live_size } => (result, live_size),
            stopped => return Ok(Observed::Stopped(stopped)),
        };
        let value = match instance.observe(slot, result) {
            Ok(value) => value,
            Err(stop) => return Ok(Observed::Stopped(stop)),
        };
        let end = match instance.live_size() {
            Ok(end) => end,
            Err(stop) => return Ok(Observed::Stopped(stop)),
        };
        Ok(Observed::Completed {
            value,
            live: LiveSizes {
                start,
                with_result,
                end,
            },
        })
    }
}

impl Instance {
    /// Reads the result a completed entry left in `slot` as a value of type
    /// `result`, and releases the ID the slot held when the result is an arena
    /// value.
    ///
    /// # Errors
    ///
    /// The [`Outcome`] of a stop while reading, or a defect when the slot does
    /// not fit the type or the host has no reader for the type.
    pub fn observe(
        &mut self,
        slot: ResultSlot,
        result: &Type,
    ) -> Result<ObservedValue, Outcome> {
        if let Some(value) = scalar(slot, result)? {
            return Ok(ObservedValue::Primitive(value));
        }
        let id = self.result_id(slot);
        let read = self.read_arena(result, &id);
        let released = self.release(id);
        let value = read?;
        released?;
        Ok(ObservedValue::Primitive(value))
    }

    /// Reads an arena value whose type has a row here.
    fn read_arena(&mut self, ty: &Type, id: &ValueId) -> Result<Value, Outcome> {
        match ty {
            Type::Bool => match self.variant(id)? {
                0 => Ok(Value::Bool(false)),
                1 => Ok(Value::Bool(true)),
                other => Err(defect(format!("a `bool` with the variant {other}"))),
            },
            Type::Str => Ok(Value::Str(self.characters(id)?)),
            Type::Atom | Type::AtomSingleton(_) => {
                Ok(Value::Atom(self.characters(id)?))
            }
            Type::Bytes => {
                let length = self.length(id)?;
                let mut bytes = Vec::new();
                for index in 0..length {
                    let byte = self.read_i32(id, index)?;
                    let byte = u8::try_from(byte).map_err(|_| {
                        defect(format!("a byte of {byte} in a `bytes` value"))
                    })?;
                    bytes.push(byte);
                }
                Ok(Value::Bytes(bytes))
            }
            other => Err(defect(format!(
                "the host has no reader for the type {other}"
            ))),
        }
    }

    /// The Unicode scalars of a `str` or an atom.
    fn characters(&mut self, id: &ValueId) -> Result<String, Outcome> {
        let length = self.length(id)?;
        let mut text = String::new();
        for index in 0..length {
            let scalar = self.read_i32(id, index)?.cast_unsigned();
            let character = char::from_u32(scalar).ok_or_else(|| {
                defect(format!("{scalar:#x} is not a Unicode scalar value"))
            })?;
            text.push(character);
        }
        Ok(text)
    }
}

fn defect(cause: String) -> Outcome {
    Outcome::Defect { cause }
}

/// The scalar a slot holds for a scalar type, `None` for a type whose result is
/// an arena value.
fn scalar(slot: ResultSlot, ty: &Type) -> Result<Option<Value>, Outcome> {
    let bits = slot.0;
    let mismatch = || defect(format!("the result slot does not fit the type {ty}"));
    // A type that crosses in an `i32` slot has no bit above the 32nd.
    let narrow = || u32::try_from(bits).map_err(|_| mismatch());
    Ok(Some(match ty {
        Type::Void => {
            if bits != 0 {
                return Err(mismatch());
            }
            Value::Void
        }
        Type::Char => Value::Char(char::from_u32(narrow()?).ok_or_else(mismatch)?),
        Type::I8 => {
            Value::I8(i8::try_from(narrow()?.cast_signed()).map_err(|_| mismatch())?)
        }
        Type::I16 => {
            Value::I16(i16::try_from(narrow()?.cast_signed()).map_err(|_| mismatch())?)
        }
        Type::I32 => Value::I32(narrow()?.cast_signed()),
        Type::U8 => Value::U8(u8::try_from(narrow()?).map_err(|_| mismatch())?),
        Type::U16 => Value::U16(u16::try_from(narrow()?).map_err(|_| mismatch())?),
        Type::U32 => Value::U32(narrow()?),
        Type::I64 => Value::I64(bits.cast_signed()),
        Type::U64 => Value::U64(bits),
        Type::F32 => Value::F32(narrow()?),
        Type::F64 => Value::F64(bits),
        _ => return Ok(None),
    }))
}

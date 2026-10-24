//! The imports of the host import module a runner supplies.
//!
//! `docs/spec/06-runtime.md`, "Host calls and the builders": a module imports
//! one function from `vibra_v1` for each host operation it applies, and the
//! import field is the registry symbol. Every parameter that is an arena value
//! crosses as an `i64` ID that the module keeps live for the call and the host
//! never releases; the result is one `i64` ID of an owned `(result t e)` value
//! that the host builds with the builders and seals, and that the module
//! adopts.
//!
//! The operations themselves are written once, in `vibra-host`, against the
//! value-access trait. This module is the runner's half of that trait: it reads
//! an argument through the exported accessors (`vibra_v1_length`,
//! `vibra_v1_read_i32`, `vibra_v1_read_id`) and builds a result through the
//! seven builders, and it never reads the instance's linear memory
//! (`vibra_v1_memory`), which only toolchain-owned native code may. A call that
//! completed appends its audit event, built by the same
//! `AuditEvent::completed` the reference interpreter calls.

use vibra_host::{
    AuditEvent, HostCall, HostEnum, HostRecord, HostValues, ReadError, build_reply,
    perform,
};
use vibra_ir::boundary::{
    HOST_IMPORT_MODULE, LENGTH_EXPORT, NEW_EXPORT, READ_I32_EXPORT, READ_ID_EXPORT,
    RELEASE_EXPORT, SEAL_EXPORT, WRITE_I32_EXPORT, WRITE_I64_EXPORT, WRITE_ID_EXPORT,
};
use vibra_ir::external::host::{HOST_ROWS, HostType};
use wasmtime::{Caller, Extern, FuncType, Linker, TypedFunc, Val, ValType};

use crate::Host;

/// The kind codes of `vibra_v1_new` (`docs/spec/06-runtime.md`, "Host calls and
/// the builders").
const KIND_STR: i32 = 1;
const KIND_BYTES: i32 = 2;
const KIND_ARRAY: i32 = 4;
const KIND_RECORD: i32 = 6;
const KIND_ENUM: i32 = 7;

/// A value of the runner's side of the interface: the ID of an arena value the
/// host holds, a scalar, or the `void` value.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Wv {
    Void,
    Id(i64),
    Scalar(i64),
}

/// Why a host call could not continue: the engine's error, which the call
/// returns to the module so the call stops. The module recorded the stop before
/// it raised the error.
type Failed = wasmtime::Error;

struct Exports {
    length: TypedFunc<i64, i64>,
    read_i32: TypedFunc<(i64, i64), i32>,
    read_id: TypedFunc<(i64, i64), i64>,
    release: TypedFunc<i64, ()>,
    new: TypedFunc<(i32, i64, i32), i64>,
    write_i32: TypedFunc<(i64, i64, i32), ()>,
    write_i64: TypedFunc<(i64, i64, i64), ()>,
    write_id: TypedFunc<(i64, i64, i64), ()>,
    seal: TypedFunc<i64, ()>,
}

/// The accessors and builders of one call, and the first stop one of them met.
struct Access<'c, 'd> {
    caller: &'c mut Caller<'d, Host>,
    exports: Exports,
    failed: Option<Failed>,
}

fn export(
    caller: &mut Caller<'_, Host>,
    name: &'static str,
) -> wasmtime::Result<wasmtime::Func> {
    caller
        .get_export(name)
        .and_then(Extern::into_func)
        .ok_or_else(|| wasmtime::Error::msg(format!("the module exports no `{name}`")))
}

fn typed<P, R>(
    caller: &mut Caller<'_, Host>,
    name: &'static str,
) -> wasmtime::Result<TypedFunc<P, R>>
where
    P: wasmtime::WasmParams,
    R: wasmtime::WasmResults,
{
    export(caller, name)?.typed(&*caller)
}

impl<'c, 'd> Access<'c, 'd> {
    fn new(caller: &'c mut Caller<'d, Host>) -> wasmtime::Result<Self> {
        let exports = Exports {
            length: typed(caller, LENGTH_EXPORT)?,
            read_i32: typed(caller, READ_I32_EXPORT)?,
            read_id: typed(caller, READ_ID_EXPORT)?,
            release: typed(caller, RELEASE_EXPORT)?,
            new: typed(caller, NEW_EXPORT)?,
            write_i32: typed(caller, WRITE_I32_EXPORT)?,
            write_i64: typed(caller, WRITE_I64_EXPORT)?,
            write_id: typed(caller, WRITE_ID_EXPORT)?,
            seal: typed(caller, SEAL_EXPORT)?,
        };
        Ok(Self {
            caller,
            exports,
            failed: None,
        })
    }

    /// Keeps the first stop and answers the default of the value.
    fn stop<T: Default>(&mut self, result: wasmtime::Result<T>) -> T {
        match result {
            Ok(value) => value,
            Err(error) => {
                self.failed.get_or_insert(error);
                T::default()
            }
        }
    }

    fn length(&mut self, id: i64) -> Result<u64, ReadError> {
        match self.exports.length.call(&mut *self.caller, id) {
            Ok(length) => Ok(length.cast_unsigned()),
            Err(error) => {
                self.failed.get_or_insert(error);
                Err(ReadError::Value)
            }
        }
    }

    fn read_i32(&mut self, id: i64, index: u64) -> Result<i32, ReadError> {
        match self
            .exports
            .read_i32
            .call(&mut *self.caller, (id, index.cast_signed()))
        {
            Ok(value) => Ok(value),
            Err(error) => {
                self.failed.get_or_insert(error);
                Err(ReadError::Value)
            }
        }
    }

    fn id_of(value: &Wv) -> Result<i64, ReadError> {
        match value {
            Wv::Id(id) => Ok(*id),
            Wv::Void | Wv::Scalar(_) => Err(ReadError::Value),
        }
    }

    fn release(&mut self, id: i64) {
        let result = self.exports.release.call(&mut *self.caller, id);
        self.stop(result);
    }

    fn new_value(&mut self, kind: i32, length: usize, variant: u32) -> i64 {
        let length = i64::try_from(length).unwrap_or(i64::MAX);
        let result = self
            .exports
            .new
            .call(&mut *self.caller, (kind, length, variant.cast_signed()));
        self.stop(result)
    }

    fn seal(&mut self, id: i64) -> Wv {
        let result = self.exports.seal.call(&mut *self.caller, id);
        self.stop(result);
        Wv::Id(id)
    }

    /// Writes `child` at `index`: an ID takes a reference in the parent and the
    /// host drops its own, and a scalar is written as an `i64`.
    fn write_child(&mut self, parent: i64, index: usize, child: Wv) {
        let index = i64::try_from(index).unwrap_or(i64::MAX);
        match child {
            Wv::Id(id) => {
                let result = self
                    .exports
                    .write_id
                    .call(&mut *self.caller, (parent, index, id));
                self.stop(result);
                self.release(id);
            }
            Wv::Scalar(bits) => {
                let result = self
                    .exports
                    .write_i64
                    .call(&mut *self.caller, (parent, index, bits));
                self.stop(result);
            }
            Wv::Void => {}
        }
    }

    fn build_compound(&mut self, kind: i32, variant: u32, children: Vec<Wv>) -> Wv {
        let id = self.new_value(kind, children.len(), variant);
        for (index, child) in children.into_iter().enumerate() {
            self.write_child(id, index, child);
        }
        self.seal(id)
    }
}

impl HostValues for Access<'_, '_> {
    type Value = Wv;

    fn read_text(&mut self, value: &Wv) -> Result<String, ReadError> {
        let id = Self::id_of(value)?;
        let length = self.length(id)?;
        let mut text = String::new();
        for index in 0..length {
            let scalar = self.read_i32(id, index)?;
            text.push(char::from_u32(scalar.cast_unsigned()).ok_or(ReadError::Value)?);
        }
        Ok(text)
    }

    fn read_bytes(&mut self, value: &Wv) -> Result<Vec<u8>, ReadError> {
        let id = Self::id_of(value)?;
        let length = self.length(id)?;
        let mut bytes = Vec::new();
        for index in 0..length {
            let byte = self.read_i32(id, index)?;
            bytes.push(u8::try_from(byte).map_err(|_| ReadError::Value)?);
        }
        Ok(bytes)
    }

    fn read_path(&mut self, value: &Wv) -> Result<String, ReadError> {
        // A `path` is a wrapper whose one component is its `str`.
        let id = Self::id_of(value)?;
        let inner = match self.exports.read_id.call(&mut *self.caller, (id, 0)) {
            Ok(inner) => inner,
            Err(error) => {
                self.failed.get_or_insert(error);
                return Err(ReadError::Value);
            }
        };
        let text = self.read_text(&Wv::Id(inner));
        self.release(inner);
        text
    }

    fn read_u64(&mut self, value: &Wv) -> Result<u64, ReadError> {
        match value {
            Wv::Scalar(bits) => Ok(bits.cast_unsigned()),
            Wv::Void | Wv::Id(_) => Err(ReadError::Value),
        }
    }

    fn build_void(&mut self) -> Wv {
        Wv::Void
    }

    fn build_text(&mut self, text: &str) -> Wv {
        let id = self.new_value(KIND_STR, text.chars().count(), 0);
        for (index, character) in text.chars().enumerate() {
            let result = self.exports.write_i32.call(
                &mut *self.caller,
                (
                    id,
                    i64::try_from(index).unwrap_or(i64::MAX),
                    u32::from(character).cast_signed(),
                ),
            );
            self.stop(result);
        }
        self.seal(id)
    }

    fn build_bytes(&mut self, bytes: &[u8]) -> Wv {
        let id = self.new_value(KIND_BYTES, bytes.len(), 0);
        for (index, byte) in bytes.iter().enumerate() {
            let result = self.exports.write_i32.call(
                &mut *self.caller,
                (
                    id,
                    i64::try_from(index).unwrap_or(i64::MAX),
                    i32::from(*byte),
                ),
            );
            self.stop(result);
        }
        self.seal(id)
    }

    fn build_i64(&mut self, value: i64) -> Wv {
        Wv::Scalar(value)
    }

    fn build_u64(&mut self, value: u64) -> Wv {
        Wv::Scalar(value.cast_signed())
    }

    fn build_array(&mut self, elements: Vec<Wv>) -> Wv {
        self.build_compound(KIND_ARRAY, 0, elements)
    }

    fn build_enum(&mut self, _ty: HostEnum, variant: u32, payload: Option<Wv>) -> Wv {
        self.build_compound(KIND_ENUM, variant, payload.into_iter().collect())
    }

    fn build_record(&mut self, _ty: HostRecord, fields: Vec<Wv>) -> Wv {
        self.build_compound(KIND_RECORD, 0, fields)
    }
}

/// Runs one host import: reads the call, performs it on the instance's
/// provider, records its event once it completed, and answers the sealed result.
fn run(
    symbol: &'static str,
    caller: &mut Caller<'_, Host>,
    params: &[Val],
    results: &mut [Val],
) -> wasmtime::Result<()> {
    let arguments = params
        .iter()
        .map(|param| match param {
            Val::I64(value) => Ok(Wv::Id(*value)),
            _ => Err(wasmtime::Error::msg(format!(
                "host operation `{symbol}` takes only `i64` slots"
            ))),
        })
        .collect::<wasmtime::Result<Vec<_>>>()?;
    // The row says which operands are IDs and which are scalars.
    let row = HOST_ROWS
        .iter()
        .find(|row| row.symbol == symbol)
        .ok_or_else(|| wasmtime::Error::msg(format!("`{symbol}` is not a row")))?;
    let arguments = arguments
        .into_iter()
        .zip(row.parameters)
        .map(|(value, parameter)| match (value, parameter.ty) {
            (Wv::Id(bits), HostType::I64 | HostType::U64) => Wv::Scalar(bits),
            (other, _) => other,
        })
        .collect::<Vec<_>>();
    let mut access = Access::new(caller)?;
    let call = match HostCall::read(symbol, &arguments, &mut access) {
        Ok(call) => call,
        Err(error) => {
            return Err(access.failed.take().unwrap_or_else(|| {
                wasmtime::Error::msg(format!("host operation `{symbol}`: {error}"))
            }));
        }
    };
    let reply = match perform(&call, access.caller.data_mut().provider.as_mut()) {
        Ok(reply) => reply,
        Err(_) => {
            // The provider could not allocate the answer: the host event.
            access.caller.data_mut().memory_event = true;
            return Err(wasmtime::Error::msg("the host exhausted its memory"));
        }
    };
    access
        .caller
        .data_mut()
        .audit
        .push(AuditEvent::completed(&call, &reply));
    let value = build_reply(&reply, &mut access);
    if let Some(error) = access.failed.take() {
        return Err(error);
    }
    match (value, results.first_mut()) {
        (Wv::Id(id), Some(slot)) => {
            *slot = Val::I64(id);
            Ok(())
        }
        _ => Err(wasmtime::Error::msg(format!(
            "host operation `{symbol}` did not build a value"
        ))),
    }
}

/// Defines the import of every row of the registry in `linker`.
///
/// # Errors
///
/// The engine's error when a definition conflicts with another.
pub(crate) fn define(linker: &mut Linker<Host>) -> wasmtime::Result<()> {
    for row in &HOST_ROWS {
        let symbol = row.symbol;
        let ty = FuncType::new(
            linker.engine(),
            row.parameters.iter().map(|_| ValType::I64),
            [ValType::I64],
        );
        linker.func_new(
            HOST_IMPORT_MODULE,
            symbol,
            ty,
            move |mut caller, params, results| {
                run(symbol, &mut caller, params, results)
            },
        )?;
    }
    Ok(())
}

//! The imports of the native import module a runner supplies.
//!
//! `docs/spec/06-runtime.md`, "Native implementations": a module reaches
//! toolchain-owned native code only through imports from the pure module
//! `vibra_native_v1`, and the runner supplies them. The closed list of names and
//! scalar signatures is the table `vibra_ir::external::NATIVE_ROWS`, which the
//! emitter reads too, and the code behind each name is `vibra-native`, the one
//! implementation that the reference interpreter calls directly. This is the one
//! place the runner adds a native: it defines each in the linker, against the
//! instance's own memory (`vibra_v1_memory`), and nothing else about running a
//! module changes.
//!
//! An import is not a host operation: it has no effect and no audit event, and
//! it reads and writes only the instance's memory. A native that refuses its
//! operands, an offset outside the memory, an object of another kind, or a
//! destination of the wrong size, stops the call without a recorded status,
//! which the runner reports as a defect of the toolchain and never as a read
//! outside the arena.

use vibra_ir::boundary::{MEMORY_EXPORT, NATIVE_IMPORT_MODULE};
use vibra_ir::external::{NATIVE_ROWS, NativeRow, Scalar};
use vibra_native::arena;
use wasmtime::{Caller, Extern, FuncType, Linker, Val, ValType};

use crate::Host;

fn val_type(scalar: Scalar) -> ValType {
    match scalar {
        Scalar::I32 => ValType::I32,
        Scalar::I64 => ValType::I64,
    }
}

/// Runs one import: reads its scalars, calls the native over the instance's
/// memory, and writes the answer.
fn run(
    row: &NativeRow,
    caller: &mut Caller<'_, Host>,
    params: &[Val],
    results: &mut [Val],
) -> wasmtime::Result<()> {
    let memory = caller
        .get_export(MEMORY_EXPORT)
        .and_then(Extern::into_memory)
        .ok_or_else(|| {
            wasmtime::Error::msg(format!(
                "native `{}`: the instance exports no `{MEMORY_EXPORT}`",
                row.name()
            ))
        })?;
    let scalars = params
        .iter()
        .map(|param| match param {
            Val::I32(value) => Ok(i64::from(*value)),
            Val::I64(value) => Ok(*value),
            _ => Err(wasmtime::Error::msg(format!(
                "native `{}` takes only integers",
                row.name()
            ))),
        })
        .collect::<wasmtime::Result<Vec<_>>>()?;
    let answer =
        arena::call(row, arena::memory(memory.data_mut(&mut *caller)), &scalars)
            .map_err(|fault| {
                wasmtime::Error::msg(format!("native `{}`: {fault}", row.name()))
            })?;
    let value = match row.result() {
        // An `ordering` is `-1`, `0`, or `1`.
        Scalar::I32 => Val::I32(i32::try_from(answer).unwrap_or(0)),
        Scalar::I64 => Val::I64(answer),
    };
    match results.first_mut() {
        Some(slot) => {
            *slot = value;
            Ok(())
        }
        None => Err(wasmtime::Error::msg(format!(
            "native `{}` has no result slot",
            row.name()
        ))),
    }
}

/// Defines every native import in `linker`.
///
/// # Errors
///
/// The engine's error when a definition conflicts with another.
pub(crate) fn define(linker: &mut Linker<Host>) -> wasmtime::Result<()> {
    for row in NATIVE_ROWS {
        let ty = FuncType::new(
            linker.engine(),
            row.params().into_iter().map(val_type),
            [val_type(row.result())],
        );
        linker.func_new(
            NATIVE_IMPORT_MODULE,
            row.name(),
            ty,
            move |mut caller, params, results| run(&row, &mut caller, params, results),
        )?;
    }
    Ok(())
}

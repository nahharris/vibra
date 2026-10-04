//! The imports of the native import module a runner supplies.
//!
//! `docs/spec/06-runtime.md`, "Native implementations": a module reaches
//! toolchain-owned native code only through imports from the pure module
//! `vibra_native_v1`, and the runner supplies them. The native-code crate
//! arrives in Step 8c and the closed table of names in Step 10; until then the
//! set is empty, and a module that imports anything is refused with the name
//! it asked for. This is the one place the runner adds a native: it defines
//! each in the linker, against the instance's own memory, and nothing else
//! about running a module changes.

use wasmtime::Linker;

use crate::Host;

/// Defines every native import in `linker`.
///
/// # Errors
///
/// The engine's error when a definition conflicts with another.
pub(crate) fn define(_linker: &mut Linker<Host>) -> wasmtime::Result<()> {
    Ok(())
}

//! WASI-first embedded runtime (wasmer + wasmer-wasix).

mod wasi_env;

pub mod fs;
pub mod io;

pub use wasi_env::{build_wasi_builder, RunConfig};

use anyhow::{Context, Result};
use wasmer::{Module, Store};
use wasmer_wasix::wasmer_wasix_types::wasi::ExitCode;
use wasmer_wasix::WasiRuntimeError;

/// Compile `wasm`, attach WASI imports, call exported `main` (`() -> ()`).
///
/// wasmer-wasix uses Tokio internally; when no runtime is active (e.g. unit tests), this spawns one.
pub fn run_module(wasm: &[u8], config: RunConfig) -> Result<()> {
    if tokio::runtime::Handle::try_current().is_ok() {
        run_module_in_current_runtime(wasm, config)
    } else {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .context("tokio runtime (required by WASI host)")?
            .block_on(async { run_module_in_current_runtime(wasm, config) })
    }
}

fn run_module_in_current_runtime(wasm: &[u8], config: RunConfig) -> Result<()> {
    let mut store = Store::default();
    let module = Module::new(&store, wasm).context("Wasmer: compile module")?;
    let builder = build_wasi_builder(config).map_err(wasi_creation_err)?;
    let (instance, wasi_env) = builder
        .instantiate(module, &mut store)
        .map_err(wasi_runtime_err)?;

    let rewind = unsafe { wasi_env.bootstrap(&mut store) }.map_err(wasi_runtime_err)?;
    if rewind.is_some() {
        anyhow::bail!(
            "vibra: WASI bootstrap requested a journal rewind; this runner does not support that"
        );
    }

    wasi_env.data(&store).thread.set_status_running();

    let main = instance
        .exports
        .get_function("main")
        .context("WASM must export `main`")?;
    main.call(&mut store, &[]).context("Wasmer: call `main`")?;

    wasi_env.on_exit(&mut store, Some(ExitCode::Other(0)));
    Ok(())
}

fn wasi_creation_err(e: wasmer_wasix::WasiStateCreationError) -> anyhow::Error {
    e.into()
}

fn wasi_runtime_err(e: WasiRuntimeError) -> anyhow::Error {
    match e {
        WasiRuntimeError::Init(err) => err.into(),
        other => anyhow::anyhow!("{other:#}"),
    }
}

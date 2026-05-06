//! Run generated WASM with Wasmer and a host `env.println` that reads guest memory.

use anyhow::Context;
use std::io::Write;
use std::sync::{Arc, Mutex};
use wasmer::{
    imports, AsStoreRef, Function, FunctionEnv, FunctionEnvMut, Instance, Memory, Module, Store,
};

#[derive(Clone)]
pub struct HostState {
    pub memory: Arc<Mutex<Option<Memory>>>,
}

fn println_host(env: FunctionEnvMut<HostState>, ptr: i32, len: i32) {
    let memory = {
        let guard = env.data().memory.lock().expect("host state lock");
        guard.clone()
    };
    let Some(mem) = memory else {
        eprintln!("vibra: internal error: println before memory export wired");
        return;
    };

    let Ok(len_u) = usize::try_from(len) else {
        return;
    };
    if len_u > 1 << 20 {
        eprintln!("vibra: println length too large");
        return;
    }

    let mut buf = vec![0u8; len_u];
    let store_ref = env.as_store_ref();
    let view = mem.view(&store_ref);
    if view.read(ptr as u64, &mut buf).is_err() {
        eprintln!("vibra: invalid memory read in println");
        return;
    }

    let s = String::from_utf8_lossy(&buf);
    print!("{s}");
    let _ = std::io::stdout().flush();
}

pub fn run_wasm(wasm: &[u8]) -> anyhow::Result<()> {
    let mut store = Store::default();
    let host = HostState {
        memory: Arc::new(Mutex::new(None)),
    };
    let env = FunctionEnv::new(&mut store, host.clone());
    let println_fn = Function::new_typed_with_env(&mut store, &env, println_host);

    let import_object = imports! {
        "env" => {
            "println" => println_fn
        }
    };

    let module = Module::new(&store, wasm).context("Wasmer: compile module")?;
    let instance = Instance::new(&mut store, &module, &import_object)
        .context("Wasmer: instantiate")?;

    let memory = instance
        .exports
        .get_memory("memory")
        .context("WASM must export `memory`")?;
    *host.memory.lock().expect("host state lock") = Some(memory.clone());

    let main = instance
        .exports
        .get_function("main")
        .context("WASM must export `main`")?;
    main.call(&mut store, &[]).context("Wasmer: call `main`")?;
    Ok(())
}

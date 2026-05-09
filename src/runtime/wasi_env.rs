//! Build [`WasiEnvBuilder`](wasmer_wasix::WasiEnvBuilder): stdio inheritance, argv, preopened dirs.

use std::path::PathBuf;
use wasmer_wasix::{WasiEnv, WasiEnvBuilder, WasiStateCreationError};

/// Configuration for [`super::run_module`](crate::runtime::run_module).
#[derive(Debug, Clone)]
pub struct RunConfig {
    /// `argv[0]`-style program name visible to WASI.
    pub program_name: String,
    /// Extra argv entries after `program_name` (MVP: often empty).
    pub argv: Vec<String>,
    /// Host directories preopened at the WASI virtual root (`/`).
    /// Deprecated compatibility field. These paths also seed read/write grants
    /// for the embedded interpreter until callers migrate to explicit grants.
    pub preopen_host_dirs: Vec<PathBuf>,
    /// Directories readable by grant-aware filesystem APIs.
    pub allow_read: Vec<PathBuf>,
    /// Directories writable by grant-aware filesystem APIs.
    pub allow_write: Vec<PathBuf>,
    /// Allow reading from stdin. Stdout/stderr writes remain baseline.
    pub allow_stdin: bool,
    pub allow_env: Vec<String>,
    pub allow_env_write: Vec<String>,
    pub allow_net: Vec<String>,
    pub allow_net_listen: Vec<String>,
    pub allow_run: Vec<String>,
    pub allow_clock: bool,
    pub allow_random: bool,
    pub allow_system_info: bool,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            program_name: "vibra".to_string(),
            argv: Vec::new(),
            preopen_host_dirs: Vec::new(),
            allow_read: Vec::new(),
            allow_write: Vec::new(),
            allow_stdin: false,
            allow_env: Vec::new(),
            allow_env_write: Vec::new(),
            allow_net: Vec::new(),
            allow_net_listen: Vec::new(),
            allow_run: Vec::new(),
            allow_clock: false,
            allow_random: false,
            allow_system_info: false,
        }
    }
}

/// Construct a [`WasiEnvBuilder`] from [`RunConfig`].
pub fn build_wasi_builder(config: RunConfig) -> Result<WasiEnvBuilder, WasiStateCreationError> {
    let mut builder = WasiEnv::builder(config.program_name);
    if !config.argv.is_empty() {
        builder.add_args(config.argv);
    }

    for d in config.preopen_host_dirs {
        builder = builder.preopen_dir(d)?;
    }

    Ok(builder)
}

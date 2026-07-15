//! Build [`WasiEnvBuilder`](wasmer_wasix::WasiEnvBuilder): stdio inheritance, argv, preopened dirs.

use crate::lower::{GrantRequirement, PolicyGroup, PolicyScope, PolicyType};
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
    pub approved_policy: Option<PolicyType>,
    /// Maximum byte length the runtime will allocate for a single
    /// program-controlled buffer (e.g. `read-raw`, `random.bytes`). Guards
    /// against user-controlled out-of-memory via unbounded length arguments.
    pub max_alloc_len: usize,
    /// Maximum number of concurrently open file handles the interpreter's
    /// `FileTable` may hold (excluding the reserved stdio entries). `0` means
    /// unlimited. Opening past this cap yields a matchable `too-many-open-files`
    /// filesystem error instead of exhausting OS file descriptors.
    pub max_open_files: usize,
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
            approved_policy: None,
            max_alloc_len: 64 * 1024 * 1024,
            max_open_files: 1024,
        }
    }
}

impl RunConfig {
    /// The approved policy for this run: the single source of runtime
    /// authority that `$policy`-typed root arguments are intersected against.
    ///
    /// Embedders may set [`RunConfig::approved_policy`] directly; otherwise it
    /// is derived from the CLI-facing `allow_*` fields (`--allow-read`,
    /// `--allow-env`, ...). `preopen_host_dirs` entries keep seeding read and
    /// write scopes for backward compatibility until callers migrate.
    pub fn effective_approved_policy(&self) -> PolicyType {
        if let Some(policy) = &self.approved_policy {
            return policy.clone();
        }
        let mut domains = std::collections::BTreeMap::new();
        let mut add = |domain: &str, scopes: Vec<PolicyScope>| {
            if scopes.is_empty() {
                return;
            }
            domains.insert(
                domain.to_string(),
                vec![PolicyGroup {
                    requirement: GrantRequirement::Optional,
                    scopes,
                }],
            );
        };
        let dir_scopes = |dirs: &[PathBuf]| -> Vec<PolicyScope> {
            dirs.iter()
                .map(|dir| PolicyScope::Dir(dir.display().to_string()))
                .collect()
        };
        let name_scopes = |names: &[String]| -> Vec<PolicyScope> {
            names
                .iter()
                .map(|name| {
                    if name == "*" {
                        PolicyScope::Any
                    } else {
                        PolicyScope::Exact(name.clone())
                    }
                })
                .collect()
        };
        let mut read_dirs = self.allow_read.clone();
        let mut write_dirs = self.allow_write.clone();
        read_dirs.extend(self.preopen_host_dirs.iter().cloned());
        write_dirs.extend(self.preopen_host_dirs.iter().cloned());
        add("fs-read", dir_scopes(&read_dirs));
        add("fs-write", dir_scopes(&write_dirs));
        if self.allow_stdin {
            add("stdin-read", vec![PolicyScope::Any]);
        }
        add("env-read", name_scopes(&self.allow_env));
        add("env-write", name_scopes(&self.allow_env_write));
        add("net-connect", name_scopes(&self.allow_net));
        add("net-listen", name_scopes(&self.allow_net_listen));
        add("process-run", name_scopes(&self.allow_run));
        if self.allow_clock {
            add("clock", vec![PolicyScope::Any]);
        }
        if self.allow_random {
            add("random", vec![PolicyScope::Any]);
        }
        if self.allow_system_info {
            add("system-info", vec![PolicyScope::Any]);
        }
        PolicyType { domains }
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

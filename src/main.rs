//! Vibra compiler CLI. See [DRAFT.md](../DRAFT.md).

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use vibra::{execute, load, lower, runtime};

#[derive(Parser)]
#[command(name = "vibra", version, about = "Vibra language toolchain")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse, compile (MVP), and run a `.vibra` module via embedded Wasmer.
    Run {
        /// Entry module path (e.g. examples/hello.vibra).
        path: PathBuf,
        /// Deprecated: seed both read and write filesystem grants.
        #[arg(long = "preopen")]
        preopen: Vec<PathBuf>,
        /// Allow filesystem reads under this host path (repeatable).
        #[arg(long = "allow-read")]
        allow_read: Vec<PathBuf>,
        /// Allow filesystem writes under this host path (repeatable).
        #[arg(long = "allow-write")]
        allow_write: Vec<PathBuf>,
        /// Allow reading from stdin.
        #[arg(long = "allow-stdin")]
        allow_stdin: bool,
        /// Allow reading the named environment variable (repeatable).
        #[arg(long = "allow-env")]
        allow_env: Vec<String>,
        /// Allow writing the named environment variable (repeatable).
        #[arg(long = "allow-env-write")]
        allow_env_write: Vec<String>,
        /// Allow outbound network access to HOST[:PORT] (repeatable).
        #[arg(long = "allow-net")]
        allow_net: Vec<String>,
        /// Allow listening on HOST[:PORT] (repeatable).
        #[arg(long = "allow-net-listen")]
        allow_net_listen: Vec<String>,
        /// Allow running the named command (repeatable).
        #[arg(long = "allow-run")]
        allow_run: Vec<String>,
        /// Allow clock/time access.
        #[arg(long = "allow-clock")]
        allow_clock: bool,
        /// Allow randomness access.
        #[arg(long = "allow-random")]
        allow_random: bool,
        /// Allow system information access.
        #[arg(long = "allow-sys-info")]
        allow_system_info: bool,
        /// Allow every modeled non-filesystem permission and filesystem access under the current directory.
        #[arg(long = "allow-all")]
        allow_all: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Run {
            path,
            preopen,
            allow_read,
            allow_write,
            allow_stdin,
            allow_env,
            allow_env_write,
            allow_net,
            allow_net_listen,
            allow_run,
            allow_clock,
            allow_random,
            allow_system_info,
            allow_all,
        } => {
            let program = load::load_program(&path)?;
            let lowered = lower::lower_program(&program)?;
            for warning in &lowered.warnings {
                eprintln!("warning: {warning}");
            }
            let config = runtime::RunConfig {
                preopen_host_dirs: preopen,
                allow_read: if allow_all {
                    vec![PathBuf::from(".")]
                } else {
                    allow_read
                },
                allow_write: if allow_all {
                    vec![PathBuf::from(".")]
                } else {
                    allow_write
                },
                allow_stdin: allow_all || allow_stdin,
                allow_env: if allow_all {
                    vec!["*".to_string()]
                } else {
                    allow_env
                },
                allow_env_write: if allow_all {
                    vec!["*".to_string()]
                } else {
                    allow_env_write
                },
                allow_net: if allow_all {
                    vec!["*".to_string()]
                } else {
                    allow_net
                },
                allow_net_listen: if allow_all {
                    vec!["*".to_string()]
                } else {
                    allow_net_listen
                },
                allow_run: if allow_all {
                    vec!["*".to_string()]
                } else {
                    allow_run
                },
                allow_clock: allow_all || allow_clock,
                allow_random: allow_all || allow_random,
                allow_system_info: allow_all || allow_system_info,
                ..runtime::RunConfig::default()
            };
            execute::run_lowered(&lowered, &config)?;
        }
    }
    Ok(())
}

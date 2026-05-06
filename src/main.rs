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
        /// Preopen host directory for filesystem operations (repeatable).
        #[arg(long = "preopen")]
        preopen: Vec<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Run { path, preopen } => {
            let program = load::load_program(&path)?;
            let lowered = lower::lower_program(&program)?;
            let config = runtime::RunConfig {
                preopen_host_dirs: preopen,
                ..runtime::RunConfig::default()
            };
            execute::run_lowered(&lowered, &config)?;
        }
    }
    Ok(())
}

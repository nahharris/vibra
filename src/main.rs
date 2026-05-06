//! Vibra compiler CLI. See [DRAFT.md](../DRAFT.md).

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use vibra::{emit, load, lower, run_wasmer};

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
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Run { path } => {
            let program = load::load_program(&path)?;
            let message = lower::extract_print_message(&program)?;
            let wasm = emit::emit_println_wasm(message.as_bytes());
            run_wasmer::run_wasm(&wasm)?;
        }
    }
    Ok(())
}

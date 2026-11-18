//! Process entry point for the `vibra` command.

use std::path::PathBuf;

fn main() {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    let current_directory =
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let code = vibra_cli::run(
        arguments,
        &current_directory,
        std::io::stdout(),
        std::io::stderr(),
    );
    std::process::exit(code);
}

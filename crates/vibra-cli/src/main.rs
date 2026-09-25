//! Process entry point for the `vibra` command.

fn main() {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    let code = vibra_cli::run(arguments, std::io::stdout(), std::io::stderr());
    std::process::exit(code);
}

mod cli;
mod cli_output;
mod doctor;

use clap::Parser;
use std::process::ExitCode;

use arnes::Roots;
use arnes::export;
use cli::{Cli, Command};

fn main() -> ExitCode {
    let Cli { command } = Cli::parse();

    match command {
        Command::Export { check, sources } => run_export(check, sources.as_deref()),
        Command::Doctor(args) => doctor::run(args),
    }
}

fn run_export(check: bool, sources: Option<&std::path::Path>) -> ExitCode {
    let result = Roots::from_environment()
        .map_err(|error| error.to_string())
        .and_then(|roots| export::run(&roots, check, sources).map_err(|error| error.to_string()));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = crate::cli_output::write_error(format_args!("export: {error}"));
            ExitCode::from(2)
        }
    }
}

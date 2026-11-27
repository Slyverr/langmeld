//! The `calc` program.

use std::process::ExitCode;

fn main() -> ExitCode {
    calc::cli::app().run()
}

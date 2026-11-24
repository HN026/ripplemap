mod cli;

use std::process::ExitCode;

fn main() -> ExitCode {
    match cli::run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::from(1)
        }
    }
}

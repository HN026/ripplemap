mod cli;

use std::process::ExitCode;

/**
 * Return the command outcome and send contextual execution errors to stderr.
 */
fn main() -> ExitCode {
    match cli::run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::from(1)
        }
    }
}

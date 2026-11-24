mod analyze;
mod api;
mod args;
mod dependencies;
mod ingest;
mod repositories;
mod snapshots;
mod usages;

use anyhow::Result;
use clap::Parser;
use ripplemap::store::SnapshotInput;

use args::{Cli, Command};

pub fn run() -> Result<std::process::ExitCode> {
    let cli = Cli::parse();
    match cli.command {
        Command::Analyze(args) => return analyze::run(&cli.database, args),
        Command::Runs { command } => analyze::runs(&cli.database, command),
        Command::Api { command } => api::run(&cli.database, command),
        Command::Repositories { command } => repositories::run(command),
        Command::Dependencies { command } => dependencies::run(&cli.database, command),
        Command::Ingest {
            scip,
            repository,
            commit,
            indexed_at,
        } => ingest::run(
            &cli.database,
            &scip,
            SnapshotInput {
                repository,
                commit,
                indexed_at,
            },
        ),
        Command::Usages {
            symbol,
            json,
            limit,
        } => usages::run(&cli.database, &symbol, json, limit),
        Command::Snapshots { json } => snapshots::run(&cli.database, json),
    }?;
    Ok(std::process::ExitCode::SUCCESS)
}

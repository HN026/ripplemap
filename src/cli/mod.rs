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

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
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
    }
}

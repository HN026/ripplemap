use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use ripplemap::store::{SnapshotInput, Store};

#[derive(Parser)]
#[command(
    version,
    about = "Inspect observed API references in pre-generated SCIP indexes"
)]
struct Cli {
    #[arg(long, global = true, default_value = "ripplemap.sqlite3")]
    database: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Atomically import or replace a repository/commit snapshot.
    Ingest {
        #[arg(long)]
        scip: PathBuf,
        #[arg(long)]
        repository: String,
        /// Full Git object ID (40 or 64 hex characters).
        #[arg(long)]
        commit: String,
        /// Successful index-generation time, RFC 3339 with time zone.
        #[arg(long)]
        indexed_at: String,
    },
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Ingest {
            scip,
            repository,
            commit,
            indexed_at,
        } => {
            let snapshot = Store::open(&cli.database)?.ingest(
                &scip,
                &SnapshotInput {
                    repository,
                    commit,
                    indexed_at,
                },
            )?;
            eprintln!(
                "Imported {:?} at {}: {} documents, {} definitions, {} references (SHA-256 {}).",
                snapshot.repository,
                snapshot.commit,
                snapshot.documents,
                snapshot.definitions,
                snapshot.references,
                snapshot.index_digest
            );
        }
    }
    Ok(())
}

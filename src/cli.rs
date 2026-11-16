use std::{io, path::PathBuf};

use anyhow::Result;
use clap::{Parser, Subcommand};
use ripplemap::{
    impact, report,
    store::{SnapshotInput, Store},
};

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
    /// Look up non-definition occurrences of an exact global SCIP symbol.
    Usages {
        #[arg(long)]
        symbol: String,
        #[arg(long)]
        json: bool,
        /// Maximum displayed references; totals always include every match.
        #[arg(long, default_value_t = impact::DEFAULT_LIMIT, value_parser = clap::value_parser!(u32).range(1..=i64::from(impact::MAX_LIMIT)))]
        limit: u32,
    },
    /// List stored repository/commit snapshots.
    Snapshots {
        #[arg(long)]
        json: bool,
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
        Command::Usages {
            symbol,
            json,
            limit,
        } => {
            let store = Store::open_read_only(&cli.database)?;
            let result = impact::usages(&store, &symbol, limit)?;
            report::usages(&mut io::stdout().lock(), &result, json)?;
        }
        Command::Snapshots { json } => {
            let snapshots = Store::open_read_only(&cli.database)?.snapshots()?;
            report::snapshots(&mut io::stdout().lock(), &snapshots, json)?;
        }
    }
    Ok(())
}

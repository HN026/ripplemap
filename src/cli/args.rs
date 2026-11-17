use std::path::PathBuf;

use clap::{Parser, Subcommand};
use ripplemap::impact;

use super::repositories;

#[derive(Parser)]
#[command(
    version,
    about = "Inspect observed API references in pre-generated SCIP indexes"
)]
pub(super) struct Cli {
    #[arg(long, global = true, default_value = "ripplemap.sqlite3")]
    pub(super) database: PathBuf,
    #[command(subcommand)]
    pub(super) command: Command,
}

#[derive(Subcommand)]
pub(super) enum Command {
    /// Manage the manifest of registered local repositories.
    Repositories {
        #[command(subcommand)]
        command: repositories::Command,
    },
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

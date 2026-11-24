use std::{
    io,
    path::{Path, PathBuf},
};

use anyhow::Result;
use clap::Subcommand;
use ripplemap::{dependencies, report, repository::RepositoryManifest, store::Store};

#[derive(Subcommand)]
pub(super) enum Command {
    /// Inspect committed npm evidence and attach it to an imported snapshot.
    Scan {
        #[arg(long, default_value = "ripplemap.json")]
        manifest: PathBuf,
        #[arg(long)]
        repository: String,
        #[arg(long)]
        commit: String,
        #[arg(long)]
        json: bool,
    },
    /// List recorded dependencies without rereading the checkout.
    List {
        #[arg(long)]
        repository: String,
        #[arg(long)]
        commit: String,
        #[arg(long)]
        json: bool,
    },
}

/**
 * Dispatch committed dependency inspection or read already saved evidence.
 */
pub(super) fn run(database: &Path, command: Command) -> Result<()> {
    let (result, json) = match command {
        Command::Scan {
            manifest,
            repository,
            commit,
            json,
        } => {
            let manifest = RepositoryManifest::load(&manifest)?;
            let repository = manifest.repository(&repository)?;
            let mut store = Store::open(database)?;
            (dependencies::scan(&mut store, repository, &commit)?, json)
        }
        Command::List {
            repository,
            commit,
            json,
        } => (
            Store::open_read_only(database)?.dependencies(&repository, &commit)?,
            json,
        ),
    };
    report::dependencies::write(&mut io::stdout().lock(), &result, json)
}

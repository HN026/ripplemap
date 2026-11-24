use std::{
    io,
    path::{Path, PathBuf},
};

use anyhow::Result;
use clap::Subcommand;
use ripplemap::{api, impact, report, repository::RepositoryManifest, store::Store};

#[derive(Subcommand)]
pub(super) enum Command {
    /// Verify declarations from an exact Git revision and bind public exports to SCIP definitions.
    Extract {
        #[arg(long, default_value = "ripplemap.json")]
        manifest: PathBuf,
        #[arg(long)]
        repository: String,
        #[arg(long)]
        commit: String,
        #[arg(long)]
        declarations_repository: String,
        #[arg(long)]
        source_entry: String,
        /// Trusted compiler-helper file; runs explicitly, never an indexer or project script.
        #[arg(long)]
        helper: PathBuf,
        #[arg(long, default_value = "node")]
        node: PathBuf,
        /// Atomically written only after extraction and validation succeed.
        #[arg(long)]
        output: PathBuf,
    },
    /// Map an exact consumer declaration symbol through an API artifact.
    Resolve {
        #[arg(long)]
        api: PathBuf,
        #[arg(long)]
        consumer_repository: String,
        #[arg(long)]
        consumer_commit: String,
        #[arg(long)]
        symbol: String,
        #[arg(long)]
        json: bool,
        #[arg(long, default_value_t = impact::DEFAULT_LIMIT, value_parser = clap::value_parser!(u32).range(1..=i64::from(impact::MAX_LIMIT)))]
        limit: u32,
    },
    /// Check one public contract identity across two explicitly supplied API artifacts.
    Bridge {
        #[arg(long)]
        base: PathBuf,
        #[arg(long)]
        head: PathBuf,
        #[arg(long)]
        export: String,
        #[arg(long)]
        json: bool,
    },
}

/**
 * Dispatch explicit API extraction, consumer resolution, or cross-version binding.
 */
pub(super) fn run(database: &Path, command: Command) -> Result<()> {
    let store = Store::open_read_only(database)?;
    match command {
        Command::Extract {
            manifest,
            repository,
            commit,
            declarations_repository,
            source_entry,
            helper,
            node,
            output,
        } => {
            let manifest = RepositoryManifest::load(&manifest)?;
            let repository = manifest.repository(&repository)?;
            let snapshot = api::extract(
                &store,
                repository,
                &commit,
                &declarations_repository,
                &source_entry,
                &node,
                &helper,
            )?;
            api::files::write(&output, &snapshot)?;
            eprintln!(
                "Wrote API evidence to {} ({} public export records).",
                output.display(),
                snapshot.mappings.len()
            );
        }
        Command::Resolve {
            api: path,
            consumer_repository,
            consumer_commit,
            symbol,
            json,
            limit,
        } => {
            let snapshot: api::ApiSnapshot = api::files::read(&path)?;
            let consumer = store.snapshot(&consumer_repository, &consumer_commit)?;
            let resolution = api::resolve(&store, &snapshot, &consumer, &symbol, limit)?;
            report::api::resolution(&mut io::stdout().lock(), &resolution, json)?;
        }
        Command::Bridge {
            base,
            head,
            export,
            json,
        } => {
            let base = api::files::read(&base)?;
            let head = api::files::read(&head)?;
            let bridge = api::bridge(&store, &base, &head, &export)?;
            report::api::bridge(&mut io::stdout().lock(), &bridge, json)?;
        }
    }
    Ok(())
}

use std::{
    io::{self, Write},
    path::PathBuf,
};

use anyhow::Result;
use clap::Subcommand;
use ripplemap::repository::RepositoryManifest;

#[derive(Subcommand)]
pub(super) enum Command {
    /// Register an explicit local Git checkout or workspace path. Does not index it.
    Register {
        #[arg(long, default_value = "ripplemap.json")]
        manifest: PathBuf,
        #[arg(long)]
        repository: String,
        #[arg(long)]
        path: PathBuf,
    },
    /// List repositories included in a repository manifest.
    List {
        #[arg(long, default_value = "ripplemap.json")]
        manifest: PathBuf,
        #[arg(long)]
        json: bool,
    },
}

pub(super) fn run(command: Command) -> Result<()> {
    match command {
        Command::Register {
            manifest,
            repository,
            path,
        } => {
            RepositoryManifest::register(&manifest, &repository, &path)?;
            eprintln!(
                "Registered {repository:?} in {}. No index was generated.",
                manifest.display()
            );
        }
        Command::List { manifest, json } => {
            let manifest = RepositoryManifest::load(&manifest)?;
            let mut out = io::stdout().lock();
            if json {
                serde_json::to_writer_pretty(&mut out, &manifest)?;
                writeln!(out)?;
            } else {
                for repository in manifest.repositories {
                    writeln!(
                        out,
                        "{} | {}",
                        serde_json::to_string(&repository.id)?,
                        serde_json::to_string(&repository.path)?
                    )?;
                }
            }
        }
    }
    Ok(())
}

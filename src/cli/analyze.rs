use anyhow::Result;
use clap::{Args, Subcommand};
use ripplemap::{
    analysis::{self, Options},
    api, impact, report,
    store::Store,
};
use std::{
    io,
    path::{Path, PathBuf},
    process::ExitCode,
};

#[derive(Args)]
pub(super) struct Analyze {
    #[arg(long)]
    base: PathBuf,
    #[arg(long)]
    head: PathBuf,
    /// Required analysis manifest: one explicit commit per expected consumer.
    #[arg(long)]
    consumers: PathBuf,
    #[arg(long)]
    json: bool,
    /// Global displayed-reference limit; counts always include all matches.
    #[arg(long, default_value_t = impact::DEFAULT_LIMIT, value_parser = clap::value_parser!(u32).range(1..=i64::from(impact::MAX_LIMIT)))]
    limit: u32,
    #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u32).range(0..=36500))]
    max_index_age_days: u32,
}

#[derive(Subcommand)]
pub(super) enum Runs {
    /// Display an immutable saved result, even if indexes have since changed.
    Show {
        #[arg(long, value_parser = clap::value_parser!(i64).range(1..))]
        id: i64,
        #[arg(long)]
        json: bool,
    },
    /// List the newest saved runs, up to the explicit limit.
    List {
        #[arg(long)]
        json: bool,
        #[arg(long, default_value_t = 100, value_parser = clap::value_parser!(u32).range(1..=1000))]
        limit: u32,
    },
}

/**
 * Load explicit analysis inputs, invoke the shared engine, and emit its outcome.
 */
pub(super) fn run(database: &Path, args: Analyze) -> Result<ExitCode> {
    let base = api::files::read(&args.base)?;
    let head = api::files::read(&args.head)?;
    let consumers = api::files::read(&args.consumers)?;
    let mut store = Store::open(database)?;
    let result = analysis::analyze(
        &mut store,
        base,
        head,
        consumers,
        Options {
            limit: args.limit,
            max_index_age_days: args.max_index_age_days,
        },
    )?;
    report::analysis::write(&mut io::stdout().lock(), &result, args.json)?;
    Ok(ExitCode::from(result.exit_code()))
}

/**
 * Dispatch saved-run listing or replay without rerunning an analysis.
 */
pub(super) fn runs(database: &Path, command: Runs) -> Result<()> {
    let store = Store::open_read_only(database)?;
    match command {
        Runs::Show { id, json } => {
            report::analysis::write(&mut io::stdout().lock(), &store.analysis_run(id)?, json)
        }
        Runs::List { json, limit } => {
            let runs = store.analysis_runs(limit)?;
            if json {
                serde_json::to_writer_pretty(&mut io::stdout().lock(), &runs)?;
                println!();
            } else {
                for run in runs {
                    println!(
                        "{} | {} | {} | {} | observed repositories {} | candidates {}",
                        run.id,
                        run.evaluated_at,
                        run.analysis_version,
                        serde_json::to_string(&run.package)?,
                        run.observed_repositories,
                        run.candidate_repositories
                    );
                }
            }
            Ok(())
        }
    }
}

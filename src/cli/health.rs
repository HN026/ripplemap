use anyhow::{Result, ensure};
use clap::{ArgGroup, Args};
use ripplemap::{
    analysis::{Consumer, ConsumerManifest},
    api,
    health::{self, Compiler, Options},
    report,
    repository::RepositoryManifest,
    store::Store,
};
use std::{
    io,
    path::{Path, PathBuf},
    process::ExitCode,
};

#[derive(Args)]
#[command(group(ArgGroup::new("target").required(true).args(["repository", "selection"])))]
pub(super) struct Health {
    #[arg(long, default_value = "ripplemap.json")]
    manifest: PathBuf,
    #[arg(long, requires = "commit", conflicts_with = "selection")]
    repository: Option<String>,
    #[arg(long, requires = "repository", conflicts_with = "selection")]
    commit: Option<String>,
    /// Check an explicit selection of registered repositories and commits (same format as analyze consumers).
    #[arg(long, conflicts_with_all = ["repository", "commit"])]
    selection: Option<PathBuf>,
    #[arg(long)]
    json: bool,
    #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u32).range(0..=36500))]
    max_index_age_days: u32,
    #[arg(long, default_value_t = 100, value_parser = clap::value_parser!(u32).range(1..=1000))]
    limit: u32,
    /// Explicitly authorize running this trusted TypeScript helper, with no install or project scripts.
    #[arg(long)]
    compiler_helper: Option<PathBuf>,
    #[arg(long, requires = "compiler_helper")]
    node: Option<PathBuf>,
    #[arg(long, requires = "compiler_helper", value_parser = clap::value_parser!(u32).range(1..=300))]
    compiler_timeout_seconds: Option<u32>,
}

/**
 * Validate the explicit repository selection and run the selected health profile.
 */
pub(super) fn run(database: &Path, args: Health) -> Result<ExitCode> {
    let manifest = RepositoryManifest::load(&args.manifest)?;
    let mut selected = if let Some(selection) = args.selection {
        let selection: ConsumerManifest = api::files::read(&selection)?;
        ensure!(
            selection.schema_version == 1,
            "unsupported selection version"
        );
        selection.consumers
    } else {
        vec![Consumer {
            repository: args.repository.unwrap(),
            commit: args.commit.unwrap(),
        }]
    };
    ensure!(
        !selected.is_empty() && selected.len() <= 256,
        "select 1..256 registered repositories"
    );
    selected.sort();
    ensure!(
        selected
            .windows(2)
            .all(|p| p[0].repository != p[1].repository),
        "select exactly one commit per repository"
    );
    let options = Options {
        max_index_age_days: args.max_index_age_days,
        limit: args.limit,
        compiler: args.compiler_helper.map(|helper| Compiler {
            node: args.node.unwrap_or_else(|| PathBuf::from("node")),
            helper,
            timeout_seconds: args.compiler_timeout_seconds.unwrap_or(60),
        }),
    };
    let store = Store::open_read_only(database)?;
    let mut reports = Vec::new();
    for selection in selected {
        reports.push(health::check(
            &store,
            manifest.repository(&selection.repository)?,
            &selection.commit,
            &options,
        )?);
    }
    let status = reports
        .iter()
        .map(|r| r.status)
        .max()
        .unwrap_or(health::Status::Unknown);
    report::health::write(&mut io::stdout().lock(), status, &reports, args.json)?;
    Ok(ExitCode::from(status.exit_code()))
}

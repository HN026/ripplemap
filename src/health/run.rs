use super::{Diagnostic, Options, Report, Status, compiler, consistency, versions};
use crate::{
    repository::{self, Repository, Revision},
    store::Store,
};
use anyhow::{Result, ensure};

/**
 * Coordinate a health check for one explicit recorded revision and finalize its
 * outcome.
 */
pub fn check(
    store: &Store,
    repository: &Repository,
    commit: &str,
    options: &Options,
) -> Result<Report> {
    validate_options(commit, options)?;
    let mut report = super::profile::report(repository, commit, options)?;
    evaluate(store, repository, options, &mut report)?;
    report.finish(options.limit);
    Ok(report)
}

/**
 * Run committed evidence checks and the optional compiler against matching inputs.
 */
fn evaluate(
    store: &Store,
    repository: &Repository,
    options: &Options,
    report: &mut Report,
) -> Result<()> {
    let Some(saved) = super::evidence::load(store, &repository.id, options, report)? else {
        return Ok(());
    };
    let snapshot = saved.snapshot.clone();
    let revision = match Revision::open(&repository.path, &report.commit) {
        Ok(revision) => revision,
        Err(error) => {
            report.diagnostics.push(Diagnostic::new(
                "evidence_consistency",
                "revision_unavailable",
                Status::Unknown,
                format!("{error:#}"),
            ));
            return Ok(());
        }
    };
    let Some(inspected) = inspect_dependencies(store, &revision, &saved, report) else {
        return Ok(());
    };
    if let Some(settings) = &options.compiler {
        compiler::check(repository, &revision, &inspected, settings, report);
    }
    if !store
        .snapshot(&repository.id, &report.commit)
        .is_ok_and(|s| s == snapshot)
        || !store
            .dependencies(&repository.id, &report.commit)
            .is_ok_and(|d| d == saved)
    {
        report.diagnostics.push(Diagnostic::new(
            "evidence_consistency",
            "evidence_changed_during_check",
            Status::Unknown,
            "Index/dependency evidence changed during validation; retry",
        ));
    }
    Ok(())
}

/**
 * Validate the exact revision and configured health bounds before checking evidence.
 */
fn validate_options(commit: &str, options: &Options) -> Result<()> {
    repository::validate_commit(commit)?;
    ensure!(
        (1..=1000).contains(&options.limit) && options.max_index_age_days <= 36500,
        "invalid health options"
    );
    if let Some(compiler) = &options.compiler {
        ensure!(
            (1..=300).contains(&compiler.timeout_seconds),
            "compiler timeout must be 1..300 seconds"
        );
    }
    Ok(())
}

/**
 * Reinspect committed dependencies and turn inspection failures into structured
 * diagnostics.
 */
fn inspect_dependencies(
    store: &Store,
    revision: &Revision,
    saved: &crate::dependencies::DependencyReport,
    report: &mut Report,
) -> Option<crate::dependencies::DependencyReport> {
    let current = match consistency::inspect(store, revision, saved) {
        Ok((current, diagnostics)) => {
            report.diagnostics.extend(diagnostics);
            current
        }
        Err(error) => {
            report.diagnostics.push(Diagnostic::new(
                "evidence_consistency",
                "inspection_failed",
                Status::CheckFailed,
                format!("{error:#}"),
            ));
            return None;
        }
    };
    report.files = current.files.clone();
    report.dependencies = versions::inspect(&current);
    Some(crate::dependencies::DependencyReport {
        snapshot: saved.snapshot.clone(),
        evidence: current,
    })
}

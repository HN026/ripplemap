use super::{Diagnostic, Options, Report, Status, compiler, consistency, versions};
use crate::{
    repository::{self, Repository, Revision},
    store::Store,
};
use anyhow::{Result, ensure};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub fn check(
    store: &Store,
    repository: &Repository,
    commit: &str,
    options: &Options,
) -> Result<Report> {
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
    let now = OffsetDateTime::now_utc();
    let mut report = Report {
        format_version: 1, repository: repository.id.clone(), commit: commit.to_ascii_lowercase(), evaluated_at: now.format(&Rfc3339)?,
        snapshot: None, profile: if options.compiler.is_some() { "typescript-v1" } else { "dependencies-v1" }.into(),
        enabled_checks: vec!["dependency_constraints".into(), "peer_compatibility".into(), "evidence_consistency".into(), "index_age".into()],
        disabled_checks: if options.compiler.is_none() { vec!["typescript_compiler".into(), "missing_exports".into()] } else { vec![] },
        max_index_age_days: options.max_index_age_days, status: Status::Unknown, diagnostics: vec![], dependencies: vec![], files: vec![], compiler: None,
        diagnostics_total: 0, truncated: false,
        scope_notes: vec![
            "Healthy applies only to the enabled checks, exact recorded commit, and documented profile; it is not complete API or runtime compatibility.".into(),
            "Dependency checks validate committed npm resolution evidence, not installed package contents. Unsupported dependency specs or missing evidence produce Unknown.".into(),
            "SCIP commit/time metadata is caller-attested. Consistency checks compare committed dependency files, indexed document existence, and exact referenced package versions; they do not authenticate indexed source contents or prove complete reference coverage.".into(),
            "The optional compiler checks the clean checkout and installed declarations, records their hashes, and does not install packages or execute project scripts. Installed package integrity is not established by matching name/version metadata.".into(),
        ],
    };
    if options.compiler.is_some() {
        report
            .enabled_checks
            .extend(["typescript_compiler".into(), "missing_exports".into()]);
    }
    evaluate(store, repository, options, &mut report)?;
    report.finish(options.limit);
    Ok(report)
}

fn evaluate(
    store: &Store,
    repository: &Repository,
    options: &Options,
    report: &mut Report,
) -> Result<()> {
    let snapshots = store.snapshots()?;
    let Some(snapshot) = snapshots
        .into_iter()
        .find(|s| s.repository == repository.id && s.commit == report.commit)
    else {
        report.diagnostics.push(Diagnostic::new(
            "evidence_consistency",
            "missing_snapshot",
            Status::Unknown,
            "No index for the explicitly requested repository/commit",
        ));
        return Ok(());
    };
    report.snapshot = Some(snapshot.clone());
    let evaluated_at = OffsetDateTime::parse(&report.evaluated_at, &Rfc3339)?;
    let indexed_at = OffsetDateTime::parse(&snapshot.indexed_at, &Rfc3339)?;
    let age = (evaluated_at - indexed_at).whole_seconds();
    report.diagnostics.push(Diagnostic::new(
        "index_age",
        "index_age",
        if indexed_at <= evaluated_at && age <= i64::from(options.max_index_age_days) * 86400 {
            Status::Healthy
        } else {
            Status::Unknown
        },
        format!(
            "Recorded index age: {age} seconds; maximum {} days (future timestamps are unknown)",
            options.max_index_age_days
        ),
    ));
    let saved = match store.dependencies(&repository.id, &report.commit) {
        Ok(saved) => saved,
        Err(error) => {
            let missing = matches!(
                error.downcast_ref::<rusqlite::Error>(),
                Some(rusqlite::Error::QueryReturnedNoRows)
            );
            report.diagnostics.push(Diagnostic::new(
                "evidence_consistency",
                "dependency_evidence_unavailable",
                if missing {
                    Status::Unknown
                } else {
                    Status::CheckFailed
                },
                format!("{error:#}"),
            ));
            return Ok(());
        }
    };
    if saved.snapshot != snapshot {
        report.diagnostics.push(Diagnostic::new(
            "evidence_consistency",
            "snapshot_mismatch",
            Status::Unknown,
            "Saved dependency evidence belongs to different index metadata; scan again",
        ));
        return Ok(());
    }
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
    let current = match consistency::inspect(store, &revision, &saved) {
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
            return Ok(());
        }
    };
    report.files = current.files.clone();
    report.dependencies = versions::inspect(&current);
    let inspected = crate::dependencies::DependencyReport {
        snapshot: snapshot.clone(),
        evidence: current,
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

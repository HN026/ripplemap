use super::{Diagnostic, Options, Report, Status};
use crate::{
    dependencies::DependencyReport,
    store::{Snapshot, Store},
};
use anyhow::Result;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

/**
 * Load the requested snapshot and matching saved dependencies, preserving missing
 * evidence.
 */
pub(super) fn load(
    store: &Store,
    repository: &str,
    options: &Options,
    report: &mut Report,
) -> Result<Option<DependencyReport>> {
    let snapshots = store.snapshots()?;
    let Some(snapshot) = snapshots
        .into_iter()
        .find(|s| s.repository == repository && s.commit == report.commit)
    else {
        report.diagnostics.push(Diagnostic::new(
            "evidence_consistency",
            "missing_snapshot",
            Status::Unknown,
            "No index for the explicitly requested repository/commit",
        ));
        return Ok(None);
    };
    report.snapshot = Some(snapshot.clone());
    check_index_age(&snapshot, options, report)?;
    let saved = match store.dependencies(repository, &report.commit) {
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
            return Ok(None);
        }
    };
    if saved.snapshot != snapshot {
        report.diagnostics.push(Diagnostic::new(
            "evidence_consistency",
            "snapshot_mismatch",
            Status::Unknown,
            "Saved dependency evidence belongs to different index metadata; scan again",
        ));
        return Ok(None);
    }
    Ok(Some(saved))
}

/**
 * Classify recorded index age without accepting future timestamps as current evidence.
 */
fn check_index_age(snapshot: &Snapshot, options: &Options, report: &mut Report) -> Result<()> {
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
    Ok(())
}

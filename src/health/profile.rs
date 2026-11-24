use super::{Options, Report, Status};
use crate::repository::Repository;
use anyhow::Result;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

/**
 * Initialize the named health profile, enabled checks, and explicit evidence
 * limitations.
 */
pub(super) fn report(repository: &Repository, commit: &str, options: &Options) -> Result<Report> {
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
    Ok(report)
}

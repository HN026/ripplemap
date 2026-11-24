use super::{ConsumerManifest, Options, Report, SavedRun, compare};
use crate::{
    api::{self, ApiSnapshot},
    impact,
    store::Store,
};
use anyhow::Result;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

/**
 * Coordinate validated API comparison, consumer lookup, and atomic run persistence.
 */
pub fn analyze(
    store: &mut Store,
    base: ApiSnapshot,
    head: ApiSnapshot,
    mut consumers: ConsumerManifest,
    options: Options,
) -> Result<SavedRun> {
    super::inputs::validate(&base, &head, &mut consumers, &options)?;
    api::validate(store, &base)?;
    api::validate(store, &head)?;
    let comparison = compare(&base, &head)?;
    let now = OffsetDateTime::now_utc();
    let coverage =
        super::coverage::collect(store, &base, &consumers, now, options.max_index_age_days)?;
    let (findings, observed_repositories, candidate_repositories) =
        super::lookup::findings(store, &comparison.changes, &coverage, options.limit)?;
    let report = Report {
        format_version: 1,
        analysis_version: "ripplemap-analysis-1".into(),
        evaluated_at: now.format(&Rfc3339)?,
        options,
        base,
        head,
        consumers,
        coverage,
        comparison,
        observed_repositories,
        candidate_repositories,
        truncated: findings.iter().any(|f| f.truncated),
        findings,
        scope_notes: vec![
            impact::COVERAGE_NOTE.into(),
            "Only removed named function exports and increased minimum positional argument counts are classified. Retained bindings are not proof of signature or behavior compatibility; other type/signature changes are unchecked.".into(),
            "Exact baseline declaration identities only. Other versions are counted separately and deferred, never silently combined. Ambiguous aliases produce candidate evidence only.".into(),
            "Freshness measures caller-supplied index age at evaluated_at against max_index_age_days; current source/dependency freshness is unverified. Consumer declarations and lockfiles are not inferred from references.".into(),
            "Document-local references cannot be attributed to package APIs. Local-reference totals cover all local symbols, not specifically this library; import tokens may be absent from global results.".into(),
        ],
    };
    store.save_analysis(report)
}

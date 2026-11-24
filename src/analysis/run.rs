use super::{ConsumerManifest, Finding, Options, Report, SavedRun, compare, coverage};
use crate::{
    api::{self, ApiSnapshot, Status},
    impact, repository,
    store::Store,
};
use anyhow::{Result, ensure};
use std::collections::BTreeSet;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub fn analyze(
    store: &mut Store,
    base: ApiSnapshot,
    head: ApiSnapshot,
    mut consumers: ConsumerManifest,
    options: Options,
) -> Result<SavedRun> {
    ensure!(
        (1..=impact::MAX_LIMIT).contains(&options.limit),
        "invalid display limit"
    );
    ensure!(
        options.max_index_age_days <= 36500,
        "index age threshold exceeds 36500 days"
    );
    ensure!(
        consumers.schema_version == 1 && consumers.consumers.len() <= 256,
        "consumer manifest requires version 1 and at most 256 repositories"
    );
    let mut ids = BTreeSet::new();
    for consumer in &mut consumers.consumers {
        repository::validate_id(&consumer.repository)?;
        repository::validate_commit(&consumer.commit)?;
        consumer.commit.make_ascii_lowercase();
        ensure!(
            ids.insert(consumer.repository.clone()),
            "select exactly one commit per consumer repository"
        );
        ensure!(
            ![
                &base.producer.repository,
                &base.declarations.repository,
                &head.producer.repository,
                &head.declarations.repository
            ]
            .contains(&&consumer.repository),
            "library index repositories cannot also be selected as consumers"
        );
    }
    consumers.consumers.sort();
    api::validate(store, &base)?;
    api::validate(store, &head)?;
    let comparison = compare(&base, &head)?;
    let now = OffsetDateTime::now_utc();
    let available = store.snapshots()?;
    let mut coverage = Vec::new();
    for consumer in &consumers.consumers {
        let snapshot = available
            .iter()
            .find(|s| s.repository == consumer.repository && s.commit == consumer.commit)
            .cloned();
        coverage.push(coverage::inspect(
            store,
            &base,
            consumer,
            snapshot,
            now,
            options.max_index_age_days,
        )?);
    }
    let mut remaining = options.limit as usize;
    let (mut observed, mut candidates) = (BTreeSet::new(), BTreeSet::new());
    let mut findings = Vec::new();
    for change in &comparison.changes {
        let ambiguous = change.reference_status != Status::Resolved;
        let mut finding = Finding {
            export: change.export.clone(),
            observed_repositories: 0,
            candidate_repositories: 0,
            observed_references: 0,
            candidate_references: 0,
            references: vec![],
            truncated: false,
        };
        let mut repositories = BTreeSet::new();
        for evidence in &coverage {
            let Some(snapshot) = &evidence.snapshot else {
                continue;
            };
            for symbol in &change.mapping.declaration_symbols {
                let usages =
                    impact::usages_in_snapshot(store, symbol, (remaining as u32).max(1), snapshot)?;
                if usages.total_references > 0 {
                    repositories.insert(snapshot.repository.clone());
                }
                if ambiguous {
                    finding.candidate_references += usages.total_references;
                } else {
                    finding.observed_references += usages.total_references;
                }
                let shown = usages
                    .references
                    .into_iter()
                    .take(remaining)
                    .collect::<Vec<_>>();
                remaining -= shown.len();
                finding.references.extend(shown);
            }
        }
        finding.truncated = finding.observed_references + finding.candidate_references
            > finding.references.len() as i64;
        if ambiguous {
            finding.candidate_repositories = repositories.len();
            candidates.extend(repositories);
        } else {
            finding.observed_repositories = repositories.len();
            observed.extend(repositories);
        }
        findings.push(finding);
    }
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
        observed_repositories: observed.len(),
        candidate_repositories: candidates.len(),
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

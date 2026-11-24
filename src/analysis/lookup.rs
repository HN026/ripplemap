use super::{Change, Coverage, Finding};
use crate::{api::Status, impact, store::Store};
use anyhow::Result;
use std::collections::BTreeSet;

/**
 * Aggregate distinct affected repositories while sharing one displayed-reference
 * budget.
 */
pub(super) fn findings(
    store: &Store,
    changes: &[Change],
    coverage: &[Coverage],
    limit: u32,
) -> Result<(Vec<Finding>, usize, usize)> {
    let mut remaining = limit as usize;
    let (mut observed, mut candidates) = (BTreeSet::new(), BTreeSet::new());
    let mut findings = Vec::new();
    for change in changes {
        let (mut finding, repositories) = one_change(store, change, coverage, &mut remaining)?;
        let ambiguous = change.reference_status != Status::Resolved;
        if ambiguous {
            finding.candidate_repositories = repositories.len();
            candidates.extend(repositories);
        } else {
            finding.observed_repositories = repositories.len();
            observed.extend(repositories);
        }
        findings.push(finding);
    }
    Ok((findings, observed.len(), candidates.len()))
}

/**
 * Collect exact baseline usages for one change, keeping ambiguous aliases as
 * candidates.
 */
fn one_change(
    store: &Store,
    change: &Change,
    coverage: &[Coverage],
    remaining: &mut usize,
) -> Result<(Finding, BTreeSet<String>)> {
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
    for evidence in coverage {
        let Some(snapshot) = &evidence.snapshot else {
            continue;
        };
        for symbol in &change.mapping.declaration_symbols {
            let usages =
                impact::usages_in_snapshot(store, symbol, (*remaining as u32).max(1), snapshot)?;
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
                .take(*remaining)
                .collect::<Vec<_>>();
            *remaining -= shown.len();
            finding.references.extend(shown);
        }
    }
    finding.truncated = finding.observed_references + finding.candidate_references
        > finding.references.len() as i64;
    Ok((finding, repositories))
}

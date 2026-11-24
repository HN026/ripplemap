use super::{Consumer, Coverage};
use crate::{
    api::ApiSnapshot,
    store::{Snapshot, Store},
};
use anyhow::Result;
use rusqlite::params;
use std::collections::BTreeSet;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub(super) fn inspect(
    store: &Store,
    api: &ApiSnapshot,
    requested: &Consumer,
    snapshot: Option<Snapshot>,
    now: OffsetDateTime,
    max_age_days: u32,
) -> Result<Coverage> {
    let mut coverage = Coverage {
        requested: requested.clone(),
        snapshot,
        freshness: "missing_index".into(),
        age_seconds: None,
        observed_package_versions: vec![],
        versions_truncated: false,
        other_version_references: 0,
        unmapped_baseline_references: 0,
        local_references: 0,
    };
    let Some(snapshot) = &coverage.snapshot else {
        return Ok(coverage);
    };
    let age = (now - OffsetDateTime::parse(&snapshot.indexed_at, &Rfc3339)?).whole_seconds();
    coverage.age_seconds = Some(age);
    coverage.freshness = if age < 0 {
        "future_timestamp"
    } else if age > i64::from(max_age_days) * 86400 {
        "stale"
    } else {
        "within_age_threshold"
    }
    .into();
    let known: BTreeSet<_> = api
        .mappings
        .iter()
        .flat_map(|m| &m.declaration_symbols)
        .collect();
    let mut statement = store.connection.prepare(
        "SELECT sym.symbol, sym.package_version, count(*) FROM symbol_references o
         JOIN symbols sym ON sym.id = o.symbol_id JOIN documents d ON d.id = o.document_id
         WHERE d.snapshot_id = ?1 AND sym.scope = '' AND sym.scheme = 'scip-typescript'
           AND sym.package_manager = 'npm' AND sym.package_name = ?2
         GROUP BY sym.symbol, sym.package_version ORDER BY sym.symbol",
    )?;
    let mut rows = statement.query(params![snapshot.id, api.evidence.package.name])?;
    let mut versions = BTreeSet::new();
    while let Some(row) = rows.next()? {
        let symbol: String = row.get(0)?;
        let version: Option<String> = row.get(1)?;
        let count: i64 = row.get(2)?;
        if version.as_deref() != Some(&api.evidence.package.version) {
            coverage.other_version_references += count;
        } else if !known.contains(&symbol) {
            coverage.unmapped_baseline_references += count;
        }
        let version = version.unwrap_or_else(|| "unknown".into());
        versions.insert(version);
        if versions.len() > 64 {
            versions.pop_last();
            coverage.versions_truncated = true;
        }
    }
    coverage.observed_package_versions = versions.into_iter().collect();
    coverage.local_references = store.connection.query_row(
        "SELECT count(*) FROM symbol_references o JOIN symbols sym ON sym.id = o.symbol_id
         JOIN documents d ON d.id = o.document_id WHERE d.snapshot_id = ?1 AND sym.scope != ''",
        [snapshot.id],
        |r| r.get(0),
    )?;
    Ok(coverage)
}

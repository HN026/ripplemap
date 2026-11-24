//! Exact occurrence lookup; no cross-version mapping or impact inference.

use anyhow::{Context, Result, ensure};
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::{scip, store::Store};

pub const DEFAULT_LIMIT: u32 = 100;
pub const MAX_LIMIT: u32 = 1000;
pub const COVERAGE_NOTE: &str = "Only observed static references in stored snapshots are reported. Zero references do not prove safety or complete consumer coverage.";

#[derive(Debug, Serialize, PartialEq)]
pub struct UsageReport {
    pub symbol: String,
    pub package: scip::Package,
    pub indexed_snapshots: i64,
    pub observed_repositories: i64,
    pub total_references: i64,
    pub references: Vec<Usage>,
    pub truncated: bool,
    pub coverage_note: &'static str,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Usage {
    pub snapshot_id: i64,
    pub repository: String,
    pub commit: String,
    pub indexed_at: String,
    pub index_digest: String,
    pub path: String,
    pub position_encoding: String,
    pub start: Position,
    pub end: Position,
    pub symbol_roles: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Position {
    pub line: u64,
    pub column: u64,
}

const MATCHES: &str = "FROM symbol_references o
    JOIN symbols sym ON sym.id = o.symbol_id
    JOIN documents d ON d.id = o.document_id
    JOIN snapshots s ON s.id = d.snapshot_id
    WHERE sym.symbol = ?1 AND sym.scope = '' AND (?2 IS NULL OR s.id = ?2)";

/**
 * Look up an exact global SCIP symbol across all stored indexed commits.
 */
pub fn usages(store: &Store, symbol: &str, limit: u32) -> Result<UsageReport> {
    lookup(store, symbol, limit, None)
}

/**
 * Restrict exact-symbol lookup to one explicitly supplied consumer snapshot.
 */
pub(crate) fn usages_in_snapshot(
    store: &Store,
    symbol: &str,
    limit: u32,
    snapshot: &crate::store::Snapshot,
) -> Result<UsageReport> {
    lookup(store, symbol, limit, Some(snapshot))
}

/**
 * Read matching counts and locations in one transaction so replacement cannot mix
 * states.
 */
fn lookup(
    store: &Store,
    symbol: &str,
    limit: u32,
    snapshot: Option<&crate::store::Snapshot>,
) -> Result<UsageReport> {
    ensure!(
        (1..=MAX_LIMIT).contains(&limit),
        "reference limit must be between 1 and {MAX_LIMIT}"
    );
    let package = scip::package(symbol)?.context(
        "local symbols are document-scoped; usages requires an exact global SCIP symbol",
    )?;
    let tx = store.connection.unchecked_transaction()?;
    if let Some(expected) = snapshot {
        let actual = store.snapshot(&expected.repository, &expected.commit)?;
        ensure!(
            actual == *expected,
            "consumer snapshot changed during lookup"
        );
    }
    let snapshot = snapshot.map(|s| s.id);
    let indexed_snapshots = tx.query_row(
        "SELECT count(*) FROM snapshots WHERE ?1 IS NULL OR id = ?1",
        [snapshot],
        |row| row.get(0),
    )?;
    let (total_references, observed_repositories): (i64, i64) = tx.query_row(
        &format!("SELECT count(*), count(DISTINCT s.repository) {MATCHES}"),
        params![symbol, snapshot],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let mut statement = tx.prepare(&format!(
        "SELECT s.id, s.repository, s.commit_id, s.indexed_at, s.index_digest, d.path,
         d.position_encoding, o.start_line, o.start_character, o.end_line, o.end_character, o.symbol_roles
         {MATCHES}
         ORDER BY s.repository, s.commit_id, d.path, o.start_line, o.start_character,
                  o.end_line, o.end_character, o.symbol_roles LIMIT ?3"
    ))?;
    let mut rows = statement.query(params![symbol, snapshot, limit])?;
    let mut references = Vec::new();
    while let Some(row) = rows.next()? {
        references.push(usage_row(row)?);
    }
    Ok(UsageReport {
        symbol: symbol.into(),
        package,
        indexed_snapshots,
        observed_repositories,
        total_references,
        truncated: total_references > references.len() as i64,
        references,
        coverage_note: COVERAGE_NOTE,
    })
}

/**
 * Decode a stored occurrence and convert zero-based positions to displayed
 * coordinates.
 */
fn usage_row(row: &rusqlite::Row<'_>) -> Result<Usage> {
    let encoding: i32 = row.get(6)?;
    let position_encoding = match encoding {
        1 => "utf-8",
        2 => "utf-16",
        3 => "utf-32",
        _ => anyhow::bail!("unsupported stored position encoding {encoding}"),
    };
    Ok(Usage {
        snapshot_id: row.get(0)?,
        repository: row.get(1)?,
        commit: row.get(2)?,
        indexed_at: row.get(3)?,
        index_digest: row.get(4)?,
        path: row.get(5)?,
        position_encoding: position_encoding.into(),
        start: Position {
            line: u64::from(row.get::<_, u32>(7)?) + 1,
            column: u64::from(row.get::<_, u32>(8)?) + 1,
        },
        end: Position {
            line: u64::from(row.get::<_, u32>(9)?) + 1,
            column: u64::from(row.get::<_, u32>(10)?) + 1,
        },
        symbol_roles: row.get(11)?,
    })
}

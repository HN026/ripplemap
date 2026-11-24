use anyhow::{Context, Result, ensure};
use rusqlite::{TransactionBehavior, params};

use super::Store;
use crate::{dependencies::DependencyReport, repository::validate_commit};

impl Store {
    /**
     * Replace saved dependency evidence only while its indexed snapshot still matches.
     */
    pub(crate) fn save_dependencies(&mut self, report: &DependencyReport) -> Result<()> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = Self::select_snapshot(&tx, report.snapshot.id)?;
        ensure!(
            current == report.snapshot,
            "snapshot changed while dependency inspection was running; scan again"
        );
        tx.execute(
            "INSERT INTO dependency_evidence(snapshot_id, report_json) VALUES (?1, ?2)
             ON CONFLICT(snapshot_id) DO UPDATE SET report_json = excluded.report_json",
            params![report.snapshot.id, serde_json::to_string(report)?],
        )?;
        tx.commit().context("save dependency evidence")?;
        Ok(())
    }

    /**
     * Read dependency evidence for one explicitly selected repository and full commit.
     */
    pub fn dependencies(&self, repository: &str, commit: &str) -> Result<DependencyReport> {
        validate_commit(commit)?;
        let json: String = self.connection.query_row(
            "SELECT e.report_json FROM dependency_evidence e
             JOIN snapshots s ON s.id = e.snapshot_id
             WHERE s.repository = ?1 AND s.commit_id = ?2",
            params![repository, commit.to_ascii_lowercase()], |row| row.get(0),
        ).with_context(|| format!("no dependency evidence for {repository:?} at {commit}; ingest the snapshot and run dependencies scan"))?;
        serde_json::from_str(&json).context("decode stored dependency evidence")
    }
}

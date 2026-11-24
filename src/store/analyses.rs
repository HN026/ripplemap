use super::{Snapshot, Store};
use crate::analysis::{Report, SavedRun};
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::Serialize;

#[derive(Serialize)]
pub struct RunSummary {
    pub id: i64,
    pub evaluated_at: String,
    pub analysis_version: String,
    pub package: String,
    pub observed_repositories: i64,
    pub candidate_repositories: i64,
}

impl Store {
    pub(crate) fn save_analysis(&mut self, report: Report) -> Result<SavedRun> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        for snapshot in [
            &report.base.producer,
            &report.base.declarations,
            &report.head.producer,
            &report.head.declarations,
        ]
        .into_iter()
        .chain(report.coverage.iter().filter_map(|c| c.snapshot.as_ref()))
        {
            let actual = tx
                .query_row(
                    "SELECT id FROM snapshots WHERE repository = ?1 AND commit_id = ?2",
                    params![snapshot.repository, snapshot.commit],
                    |r| r.get::<_, i64>(0),
                )
                .optional()?;
            let mut expected: Snapshot = snapshot.clone();
            expected.id = actual.context("input snapshot disappeared during analysis; retry")?;
            ensure!(
                Self::select_snapshot(&tx, expected.id)? == expected,
                "input snapshot changed during analysis; retry"
            );
        }
        for missing in report.coverage.iter().filter(|c| c.snapshot.is_none()) {
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM snapshots WHERE repository = ?1 AND commit_id = ?2)",
                params![missing.requested.repository, missing.requested.commit],
                |r| r.get(0),
            )?;
            ensure!(
                !exists,
                "a missing consumer index arrived during analysis; retry"
            );
        }
        tx.execute(
            "INSERT INTO analysis_runs(report_json) VALUES (?1)",
            [serde_json::to_string(&report)?],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit().context("persist analysis run")?;
        Ok(SavedRun { id, report })
    }

    pub fn analysis_run(&self, id: i64) -> Result<SavedRun> {
        ensure!(id > 0, "run ID must be positive");
        let json: String = self
            .connection
            .query_row(
                "SELECT report_json FROM analysis_runs WHERE id = ?1",
                [id],
                |r| r.get(0),
            )
            .with_context(|| format!("missing analysis run {id}"))?;
        Ok(SavedRun {
            id,
            report: serde_json::from_str(&json).context("decode saved analysis run")?,
        })
    }

    pub fn analysis_runs(&self, limit: u32) -> Result<Vec<RunSummary>> {
        ensure!(
            (1..=1000).contains(&limit),
            "run list limit must be 1..1000"
        );
        let mut statement = self.connection.prepare(
            "SELECT id, json_extract(report_json, '$.evaluated_at'),
             json_extract(report_json, '$.analysis_version'),
             json_extract(report_json, '$.base.evidence.package.name'),
             json_extract(report_json, '$.observed_repositories'),
             json_extract(report_json, '$.candidate_repositories')
             FROM analysis_runs ORDER BY id DESC LIMIT ?1",
        )?;
        Ok(statement
            .query_map([limit], |r| {
                Ok(RunSummary {
                    id: r.get(0)?,
                    evaluated_at: r.get(1)?,
                    analysis_version: r.get(2)?,
                    package: r.get(3)?,
                    observed_repositories: r.get(4)?,
                    candidate_repositories: r.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }
}

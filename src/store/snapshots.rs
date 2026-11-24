use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, Transaction, params};
use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use super::Store;
use crate::scip::Metadata;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Snapshot {
    pub id: i64,
    pub repository: String,
    pub commit: String,
    pub indexed_at: String,
    pub indexer_name: String,
    pub indexer_version: String,
    pub index_digest: String,
    pub documents: i64,
    pub definitions: i64,
    pub references: i64,
    pub symbol_less_occurrences: i64,
}

pub struct SnapshotInput {
    pub repository: String,
    pub commit: String,
    pub indexed_at: String,
}

impl SnapshotInput {
    /**
     * Validate snapshot repository identity, exact commit, and explicit generation
     * timestamp.
     */
    pub(super) fn validate(&self) -> Result<()> {
        crate::repository::validate_id(&self.repository)?;
        crate::repository::validate_commit(&self.commit)?;
        OffsetDateTime::parse(&self.indexed_at, &Rfc3339)
            .context("index generation time must be RFC 3339 with an explicit time zone")?;
        Ok(())
    }
}

impl Store {
    /**
     * Read a snapshot by its local database handle within the supplied connection.
     */
    pub(super) fn select_snapshot(connection: &Connection, id: i64) -> Result<Snapshot> {
        Ok(connection.query_row(
            "SELECT id, repository, commit_id, indexed_at, indexer_name, indexer_version,
                    index_digest, documents, definitions, references_count, symbol_less_occurrences
             FROM snapshots WHERE id = ?1",
            [id],
            Self::snapshot_row,
        )?)
    }

    /**
     * Decode stored snapshot columns into their reusable metadata representation.
     */
    fn snapshot_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Snapshot> {
        Ok(Snapshot {
            id: row.get(0)?,
            repository: row.get(1)?,
            commit: row.get(2)?,
            indexed_at: row.get(3)?,
            indexer_name: row.get(4)?,
            indexer_version: row.get(5)?,
            index_digest: row.get(6)?,
            documents: row.get(7)?,
            definitions: row.get(8)?,
            references: row.get(9)?,
            symbol_less_occurrences: row.get(10)?,
        })
    }

    /**
     * Read exactly the requested repository/commit snapshot without falling back.
     */
    pub fn snapshot(&self, repository: &str, commit: &str) -> Result<Snapshot> {
        crate::repository::validate_commit(commit)?;
        self.connection
            .query_row(
                "SELECT id, repository, commit_id, indexed_at, indexer_name, indexer_version,
                    index_digest, documents, definitions, references_count, symbol_less_occurrences
             FROM snapshots WHERE repository = ?1 AND commit_id = ?2",
                params![repository, commit.to_ascii_lowercase()],
                Self::snapshot_row,
            )
            .with_context(|| format!("missing indexed snapshot for {repository:?} at {commit}"))
    }

    /**
     * List snapshot metadata deterministically by repository and commit.
     */
    pub fn snapshots(&self) -> Result<Vec<Snapshot>> {
        let mut statement = self.connection.prepare(
            "SELECT id, repository, commit_id, indexed_at, indexer_name, indexer_version,
                    index_digest, documents, definitions, references_count, symbol_less_occurrences
             FROM snapshots ORDER BY repository, commit_id",
        )?;
        Ok(statement
            .query_map([], Self::snapshot_row)?
            .collect::<rusqlite::Result<_>>()?)
    }
}

/**
 * Replace one snapshot identity inside the caller transaction and normalize its
 * timestamp.
 */
pub(super) fn replace(
    tx: &Transaction<'_>,
    input: &SnapshotInput,
    metadata: &Metadata,
) -> Result<i64> {
    let tool = metadata
        .tool_info
        .as_ref()
        .context("missing SCIP indexer information")?;
    ensure!(
        !tool.name.is_empty() && !tool.version.is_empty(),
        "missing SCIP indexer name or version"
    );
    let indexed_at = OffsetDateTime::parse(&input.indexed_at, &Rfc3339)?
        .to_offset(time::UtcOffset::UTC)
        .format(&Rfc3339)?;
    tx.execute(
        "DELETE FROM snapshots WHERE repository = ?1 AND commit_id = ?2",
        params![input.repository, input.commit.to_ascii_lowercase()],
    )?;
    tx.execute(
        "INSERT INTO snapshots(repository, commit_id, indexed_at, indexer_name, indexer_version, index_digest, project_root, text_encoding)
         VALUES (?1, ?2, ?3, ?4, ?5, '', ?6, ?7)",
        params![
            input.repository,
            input.commit.to_ascii_lowercase(),
            indexed_at,
            tool.name,
            tool.version,
            metadata.project_root,
            metadata.text_document_encoding,
        ],
    )?;
    Ok(tx.last_insert_rowid())
}

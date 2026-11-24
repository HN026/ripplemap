use std::path::Path;

use anyhow::Result;
use ripplemap::store::{SnapshotInput, Store};

/**
 * Import one supplied index and write its ingestion summary to stderr.
 */
pub(super) fn run(database: &Path, scip: &Path, input: SnapshotInput) -> Result<()> {
    let snapshot = Store::open(database)?.ingest(scip, &input)?;
    eprintln!(
        "Imported {:?} at {}: {} documents, {} definitions, {} references (SHA-256 {}).",
        snapshot.repository,
        snapshot.commit,
        snapshot.documents,
        snapshot.definitions,
        snapshot.references,
        snapshot.index_digest
    );
    Ok(())
}

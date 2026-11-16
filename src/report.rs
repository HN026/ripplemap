use std::io::Write;

use anyhow::Result;

use crate::store::Snapshot;

pub fn snapshots(out: &mut impl Write, snapshots: &[Snapshot], json: bool) -> Result<()> {
    if json {
        serde_json::to_writer_pretty(&mut *out, snapshots)?;
        writeln!(out)?;
    } else if snapshots.is_empty() {
        writeln!(out, "No indexed snapshots.")?;
    } else {
        for snapshot in snapshots {
            writeln!(
                out,
                "{} @ {} | indexed {} | {} {} | {} docs, {} definitions, {} references | sha256:{}",
                serde_json::to_string(&snapshot.repository)?,
                snapshot.commit,
                snapshot.indexed_at,
                serde_json::to_string(&snapshot.indexer_name)?,
                serde_json::to_string(&snapshot.indexer_version)?,
                snapshot.documents,
                snapshot.definitions,
                snapshot.references,
                snapshot.index_digest
            )?;
        }
    }
    Ok(())
}

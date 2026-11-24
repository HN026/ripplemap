pub mod analysis;
pub mod api;
pub mod dependencies;

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

pub fn usages(out: &mut impl Write, report: &crate::impact::UsageReport, json: bool) -> Result<()> {
    if json {
        serde_json::to_writer_pretty(&mut *out, report)?;
        writeln!(out)?;
        return Ok(());
    }
    writeln!(out, "Symbol: {}", serde_json::to_string(&report.symbol)?)?;
    writeln!(
        out,
        "Referenced package: {} @ {} (from symbol identity)",
        serde_json::to_string(&report.package.name)?,
        serde_json::to_string(&report.package.version)?
    )?;
    writeln!(
        out,
        "Observed references: {}; distinct repositories: {}; indexed snapshots: {}; displayed: {}.",
        report.total_references,
        report.observed_repositories,
        report.indexed_snapshots,
        report.references.len()
    )?;
    for usage in &report.references {
        writeln!(
            out,
            "  {} @ {} | {}:{}:{}-{}:{} [{}] | indexed {} | sha256:{}",
            serde_json::to_string(&usage.repository)?,
            usage.commit,
            serde_json::to_string(&usage.path)?,
            usage.start.line,
            usage.start.column,
            usage.end.line,
            usage.end.column,
            usage.position_encoding,
            usage.indexed_at,
            usage.index_digest
        )?;
    }
    if report.truncated {
        writeln!(
            out,
            "Results truncated; increase --limit (maximum {}).",
            crate::impact::MAX_LIMIT
        )?;
    }
    writeln!(out, "{}", report.coverage_note)?;
    Ok(())
}

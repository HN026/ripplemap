use crate::analysis::{Change, Coverage, Finding, SavedRun};
use anyhow::Result;
use std::io::Write;

/**
 * Render an analysis run as JSON or ordered terminal sections.
 */
pub fn write(out: &mut impl Write, run: &SavedRun, json: bool) -> Result<()> {
    if json {
        serde_json::to_writer_pretty(&mut *out, run)?;
        writeln!(out)?;
        return Ok(());
    }
    let report = &run.report;
    write_inputs(out, run)?;
    for (change, finding) in report.comparison.changes.iter().zip(&report.findings) {
        write_change(out, change, finding, &report.base.evidence.package.version)?;
    }
    if report.comparison.changes.is_empty() {
        writeln!(
            out,
            "No supported changes detected; this is not a compatibility verdict."
        )?;
    }
    for issue in &report.comparison.issues {
        writeln!(
            out,
            "Evidence {:?}: {} | {}",
            issue.status,
            serde_json::to_string(&issue.export)?,
            issue.explanation
        )?;
    }
    writeln!(
        out,
        "\nConsumer coverage (index age threshold: {} days):",
        report.options.max_index_age_days
    )?;
    for coverage in &report.coverage {
        write_coverage(out, coverage)?;
    }
    writeln!(out, "Displayed references truncated: {}", report.truncated)?;
    for note in &report.scope_notes {
        writeln!(out, "{note}")?;
    }
    Ok(())
}

/**
 * Render the run identity, exact library revisions, and distinct repository totals.
 */
fn write_inputs(out: &mut impl Write, run: &SavedRun) -> Result<()> {
    let report = &run.report;
    writeln!(
        out,
        "Analysis run {} | {} | evaluated {}",
        run.id, report.analysis_version, report.evaluated_at
    )?;
    writeln!(
        out,
        "Package: {} | entry {}",
        serde_json::to_string(&report.base.evidence.package.name)?,
        serde_json::to_string(&report.base.evidence.entrypoint)?
    )?;
    for (label, api) in [("base", &report.base), ("head", &report.head)] {
        writeln!(
            out,
            "  {label}: version {} | {} @ {} | indexed {} | source sha256:{} | declarations sha256:{}",
            serde_json::to_string(&api.evidence.package.version)?,
            serde_json::to_string(&api.producer.repository)?,
            api.producer.commit,
            api.producer.indexed_at,
            api.producer.index_digest,
            api.declarations.index_digest
        )?;
    }
    writeln!(
        out,
        "Observed affected repositories: {}; candidate repositories: {} (distinct, may overlap).",
        report.observed_repositories, report.candidate_repositories
    )?;
    Ok(())
}

/**
 * Render one classified API change, its raw symbols, and bounded usage evidence.
 */
fn write_change(
    out: &mut impl Write,
    change: &Change,
    finding: &Finding,
    version: &str,
) -> Result<()> {
    writeln!(
        out,
        "\n{:?}: {} | {:?}",
        change.kind,
        serde_json::to_string(&change.export)?,
        change.reference_status
    )?;
    writeln!(out, "  {}", change.explanation)?;
    writeln!(
        out,
        "  API before: {}",
        serde_json::to_string(&change.before)?
    )?;
    writeln!(
        out,
        "  API after: {}",
        serde_json::to_string(&change.after)?
    )?;
    writeln!(
        out,
        "  Observed references: {}; candidates: {}; displayed: {} | truncated: {}",
        finding.observed_references,
        finding.candidate_references,
        finding.references.len(),
        finding.truncated
    )?;
    if change.reference_candidates.len() > 1 {
        writeln!(
            out,
            "  Public binding candidates: {}",
            serde_json::to_string(&change.reference_candidates)?
        )?;
    }
    for symbol in &change.mapping.declaration_symbols {
        writeln!(
            out,
            "  Baseline identity: {}",
            serde_json::to_string(symbol)?
        )?;
    }
    for usage in &finding.references {
        writeln!(
            out,
            "    {} @ {} | {}:{}:{}-{}:{} [{}] | dependency {} | indexed {} | sha256:{}",
            serde_json::to_string(&usage.repository)?,
            usage.commit,
            serde_json::to_string(&usage.path)?,
            usage.start.line,
            usage.start.column,
            usage.end.line,
            usage.end.column,
            usage.position_encoding,
            serde_json::to_string(version)?,
            usage.indexed_at,
            usage.index_digest
        )?;
    }
    Ok(())
}

/**
 * Render index freshness and deferred/unmapped reference coverage for one consumer.
 */
fn write_coverage(out: &mut impl Write, coverage: &Coverage) -> Result<()> {
    writeln!(
        out,
        "  {} @ {} | {} | observed versions {} | deferred other-version refs {} | unmapped baseline refs {} | document-local refs {}",
        serde_json::to_string(&coverage.requested.repository)?,
        coverage.requested.commit,
        coverage.freshness,
        serde_json::to_string(&coverage.observed_package_versions)?,
        coverage.other_version_references,
        coverage.unmapped_baseline_references,
        coverage.local_references
    )?;
    if let Some(snapshot) = &coverage.snapshot {
        writeln!(
            out,
            "    indexed {} | sha256:{}",
            snapshot.indexed_at, snapshot.index_digest
        )?;
    }
    if coverage.versions_truncated {
        writeln!(out, "    Version list truncated.")?;
    }
    Ok(())
}

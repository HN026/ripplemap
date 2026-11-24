use crate::health::{Diagnostic, Report, Status};
use anyhow::Result;
use std::io::Write;

pub fn write(out: &mut impl Write, status: Status, reports: &[Report], json: bool) -> Result<()> {
    if json {
        serde_json::to_writer_pretty(
            &mut *out,
            &serde_json::json!({"format_version":1, "status":status, "reports":reports}),
        )?;
        writeln!(out)?;
        return Ok(());
    }
    writeln!(out, "Health: {status:?}")?;
    for report in reports {
        writeln!(
            out,
            "\n{} @ {} | {:?} | profile {} | evaluated {}",
            serde_json::to_string(&report.repository)?,
            report.commit,
            report.status,
            report.profile,
            report.evaluated_at
        )?;
        writeln!(out, "Enabled checks: {}", report.enabled_checks.join(", "))?;
        writeln!(
            out,
            "Disabled checks: {}",
            report.disabled_checks.join(", ")
        )?;
        if let Some(snapshot) = &report.snapshot {
            writeln!(
                out,
                "Indexed {} | sha256:{}",
                snapshot.indexed_at, snapshot.index_digest
            )?;
        }
        for diagnostic in &report.diagnostics {
            write_diagnostic(out, diagnostic)?;
        }
        for dependency in &report.dependencies {
            writeln!(
                out,
                "  {:?}: {} from {} | {:?} | requested {} | resolved {}",
                dependency.status,
                serde_json::to_string(&dependency.dependency.name)?,
                serde_json::to_string(&dependency.dependency.from)?,
                dependency.dependency.kind,
                serde_json::to_string(&dependency.dependency.requirement)?,
                serde_json::to_string(
                    &dependency
                        .resolved
                        .as_ref()
                        .and_then(|p| p.version.as_ref())
                )?
            )?;
            for diagnostic in &dependency.diagnostics {
                write_diagnostic(out, diagnostic)?;
            }
        }
        writeln!(
            out,
            "Diagnostics: {} total | truncated: {}",
            report.diagnostics_total, report.truncated
        )?;
        if let Some(compiler) = &report.compiler {
            writeln!(
                out,
                "Compiler: {} | exit {:?} | {} diagnostics | {} input hashes",
                serde_json::to_string(&compiler.command)?,
                compiler.exit_code,
                compiler.diagnostics_total,
                compiler.files.len()
            )?;
        }
        for note in &report.scope_notes {
            writeln!(out, "{note}")?;
        }
    }
    Ok(())
}

fn write_diagnostic(out: &mut impl Write, diagnostic: &Diagnostic) -> Result<()> {
    writeln!(
        out,
        "    {:?} {} [{}]: {}",
        diagnostic.status,
        diagnostic.code,
        diagnostic.check,
        serde_json::to_string(&diagnostic.message)?
    )?;
    if let Some(path) = &diagnostic.path {
        write!(out, "      {}", serde_json::to_string(path)?)?;
        if let Some(start) = &diagnostic.start {
            write!(out, ":{}:{}", start.line, start.column)?;
        }
        writeln!(out)?;
    }
    for evidence in &diagnostic.evidence {
        writeln!(
            out,
            "      evidence {}#{}",
            serde_json::to_string(&evidence.file)?,
            serde_json::to_string(&evidence.pointer)?
        )?;
    }
    Ok(())
}

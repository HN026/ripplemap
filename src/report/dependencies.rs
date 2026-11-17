use std::{collections::BTreeMap, io::Write};

use anyhow::Result;

use crate::dependencies::DependencyReport;

pub fn write(out: &mut impl Write, report: &DependencyReport, json: bool) -> Result<()> {
    if json {
        serde_json::to_writer_pretty(&mut *out, report)?;
        writeln!(out)?;
        return Ok(());
    }
    writeln!(
        out,
        "Dependencies for {} @ {} | index sha256:{}",
        serde_json::to_string(&report.snapshot.repository)?,
        report.snapshot.commit,
        report.snapshot.index_digest
    )?;
    let packages: BTreeMap<_, _> = report
        .evidence
        .packages
        .iter()
        .map(|p| (&p.location, p))
        .collect();
    for dependency in &report.evidence.dependencies {
        let target = dependency.target.as_ref().and_then(|p| packages.get(p));
        writeln!(
            out,
            "  {} -> {} | resolved {} @ {} | {} {:?} {:?} | declared: {} {}",
            serde_json::to_string(&dependency.from)?,
            serde_json::to_string(&dependency.name)?,
            serde_json::to_string(&target.and_then(|p| p.name.as_ref()))?,
            serde_json::to_string(&target.and_then(|p| p.version.as_ref()))?,
            if dependency.direct {
                "direct"
            } else {
                "transitive"
            },
            dependency.kind,
            dependency.status,
            serde_json::to_string(&dependency.declared_at.file)?,
            serde_json::to_string(&dependency.declared_at.pointer)?
        )?;
        writeln!(out, "    {}", dependency.explanation)?;
        for evidence in &dependency.resolved_at {
            writeln!(
                out,
                "    resolution: {} {}",
                serde_json::to_string(&evidence.file)?,
                serde_json::to_string(&evidence.pointer)?
            )?;
        }
    }
    for issue in &report.evidence.issues {
        writeln!(
            out,
            "  Warning {}: {} | {}",
            issue.code,
            serde_json::to_string(&issue.message)?,
            serde_json::to_string(&issue.evidence.file)?
        )?;
    }
    for file in &report.evidence.files {
        writeln!(
            out,
            "  evidence: {} sha256:{}",
            serde_json::to_string(&file.path)?,
            file.sha256
        )?;
    }
    writeln!(
        out,
        "Declared dependencies and observed API references are separate facts. This report does not establish compatibility."
    )?;
    Ok(())
}

use std::io::Write;

use anyhow::Result;

use crate::api::{Bridge, Resolution};

pub fn resolution(out: &mut impl Write, result: &Resolution, json: bool) -> Result<()> {
    if json {
        serde_json::to_writer_pretty(&mut *out, result)?;
        writeln!(out)?;
        return Ok(());
    }
    writeln!(
        out,
        "API mapping: {:?} | {} @ {}",
        result.status,
        serde_json::to_string(&result.package.name)?,
        serde_json::to_string(&result.package.version)?
    )?;
    writeln!(out, "{}", result.explanation)?;
    writeln!(
        out,
        "Consumer: {} @ {}",
        serde_json::to_string(&result.consumer.repository)?,
        result.consumer.commit
    )?;
    for mapping in &result.candidates {
        writeln!(
            out,
            "  candidate {}: {:?}",
            serde_json::to_string(&mapping.name)?,
            mapping.status
        )?;
        for symbol in &mapping.producer_symbols {
            writeln!(out, "    producer: {}", serde_json::to_string(symbol)?)?;
        }
        for symbol in &mapping.declaration_symbols {
            writeln!(out, "    declaration: {}", serde_json::to_string(symbol)?)?;
        }
    }
    if let Some(usages) = &result.usages {
        super::usages(out, usages, false)?;
    } else {
        writeln!(out, "No global usage lookup for document-local symbols.")?;
    }
    Ok(())
}

pub fn bridge(out: &mut impl Write, result: &Bridge, json: bool) -> Result<()> {
    if json {
        serde_json::to_writer_pretty(&mut *out, result)?;
        writeln!(out)?;
        return Ok(());
    }
    writeln!(
        out,
        "Public binding {}: {:?}",
        serde_json::to_string(&result.export)?,
        result.status
    )?;
    writeln!(out, "{}", result.explanation)?;
    for (label, package, snapshot, mapping) in [
        (
            "base",
            &result.base_package,
            &result.base,
            &result.base_mapping,
        ),
        (
            "head",
            &result.head_package,
            &result.head,
            &result.head_mapping,
        ),
    ] {
        writeln!(
            out,
            "  {label}: {} @ {} | commit {} | index sha256:{}",
            serde_json::to_string(&package.name)?,
            serde_json::to_string(&package.version)?,
            snapshot.commit,
            snapshot.index_digest
        )?;
        if let Some(mapping) = mapping {
            for symbol in mapping
                .producer_symbols
                .iter()
                .chain(&mapping.declaration_symbols)
            {
                writeln!(out, "    {}", serde_json::to_string(symbol)?)?;
            }
        }
    }
    Ok(())
}

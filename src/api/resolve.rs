use anyhow::{Result, ensure};

use super::{ApiSnapshot, Bridge, Resolution, Status, validate};
use crate::{
    impact, scip,
    store::{Snapshot, Store},
};

pub fn resolve(
    store: &Store,
    api: &ApiSnapshot,
    consumer: &Snapshot,
    raw_symbol: &str,
    limit: u32,
) -> Result<Resolution> {
    ensure!(
        (1..=impact::MAX_LIMIT).contains(&limit),
        "invalid reference limit"
    );
    validate(store, api)?;
    let actual = store.snapshot(&consumer.repository, &consumer.commit)?;
    let mut expected = consumer.clone();
    expected.id = actual.id;
    ensure!(actual == expected, "consumer snapshot changed");
    let package = scip::package(raw_symbol)?;
    let candidates: Vec<_> = api
        .mappings
        .iter()
        .filter(|mapping| mapping.declaration_symbols.iter().any(|s| s == raw_symbol))
        .cloned()
        .collect();
    let (mut status, mut explanation) = match package.as_ref() {
        None => (
            Status::Unsupported,
            "Local symbols require document scope and cannot be mapped by a global identity",
        ),
        Some(package)
            if package.scheme != "scip-typescript" || package.manager.as_deref() != Some("npm") =>
        {
            (Status::Unsupported, "Unsupported SCIP package scheme")
        }
        Some(package)
            if package.name.as_deref() != Some(&api.evidence.package.name)
                || package.version.as_deref() != Some(&api.evidence.package.version) =>
        {
            (
                Status::Missing,
                "No exact package/version identity in this API snapshot; versions are never stripped",
            )
        }
        Some(_) if candidates.is_empty() => (
            Status::Missing,
            "No public export mapping for this exact declaration symbol",
        ),
        Some(_) if candidates.iter().any(|m| m.status == Status::Unsupported) => (
            Status::Unsupported,
            "This public export construct is unsupported",
        ),
        Some(_)
            if candidates.len() > 1 || candidates.iter().any(|m| m.status == Status::Ambiguous) =>
        {
            (
                Status::Ambiguous,
                "SCIP does not distinguish the public export candidates; no single export is selected",
            )
        }
        Some(_) => (
            candidates[0].status.clone(),
            "Exact versioned declaration symbol maps to the supported public export",
        ),
    };
    let usages = if package.is_some() {
        Some(impact::usages_in_snapshot(
            store, raw_symbol, limit, &actual,
        )?)
    } else {
        None
    };
    if usages.as_ref().is_some_and(|u| u.total_references == 0)
        && matches!(status, Status::Resolved | Status::Ambiguous)
    {
        status = Status::Missing;
        explanation = "No exact symbol reference in the selected consumer snapshot";
    }
    Ok(Resolution {
        status,
        explanation: explanation.into(),
        raw_symbol: raw_symbol.into(),
        package: api.evidence.package.clone(),
        entrypoint: api.evidence.entrypoint.clone(),
        producer: api.producer.clone(),
        declarations: api.declarations.clone(),
        consumer: actual,
        candidates,
        usages,
    })
}

pub fn bridge(store: &Store, base: &ApiSnapshot, head: &ApiSnapshot, name: &str) -> Result<Bridge> {
    validate(store, base)?;
    validate(store, head)?;
    let before = base.mappings.iter().find(|m| m.name == name).cloned();
    let after = head.mappings.iter().find(|m| m.name == name).cloned();
    let (status, explanation) = if base.evidence.package.manager != head.evidence.package.manager
        || base.evidence.package.name != head.evidence.package.name
        || base.evidence.entrypoint != head.evidence.entrypoint
    {
        (
            Status::Missing,
            "Public bindings in different packages or entry points are not linked",
        )
    } else if before
        .as_ref()
        .is_some_and(|m| m.status == Status::Unsupported)
        || after
            .as_ref()
            .is_some_and(|m| m.status == Status::Unsupported)
    {
        (
            Status::Unsupported,
            "At least one public export construct is unsupported",
        )
    } else if before
        .as_ref()
        .is_some_and(|m| m.status == Status::Ambiguous)
        || after
            .as_ref()
            .is_some_and(|m| m.status == Status::Ambiguous)
    {
        (
            Status::Ambiguous,
            "At least one compiler-to-SCIP mapping is ambiguous",
        )
    } else if before.as_ref().is_none_or(|m| m.status != Status::Resolved)
        || after.as_ref().is_none_or(|m| m.status != Status::Resolved)
    {
        (
            Status::Missing,
            "A supported public binding is absent or lacks definition evidence in one supplied snapshot",
        )
    } else {
        (
            Status::Resolved,
            "Same compiler-verified public export contract (npm package, entry point, export binding) in the explicitly supplied versions. This does not establish implementation, signature, or behavior compatibility; raw identities and usage sets remain separate",
        )
    };
    Ok(Bridge {
        status,
        explanation: explanation.into(),
        export: name.into(),
        base_package: base.evidence.package.clone(),
        head_package: head.evidence.package.clone(),
        base: base.producer.clone(),
        head: head.producer.clone(),
        base_mapping: before,
        head_mapping: after,
    })
}

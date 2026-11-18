use std::collections::BTreeSet;

use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, params};

use super::{ApiSnapshot, Extraction, Mapping, Package, Site, Status};
use crate::{
    scip,
    store::{Snapshot, Store},
};

pub fn bind(
    store: &Store,
    evidence: Extraction,
    producer: &Snapshot,
    declarations: &Snapshot,
) -> Result<ApiSnapshot> {
    validate_evidence(&evidence)?;
    let _read = store.connection.unchecked_transaction()?;
    let producer = current_snapshot(store, producer)?;
    let declarations = current_snapshot(store, declarations)?;
    ensure!(
        producer.commit == declarations.commit,
        "producer and declaration snapshots must record the same exact source commit"
    );
    ensure!(
        producer.indexer_name == "scip-typescript"
            && declarations.indexer_name == "scip-typescript",
        "only scip-typescript snapshots are supported for API binding"
    );
    let mut mappings = Vec::new();
    for export in &evidence.exports {
        let source = symbols_at(store, &producer, export.source.as_ref(), &evidence.package)?;
        let types = symbols_at(
            store,
            &declarations,
            export.declaration.as_ref(),
            &evidence.package,
        )?;
        let (status, explanation) = if export.status == Status::Unsupported {
            (Status::Unsupported, export.explanation.clone())
        } else if source.is_empty() || types.is_empty() {
            (
                Status::Missing,
                "No matching versioned SCIP definition at a compiler-derived site".into(),
            )
        } else if source.len() != 1 || types.len() != 1 {
            (
                Status::Ambiguous,
                "Multiple SCIP identities occur at a compiler-derived definition site".into(),
            )
        } else {
            (Status::Resolved, "Source and emitted declaration joined through compiler-derived sites and exact package/version identities".into())
        };
        mappings.push(Mapping {
            name: export.name.clone(),
            status,
            explanation,
            producer_symbols: source,
            declaration_symbols: types,
        });
    }
    mappings.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(ApiSnapshot {
        producer,
        declarations,
        evidence,
        mappings,
    })
}

pub fn validate(store: &Store, api: &ApiSnapshot) -> Result<()> {
    let rebuilt = bind(
        store,
        api.evidence.clone(),
        &api.producer,
        &api.declarations,
    )?;
    ensure!(
        rebuilt.mappings == api.mappings,
        "API mapping evidence does not agree with the stored SCIP definitions"
    );
    Ok(())
}

fn current_snapshot(store: &Store, expected: &Snapshot) -> Result<Snapshot> {
    let actual = store.snapshot(&expected.repository, &expected.commit)?;
    let mut normalized = expected.clone();
    normalized.id = actual.id;
    ensure!(
        actual == normalized,
        "indexed snapshot changed since API extraction for {:?}; extract again",
        expected.repository
    );
    Ok(actual)
}

fn validate_evidence(evidence: &Extraction) -> Result<()> {
    ensure!(
        evidence.format_version == 1 && evidence.typescript_version == "6.0.3",
        "unsupported API evidence/compiler version"
    );
    ensure!(
        evidence.entrypoint == "." && evidence.package.manager == "npm",
        "only the npm root entry point is supported"
    );
    ensure!(
        !evidence.package.name.is_empty() && !evidence.package.version.is_empty(),
        "missing exact package identity"
    );
    scip::validate_path(&evidence.source_entry)?;
    scip::validate_path(&evidence.declaration_entry)?;
    let mut files = BTreeSet::new();
    for file in &evidence.files {
        scip::validate_path(&file.path)?;
        ensure!(
            files.insert(file.path.as_str()),
            "duplicate API evidence file"
        );
        ensure!(
            file.sha256.len() == 64 && file.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid evidence SHA-256"
        );
    }
    for required in [
        "package.json",
        "tsconfig.json",
        &evidence.source_entry,
        &evidence.declaration_entry,
    ] {
        ensure!(
            files.contains(required),
            "missing API file evidence for {required:?}"
        );
    }
    let mut names = BTreeSet::new();
    for export in &evidence.exports {
        ensure!(
            !export.name.is_empty() && names.insert(&export.name),
            "empty or duplicate public export"
        );
        ensure!(
            matches!(export.status, Status::Resolved | Status::Unsupported),
            "invalid compiler export status"
        );
        if export.status == Status::Resolved {
            ensure!(
                export.source.is_some() && export.declaration.is_some(),
                "supported export is missing compiler sites"
            );
        }
        for site in [export.source.as_ref(), export.declaration.as_ref()]
            .into_iter()
            .flatten()
        {
            scip::validate_path(&site.path)?;
            ensure!(
                files.contains(site.path.as_str()),
                "compiler site has no file evidence"
            );
            ensure!(
                site.range.iter().all(|n| *n >= 0)
                    && (site.range[0], site.range[1]) <= (site.range[2], site.range[3]),
                "invalid compiler source range"
            );
        }
    }
    Ok(())
}

fn symbols_at(
    store: &Store,
    snapshot: &Snapshot,
    site: Option<&Site>,
    package: &Package,
) -> Result<Vec<String>> {
    let Some(site) = site else {
        return Ok(Vec::new());
    };
    let encoding: Option<i32> = store
        .connection
        .query_row(
            "SELECT position_encoding FROM documents WHERE snapshot_id = ?1 AND path = ?2",
            params![snapshot.id, site.path],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(encoding) = encoding {
        ensure!(
            encoding == 2,
            "unsupported API mapping position encoding {encoding}; UTF-16 is required"
        );
    }
    let mut statement = store.connection.prepare(
        "SELECT DISTINCT sym.symbol FROM definitions o
         JOIN documents d ON d.id = o.document_id JOIN symbols sym ON sym.id = o.symbol_id
         WHERE d.snapshot_id = ?1 AND d.path = ?2 AND o.start_line = ?3 AND o.start_character = ?4
           AND o.end_line = ?5 AND o.end_character = ?6 AND sym.scope = ''
           AND sym.scheme = 'scip-typescript' AND sym.package_manager = 'npm'
           AND sym.package_name = ?7 AND sym.package_version = ?8
         ORDER BY sym.symbol LIMIT 17",
    )?;
    let symbols: Vec<String> = statement
        .query_map(
            params![
                snapshot.id,
                site.path,
                site.range[0],
                site.range[1],
                site.range[2],
                site.range[3],
                package.name,
                package.version
            ],
            |row| row.get(0),
        )?
        .collect::<rusqlite::Result<_>>()
        .context("look up compiler-derived SCIP definition sites")?;
    ensure!(
        symbols.len() <= 16,
        "too many identities at one API definition site"
    );
    Ok(symbols)
}

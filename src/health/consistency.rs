use super::{Diagnostic, Status};
use crate::{dependencies::DependencyReport, impact, npm, repository::Revision, store::Store};
use anyhow::Result;
use rusqlite::params;
use std::collections::BTreeSet;

pub(super) fn inspect(
    store: &Store,
    revision: &Revision,
    saved: &DependencyReport,
) -> Result<(npm::EvidenceSet, Vec<Diagnostic>)> {
    let current = npm::inspect(revision)?;
    let mut result = Vec::new();
    if current != saved.evidence {
        result.push(Diagnostic::new("evidence_consistency", "dependency_evidence_changed", Status::Unknown,
            "Committed dependency inspection differs from saved evidence; run dependencies scan again"));
    } else {
        result.push(Diagnostic::new(
            "evidence_consistency",
            "committed_evidence_matches",
            Status::Healthy,
            "Saved dependency evidence matches inspection of the exact committed files",
        ));
    }
    for issue in &current.issues {
        let mut diagnostic = Diagnostic::new(
            "evidence_consistency",
            &issue.code,
            Status::Unknown,
            &issue.message,
        );
        diagnostic.path = Some(issue.evidence.file.clone());
        diagnostic.evidence.push(issue.evidence.clone());
        result.push(diagnostic);
    }
    let mut documents = store
        .connection
        .prepare("SELECT path FROM documents WHERE snapshot_id = ?1 ORDER BY path")?;
    let paths = documents.query_map([saved.snapshot.id], |r| r.get::<_, String>(0))?;
    let mut count = 0;
    for path in paths {
        let path = path?;
        let full = if revision.prefix.is_empty() {
            path.clone()
        } else {
            format!("{}/{path}", revision.prefix)
        };
        if revision.read(&full)?.is_none() {
            let mut diagnostic = Diagnostic::new(
                "evidence_consistency",
                "indexed_document_missing",
                Status::Unknown,
                "Indexed document is absent from the exact committed package; source/index correspondence is unresolved",
            );
            diagnostic.path = Some(path);
            result.push(diagnostic);
        }
        count += 1;
    }
    if count == 0 {
        result.push(Diagnostic::new(
            "evidence_consistency",
            "empty_semantic_snapshot",
            Status::Unknown,
            "Semantic snapshot has no indexed documents",
        ));
    }
    let packages: BTreeSet<_> = current
        .packages
        .iter()
        .filter_map(|p| Some((p.name.as_deref()?, p.version.as_deref()?)))
        .collect();
    let mut statement = store.connection.prepare(
        "SELECT sym.package_name, sym.package_version, min(sym.symbol) FROM symbol_references o
         JOIN symbols sym ON sym.id = o.symbol_id JOIN documents d ON d.id = o.document_id
         WHERE d.snapshot_id = ?1 AND sym.scope = '' AND sym.scheme = 'scip-typescript'
         AND sym.package_manager = 'npm' GROUP BY sym.package_name, sym.package_version
         ORDER BY sym.package_name, sym.package_version",
    )?;
    let mut rows = statement.query(params![saved.snapshot.id])?;
    while let Some(row) = rows.next()? {
        let name: Option<String> = row.get(0)?;
        let version: Option<String> = row.get(1)?;
        if name
            .as_deref()
            .zip(version.as_deref())
            .is_some_and(|p| packages.contains(&p))
        {
            continue;
        }
        let symbol: String = row.get(2)?;
        let mut diagnostic = Diagnostic::new(
            "evidence_consistency",
            "semantic_package_not_resolved",
            Status::Unknown,
            format!(
                "Semantic package {:?}@{:?} has no matching identity/version in the inspected dependency graph",
                name, version
            ),
        );
        diagnostic.dependency = name;
        if let Some(usage) = impact::usages_in_snapshot(store, &symbol, 1, &saved.snapshot)?
            .references
            .into_iter()
            .next()
        {
            diagnostic.path = Some(usage.path);
            diagnostic.start = Some(usage.start);
        }
        result.push(diagnostic);
    }
    Ok((current, result))
}

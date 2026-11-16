use anyhow::{Context, Result, ensure};
use rusqlite::{Transaction, params};

use crate::scip::{self, DEFINITION, Document};

pub(super) fn insert_symbol(
    tx: &Transaction<'_>,
    snapshot: i64,
    path: &str,
    symbol: &str,
) -> Result<i64> {
    let package = scip::package(symbol)?;
    let scope = if package.is_none() { path } else { "" };
    let mut insert = tx.prepare_cached(
        "INSERT OR IGNORE INTO symbols(snapshot_id, scope, symbol, scheme, package_manager, package_name, package_version)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    insert.execute(params![
        snapshot,
        scope,
        symbol,
        package.as_ref().map(|p| &p.scheme),
        package.as_ref().and_then(|p| p.manager.as_ref()),
        package.as_ref().and_then(|p| p.name.as_ref()),
        package.as_ref().and_then(|p| p.version.as_ref()),
    ])?;
    Ok(tx
        .prepare_cached(
            "SELECT id FROM symbols WHERE snapshot_id = ?1 AND scope = ?2 AND symbol = ?3",
        )?
        .query_row(params![snapshot, scope, symbol], |r| r.get(0))?)
}

pub(super) fn insert_document(
    tx: &Transaction<'_>,
    snapshot: i64,
    document: &Document,
) -> Result<()> {
    scip::validate_path(&document.relative_path)?;
    ensure!(
        (1..=3).contains(&document.position_encoding),
        "unsupported or unspecified position encoding {}",
        document.position_encoding
    );
    tx.execute(
        "INSERT INTO documents(snapshot_id, path, language, position_encoding)
         VALUES (?1, ?2, ?3, ?4)",
        params![
            snapshot,
            document.relative_path,
            document.language,
            document.position_encoding
        ],
    )?;
    let document_id = tx.last_insert_rowid();
    for symbol in &document.symbols {
        insert_symbol(tx, snapshot, &document.relative_path, &symbol.symbol)?;
    }
    let (mut definitions, mut references, mut symbol_less) = (0, 0, 0);
    let mut insert = tx.prepare_cached(
        "INSERT OR IGNORE INTO occurrences(document_id, symbol_id, symbol_roles, start_line, start_character, end_line, end_character)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    for occurrence in &document.occurrences {
        let [line, start, end_line, end] = occurrence
            .position()
            .with_context(|| format!("symbol {:?}", occurrence.symbol))?;
        ensure!(occurrence.symbol_roles >= 0, "negative symbol roles");
        if occurrence.symbol.is_empty() {
            symbol_less += 1;
            continue;
        }
        let symbol = insert_symbol(tx, snapshot, &document.relative_path, &occurrence.symbol)?;
        let added = insert.execute(params![
            document_id,
            symbol,
            occurrence.symbol_roles,
            line,
            start,
            end_line,
            end
        ])? as i64;
        if occurrence.symbol_roles & DEFINITION != 0 {
            definitions += added;
        } else {
            references += added;
        }
    }
    tx.execute(
        "UPDATE snapshots SET documents = documents + 1, definitions = definitions + ?1,
         references_count = references_count + ?2, symbol_less_occurrences = symbol_less_occurrences + ?3 WHERE id = ?4",
        params![definitions, references, symbol_less, snapshot],
    )?;
    Ok(())
}

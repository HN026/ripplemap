use std::{fs::File, path::Path, time::Duration};

use anyhow::{Context, Result, ensure};
use prost::Message;
use rusqlite::{Connection, OpenFlags, Transaction, TransactionBehavior, params};
use serde::Serialize;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::scip::{self, DEFINITION, Document, Metadata, SymbolInformation};

const SCHEMA_VERSION: i64 = 1;
const APPLICATION_ID: i64 = 0x524d4150;

const SCHEMA: &str = "
CREATE TABLE snapshots (
    id INTEGER PRIMARY KEY,
    repository TEXT NOT NULL,
    commit_id TEXT NOT NULL,
    indexed_at TEXT NOT NULL,
    indexer_name TEXT NOT NULL,
    indexer_version TEXT NOT NULL,
    index_digest TEXT NOT NULL,
    project_root TEXT NOT NULL,
    text_encoding INTEGER NOT NULL,
    documents INTEGER NOT NULL DEFAULT 0,
    definitions INTEGER NOT NULL DEFAULT 0,
    references_count INTEGER NOT NULL DEFAULT 0,
    symbol_less_occurrences INTEGER NOT NULL DEFAULT 0,
    UNIQUE(repository, commit_id)
);
CREATE TABLE documents (
    id INTEGER PRIMARY KEY,
    snapshot_id INTEGER NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    language TEXT NOT NULL,
    position_encoding INTEGER NOT NULL,
    UNIQUE(snapshot_id, path)
);
CREATE TABLE symbols (
    id INTEGER PRIMARY KEY,
    snapshot_id INTEGER NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
    scope TEXT NOT NULL,
    symbol TEXT NOT NULL,
    scheme TEXT,
    package_manager TEXT,
    package_name TEXT,
    package_version TEXT,
    UNIQUE(snapshot_id, scope, symbol)
);
CREATE INDEX symbols_lookup ON symbols(symbol) WHERE scope = '';
CREATE TABLE occurrences (
    document_id INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    symbol_id INTEGER NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
    symbol_roles INTEGER NOT NULL,
    start_line INTEGER NOT NULL,
    start_character INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    end_character INTEGER NOT NULL,
    UNIQUE(document_id, symbol_id, symbol_roles, start_line, start_character, end_line, end_character)
);
CREATE INDEX occurrences_symbol ON occurrences(symbol_id);
CREATE VIEW definitions AS SELECT * FROM occurrences WHERE (symbol_roles & 1) != 0;
CREATE VIEW symbol_references AS SELECT * FROM occurrences WHERE (symbol_roles & 1) = 0;
";

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Snapshot {
    pub id: i64,
    pub repository: String,
    pub commit: String,
    pub indexed_at: String,
    pub indexer_name: String,
    pub indexer_version: String,
    pub index_digest: String,
    pub documents: i64,
    pub definitions: i64,
    pub references: i64,
    pub symbol_less_occurrences: i64,
}

pub struct SnapshotInput {
    pub repository: String,
    pub commit: String,
    pub indexed_at: String,
}

impl SnapshotInput {
    fn validate(&self) -> Result<()> {
        ensure!(
            !self.repository.trim().is_empty() && !self.repository.contains(char::is_control),
            "repository must be a nonempty identifier without control characters"
        );
        ensure!(
            matches!(self.commit.len(), 40 | 64)
                && self.commit.bytes().all(|b| b.is_ascii_hexdigit()),
            "commit must be a full 40- or 64-character Git object ID, not a branch or short hash"
        );
        OffsetDateTime::parse(&self.indexed_at, &Rfc3339)
            .context("index generation time must be RFC 3339 with an explicit time zone")?;
        Ok(())
    }
}

pub struct Store {
    pub(crate) connection: Connection,
}

impl Store {
    /// Create an empty Ripplemap database or open schema version 1. No migrations.
    pub fn open(path: &Path) -> Result<Self> {
        let mut connection =
            Connection::open(path).with_context(|| format!("open database {}", path.display()))?;
        Self::configure(&connection)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let application: i64 = tx.pragma_query_value(None, "application_id", |r| r.get(0))?;
        let version: i64 = tx.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if application == 0 && version == 0 {
            let tables: i64 = tx.query_row(
                "SELECT count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
                [],
                |r| r.get(0),
            )?;
            ensure!(
                tables == 0,
                "refusing to initialize a nonempty non-Ripplemap database"
            );
            tx.execute_batch(SCHEMA)?;
            tx.pragma_update(None, "application_id", APPLICATION_ID)?;
            tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        } else {
            Self::check_schema(&tx)?;
        }
        tx.commit()?;
        Ok(Self { connection })
    }

    /// Queries must not create a missing database or mutate an existing one.
    pub fn open_read_only(path: &Path) -> Result<Self> {
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .with_context(|| format!("open existing database {}", path.display()))?;
        Self::configure(&connection)?;
        Self::check_schema(&connection)?;
        Ok(Self { connection })
    }

    fn configure(connection: &Connection) -> Result<()> {
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", true)?;
        connection.pragma_update(None, "cache_size", -8192)?;
        Ok(())
    }

    fn check_schema(connection: &Connection) -> Result<()> {
        let application: i64 =
            connection.pragma_query_value(None, "application_id", |r| r.get(0))?;
        let version: i64 = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
        ensure!(application == APPLICATION_ID, "not a Ripplemap database");
        ensure!(
            version == SCHEMA_VERSION,
            "unsupported database schema version {version}; expected {SCHEMA_VERSION}"
        );
        Ok(())
    }

    pub fn ingest(&mut self, path: &Path, input: &SnapshotInput) -> Result<Snapshot> {
        self.ingest_inner(path, input).with_context(|| {
            format!(
                "ingest {} for repository {:?} at {}",
                path.display(),
                input.repository,
                input.commit
            )
        })
    }

    fn ingest_inner(&mut self, path: &Path, input: &SnapshotInput) -> Result<Snapshot> {
        input.validate()?;
        let file =
            File::open(path).with_context(|| format!("open SCIP file {}", path.display()))?;
        ensure!(
            file.metadata()?.is_file(),
            "SCIP input must be a regular file"
        );
        let mut reader = scip::Reader::new(file);
        let (tag, bytes) = reader
            .read_field()?
            .context("empty SCIP index: missing metadata")?;
        ensure!(tag == 1, "SCIP metadata must be the first field");
        let metadata = Metadata::decode(bytes.as_slice()).context("decode SCIP metadata")?;
        drop(bytes);
        ensure!(
            metadata.version == 0,
            "unsupported SCIP protocol version {}",
            metadata.version
        );
        let tool = metadata
            .tool_info
            .context("missing SCIP indexer information")?;
        ensure!(
            !tool.name.is_empty() && !tool.version.is_empty(),
            "missing SCIP indexer name or version"
        );
        ensure!(
            !metadata.project_root.is_empty(),
            "missing SCIP project root"
        );
        let indexed_at = OffsetDateTime::parse(&input.indexed_at, &Rfc3339)?
            .to_offset(time::UtcOffset::UTC)
            .format(&Rfc3339)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "DELETE FROM snapshots WHERE repository = ?1 AND commit_id = ?2",
            params![input.repository, input.commit.to_ascii_lowercase()],
        )?;
        tx.execute(
            "INSERT INTO snapshots(repository, commit_id, indexed_at, indexer_name, indexer_version, index_digest, project_root, text_encoding)
             VALUES (?1, ?2, ?3, ?4, ?5, '', ?6, ?7)",
            params![input.repository, input.commit.to_ascii_lowercase(), indexed_at, tool.name, tool.version, metadata.project_root, metadata.text_document_encoding],
        )?;
        let id = tx.last_insert_rowid();
        while let Some((tag, bytes)) = reader.read_field()? {
            match tag {
                1 => anyhow::bail!("duplicate SCIP metadata"),
                2 => {
                    let document =
                        Document::decode(bytes.as_slice()).context("decode SCIP document")?;
                    drop(bytes);
                    Self::insert_document(&tx, id, &document)
                        .with_context(|| format!("document {:?}", document.relative_path))?;
                }
                3 => {
                    let symbol = SymbolInformation::decode(bytes.as_slice())
                        .context("decode external symbol")?;
                    ensure!(
                        !symbol.symbol.starts_with("local "),
                        "external symbol cannot be local"
                    );
                    Self::insert_symbol(&tx, id, "", &symbol.symbol)?;
                }
                _ => unreachable!(),
            }
        }
        tx.execute(
            "UPDATE snapshots SET index_digest = ?1 WHERE id = ?2",
            params![reader.finish(), id],
        )?;
        let snapshot = Self::select_snapshot(&tx, id)?;
        tx.commit().context("commit snapshot replacement")?;
        Ok(snapshot)
    }

    fn insert_symbol(tx: &Transaction<'_>, snapshot: i64, path: &str, symbol: &str) -> Result<i64> {
        let package = scip::package(symbol)?;
        let scope = if package.is_none() { path } else { "" };
        tx.prepare_cached(
            "INSERT OR IGNORE INTO symbols(snapshot_id, scope, symbol, scheme, package_manager, package_name, package_version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?.execute(params![snapshot, scope, symbol, package.as_ref().map(|p| &p.scheme), package.as_ref().and_then(|p| p.manager.as_ref()), package.as_ref().and_then(|p| p.name.as_ref()), package.as_ref().and_then(|p| p.version.as_ref())])?;
        Ok(tx
            .prepare_cached(
                "SELECT id FROM symbols WHERE snapshot_id = ?1 AND scope = ?2 AND symbol = ?3",
            )?
            .query_row(params![snapshot, scope, symbol], |r| r.get(0))?)
    }

    fn insert_document(tx: &Transaction<'_>, snapshot: i64, document: &Document) -> Result<()> {
        scip::validate_path(&document.relative_path)?;
        ensure!(
            (1..=3).contains(&document.position_encoding),
            "unsupported or unspecified position encoding {}",
            document.position_encoding
        );
        tx.execute("INSERT INTO documents(snapshot_id, path, language, position_encoding) VALUES (?1, ?2, ?3, ?4)", params![snapshot, document.relative_path, document.language, document.position_encoding])?;
        let document_id = tx.last_insert_rowid();
        for symbol in &document.symbols {
            Self::insert_symbol(tx, snapshot, &document.relative_path, &symbol.symbol)?;
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
            let symbol =
                Self::insert_symbol(tx, snapshot, &document.relative_path, &occurrence.symbol)?;
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

    fn select_snapshot(connection: &Connection, id: i64) -> Result<Snapshot> {
        Ok(connection.query_row("SELECT id, repository, commit_id, indexed_at, indexer_name, indexer_version, index_digest, documents, definitions, references_count, symbol_less_occurrences FROM snapshots WHERE id = ?1", [id], Self::snapshot_row)?)
    }

    fn snapshot_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Snapshot> {
        Ok(Snapshot {
            id: row.get(0)?,
            repository: row.get(1)?,
            commit: row.get(2)?,
            indexed_at: row.get(3)?,
            indexer_name: row.get(4)?,
            indexer_version: row.get(5)?,
            index_digest: row.get(6)?,
            documents: row.get(7)?,
            definitions: row.get(8)?,
            references: row.get(9)?,
            symbol_less_occurrences: row.get(10)?,
        })
    }

    pub fn snapshots(&self) -> Result<Vec<Snapshot>> {
        let mut statement = self.connection.prepare("SELECT id, repository, commit_id, indexed_at, indexer_name, indexer_version, index_digest, documents, definitions, references_count, symbol_less_occurrences FROM snapshots ORDER BY repository, commit_id")?;
        Ok(statement
            .query_map([], Self::snapshot_row)?
            .collect::<rusqlite::Result<_>>()?)
    }
}

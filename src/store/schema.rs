use anyhow::{Result, ensure};
use rusqlite::{Connection, Transaction};

const SCHEMA_VERSION: i64 = 2;
const APPLICATION_ID: i64 = 0x524d4150;

pub(super) fn initialize(tx: &Transaction<'_>) -> Result<()> {
    let application: i64 = tx.pragma_query_value(None, "application_id", |r| r.get(0))?;
    let version: i64 = tx.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if application == 0 && version == 0 {
        let tables: i64 = tx.query_row("SELECT count(*) FROM sqlite_schema", [], |r| r.get(0))?;
        ensure!(
            tables == 0,
            "refusing to initialize a nonempty non-Ripplemap database"
        );
        tx.execute_batch(include_str!("schema.sql"))?;
        tx.pragma_update(None, "application_id", APPLICATION_ID)?;
        tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    } else {
        check(tx)?;
    }
    Ok(())
}

pub(super) fn check(connection: &Connection) -> Result<()> {
    let application: i64 = connection.pragma_query_value(None, "application_id", |r| r.get(0))?;
    let version: i64 = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
    ensure!(application == APPLICATION_ID, "not a Ripplemap database");
    ensure!(
        version == SCHEMA_VERSION,
        "unsupported database schema version {version}; expected {SCHEMA_VERSION}. Use a fresh database and reimport indexes; automatic migration is not supported"
    );
    Ok(())
}

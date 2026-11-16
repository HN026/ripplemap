mod documents;
mod ingest;
mod schema;
mod snapshots;

use std::{path::Path, time::Duration};

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags, TransactionBehavior};

pub use snapshots::{Snapshot, SnapshotInput};

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
        schema::initialize(&tx)?;
        tx.commit()?;
        Ok(Self { connection })
    }

    /// Queries must not create a missing database or mutate an existing one.
    pub fn open_read_only(path: &Path) -> Result<Self> {
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .with_context(|| format!("open existing database {}", path.display()))?;
        Self::configure(&connection)?;
        schema::check(&connection)?;
        Ok(Self { connection })
    }

    fn configure(connection: &Connection) -> Result<()> {
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", true)?;
        connection.pragma_update(None, "cache_size", -8192)?;
        Ok(())
    }
}

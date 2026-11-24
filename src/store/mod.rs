mod analyses;
mod dependencies;
mod documents;
mod ingest;
mod schema;
mod snapshots;

use std::{path::Path, time::Duration};

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags, TransactionBehavior};

pub use analyses::RunSummary;
pub use snapshots::{Snapshot, SnapshotInput};

pub struct Store {
    pub(crate) connection: Connection,
}

impl Store {
    /**
     * Create an empty Ripplemap database or open schema version 3 without migrations.
     */
    pub fn open(path: &Path) -> Result<Self> {
        let mut connection =
            Connection::open(path).with_context(|| format!("open database {}", path.display()))?;
        Self::configure(&connection)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        schema::initialize(&tx)?;
        tx.commit()?;
        Ok(Self { connection })
    }

    /**
     * Open an existing compatible database without creating or mutating it.
     */
    pub fn open_read_only(path: &Path) -> Result<Self> {
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .with_context(|| format!("open existing database {}", path.display()))?;
        Self::configure(&connection)?;
        schema::check(&connection)?;
        Ok(Self { connection })
    }

    /**
     * Apply bounded cache, busy-timeout, and foreign-key settings to a connection.
     */
    fn configure(connection: &Connection) -> Result<()> {
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", true)?;
        connection.pragma_update(None, "cache_size", -8192)?;
        Ok(())
    }
}

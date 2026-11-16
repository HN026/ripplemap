use std::{io, path::Path};

use anyhow::Result;
use ripplemap::{report, store::Store};

pub(super) fn run(database: &Path, json: bool) -> Result<()> {
    let snapshots = Store::open_read_only(database)?.snapshots()?;
    report::snapshots(&mut io::stdout().lock(), &snapshots, json)
}

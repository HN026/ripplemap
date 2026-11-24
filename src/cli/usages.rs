use std::{io, path::Path};

use anyhow::Result;
use ripplemap::{impact, report, store::Store};

/**
 * Query an exact SCIP identity and render bounded usages with accurate totals.
 */
pub(super) fn run(database: &Path, symbol: &str, json: bool, limit: u32) -> Result<()> {
    let store = Store::open_read_only(database)?;
    let result = impact::usages(&store, symbol, limit)?;
    report::usages(&mut io::stdout().lock(), &result, json)
}

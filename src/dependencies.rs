use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::{
    npm::{self, EvidenceSet},
    repository::{Repository, Revision},
    store::{Snapshot, Store},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DependencyReport {
    pub snapshot: Snapshot,
    pub evidence: EvidenceSet,
}

/**
 * Inspect dependency files at the exact indexed commit and atomically save evidence.
 */
pub fn scan(store: &mut Store, repository: &Repository, commit: &str) -> Result<DependencyReport> {
    let snapshot = store.snapshot(&repository.id, commit)?;
    let revision = Revision::open(&repository.path, &snapshot.commit)?;
    let report = DependencyReport {
        snapshot,
        evidence: npm::inspect(&revision)?,
    };
    store.save_dependencies(&report)?;
    Ok(report)
}

mod git;
mod manifest;

use anyhow::{Result, ensure};

pub use git::Revision;
pub use manifest::{Repository, RepositoryManifest};

/**
 * Reject empty repository identifiers and control characters.
 */
pub fn validate_id(id: &str) -> Result<()> {
    ensure!(
        !id.trim().is_empty() && !id.contains(char::is_control),
        "repository must be a nonempty identifier without control characters"
    );
    Ok(())
}

/**
 * Require a complete hexadecimal Git object ID rather than a branch or short hash.
 */
pub fn validate_commit(commit: &str) -> Result<()> {
    ensure!(
        matches!(commit.len(), 40 | 64) && commit.bytes().all(|b| b.is_ascii_hexdigit()),
        "commit must be a full 40- or 64-character Git object ID, not a branch or short hash"
    );
    Ok(())
}

use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};

use anyhow::{Context, Result, ensure};
use serde::de::DeserializeOwned;
use tempfile::NamedTempFile;

use super::ApiSnapshot;

/**
 * Decode a bounded required JSON artifact, rejecting missing or nonregular inputs.
 */
pub fn read<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let file = File::open(path)
        .with_context(|| format!("open required API artifact {}", path.display()))?;
    ensure!(
        file.metadata()?.is_file(),
        "API artifact must be a regular file"
    );
    let mut bytes = Vec::new();
    file.take(8 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 8 * 1024 * 1024, "API artifact exceeds 8 MiB");
    serde_json::from_slice(&bytes).with_context(|| format!("parse API artifact {}", path.display()))
}

/**
 * Atomically replace an API artifact only after serialization and syncing succeed.
 */
pub fn write(path: &Path, api: &ApiSnapshot) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(temporary.as_file_mut(), api)?;
    writeln!(temporary)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .with_context(|| format!("save API artifact {}", path.display()))?;
    Ok(())
}

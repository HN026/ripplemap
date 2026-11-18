use std::{
    collections::BTreeMap,
    io::{Read, Seek},
    path::Path,
    process::{Command, Stdio},
};

use anyhow::{Context, Result, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;

use super::{ApiSnapshot, Extraction, bind};
use crate::{
    repository::{Repository, Revision},
    store::Store,
};

const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Serialize)]
struct Input<'a> {
    files: &'a BTreeMap<String, String>,
    source_entry: &'a str,
}

pub fn extract(
    store: &Store,
    repository: &Repository,
    commit: &str,
    declarations_repository: &str,
    source_entry: &str,
    node: &Path,
    helper: &Path,
) -> Result<ApiSnapshot> {
    crate::scip::validate_path(source_entry)?;
    let producer = store.snapshot(&repository.id, commit)?;
    let declarations = store.snapshot(declarations_repository, commit)?;
    let revision = Revision::open(&repository.path, commit)?;
    let files = read_package(&revision)?;
    let evidence = run_helper(
        &Input {
            files: &files,
            source_entry,
        },
        node,
        helper,
    )?;
    ensure!(
        evidence.files.len() == files.len(),
        "compiler helper returned incomplete file evidence"
    );
    for file in &evidence.files {
        let text = files
            .get(&file.path)
            .context("compiler helper referenced an unknown file")?;
        ensure!(
            format!("{:x}", Sha256::digest(text.as_bytes())) == file.sha256,
            "compiler file digest mismatch"
        );
    }
    bind(store, evidence, &producer, &declarations)
}

fn read_package(revision: &Revision) -> Result<BTreeMap<String, String>> {
    let prefix = if revision.prefix.is_empty() {
        String::new()
    } else {
        format!("{}/", revision.prefix)
    };
    let mut files = BTreeMap::new();
    let mut total = 0;
    for path in revision.files()? {
        let relative = path
            .strip_prefix(&prefix)
            .context("Git path outside registered package")?;
        if !relative.ends_with(".ts") && !matches!(relative, "package.json" | "tsconfig.json") {
            continue;
        }
        ensure!(
            files.len() < 256,
            "API extraction supports at most 256 input files"
        );
        let bytes = revision
            .read(&path)?
            .context("committed API input disappeared")?;
        total += bytes.len();
        ensure!(
            total <= MAX_INPUT_BYTES,
            "API package input exceeds {MAX_INPUT_BYTES} bytes"
        );
        files.insert(
            relative.into(),
            String::from_utf8(bytes).context("API input must be UTF-8")?,
        );
    }
    Ok(files)
}

fn run_helper(input: &Input<'_>, node: &Path, helper: &Path) -> Result<Extraction> {
    let mut errors = NamedTempFile::new()?;
    let mut child = Command::new(node)
        .arg(helper)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(errors.as_file().try_clone()?)
        .spawn()
        .context("start TypeScript helper; install npm tooling and supply --helper")?;
    let written = serde_json::to_writer(child.stdin.take().context("capture helper stdin")?, input);
    if let Err(error) = written {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error).context("write TypeScript helper input");
    }
    let mut bytes = Vec::new();
    let read = child
        .stdout
        .take()
        .context("capture helper stdout")?
        .take((MAX_INPUT_BYTES + 1) as u64)
        .read_to_end(&mut bytes);
    if read.is_err() || bytes.len() > MAX_INPUT_BYTES {
        let _ = child.kill();
        let _ = child.wait();
        read.context("read TypeScript helper output")?;
        anyhow::bail!("TypeScript helper output exceeds {MAX_INPUT_BYTES} bytes");
    }
    let status = child.wait()?;
    if !status.success() {
        errors.as_file_mut().rewind()?;
        let mut diagnostic = String::new();
        errors
            .as_file_mut()
            .take(65536)
            .read_to_string(&mut diagnostic)?;
        anyhow::bail!("TypeScript helper failed ({status}): {diagnostic}");
    }
    serde_json::from_slice(&bytes).context("decode TypeScript helper evidence")
}

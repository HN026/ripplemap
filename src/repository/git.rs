use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, Result, ensure};

use super::validate_commit;

const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;

/// Reads committed blobs only; never checks out a revision or executes project code.
pub struct Revision {
    root: PathBuf,
    pub prefix: String,
    commit: String,
}

fn command(path: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .args(["--no-optional-locks", "-C"])
        .arg(path)
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE");
    command
}

impl Revision {
    pub fn check_checkout(path: &Path) -> Result<PathBuf> {
        let output = command(path)
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .context("run Git")?;
        ensure!(
            output.status.success(),
            "registered path must be inside a Git checkout: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(
            PathBuf::from(String::from_utf8(output.stdout)?.trim_end_matches(['\r', '\n']))
                .canonicalize()?,
        )
    }

    pub fn open(path: &Path, commit: &str) -> Result<Self> {
        validate_commit(commit)?;
        let root = Self::check_checkout(path)?;
        let output = command(&root).args(["cat-file", "-t", commit]).output()?;
        ensure!(
            output.status.success() && output.stdout == b"commit\n",
            "exact commit {commit} is unavailable or is not a commit in {}",
            root.display()
        );
        let prefix = path
            .canonicalize()?
            .strip_prefix(&root)?
            .to_str()
            .context("repository path must be UTF-8")?
            .replace('\\', "/");
        Ok(Self {
            root,
            prefix,
            commit: commit.to_ascii_lowercase(),
        })
    }

    /// Enumerate tracked package files in the exact revision, never the working tree.
    pub fn files(&self) -> Result<Vec<String>> {
        let mut command = command(&self.root);
        command.args([
            "--literal-pathspecs",
            "ls-tree",
            "-rz",
            "--name-only",
            &self.commit,
        ]);
        if !self.prefix.is_empty() {
            command.args(["--", &self.prefix]);
        }
        let bytes = bounded_output(command)?;
        let paths = bytes
            .split(|b| *b == 0)
            .filter(|p| !p.is_empty())
            .map(|p| String::from_utf8(p.to_vec()).context("Git paths must be UTF-8"))
            .collect::<Result<Vec<_>>>()?;
        Ok(paths)
    }

    pub fn read(&self, path: &str) -> Result<Option<Vec<u8>>> {
        crate::scip::validate_path(path)?;
        let entry = command(&self.root)
            .args([
                "--literal-pathspecs",
                "ls-tree",
                "-z",
                &self.commit,
                "--",
                path,
            ])
            .output()?;
        ensure!(
            entry.status.success(),
            "inspect committed path {path:?}: {}",
            String::from_utf8_lossy(&entry.stderr)
        );
        if entry.stdout.is_empty() {
            return Ok(None);
        }
        ensure!(
            entry.stdout.starts_with(b"100644 blob ") || entry.stdout.starts_with(b"100755 blob "),
            "dependency evidence must be a regular committed file: {path:?}"
        );
        let mut command = command(&self.root);
        command.args(["cat-file", "blob", &format!("{}:{path}", self.commit)]);
        bounded_output(command)
            .with_context(|| format!("read committed file {path:?}"))
            .map(Some)
    }
}

fn bounded_output(mut command: Command) -> Result<Vec<u8>> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("run Git")?;
    let mut bytes = Vec::new();
    let read = child
        .stdout
        .take()
        .context("capture Git stdout")?
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes);
    if read.is_err() || bytes.len() as u64 > MAX_FILE_BYTES {
        let _ = child.kill();
        let _ = child.wait();
        read.context("read Git output")?;
        anyhow::bail!("Git output exceeds {MAX_FILE_BYTES} bytes");
    }
    let output = child.wait_with_output()?;
    ensure!(
        output.status.success(),
        "Git read failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(bytes)
}

use super::Output;
use crate::health::{Compiler, CompilerEvidence};
use anyhow::{Context, Result, ensure};
use std::{
    io::{Read, Seek, Write},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use tempfile::NamedTempFile;

/**
 * Execute the trusted helper with bounded files for stdin, stdout, and stderr. Files
 * avoid pipe deadlocks before the helper consumes its input.
 */
pub(super) fn invoke(
    settings: &Compiler,
    input: &serde_json::Value,
    evidence: &mut CompilerEvidence,
) -> Result<Output> {
    let mut stdout = NamedTempFile::new()?;
    let mut stderr = NamedTempFile::new()?;
    let mut stdin = NamedTempFile::new()?;
    serde_json::to_writer(stdin.as_file_mut(), input)?;
    stdin.as_file_mut().flush()?;
    stdin.as_file_mut().rewind()?;
    let mut child = Command::new(&settings.node)
        .arg(&settings.helper)
        .stdin(stdin.as_file().try_clone()?)
        .stdout(Stdio::from(stdout.as_file().try_clone()?))
        .stderr(Stdio::from(stderr.as_file().try_clone()?))
        .spawn()
        .context("start explicitly enabled TypeScript helper")?;
    let status = wait_for_completion(&mut child, settings, &stdout, &stderr)?;
    evidence.exit_code = status.code();
    decode_output(&mut stdout, &mut stderr, status)
}

/**
 * Poll helper completion within time/output bounds, killing and reaping it on failure.
 */
fn wait_for_completion(
    child: &mut std::process::Child,
    settings: &Compiler,
    stdout: &NamedTempFile,
    stderr: &NamedTempFile,
) -> Result<std::process::ExitStatus> {
    let started = Instant::now();
    let waited = (|| -> Result<_> {
        loop {
            if let Some(status) = child.try_wait()? {
                return Ok(status);
            }
            ensure!(
                started.elapsed() <= Duration::from_secs(u64::from(settings.timeout_seconds))
                    && stdout.as_file().metadata()?.len() <= 8 * 1024 * 1024
                    && stderr.as_file().metadata()?.len() <= 65536,
                "compiler exceeded timeout or output limit"
            );
            thread::sleep(Duration::from_millis(20));
        }
    })();
    match waited {
        Ok(status) => Ok(status),
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(error)
        }
    }
}

/**
 * Validate helper termination and output bounds before decoding its structured result.
 */
fn decode_output(
    stdout: &mut NamedTempFile,
    stderr: &mut NamedTempFile,
    status: std::process::ExitStatus,
) -> Result<Output> {
    ensure!(
        stdout.as_file().metadata()?.len() <= 8 * 1024 * 1024
            && stderr.as_file().metadata()?.len() <= 65536,
        "compiler output exceeds limit"
    );
    stdout.as_file_mut().rewind()?;
    stderr.as_file_mut().rewind()?;
    let mut error = String::new();
    stderr.read_to_string(&mut error)?;
    ensure!(
        status.success(),
        "compiler helper failed ({status}): {error}"
    );
    serde_json::from_reader(stdout.as_file_mut()).context("decode TypeScript helper result")
}

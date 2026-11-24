use super::{Compiler, CompilerEvidence, Diagnostic, Report, Status};
use crate::{
    dependencies::DependencyReport,
    npm,
    repository::{Repository, Revision},
};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::json;
use std::{
    io::{Read, Seek, Write},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use tempfile::NamedTempFile;

#[derive(Deserialize)]
struct Output {
    status: String,
    message: String,
    typescript_version: String,
    diagnostics: Vec<CompilerDiagnostic>,
    diagnostics_total: usize,
    files: Vec<npm::EvidenceFile>,
}
#[derive(Deserialize)]
struct CompilerDiagnostic {
    code: u32,
    dependency: Option<String>,
    path: Option<String>,
    start: Option<crate::impact::Position>,
    message: String,
}

pub(super) fn check(
    repository: &Repository,
    revision: &Revision,
    saved: &DependencyReport,
    settings: &Compiler,
    report: &mut Report,
) {
    let command = vec![
        settings.node.to_string_lossy().into_owned(),
        settings.helper.to_string_lossy().into_owned(),
    ];
    report.compiler = Some(CompilerEvidence {
        command,
        exit_code: None,
        typescript_version: None,
        diagnostics_total: 0,
        files: vec![],
    });
    match execute(repository, revision, saved, settings, report) {
        Ok(()) => {}
        Err(error) => report.diagnostics.push(Diagnostic::new(
            "typescript_compiler",
            "compiler_check_failed",
            Status::CheckFailed,
            format!("{error:#}"),
        )),
    }
}

fn execute(
    repository: &Repository,
    revision: &Revision,
    saved: &DependencyReport,
    settings: &Compiler,
    report: &mut Report,
) -> Result<()> {
    if !revision.clean_checkout()? {
        report.diagnostics.push(Diagnostic::new("typescript_compiler", "checkout_not_exact", Status::Unknown, "Compiler requires a clean checkout at the explicitly requested commit; no compiler was run"));
        return Ok(());
    }
    let lockfile =
        saved.evidence.files.iter().find(|f| {
            f.path.ends_with("package-lock.json") || f.path.ends_with("npm-shrinkwrap.json")
        });
    let Some(lockfile) = lockfile else {
        report.diagnostics.push(Diagnostic::new(
            "typescript_compiler",
            "missing_lockfile",
            Status::Unknown,
            "Compiler package-consistency evidence requires a supported lockfile",
        ));
        return Ok(());
    };
    let lock_root = revision.root().join(
        std::path::Path::new(&lockfile.path)
            .parent()
            .unwrap_or(std::path::Path::new("")),
    );
    let input = json!({ "project": repository.path.canonicalize()?, "lock_root": lock_root, "packages": saved.evidence.packages });
    let output = invoke(settings, &input, report.compiler.as_mut().unwrap())?;
    if !revision.clean_checkout()? {
        report.diagnostics.push(Diagnostic::new(
            "typescript_compiler",
            "checkout_changed",
            Status::Unknown,
            "Checkout changed during compiler validation; retry",
        ));
        return Ok(());
    }
    let compiler = report.compiler.as_mut().unwrap();
    compiler.typescript_version = Some(output.typescript_version.clone());
    compiler.files = output.files;
    compiler.diagnostics_total = output.diagnostics_total;
    ensure!(
        output.diagnostics.len() == output.diagnostics_total.min(1000),
        "invalid compiler diagnostics count"
    );
    if output.status == "unknown" {
        report.diagnostics.push(Diagnostic::new(
            "typescript_compiler",
            "compiler_evidence_unknown",
            Status::Unknown,
            output.message,
        ));
    } else {
        ensure!(
            output.status == "completed",
            "unsupported compiler output status"
        );
        ensure!(
            output.typescript_version == "6.0.3" && !compiler.files.is_empty(),
            "unsupported compiler version or missing input evidence"
        );
        if output.diagnostics_total == 0 {
            report.diagnostics.push(Diagnostic::new(
                "typescript_compiler",
                "compiler_passed",
                Status::Healthy,
                output.message,
            ));
        }
        for diagnostic in output.diagnostics {
            let missing_export = matches!(diagnostic.code, 2305 | 2724);
            let mut result = Diagnostic::new(
                if missing_export {
                    "missing_exports"
                } else {
                    "typescript_compiler"
                },
                &format!("TS{}", diagnostic.code),
                Status::Incompatible,
                diagnostic.message,
            );
            result.dependency = diagnostic.dependency;
            result.path = diagnostic.path;
            result.start = diagnostic.start;
            report.diagnostics.push(result);
        }
        if compiler.diagnostics_total > 1000 {
            report.diagnostics.push(Diagnostic::new(
                "typescript_compiler",
                "compiler_diagnostics_truncated",
                Status::Unknown,
                "More than 1000 compiler errors; see compiler.diagnostics_total",
            ));
        }
    }
    Ok(())
}

fn invoke(
    settings: &Compiler,
    input: &serde_json::Value,
    evidence: &mut CompilerEvidence,
) -> Result<Output> {
    let mut stdout = NamedTempFile::new()?;
    let mut stderr = NamedTempFile::new()?;
    // Files avoid pipe deadlocks when a helper emits diagnostics before consuming input.
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
    let status = match waited {
        Ok(status) => status,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    evidence.exit_code = status.code();
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

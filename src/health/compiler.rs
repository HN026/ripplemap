mod process;

use super::{Compiler, CompilerEvidence, Diagnostic, Report, Status};
use crate::{
    dependencies::DependencyReport,
    npm,
    repository::{Repository, Revision},
};
use anyhow::{Result, ensure};
use serde::Deserialize;
use serde_json::json;

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

/**
 * Initialize compiler evidence and convert execution errors into health diagnostics.
 */
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

/**
 * Coordinate checkout readiness, bounded helper execution, and result interpretation.
 */
fn execute(
    repository: &Repository,
    revision: &Revision,
    saved: &DependencyReport,
    settings: &Compiler,
    report: &mut Report,
) -> Result<()> {
    let Some(input) = prepare_input(repository, revision, saved, report)? else {
        return Ok(());
    };
    let output = process::invoke(settings, &input, report.compiler.as_mut().unwrap())?;
    if !revision.clean_checkout()? {
        report.diagnostics.push(Diagnostic::new(
            "typescript_compiler",
            "checkout_changed",
            Status::Unknown,
            "Checkout changed during compiler validation; retry",
        ));
        return Ok(());
    }
    apply_output(output, report)
}

/**
 * Require the exact clean checkout and construct explicit locked package inputs.
 */
fn prepare_input(
    repository: &Repository,
    revision: &Revision,
    saved: &DependencyReport,
    report: &mut Report,
) -> Result<Option<serde_json::Value>> {
    if !revision.clean_checkout()? {
        report.diagnostics.push(Diagnostic::new("typescript_compiler", "checkout_not_exact", Status::Unknown, "Compiler requires a clean checkout at the explicitly requested commit; no compiler was run"));
        return Ok(None);
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
        return Ok(None);
    };
    let lock_root = revision.root().join(
        std::path::Path::new(&lockfile.path)
            .parent()
            .unwrap_or(std::path::Path::new("")),
    );
    let input = json!({ "project": repository.path.canonicalize()?, "lock_root": lock_root, "packages": saved.evidence.packages });
    Ok(Some(input))
}

/**
 * Validate compiler evidence and record completed checks, errors, or unsupported
 * inputs.
 */
fn apply_output(output: Output, report: &mut Report) -> Result<()> {
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
        report
            .diagnostics
            .extend(output.diagnostics.into_iter().map(compiler_diagnostic));
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

/**
 * Convert one TypeScript error into a source-linked health diagnostic.
 */
fn compiler_diagnostic(diagnostic: CompilerDiagnostic) -> Diagnostic {
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
    result
}

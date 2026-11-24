use crate::{impact::Position, npm, store::Snapshot};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Healthy,
    Unknown,
    Incompatible,
    CheckFailed,
}

impl Status {
    pub fn exit_code(self) -> u8 {
        match self {
            Self::Healthy => 0,
            Self::CheckFailed => 1,
            Self::Incompatible => 3,
            Self::Unknown => 4,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Diagnostic {
    pub check: String,
    pub code: String,
    pub status: Status,
    pub message: String,
    pub dependency: Option<String>,
    pub path: Option<String>,
    pub start: Option<Position>,
    pub evidence: Vec<npm::Evidence>,
}

impl Diagnostic {
    pub(super) fn new(check: &str, code: &str, status: Status, message: impl Into<String>) -> Self {
        Self {
            check: check.into(),
            code: code.into(),
            status,
            message: message.into(),
            dependency: None,
            path: None,
            start: None,
            evidence: vec![],
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct DependencyResult {
    pub dependency: npm::Dependency,
    pub resolved: Option<npm::Package>,
    pub status: Status,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone)]
pub struct Compiler {
    pub node: PathBuf,
    pub helper: PathBuf,
    pub timeout_seconds: u32,
}

pub struct Options {
    pub max_index_age_days: u32,
    pub limit: u32,
    pub compiler: Option<Compiler>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct CompilerEvidence {
    pub command: Vec<String>,
    pub exit_code: Option<i32>,
    pub typescript_version: Option<String>,
    pub diagnostics_total: usize,
    pub files: Vec<npm::EvidenceFile>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Report {
    pub format_version: u32,
    pub repository: String,
    pub commit: String,
    pub evaluated_at: String,
    pub snapshot: Option<Snapshot>,
    pub profile: String,
    pub enabled_checks: Vec<String>,
    pub disabled_checks: Vec<String>,
    pub max_index_age_days: u32,
    pub status: Status,
    pub diagnostics: Vec<Diagnostic>,
    pub dependencies: Vec<DependencyResult>,
    pub files: Vec<npm::EvidenceFile>,
    pub compiler: Option<CompilerEvidence>,
    pub diagnostics_total: usize,
    pub truncated: bool,
    pub scope_notes: Vec<String>,
}

impl Report {
    pub fn exit_code(&self) -> u8 {
        self.status.exit_code()
    }
    pub(super) fn finish(&mut self, limit: u32) {
        let mut installations =
            std::collections::BTreeMap::<_, std::collections::BTreeSet<_>>::new();
        for dependency in &self.dependencies {
            installations
                .entry(dependency.dependency.name.clone())
                .or_default()
                .insert(dependency.dependency.target.clone());
        }
        for dependency in &mut self.dependencies {
            let ambiguous = installations[&dependency.dependency.name].len() > 1;
            if ambiguous
                && self.diagnostics.iter().any(|d| {
                    d.dependency.as_deref() == Some(&dependency.dependency.name)
                        && d.code.starts_with("TS")
                })
            {
                dependency.diagnostics.push(Diagnostic::new("typescript_compiler", "ambiguous_installation", Status::Unknown,
                    "Compiler finding names this package, but multiple installation paths exist; no individual copy is identified"));
            }
            dependency.status = self
                .diagnostics
                .iter()
                .filter(|d| d.dependency.as_deref() == Some(&dependency.dependency.name))
                .map(|d| {
                    if ambiguous && d.code.starts_with("TS") {
                        Status::Unknown
                    } else {
                        d.status
                    }
                })
                .chain(dependency.diagnostics.iter().map(|d| d.status))
                .max()
                .unwrap();
        }
        self.status = self
            .diagnostics
            .iter()
            .chain(self.dependencies.iter().flat_map(|d| &d.diagnostics))
            .map(|d| d.status)
            .max()
            .unwrap_or(Status::Unknown);
        self.diagnostics_total = self.diagnostics.len()
            + self
                .dependencies
                .iter()
                .map(|d| d.diagnostics.len())
                .sum::<usize>();
        let mut remaining = limit as usize;
        self.diagnostics.truncate(remaining);
        remaining -= self.diagnostics.len();
        for dependency in &mut self.dependencies {
            dependency.diagnostics.truncate(remaining);
            remaining -= dependency.diagnostics.len();
        }
        self.truncated = self.diagnostics_total > limit as usize;
    }
}

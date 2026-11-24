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
    /**
     * Map the health outcome to its documented command exit code.
     */
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
    /**
     * Create a structured diagnostic with empty optional location and dependency
     * evidence.
     */
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
    /**
     * Map the health outcome to its documented command exit code.
     */
    pub fn exit_code(&self) -> u8 {
        self.status.exit_code()
    }
}

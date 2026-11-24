use super::Comparison;
use crate::{api::ApiSnapshot, impact::Usage, store::Snapshot};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct Consumer {
    pub repository: String,
    pub commit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ConsumerManifest {
    pub schema_version: u32,
    pub consumers: Vec<Consumer>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Options {
    pub limit: u32,
    pub max_index_age_days: u32,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Coverage {
    pub requested: Consumer,
    pub snapshot: Option<Snapshot>,
    pub freshness: String,
    pub age_seconds: Option<i64>,
    pub observed_package_versions: Vec<String>,
    pub versions_truncated: bool,
    pub other_version_references: i64,
    pub unmapped_baseline_references: i64,
    pub local_references: i64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Finding {
    pub export: String,
    pub observed_repositories: usize,
    pub candidate_repositories: usize,
    pub observed_references: i64,
    pub candidate_references: i64,
    pub references: Vec<Usage>,
    pub truncated: bool,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Report {
    pub format_version: u32,
    pub analysis_version: String,
    pub evaluated_at: String,
    pub options: Options,
    pub base: ApiSnapshot,
    pub head: ApiSnapshot,
    pub consumers: ConsumerManifest,
    pub coverage: Vec<Coverage>,
    pub comparison: Comparison,
    pub findings: Vec<Finding>,
    pub observed_repositories: usize,
    pub candidate_repositories: usize,
    pub truncated: bool,
    pub scope_notes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct SavedRun {
    pub id: i64,
    pub report: Report,
}

impl SavedRun {
    /**
     * Translate observed impact and evidence gaps into the documented analysis exit
     * code.
     */
    pub fn exit_code(&self) -> u8 {
        if self.report.observed_repositories > 0 {
            3
        } else if self.report.candidate_repositories > 0
            || !self.report.comparison.issues.is_empty()
            || self.report.coverage.iter().any(|c| {
                c.freshness != "within_age_threshold"
                    || c.other_version_references > 0
                    || c.unmapped_baseline_references > 0
            })
            || self.report.consumers.consumers.is_empty()
        {
            4
        } else {
            0
        }
    }
}

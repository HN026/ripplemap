use serde::{Deserialize, Serialize};

use crate::{impact::UsageReport, store::Snapshot};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Resolved,
    Ambiguous,
    Unsupported,
    Missing,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub manager: String,
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Site {
    pub path: String,
    /// Zero-based, end-exclusive UTF-16 coordinates from the compiler.
    pub range: [i32; 4],
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FileEvidence {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Export {
    pub name: String,
    pub status: Status,
    pub explanation: String,
    pub source: Option<Site>,
    pub declaration: Option<Site>,
    #[serde(default)]
    pub signature: Option<Signature>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Signature {
    pub required_parameters: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Extraction {
    pub format_version: u32,
    pub typescript_version: String,
    pub package: Package,
    pub entrypoint: String,
    pub source_entry: String,
    pub declaration_entry: String,
    pub files: Vec<FileEvidence>,
    pub exports: Vec<Export>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Mapping {
    pub name: String,
    pub status: Status,
    pub explanation: String,
    pub producer_symbols: Vec<String>,
    pub declaration_symbols: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ApiSnapshot {
    pub producer: Snapshot,
    pub declarations: Snapshot,
    pub evidence: Extraction,
    pub mappings: Vec<Mapping>,
}

#[derive(Debug, Serialize)]
pub struct Resolution {
    pub status: Status,
    pub explanation: String,
    pub raw_symbol: String,
    pub package: Package,
    pub entrypoint: String,
    pub producer: Snapshot,
    pub declarations: Snapshot,
    pub consumer: Snapshot,
    pub candidates: Vec<Mapping>,
    pub usages: Option<UsageReport>,
}

#[derive(Debug, Serialize)]
pub struct Bridge {
    pub status: Status,
    pub explanation: String,
    pub export: String,
    pub base_package: Package,
    pub head_package: Package,
    pub base: Snapshot,
    pub head: Snapshot,
    pub base_mapping: Option<Mapping>,
    pub head_mapping: Option<Mapping>,
}

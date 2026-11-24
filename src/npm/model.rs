use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Production,
    Development,
    Peer,
    Optional,
}

impl Kind {
    /**
     * Return the package.json field corresponding to a dependency kind.
     */
    pub fn field(&self) -> &'static str {
        match self {
            Self::Production => "dependencies",
            Self::Development => "devDependencies",
            Self::Peer => "peerDependencies",
            Self::Optional => "optionalDependencies",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Resolved,
    Unresolved,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Evidence {
    pub file: String,
    pub pointer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvidenceFile {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Package {
    pub location: String,
    pub name: Option<String>,
    pub version: Option<String>,
    pub workspace: bool,
    pub evidence: Evidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Dependency {
    pub from: String,
    pub name: String,
    pub requirement: String,
    pub kind: Kind,
    pub direct: bool,
    pub optional_peer: bool,
    pub target: Option<String>,
    pub status: Status,
    pub explanation: String,
    pub declared_at: Evidence,
    pub resolved_at: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Issue {
    pub code: String,
    pub message: String,
    pub evidence: Evidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct EvidenceSet {
    pub files: Vec<EvidenceFile>,
    pub packages: Vec<Package>,
    pub dependencies: Vec<Dependency>,
    pub issues: Vec<Issue>,
}

/**
 * Escape a value for use as one RFC 6901 JSON pointer segment.
 */
pub fn pointer(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

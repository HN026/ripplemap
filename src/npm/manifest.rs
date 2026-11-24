use std::collections::BTreeMap;

use anyhow::{Result, ensure};
use serde::Deserialize;
use serde_json::Value;

use super::model::Kind;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Manifest {
    pub name: Option<String>,
    pub version: Option<String>,
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
    #[serde(default)]
    pub dev_dependencies: BTreeMap<String, String>,
    #[serde(default)]
    pub peer_dependencies: BTreeMap<String, String>,
    #[serde(default)]
    pub optional_dependencies: BTreeMap<String, String>,
    #[serde(default)]
    pub peer_dependencies_meta: BTreeMap<String, PeerMeta>,
    #[serde(default)]
    pub workspaces: Value,
    #[serde(default)]
    pub overrides: Value,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(super) struct PeerMeta {
    #[serde(default)]
    pub optional: bool,
}

impl Manifest {
    pub fn declarations(&self) -> impl Iterator<Item = (Kind, &String, &String)> {
        [
            (Kind::Production, &self.dependencies),
            (Kind::Development, &self.dev_dependencies),
            (Kind::Peer, &self.peer_dependencies),
            (Kind::Optional, &self.optional_dependencies),
        ]
        .into_iter()
        .flat_map(move |(kind, entries)| {
            entries.iter().filter_map(move |(name, requirement)| {
                // npm optionalDependencies overrides dependencies with the same key.
                if kind == Kind::Production && self.optional_dependencies.contains_key(name) {
                    return None;
                }
                Some((kind.clone(), name, requirement))
            })
        })
    }

    pub fn workspace_patterns(&self) -> Result<Vec<String>> {
        let value = match &self.workspaces {
            Value::Null => return Ok(Vec::new()),
            Value::Object(object) => object.get("packages").unwrap_or(&Value::Null),
            value => value,
        };
        let patterns = value
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("unsupported workspaces declaration"))?;
        patterns
            .iter()
            .map(|value| {
                let pattern = value
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("workspace pattern must be a string"))?;
                ensure!(
                    !pattern.is_empty()
                        && pattern.split('/').all(|s| s == "*"
                            || (!s.is_empty()
                                && s != "."
                                && s != ".."
                                && !s.contains(['*', '?', '[', ']', '{', '}', '!', '\\', ':']))),
                    "unsupported workspace pattern {pattern:?}"
                );
                Ok(pattern.into())
            })
            .collect()
    }
}

// shortcut: literal segments and whole-segment * only, extend for other npm workspace patterns.
pub(super) fn workspace_match(pattern: &str, path: &str) -> bool {
    let pattern: Vec<_> = pattern.split('/').collect();
    let path: Vec<_> = path.split('/').collect();
    pattern.len() == path.len() && pattern.iter().zip(path).all(|(p, s)| *p == "*" || *p == s)
}

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

use super::{
    manifest::Manifest,
    model::{Evidence, Package, Status, pointer},
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Lockfile {
    pub lockfile_version: u32,
    #[serde(default)]
    pub packages: BTreeMap<String, LockedPackage>,
}

#[derive(Debug, Deserialize)]
pub(super) struct LockedPackage {
    #[serde(default)]
    pub link: bool,
    pub resolved: Option<String>,
    #[serde(flatten)]
    pub manifest: Manifest,
}

impl Lockfile {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.packages.is_empty() && self.packages.contains_key(""),
            "npm lockfile has no root package entry"
        );
        for path in self.packages.keys().filter(|p| !p.is_empty()) {
            crate::scip::validate_path(path)?;
        }
        Ok(())
    }

    pub fn package(&self, path: &str, file: &str, workspaces: &BTreeSet<String>) -> Package {
        let entry = &self.packages[path];
        let name = entry.manifest.name.clone().or_else(|| {
            path.rsplit_once("node_modules/")
                .map(|(_, name)| name.to_string())
        });
        Package {
            location: path.into(),
            name,
            version: entry.manifest.version.clone(),
            workspace: workspaces.contains(path),
            evidence: Evidence {
                file: file.into(),
                pointer: format!("/packages/{}", pointer(path)),
            },
        }
    }

    pub fn resolve(
        &self,
        from: &str,
        name: &str,
        workspaces: &BTreeSet<String>,
    ) -> Result<(String, Vec<String>), (Status, String)> {
        if !valid_name(name) {
            return Err((Status::Unsupported, "unsupported dependency name".into()));
        }
        let mut directory = from;
        let mut found = None;
        loop {
            if directory.rsplit('/').next() != Some("node_modules") {
                let candidate = if directory.is_empty() {
                    format!("node_modules/{name}")
                } else {
                    format!("{directory}/node_modules/{name}")
                };
                if self.packages.contains_key(&candidate) {
                    found = Some(candidate);
                    break;
                }
            }
            if directory.is_empty() {
                break;
            }
            directory = directory.rsplit_once('/').map_or("", |(parent, _)| parent);
        }
        let mut path = found.ok_or_else(|| {
            (
                Status::Unresolved,
                "no matching installation path in npm lockfile".into(),
            )
        })?;
        let mut visited = BTreeSet::new();
        let mut locations = Vec::new();
        loop {
            if !visited.insert(path.clone()) {
                return Err((Status::Unsupported, "cyclic package link".into()));
            }
            locations.push(path.clone());
            let entry = self.packages.get(&path).ok_or_else(|| {
                (
                    Status::Unresolved,
                    "package link target missing from lockfile".into(),
                )
            })?;
            if !entry.link {
                break;
            }
            let target = entry
                .resolved
                .as_deref()
                .ok_or_else(|| (Status::Unresolved, "package link has no target".into()))?;
            if !workspaces.contains(target) {
                return Err((
                    Status::Unsupported,
                    "only declared in-repository workspace links are supported".into(),
                ));
            }
            path = target.into();
        }
        if self.packages[&path]
            .manifest
            .version
            .as_deref()
            .is_none_or(str::is_empty)
        {
            return Err((Status::Unresolved, "resolved package has no version".into()));
        }
        Ok((path, locations))
    }
}

fn valid_name(name: &str) -> bool {
    let parts: Vec<_> = name.split('/').collect();
    let shape = if name.starts_with('@') {
        parts.len() == 2 && parts[0].len() > 1
    } else {
        parts.len() == 1
    };
    shape
        && parts.iter().all(|p| {
            !p.is_empty()
                && *p != "."
                && *p != ".."
                && !p.contains(['\\', ':'])
                && !p.contains(char::is_control)
        })
}

pub(super) fn parse(bytes: &[u8]) -> Result<Lockfile> {
    serde_json::from_slice(bytes).context("parse npm lockfile")
}

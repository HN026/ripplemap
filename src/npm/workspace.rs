use super::{
    evidence::{issue, unresolvable},
    lockfile::Lockfile,
    manifest::{Manifest, workspace_match},
    model::*,
    source::{Source, join},
};
use anyhow::{Context, Result};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Workspace {
    pub root: String,
    pub file: String,
    pub lock: Lockfile,
    pub workspaces: BTreeSet<String>,
    pub owners: BTreeSet<String>,
    pub manifests: BTreeMap<String, Manifest>,
}

impl Workspace {
    /**
     * Resolve the supported workspace layout and committed package manifests.
     */
    pub fn load(
        source: &mut Source<'_>,
        root: String,
        file: String,
        lock: Lockfile,
        selected: &Manifest,
        result: &mut EvidenceSet,
    ) -> Result<Option<Self>> {
        let manifest_file = join(&source.revision.prefix, "package.json");
        let root_manifest_file = join(&root, "package.json");
        let root_manifest = source
            .manifest(&root_manifest_file)?
            .context("npm lockfile has no committed root package.json")?;
        if !root_manifest.overrides.is_null()
            && root_manifest
                .overrides
                .as_object()
                .is_none_or(|o| !o.is_empty())
        {
            issue(
                result,
                "unsupported_overrides",
                "npm overrides are not evaluated; declared version requirements may be overridden",
                &root_manifest_file,
            );
        }
        let workspaces = workspace_keys(&lock, &root_manifest, &root_manifest_file, result);
        let Some(owners) = owners(
            &root,
            &source.revision.prefix,
            &workspaces,
            selected,
            &manifest_file,
            result,
        )?
        else {
            return Ok(None);
        };
        let manifests = load_manifests(source, &root, root_manifest, &workspaces, &lock, result)?;
        Ok(Some(Self {
            root,
            file,
            lock,
            workspaces,
            owners,
            manifests,
        }))
    }
}

/**
 * Find locked workspace members and record unsupported or incomplete pattern coverage.
 */
fn workspace_keys(
    lock: &Lockfile,
    manifest: &Manifest,
    manifest_file: &str,
    result: &mut EvidenceSet,
) -> BTreeSet<String> {
    let patterns = match manifest.workspace_patterns() {
        Ok(patterns) => patterns,
        Err(error) => {
            issue(
                result,
                "unsupported_workspaces",
                &error.to_string(),
                manifest_file,
            );
            Vec::new()
        }
    };
    let workspaces: BTreeSet<String> = lock
        .packages
        .keys()
        .filter(|p| {
            !p.is_empty()
                && !p.split('/').any(|part| part == "node_modules")
                && patterns.iter().any(|pattern| workspace_match(pattern, p))
        })
        .cloned()
        .collect();
    for pattern in &patterns {
        if !workspaces.iter().any(|path| workspace_match(pattern, path)) {
            issue(
                result,
                "workspace_not_locked",
                &format!(
                    "Workspace pattern {pattern:?} has no entry in the lockfile; workspace discovery may be incomplete"
                ),
                manifest_file,
            );
        }
    }
    workspaces
}

/**
 * Select the registered package or all supported first-party workspace owners.
 */
fn owners(
    root: &str,
    selected_prefix: &str,
    workspaces: &BTreeSet<String>,
    selected: &Manifest,
    manifest_file: &str,
    result: &mut EvidenceSet,
) -> Result<Option<BTreeSet<String>>> {
    let selected_path = if root.is_empty() {
        selected_prefix.to_string()
    } else {
        selected_prefix
            .strip_prefix(&format!("{root}/"))
            .unwrap_or("")
            .to_string()
    };
    let owners = if selected_path.is_empty() {
        std::iter::once(String::new())
            .chain(workspaces.iter().cloned())
            .collect::<BTreeSet<_>>()
    } else if workspaces.contains(&selected_path) {
        BTreeSet::from([selected_path.clone()])
    } else {
        issue(
            result,
            "package_not_locked",
            "Registered subdirectory is not a supported workspace in this lockfile",
            manifest_file,
        );
        unresolvable(result, selected, manifest_file, Status::Unresolved);
        return Ok(None);
    };
    Ok(Some(owners))
}

/**
 * Read first-party manifests and compare their identities with lockfile metadata.
 */
fn load_manifests(
    source: &mut Source<'_>,
    root: &str,
    root_manifest: Manifest,
    workspaces: &BTreeSet<String>,
    lock: &Lockfile,
    result: &mut EvidenceSet,
) -> Result<BTreeMap<String, Manifest>> {
    if root_manifest.name != lock.packages[""].manifest.name
        || root_manifest.version != lock.packages[""].manifest.version
    {
        issue(
            result,
            "root_identity_mismatch",
            "Root manifest identity differs from lockfile metadata",
            &join(root, "package.json"),
        );
    }
    check_locked_manifest(
        &root_manifest,
        &lock.packages[""].manifest,
        &join(root, "package.json"),
        result,
    );
    let mut manifests = BTreeMap::from([(String::new(), root_manifest)]);
    for path in workspaces {
        let manifest_path = join(root, &join(path, "package.json"));
        if let Some(manifest) = source.manifest(&manifest_path)? {
            let locked = &lock.packages[path].manifest;
            check_locked_manifest(&manifest, locked, &manifest_path, result);
            if manifest.name != locked.name || manifest.version != locked.version {
                issue(
                    result,
                    "workspace_identity_mismatch",
                    "Workspace manifest identity differs from lockfile metadata",
                    &manifest_path,
                );
            }
            manifests.insert(path.clone(), manifest);
        } else {
            issue(
                result,
                "missing_workspace_manifest",
                "Locked workspace has no committed package.json",
                &manifest_path,
            );
        }
    }
    Ok(manifests)
}

/**
 * Record declaration and optional-peer metadata differences between manifest and
 * lockfile.
 */
fn check_locked_manifest(
    manifest: &Manifest,
    locked: &Manifest,
    file: &str,
    result: &mut EvidenceSet,
) {
    if manifest.declarations().ne(locked.declarations())
        || manifest.peer_dependencies.keys().any(|name| {
            manifest
                .peer_dependencies_meta
                .get(name)
                .is_some_and(|m| m.optional)
                != locked
                    .peer_dependencies_meta
                    .get(name)
                    .is_some_and(|m| m.optional)
        })
    {
        issue(
            result,
            "manifest_lock_mismatch",
            "Committed dependency declarations differ from the lockfile package entry; refresh lockfile evidence",
            file,
        );
    }
}

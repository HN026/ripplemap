use super::{evidence::declaration, model::*, source::join, workspace::Workspace};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/**
 * Traverse reachable locked packages once and collect their dependency evidence.
 */
pub(super) fn collect(workspace: &Workspace, result: &mut EvidenceSet) {
    let mut pending: VecDeque<_> = workspace.owners.iter().cloned().collect();
    let mut visited = BTreeSet::new();
    let mut packages = BTreeMap::new();
    while let Some(path) = pending.pop_front() {
        if !visited.insert(path.clone()) {
            continue;
        }
        packages.insert(
            path.clone(),
            workspace
                .lock
                .package(&path, &workspace.file, &workspace.workspaces),
        );
        collect_dependencies(workspace, &path, &mut pending, result);
    }
    result.packages = packages.into_values().collect();
}

/**
 * Read one package manifest and enqueue its supported resolved dependency targets.
 */
fn collect_dependencies(
    workspace: &Workspace,
    path: &str,
    pending: &mut VecDeque<String>,
    result: &mut EvidenceSet,
) {
    let first_party = workspace.workspaces.contains(path) || path.is_empty();
    let manifest = if first_party {
        let Some(manifest) = workspace.manifests.get(path) else {
            return;
        };
        manifest
    } else {
        &workspace.lock.packages[path].manifest
    };
    for (kind, name, requirement) in manifest.declarations() {
        if !first_party && kind == Kind::Development {
            continue;
        }
        let evidence = declaration_evidence(workspace, path, first_party, &kind, name);
        let mut dependency = declaration(
            path,
            manifest,
            kind,
            name,
            requirement,
            workspace.owners.contains(path),
            evidence,
        );
        if let Some(target) = resolve_dependency(workspace, path, first_party, &mut dependency) {
            pending.push_back(target);
        }
        result.dependencies.push(dependency);
    }
}

/**
 * Locate a declaration in its committed first-party manifest or npm lockfile entry.
 */
fn declaration_evidence(
    workspace: &Workspace,
    path: &str,
    first_party: bool,
    kind: &Kind,
    name: &str,
) -> Evidence {
    if first_party {
        Evidence {
            file: join(&workspace.root, &join(path, "package.json")),
            pointer: format!("/{}/{}", kind.field(), pointer(name)),
        }
    } else {
        Evidence {
            file: workspace.file.clone(),
            pointer: format!(
                "/packages/{}/{}/{}",
                pointer(path),
                kind.field(),
                pointer(name)
            ),
        }
    }
}

/**
 * Resolve a declaration in its installation environment and preserve failure evidence.
 * Installed packages cannot satisfy peers from their own nested node_modules.
 */
fn resolve_dependency(
    workspace: &Workspace,
    path: &str,
    first_party: bool,
    dependency: &mut Dependency,
) -> Option<String> {
    let peer_scope = if dependency.kind == Kind::Peer && !first_party {
        path.rsplit_once("/node_modules/")
            .map_or("", |(parent, _)| parent)
    } else {
        path
    };
    match workspace
        .lock
        .resolve(peer_scope, &dependency.name, &workspace.workspaces)
    {
        Ok((target, locations)) => {
            dependency.resolved_at = locations
                .iter()
                .map(|path| Evidence {
                    file: workspace.file.clone(),
                    pointer: format!("/packages/{}", pointer(path)),
                })
                .collect();
            dependency.status = Status::Resolved;
            dependency.explanation = "Resolved from committed npm lockfile installation paths; version constraints and installed files are not validated".into();
            dependency.target = Some(target.clone());
            Some(target)
        }
        Err((status, explanation)) => {
            dependency.status = status;
            dependency.explanation = explanation;
            None
        }
    }
}

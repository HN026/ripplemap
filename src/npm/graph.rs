use super::{evidence::declaration, model::*, source::join, workspace::Workspace};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

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
        let first_party = workspace.workspaces.contains(&path) || path.is_empty();
        let manifest = if first_party {
            let Some(manifest) = workspace.manifests.get(&path) else {
                continue;
            };
            manifest
        } else {
            &workspace.lock.packages[&path].manifest
        };
        for (kind, name, requirement) in manifest.declarations() {
            if !first_party && kind == Kind::Development {
                continue;
            }
            let evidence = if first_party {
                Evidence {
                    file: join(&workspace.root, &join(&path, "package.json")),
                    pointer: format!("/{}/{}", kind.field(), pointer(name)),
                }
            } else {
                Evidence {
                    file: workspace.file.clone(),
                    pointer: format!(
                        "/packages/{}/{}/{}",
                        pointer(&path),
                        kind.field(),
                        pointer(name)
                    ),
                }
            };
            let mut dependency = declaration(
                &path,
                manifest,
                kind,
                name,
                requirement,
                workspace.owners.contains(&path),
                evidence,
            );
            // Installed packages cannot satisfy peers from their own nested node_modules.
            let peer_scope = if dependency.kind == Kind::Peer && !first_party {
                path.rsplit_once("/node_modules/")
                    .map_or("", |(parent, _)| parent)
            } else {
                &path
            };
            match workspace
                .lock
                .resolve(peer_scope, name, &workspace.workspaces)
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
                    pending.push_back(target);
                }
                Err((status, explanation)) => {
                    dependency.status = status;
                    dependency.explanation = explanation;
                }
            }
            result.dependencies.push(dependency);
        }
    }
    result.packages = packages.into_values().collect();
}

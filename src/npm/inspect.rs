use anyhow::{Context, Result};

use super::{
    evidence::{finish, issue, unresolvable},
    graph, lockfile,
    model::*,
    source::{Source, join},
    workspace::Workspace,
};
use crate::repository::Revision;

/**
 * Inspect exact committed npm manifests and supported lockfiles with honest coverage
 * gaps.
 */
pub fn inspect(revision: &Revision) -> Result<EvidenceSet> {
    let mut source = Source::new(revision);
    let mut result = EvidenceSet::default();
    let manifest_file = join(&revision.prefix, "package.json");
    let Some(selected) = source.manifest(&manifest_file)? else {
        issue(
            &mut result,
            "missing_manifest",
            "No package.json at the registered path in this commit",
            &manifest_file,
        );
        return finish(source, result);
    };
    let Some((root, file, bytes)) = source.lockfile()? else {
        issue(
            &mut result,
            "missing_lockfile",
            "No npm lockfile found in the registered path or its Git ancestors; workspace and transitive coverage is unavailable",
            &manifest_file,
        );
        for directory in ancestors(&revision.prefix) {
            for name in ["pnpm-lock.yaml", "yarn.lock"] {
                let path = join(directory, name);
                if source.read(&path)?.is_some() {
                    issue(
                        &mut result,
                        "unsupported_lockfile",
                        "Only npm package-lock/shrinkwrap versions 2 and 3 are supported",
                        &path,
                    );
                }
            }
        }
        unresolvable(&mut result, &selected, &manifest_file, Status::Unresolved);
        return finish(source, result);
    };
    let lock = lockfile::parse(&bytes).with_context(|| format!("lockfile {file:?}"))?;
    if !matches!(lock.lockfile_version, 2 | 3) {
        issue(
            &mut result,
            "unsupported_lockfile_version",
            &format!("Unsupported npm lockfile version {}", lock.lockfile_version),
            &file,
        );
        unresolvable(&mut result, &selected, &manifest_file, Status::Unsupported);
        return finish(source, result);
    }
    lock.validate()?;
    let Some(workspace) = Workspace::load(&mut source, root, file, lock, &selected, &mut result)?
    else {
        return finish(source, result);
    };
    graph::collect(&workspace, &mut result);
    finish(source, result)
}

/**
 * Enumerate a selected Git-relative directory and each ancestor through the root.
 */
fn ancestors(mut directory: &str) -> Vec<&str> {
    let mut result = Vec::new();
    loop {
        result.push(directory);
        if directory.is_empty() {
            return result;
        }
        directory = directory.rsplit_once('/').map_or("", |(parent, _)| parent);
    }
}

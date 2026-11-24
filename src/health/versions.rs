use super::{DependencyResult, Diagnostic, Status};
use crate::npm::{self, Dependency, EvidenceSet, Kind};
use node_semver::{Range, Version};

/**
 * Evaluate each recorded dependency and attach declaration/resolution evidence.
 */
pub(super) fn inspect(evidence: &EvidenceSet) -> Vec<DependencyResult> {
    evidence
        .dependencies
        .iter()
        .map(|dependency| {
            let resolved = evidence
                .packages
                .iter()
                .find(|p| Some(&p.location) == dependency.target.as_ref())
                .cloned();
            let (status, code, message) = if evidence.issues.iter().any(|i| i.code == "unsupported_overrides") {
                (Status::Unknown, "unsupported_overrides", "Version requirements cannot be checked reliably in the presence of npm overrides".into())
            } else {
                evaluate(dependency, resolved.as_ref(), evidence.issues.is_empty())
            };
            let mut diagnostic = Diagnostic::new(
                if dependency.kind == Kind::Peer {
                    "peer_compatibility"
                } else {
                    "dependency_constraints"
                },
                code,
                status,
                message,
            );
            diagnostic.dependency = Some(dependency.name.clone());
            diagnostic.path = Some(dependency.declared_at.file.clone());
            diagnostic.evidence = std::iter::once(dependency.declared_at.clone())
                .chain(dependency.resolved_at.iter().cloned())
                .collect();
            DependencyResult {
                dependency: dependency.clone(),
                resolved,
                status,
                diagnostics: vec![diagnostic],
            }
        })
        .collect()
}

/**
 * Check a resolved package against its declared npm identity and version requirement.
 */
fn evaluate(
    dependency: &Dependency,
    resolved: Option<&npm::Package>,
    complete: bool,
) -> (Status, &'static str, String) {
    let optional = dependency.kind == Kind::Optional
        || (dependency.kind == Kind::Peer && dependency.optional_peer);
    if dependency.status == npm::Status::Unsupported {
        return (
            Status::Unknown,
            "unsupported_resolution",
            dependency.explanation.clone(),
        );
    }
    let Some(package) = resolved else {
        return missing_dependency(dependency, complete, optional);
    };
    let (name, range) = if let Some(alias) = dependency.requirement.strip_prefix("npm:") {
        match alias.rsplit_once('@') {
            Some((name, range)) if !name.is_empty() => (name, range),
            _ => {
                return (
                    Status::Unknown,
                    "unsupported_requirement",
                    "Only npm aliases with an explicit semantic-version range are supported".into(),
                );
            }
        }
    } else {
        (dependency.name.as_str(), dependency.requirement.as_str())
    };
    if package.name.as_deref() != Some(name) {
        return (
            Status::Unknown,
            "package_identity_mismatch",
            "Resolved package name does not agree with the declared package/alias".into(),
        );
    }
    let (Ok(range), Some(Ok(version))) = (
        range.parse::<Range>(),
        package.version.as_ref().map(|v| v.parse::<Version>()),
    ) else {
        return (Status::Unknown, "unsupported_requirement", "A semantic version and npm version range are required; file/git/workspace protocols and tags are unsupported".into());
    };
    if version.satisfies(&range) {
        (
            Status::Healthy,
            "constraint_satisfied",
            format!("Locked {} satisfies {}", version, dependency.requirement),
        )
    } else {
        (
            Status::Incompatible,
            "version_constraint_violated",
            format!(
                "Locked {} does not satisfy {}",
                version, dependency.requirement
            ),
        )
    }
}

/**
 * Classify an absent dependency only when the supported lockfile proves its absence.
 */
fn missing_dependency(
    dependency: &Dependency,
    complete: bool,
    optional: bool,
) -> (Status, &'static str, String) {
    if complete
        && dependency.status == npm::Status::Unresolved
        && dependency.explanation == "no matching installation path in npm lockfile"
    {
        if optional {
            (
                Status::Healthy,
                "optional_absent",
                "Optional dependency is absent from the recorded resolution".into(),
            )
        } else {
            (
                Status::Incompatible,
                "required_dependency_missing",
                "Required dependency or peer has no installation path in the supported lockfile"
                    .into(),
            )
        }
    } else {
        (
            Status::Unknown,
            "unresolved_dependency",
            dependency.explanation.clone(),
        )
    }
}

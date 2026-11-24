use super::{manifest::Manifest, model::*, source::Source};
use anyhow::Result;

/**
 * Create dependency evidence from one manifest declaration and its optional-peer
 * metadata.
 */
pub(super) fn declaration(
    from: &str,
    manifest: &Manifest,
    kind: Kind,
    name: &str,
    requirement: &str,
    direct: bool,
    declared_at: Evidence,
) -> Dependency {
    let optional_peer = kind == Kind::Peer
        && manifest
            .peer_dependencies_meta
            .get(name)
            .is_some_and(|meta| meta.optional);
    Dependency {
        from: from.into(),
        name: name.into(),
        requirement: requirement.into(),
        kind,
        direct,
        optional_peer,
        target: None,
        status: Status::Unresolved,
        explanation: String::new(),
        declared_at,
        resolved_at: Vec::new(),
    }
}

/**
 * Retain declarations as unresolved when supported lockfile evidence is unavailable.
 */
pub(super) fn unresolvable(
    result: &mut EvidenceSet,
    manifest: &Manifest,
    file: &str,
    status: Status,
) {
    result.packages.push(Package {
        location: String::new(),
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        workspace: false,
        evidence: Evidence {
            file: file.into(),
            pointer: String::new(),
        },
    });
    for (kind, name, requirement) in manifest.declarations() {
        let evidence = Evidence {
            file: file.into(),
            pointer: format!("/{}/{}", kind.field(), pointer(name)),
        };
        let mut dependency = declaration("", manifest, kind, name, requirement, true, evidence);
        dependency.status = status.clone();
        dependency.explanation = "No supported npm lockfile evidence for this declaration".into();
        result.dependencies.push(dependency);
    }
}

/**
 * Append a structured inspection issue with its committed file location.
 */
pub(super) fn issue(result: &mut EvidenceSet, code: &str, message: &str, file: &str) {
    result.issues.push(Issue {
        code: code.into(),
        message: message.into(),
        evidence: Evidence {
            file: file.into(),
            pointer: String::new(),
        },
    });
}

/**
 * Attach recorded file digests and deterministically order dependencies and issues.
 */
pub(super) fn finish(source: Source<'_>, mut result: EvidenceSet) -> Result<EvidenceSet> {
    result.files = source.files.into_values().collect();
    result
        .dependencies
        .sort_by(|a, b| (&a.from, &a.kind, &a.name).cmp(&(&b.from, &b.kind, &b.name)));
    result.issues.sort_by(|a, b| {
        (&a.evidence, &a.code, &a.message).cmp(&(&b.evidence, &b.code, &b.message))
    });
    Ok(result)
}

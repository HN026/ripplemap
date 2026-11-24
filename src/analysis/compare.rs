use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

use crate::api::{ApiSnapshot, Export, Mapping, Status};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    RemovedExport,
    RequiredParametersAdded,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Change {
    pub export: String,
    pub kind: ChangeKind,
    pub explanation: String,
    pub before: Export,
    pub after: Option<Export>,
    pub mapping: Mapping,
    pub reference_status: Status,
    pub reference_candidates: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Issue {
    pub export: String,
    pub status: Status,
    pub explanation: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Comparison {
    pub changes: Vec<Change>,
    pub issues: Vec<Issue>,
    pub retained_exports: Vec<String>,
}

pub fn compare(base: &ApiSnapshot, head: &ApiSnapshot) -> Result<Comparison> {
    ensure!(
        base.evidence.package.manager == head.evidence.package.manager
            && base.evidence.package.name == head.evidence.package.name
            && base.evidence.entrypoint == head.evidence.entrypoint,
        "base/head must describe the same package and public entry point"
    );
    let after: BTreeMap<_, _> = head.evidence.exports.iter().map(|e| (&e.name, e)).collect();
    let mut result = Comparison {
        changes: vec![],
        issues: vec![],
        retained_exports: vec![],
    };
    let before: BTreeMap<_, _> = base.evidence.exports.iter().map(|e| (&e.name, e)).collect();
    for (name, export) in before {
        let mapping = base
            .mappings
            .iter()
            .find(|m| &m.name == name)
            .ok_or_else(|| anyhow::anyhow!("missing base mapping for {name:?}"))?;
        let next = after.get(name).copied();
        let next_mapping = head.mappings.iter().find(|m| &m.name == name);
        if mapping.status != Status::Resolved
            || next_mapping.is_some_and(|m| m.status != Status::Resolved)
        {
            result.issues.push(Issue {
                export: name.clone(),
                status: if mapping.status != Status::Resolved {
                    mapping.status.clone()
                } else {
                    next_mapping.unwrap().status.clone()
                },
                explanation: "Unsupported or unresolved public definition; no automatic change classification".into(),
            });
            continue;
        }
        let kind = match next {
            None => Some(ChangeKind::RemovedExport),
            Some(next) => {
                result.retained_exports.push(name.clone());
                match (&export.signature, &next.signature) {
                    (Some(old), Some(new)) if new.required_parameters > old.required_parameters => {
                        Some(ChangeKind::RequiredParametersAdded)
                    }
                    (None, _) | (_, None) => {
                        result.issues.push(Issue {
                            export: name.clone(),
                            status: Status::Unsupported,
                            explanation: "Minimum-argument evidence unavailable; this retained binding's signature is unchecked".into(),
                        });
                        None
                    }
                    _ => None,
                }
            }
        };
        let Some(kind) = kind else { continue };
        let symbols: BTreeSet<_> = mapping.declaration_symbols.iter().collect();
        let candidates: Vec<_> = base
            .mappings
            .iter()
            .filter(|m| m.declaration_symbols.iter().any(|s| symbols.contains(s)))
            .map(|m| m.name.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        result.changes.push(Change {
            export: name.clone(),
            explanation: match kind {
                ChangeKind::RemovedExport => "A supported named public function binding is absent from the explicit head API".into(),
                ChangeKind::RequiredParametersAdded => "The supported function's minimum positional argument count increased; references are potential impact, not proof of a failing call".into(),
            },
            kind,
            before: export.clone(),
            after: next.cloned(),
            mapping: mapping.clone(),
            reference_status: if candidates.len() == 1 {
                Status::Resolved
            } else {
                Status::Ambiguous
            },
            reference_candidates: candidates,
        });
    }
    for export in &head.evidence.exports {
        if !base.evidence.exports.iter().any(|e| e.name == export.name)
            && export.status != Status::Resolved
        {
            result.issues.push(Issue {
                export: export.name.clone(),
                status: export.status.clone(),
                explanation: "Unsupported added head export; no compatibility classification"
                    .into(),
            });
        }
    }
    result.issues.sort_by(|a, b| a.export.cmp(&b.export));
    Ok(result)
}

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

use super::{manifest::Manifest, model::EvidenceFile};
use crate::repository::Revision;

pub(super) struct Source<'a> {
    pub revision: &'a Revision,
    pub files: BTreeMap<String, EvidenceFile>,
}

impl<'a> Source<'a> {
    pub fn new(revision: &'a Revision) -> Self {
        Self {
            revision,
            files: BTreeMap::new(),
        }
    }

    pub fn read(&mut self, path: &str) -> Result<Option<Vec<u8>>> {
        let bytes = self.revision.read(path)?;
        if let Some(bytes) = &bytes {
            self.files.insert(
                path.into(),
                EvidenceFile {
                    path: path.into(),
                    sha256: format!("{:x}", Sha256::digest(bytes)),
                },
            );
        }
        Ok(bytes)
    }

    pub fn manifest(&mut self, path: &str) -> Result<Option<Manifest>> {
        self.read(path)?
            .map(|bytes| {
                serde_json::from_slice(&bytes)
                    .with_context(|| format!("parse committed manifest {path:?}"))
            })
            .transpose()
    }

    pub fn lockfile(&mut self) -> Result<Option<(String, String, Vec<u8>)>> {
        let mut directory = self.revision.prefix.clone();
        loop {
            // npm-shrinkwrap takes precedence over package-lock in the same directory.
            for name in ["npm-shrinkwrap.json", "package-lock.json"] {
                let file = join(&directory, name);
                if let Some(bytes) = self.read(&file)? {
                    return Ok(Some((directory, file, bytes)));
                }
            }
            if directory.is_empty() {
                return Ok(None);
            }
            directory = directory
                .rsplit_once('/')
                .map_or("", |(parent, _)| parent)
                .into();
        }
    }
}

pub(super) fn join(parent: &str, child: &str) -> String {
    if parent.is_empty() {
        child.into()
    } else {
        format!("{parent}/{child}")
    }
}

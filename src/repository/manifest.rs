use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use super::{Revision, validate_id};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Repository {
    pub id: String,
    pub path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryManifest {
    pub schema_version: u32,
    pub repositories: Vec<Repository>,
}

impl RepositoryManifest {
    pub fn load(path: &Path) -> Result<Self> {
        let mut manifest: Self = serde_json::from_reader(
            File::open(path)
                .with_context(|| format!("open repository manifest {}", path.display()))?,
        )
        .with_context(|| format!("parse repository manifest {}", path.display()))?;
        ensure!(
            manifest.schema_version == 1,
            "unsupported repository manifest version {}",
            manifest.schema_version
        );
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let (mut ids, mut paths) = (BTreeSet::new(), BTreeSet::new());
        for repository in &mut manifest.repositories {
            validate_id(&repository.id)?;
            repository.path = parent
                .join(&repository.path)
                .canonicalize()
                .with_context(|| format!("resolve registered path for {:?}", repository.id))?;
            ensure!(
                repository.path.is_dir(),
                "registered path must be a directory"
            );
            ensure!(
                ids.insert(repository.id.clone()),
                "duplicate repository ID {:?}",
                repository.id
            );
            ensure!(
                paths.insert(repository.path.clone()),
                "duplicate registered repository path"
            );
        }
        manifest.repositories.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(manifest)
    }

    pub fn repository(&self, id: &str) -> Result<&Repository> {
        self.repositories
            .iter()
            .find(|r| r.id == id)
            .with_context(|| format!("repository {id:?} is not registered"))
    }

    pub fn register(manifest_path: &Path, id: &str, path: &Path) -> Result<Self> {
        validate_id(id)?;
        let path = path.canonicalize().context("resolve repository path")?;
        ensure!(path.is_dir(), "repository path must be a directory");
        Revision::check_checkout(&path)?;
        let mut manifest = if manifest_path.try_exists()? {
            Self::load(manifest_path)?
        } else {
            Self {
                schema_version: 1,
                repositories: Vec::new(),
            }
        };
        if let Some(existing) = manifest.repositories.iter().find(|r| r.id == id) {
            ensure!(
                existing.path == path,
                "repository {id:?} is already registered at a different path"
            );
            return Ok(manifest);
        }
        ensure!(
            !manifest.repositories.iter().any(|r| r.path == path),
            "repository path is already registered under another ID"
        );
        manifest.repositories.push(Repository {
            id: id.into(),
            path,
        });
        manifest.repositories.sort_by(|a, b| a.id.cmp(&b.id));
        let destination = if manifest_path.try_exists()? {
            manifest_path.canonicalize()?
        } else {
            manifest_path.to_path_buf()
        };
        let parent = destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut temporary = NamedTempFile::new_in(parent)?;
        serde_json::to_writer_pretty(temporary.as_file_mut(), &manifest)?;
        writeln!(temporary)?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(&destination)
            .with_context(|| format!("save repository manifest {}", destination.display()))?;
        // Persist the containing directory entry after the atomic rename.
        #[cfg(unix)]
        fs::File::open(parent)?.sync_all()?;
        Ok(manifest)
    }
}

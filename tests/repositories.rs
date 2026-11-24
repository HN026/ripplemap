use std::{fs, path::Path};

use ripplemap::repository::RepositoryManifest;
use tempfile::tempdir;

#[test]
/**
 * Verify repository manifest registration is atomic idempotent and explicit.
 */
fn repository_manifest_registration_is_atomic_idempotent_and_explicit() -> anyhow::Result<()> {
    let temporary = tempdir()?;
    let file = temporary.path().join("repositories.json");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = RepositoryManifest::register(&file, "workspace", root)?;
    assert_eq!(manifest.repositories.len(), 1);
    assert_eq!(
        RepositoryManifest::register(&file, "workspace", root)?
            .repositories
            .len(),
        1
    );
    let original = fs::read(&file)?;
    assert!(RepositoryManifest::register(&file, "another", root).is_err());
    assert!(
        RepositoryManifest::register(&file, "workspace", &root.join("fixtures/semantic/library"))
            .is_err()
    );
    assert!(RepositoryManifest::register(&file, "invalid", temporary.path()).is_err());
    assert_eq!(fs::read(&file)?, original);
    let fixture = RepositoryManifest::load(&root.join("fixtures/semantic/repositories.json"))?;
    assert!(fixture.repository("removed-consumer")?.path.is_absolute());
    assert!(fixture.repository("missing").is_err());
    fs::write(&file, r#"{"schema_version":9,"repositories":[]}"#)?;
    assert!(RepositoryManifest::load(&file).is_err());
    fs::write(&file, "broken JSON")?;
    assert!(RepositoryManifest::register(&file, "workspace", root).is_err());
    assert_eq!(fs::read_to_string(&file)?, "broken JSON");
    Ok(())
}

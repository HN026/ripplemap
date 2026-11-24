mod support;

use std::{fs, path::Path};

use ripplemap::{
    dependencies,
    npm::{self, Kind, Status},
    repository::{Repository, Revision},
    store::{SnapshotInput, Store},
};
use serde_json::{Value, json};
use tempfile::tempdir;

fn write(path: &Path, value: &Value) -> anyhow::Result<()> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn import(store: &mut Store, root: &Path, commit: &str) -> anyhow::Result<()> {
    store.ingest(
        &root.join("fixtures/semantic/removed-consumer/index.scip"),
        &SnapshotInput {
            repository: "consumer".into(),
            commit: commit.into(),
            indexed_at: "2026-11-11T17:43:32Z".into(),
        },
    )?;
    Ok(())
}

#[test]
fn npm_workspace_graph_resolves_versions_kinds_and_persists_exact_commit_evidence()
-> anyhow::Result<()> {
    let (checkout, _) = support::fixture_repository()?;
    let root = checkout.path();
    let manifest_path = root.join("fixtures/semantic/removed-consumer/package.json");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&manifest_path)?)?;
    manifest["dependencies"]["tiny"] = json!("^1");
    manifest["dependencies"]["aliased"] = json!("npm:actual@1.2.3");
    manifest["dependencies"]["maybe"] = json!("^1");
    manifest["optionalDependencies"] = json!({"maybe":"^2"});
    manifest["peerDependencies"]["absent-peer"] = json!(">=1");
    write(&manifest_path, &manifest)?;
    let lock_path = root.join("package-lock.json");
    let mut lock: Value = serde_json::from_slice(&fs::read(&lock_path)?)?;
    lock["packages"]["node_modules/tiny"] =
        json!({"version":"1.5.0","dependencies":{"nested":"^2"}});
    lock["packages"]["node_modules/nested"] = json!({"version":"9.0.0"});
    lock["packages"]["node_modules/tiny/node_modules/nested"] = json!({"version":"2.1.0"});
    lock["packages"]["node_modules/aliased"] = json!({"name":"actual","version":"1.2.3"});
    lock["packages"]["node_modules/maybe"] = json!({"version":"2.0.0"});
    write(&lock_path, &lock)?;
    let commit = support::commit(root)?;
    // Inspection must use committed files, even when both worktree files are broken.
    fs::write(&manifest_path, "invalid current manifest")?;
    fs::write(&lock_path, "invalid current lockfile")?;
    let temporary = tempdir()?;
    let database = temporary.path().join("index.sqlite3");
    let mut store = Store::open(&database)?;
    import(&mut store, root, &commit)?;
    let repository = Repository {
        id: "consumer".into(),
        path: root.join("fixtures/semantic/removed-consumer"),
    };
    let report = dependencies::scan(&mut store, &repository, &commit)?;
    let evidence = &report.evidence;
    let direct = evidence
        .dependencies
        .iter()
        .find(|d| d.name == "@ripplemap/library" && d.direct)
        .unwrap();
    assert_eq!(direct.status, Status::Resolved);
    assert_eq!(direct.kind, Kind::Production);
    assert_eq!(direct.target.as_deref(), Some("fixtures/semantic/library"));
    assert_eq!(direct.resolved_at.len(), 2);
    assert!(
        evidence
            .dependencies
            .iter()
            .any(|d| !d.direct && d.name == "@ripplemap/library")
    );
    assert!(
        evidence
            .dependencies
            .iter()
            .any(|d| d.kind == Kind::Development)
    );
    assert!(
        evidence
            .dependencies
            .iter()
            .any(|d| d.kind == Kind::Peer && d.optional_peer && d.name == "typescript")
    );
    assert!(
        evidence
            .dependencies
            .iter()
            .any(|d| d.name == "absent-peer" && d.status == Status::Unresolved)
    );
    assert_eq!(
        evidence
            .dependencies
            .iter()
            .filter(|d| d.name == "maybe")
            .count(),
        1
    );
    assert_eq!(
        evidence
            .dependencies
            .iter()
            .find(|d| d.name == "maybe")
            .unwrap()
            .kind,
        Kind::Optional
    );
    let nested = evidence
        .dependencies
        .iter()
        .find(|d| d.name == "nested")
        .unwrap();
    assert_eq!(
        nested.target.as_deref(),
        Some("node_modules/tiny/node_modules/nested")
    );
    assert_eq!(
        evidence
            .packages
            .iter()
            .find(|p| Some(&p.location) == nested.target.as_ref())
            .unwrap()
            .version
            .as_deref(),
        Some("2.1.0")
    );
    assert!(
        evidence
            .packages
            .iter()
            .any(|p| p.name.as_deref() == Some("actual") && p.version.as_deref() == Some("1.2.3"))
    );
    assert!(evidence.files.iter().all(|f| f.sha256.len() == 64));
    assert!(evidence.files.iter().any(|f| f.path == "package-lock.json"));
    assert_eq!(
        report,
        dependencies::scan(&mut store, &repository, &commit)?
    );
    let invalid_repository = Repository {
        id: "consumer".into(),
        path: temporary.path().into(),
    };
    assert!(dependencies::scan(&mut store, &invalid_repository, &commit).is_err());
    assert_eq!(store.dependencies("consumer", &commit)?, report);
    let mut broken = fs::read(root.join("fixtures/semantic/removed-consumer/index.scip"))?;
    broken.extend_from_slice(&[0x12, 5, 0]);
    let broken_path = temporary.path().join("broken.scip");
    fs::write(&broken_path, broken)?;
    assert!(
        store
            .ingest(
                &broken_path,
                &SnapshotInput {
                    repository: "consumer".into(),
                    commit: commit.clone(),
                    indexed_at: "2026-11-11T17:43:32Z".into(),
                }
            )
            .is_err()
    );
    assert_eq!(store.dependencies("consumer", &commit)?, report);
    drop(store);
    assert_eq!(
        Store::open_read_only(&database)?.dependencies("consumer", &commit)?,
        report
    );
    let mut store = Store::open(&database)?;
    import(&mut store, root, &commit)?;
    assert!(store.dependencies("consumer", &commit).is_err());
    assert!(store.snapshot("consumer", &"f".repeat(40)).is_err());
    assert!(Revision::open(&repository.path, &"f".repeat(40)).is_err());
    Ok(())
}

#[test]
fn missing_and_unsupported_npm_evidence_is_explicit_and_old_databases_are_preserved()
-> anyhow::Result<()> {
    let (checkout, _) = support::fixture_repository()?;
    let root = checkout.path();
    fs::remove_file(root.join("package-lock.json"))?;
    fs::write(root.join("yarn.lock"), "# unsupported lockfile")?;
    let commit = support::commit(root)?;
    let path = root.join("fixtures/semantic/removed-consumer");
    let evidence = npm::inspect(&Revision::open(&path, &commit)?)?;
    assert!(evidence.issues.iter().any(|i| i.code == "missing_lockfile"));
    assert!(
        evidence
            .issues
            .iter()
            .any(|i| i.code == "unsupported_lockfile")
    );
    assert!(
        evidence
            .dependencies
            .iter()
            .all(|d| d.status == Status::Unresolved && d.target.is_none())
    );
    fs::write(
        root.join("package-lock.json"),
        r#"{"lockfileVersion":1,"dependencies":{}}"#,
    )?;
    let commit = support::commit(root)?;
    let evidence = npm::inspect(&Revision::open(&path, &commit)?)?;
    assert!(
        evidence
            .issues
            .iter()
            .any(|i| i.code == "unsupported_lockfile_version")
    );
    assert!(
        evidence
            .dependencies
            .iter()
            .all(|d| d.status == Status::Unsupported)
    );
    let original: Value = serde_json::from_slice(&fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("package-lock.json"),
    )?)?;
    let mut version_two = original;
    version_two["lockfileVersion"] = json!(2);
    version_two["packages"]["node_modules/@ripplemap/library"]["resolved"] = json!("../../outside");
    write(&root.join("package-lock.json"), &version_two)?;
    let commit = support::commit(root)?;
    let evidence = npm::inspect(&Revision::open(&path, &commit)?)?;
    assert!(
        evidence
            .dependencies
            .iter()
            .any(|d| d.name == "@ripplemap/library" && d.status == Status::Unsupported)
    );
    fs::write(root.join("package-lock.json"), "broken")?;
    let commit = support::commit(root)?;
    assert!(npm::inspect(&Revision::open(&path, &commit)?).is_err());
    let temporary = tempdir()?;
    let database = temporary.path().join("old.sqlite3");
    drop(Store::open(&database)?);
    let connection = rusqlite::Connection::open(&database)?;
    for version in [1, 2] {
        connection.pragma_update(None, "user_version", version)?;
        assert!(Store::open(&database).is_err());
        assert!(Store::open_read_only(&database).is_err());
        assert_eq!(
            connection.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))?,
            version
        );
    }
    assert!(
        connection
            .query_row("SELECT count(*) FROM snapshots", [], |r| r.get::<_, i64>(0))
            .is_ok()
    );
    Ok(())
}

mod support;

use ripplemap::{
    dependencies,
    health::{self, Compiler, Options, Status},
    repository::Repository,
    store::{SnapshotInput, Store},
};
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};
use tempfile::tempdir;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

fn repository(root: &Path) -> Repository {
    Repository {
        id: "consumer".into(),
        path: root.join("fixtures/semantic/removed-consumer"),
    }
}
fn options() -> Options {
    Options {
        limit: 1000,
        max_index_age_days: 30,
        compiler: None,
    }
}
fn ingest(store: &mut Store, root: &Path, commit: &str) -> anyhow::Result<()> {
    store.ingest(
        &root.join("fixtures/semantic/removed-consumer/index.scip"),
        &SnapshotInput {
            repository: "consumer".into(),
            commit: commit.into(),
            indexed_at: OffsetDateTime::now_utc().format(&Rfc3339)?,
        },
    )?;
    dependencies::scan(store, &repository(root), commit)?;
    Ok(())
}
fn write(path: &Path, value: &Value) -> anyhow::Result<()> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

#[test]
fn health_checks_npm_ranges_and_peer_environment_with_versioned_evidence() -> anyhow::Result<()> {
    let (checkout, original) = support::fixture_repository()?;
    let root = checkout.path();
    let temporary = tempdir()?;
    let mut store = Store::open(&temporary.path().join("index.sqlite3"))?;
    ingest(&mut store, root, &original)?;
    let healthy = health::check(&store, &repository(root), &original, &options())?;
    assert_eq!(
        healthy.status,
        Status::Healthy,
        "{}",
        serde_json::to_string_pretty(&healthy)?
    );
    assert_eq!(healthy.exit_code(), 0);
    assert_eq!(healthy.profile, "dependencies-v1");
    assert!(healthy.disabled_checks.contains(&"missing_exports".into()));
    let manifest_path = root.join("fixtures/semantic/removed-consumer/package.json");
    let lock_path = root.join("package-lock.json");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&manifest_path)?)?;
    let mut lock: Value = serde_json::from_slice(&fs::read(&lock_path)?)?;
    manifest["dependencies"]["plugin"] = json!("^1 || ^2");
    manifest["dependencies"]["alias"] = json!("npm:actual@~1.2");
    manifest["dependencies"]["prerelease"] = json!("^1.0.0");
    manifest["peerDependencies"]["absent"] = json!(">=1");
    manifest["peerDependencies"]["optional-absent"] = json!("*");
    manifest["peerDependenciesMeta"]["optional-absent"] = json!({"optional":true});
    lock["packages"]["fixtures/semantic/removed-consumer"] = manifest.clone();
    lock["packages"]["node_modules/plugin"] =
        json!({"version":"1.5.0", "peerDependencies":{"engine":"^2"}});
    lock["packages"]["node_modules/engine"] = json!({"version":"1.0.0"});
    // This nested version cannot satisfy plugin's required peer.
    lock["packages"]["node_modules/plugin/node_modules/engine"] = json!({"version":"2.5.0"});
    lock["packages"]["node_modules/alias"] = json!({"name":"actual","version":"1.2.3"});
    lock["packages"]["node_modules/prerelease"] = json!({"version":"1.1.0-beta.1"});
    write(&manifest_path, &manifest)?;
    write(&lock_path, &lock)?;
    let commit = support::commit(root)?;
    ingest(&mut store, root, &commit)?;
    let result = health::check(&store, &repository(root), &commit, &options())?;
    assert_eq!(result.status, Status::Incompatible);
    assert_eq!(result.exit_code(), 3);
    let peer = result
        .dependencies
        .iter()
        .find(|d| d.dependency.name == "engine")
        .unwrap();
    assert_eq!(
        peer.resolved.as_ref().unwrap().location,
        "node_modules/engine"
    );
    assert_eq!(peer.status, Status::Incompatible);
    assert!(peer.diagnostics[0].evidence.len() >= 2);
    assert_eq!(
        result
            .dependencies
            .iter()
            .find(|d| d.dependency.name == "alias")
            .unwrap()
            .status,
        Status::Healthy
    );
    assert_eq!(
        result
            .dependencies
            .iter()
            .find(|d| d.dependency.name == "absent")
            .unwrap()
            .status,
        Status::Incompatible
    );
    assert_eq!(
        result
            .dependencies
            .iter()
            .find(|d| d.dependency.name == "optional-absent")
            .unwrap()
            .status,
        Status::Healthy
    );
    assert_eq!(
        result
            .dependencies
            .iter()
            .find(|d| d.dependency.name == "prerelease")
            .unwrap()
            .status,
        Status::Incompatible
    );
    Ok(())
}

#[test]
fn health_never_passes_missing_stale_or_unsupported_evidence_and_captures_execution_failure()
-> anyhow::Result<()> {
    let (checkout, commit) = support::fixture_repository()?;
    let root = checkout.path();
    let temporary = tempdir()?;
    let mut store = Store::open(&temporary.path().join("index.sqlite3"))?;
    assert_eq!(
        health::check(&store, &repository(root), &commit, &options())?.status,
        Status::Unknown
    );
    store.ingest(
        &root.join("fixtures/semantic/removed-consumer/index.scip"),
        &SnapshotInput {
            repository: "consumer".into(),
            commit: commit.clone(),
            indexed_at: "2020-01-01T00:00:00Z".into(),
        },
    )?;
    let missing = health::check(&store, &repository(root), &commit, &options())?;
    assert_eq!(missing.status, Status::Unknown);
    assert!(
        missing
            .diagnostics
            .iter()
            .any(|d| d.code == "dependency_evidence_unavailable")
    );
    dependencies::scan(&mut store, &repository(root), &commit)?;
    assert_eq!(
        health::check(&store, &repository(root), &commit, &options())?.status,
        Status::Unknown
    );
    ingest(&mut store, root, &commit)?;
    // A different real index cannot silently agree with this dependency snapshot.
    store.ingest(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/semantic/packaged/consumer-v2/index.scip"),
        &SnapshotInput {
            repository: "consumer".into(),
            commit: commit.clone(),
            indexed_at: OffsetDateTime::now_utc().format(&Rfc3339)?,
        },
    )?;
    dependencies::scan(&mut store, &repository(root), &commit)?;
    let mismatch = health::check(&store, &repository(root), &commit, &options())?;
    assert_eq!(mismatch.status, Status::Unknown);
    assert!(
        mismatch
            .diagnostics
            .iter()
            .any(|d| d.code == "semantic_package_not_resolved" && d.start.is_some())
    );
    ingest(&mut store, root, &commit)?;
    let mut compiler = options();
    compiler.compiler = Some(Compiler {
        node: temporary.path().join("missing-node"),
        helper: temporary.path().join("unused.mjs"),
        timeout_seconds: 1,
    });
    let failed = health::check(&store, &repository(root), &commit, &compiler)?;
    assert_eq!(failed.status, Status::CheckFailed);
    assert_eq!(failed.exit_code(), 1);
    // Dirty source must prevent execution, even when the supplied runtime is invalid.
    fs::write(
        root.join("fixtures/semantic/removed-consumer/src/index.ts"),
        "dirty source",
    )?;
    assert_eq!(
        health::check(&store, &repository(root), &commit, &compiler)?.status,
        Status::Unknown
    );
    support::git(root, &["checkout", "--", "."])?;
    let manifest_path = root.join("fixtures/semantic/removed-consumer/package.json");
    let lock_path = root.join("package-lock.json");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&manifest_path)?)?;
    let mut lock: Value = serde_json::from_slice(&fs::read(&lock_path)?)?;
    manifest["dependencies"]["@ripplemap/library"] = json!("latest");
    lock["packages"]["fixtures/semantic/removed-consumer"] = manifest.clone();
    write(&manifest_path, &manifest)?;
    write(&lock_path, &lock)?;
    let unsupported = support::commit(root)?;
    ingest(&mut store, root, &unsupported)?;
    let unknown = health::check(&store, &repository(root), &unsupported, &options())?;
    assert_eq!(unknown.status, Status::Unknown);
    assert!(unknown.dependencies.iter().any(|d| {
        d.diagnostics
            .iter()
            .any(|d| d.code == "unsupported_requirement")
    }));
    let root_manifest_path = root.join("package.json");
    let mut root_manifest: Value = serde_json::from_slice(&fs::read(&root_manifest_path)?)?;
    root_manifest["overrides"] = json!({"@ripplemap/library":"2.0.0"});
    write(&root_manifest_path, &root_manifest)?;
    let overridden = support::commit(root)?;
    ingest(&mut store, root, &overridden)?;
    let overridden = health::check(&store, &repository(root), &overridden, &options())?;
    assert_eq!(overridden.status, Status::Unknown);
    assert!(overridden.dependencies.iter().all(|d| {
        d.diagnostics
            .iter()
            .any(|d| d.code == "unsupported_overrides")
    }));
    fs::remove_file(root.join("package-lock.json"))?;
    let missing_lock = support::commit(root)?;
    ingest(&mut store, root, &missing_lock)?;
    assert_eq!(
        health::check(&store, &repository(root), &missing_lock, &options())?.status,
        Status::Unknown
    );
    assert_eq!(
        health::check(&store, &repository(root), &"f".repeat(40), &options())?.exit_code(),
        4
    );
    Ok(())
}

#[test]
fn health_cli_reports_collection_profiles_and_exit_codes_without_external_tooling()
-> anyhow::Result<()> {
    let (checkout, commit) = support::fixture_repository()?;
    let temporary = tempdir()?;
    let database = temporary.path().join("index.sqlite3");
    let mut store = Store::open(&database)?;
    ingest(&mut store, checkout.path(), &commit)?;
    drop(store);
    let manifest = temporary.path().join("repositories.json");
    let selection = temporary.path().join("selection.json");
    write(
        &manifest,
        &json!({"schema_version":1,"repositories":[{"id":"consumer","path":repository(checkout.path()).path}]}),
    )?;
    write(
        &selection,
        &json!({"schema_version":1,"consumers":[{"repository":"consumer","commit":commit}]}),
    )?;
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_ripplemap"))
            .arg("--database")
            .arg(&database)
            .args(["health", "--manifest", manifest.to_str().unwrap()])
            .args(args)
            .output()
    };
    let output = run(&["--selection", selection.to_str().unwrap(), "--json"])?;
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let report: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(report["status"], "healthy");
    assert_eq!(report["reports"][0]["profile"], "dependencies-v1");
    let unknown = run(&[
        "--repository",
        "consumer",
        "--commit",
        &"f".repeat(40),
        "--json",
    ])?;
    assert_eq!(unknown.status.code(), Some(4));
    assert_eq!(
        serde_json::from_slice::<Value>(&unknown.stdout)?["status"],
        "unknown"
    );
    let truncated = run(&[
        "--repository",
        "consumer",
        "--commit",
        &commit,
        "--limit",
        "1",
        "--json",
    ])?;
    assert!(truncated.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&truncated.stdout)?["reports"][0]["truncated"],
        true
    );
    let invalid = run(&["--repository", "consumer"])?;
    assert_eq!(invalid.status.code(), Some(2));
    Ok(())
}

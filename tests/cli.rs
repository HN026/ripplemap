use std::{
    path::Path,
    process::{Command, Output},
};

use serde_json::Value;
use tempfile::tempdir;

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

fn run(database: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ripplemap"))
        .arg("--database")
        .arg(database)
        .args(args)
        .output()
        .unwrap()
}

fn success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn fixture_cli_persists_indexes_and_reports_exact_usages_between_processes() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let database = dir.path().join("ripplemap.sqlite3");
    let missing = run(&database, &["snapshots", "--json"]);
    assert_eq!(missing.status.code(), Some(1));
    assert!(missing.stdout.is_empty());
    assert!(!database.exists());
    for package in ["library", "removed-consumer", "retained-consumer"] {
        let scip = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/semantic")
            .join(package)
            .join("index.scip");
        let output = run(
            &database,
            &[
                "ingest",
                "--scip",
                scip.to_str().unwrap(),
                "--repository",
                package,
                "--commit",
                COMMIT,
                "--indexed-at",
                "2026-11-11T17:43:32Z",
            ],
        );
        success(&output);
        assert!(output.stdout.is_empty());
    }
    let output = run(&database, &["snapshots", "--json"]);
    success(&output);
    assert!(output.stderr.is_empty());
    let snapshots: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(snapshots.as_array().unwrap().len(), 3);
    assert_eq!(snapshots[1]["repository"], "removed-consumer");
    assert_eq!(snapshots[1]["commit"], COMMIT);
    assert_eq!(snapshots[1]["references"], 6);
    let output = run(&database, &["snapshots"]);
    success(&output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("6 references"));
    let symbol = "scip-typescript npm @ripplemap/library 1.0.0 src/`index.ts`/removed().";
    let output = run(&database, &["usages", "--symbol", symbol, "--json"]);
    success(&output);
    assert!(output.stderr.is_empty());
    let report: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(report["total_references"], 3);
    assert_eq!(report["observed_repositories"], 1);
    assert_eq!(report["references"][2]["start"]["line"], 3);
    assert_eq!(report["references"][2]["start"]["column"], 23);
    assert_eq!(report["references"][2]["commit"], COMMIT);
    assert_eq!(
        report["references"][2]["indexed_at"],
        "2026-11-11T17:43:32Z"
    );
    assert_eq!(report["truncated"], false);
    let output = run(
        &database,
        &["usages", "--symbol", symbol, "--limit", "1", "--json"],
    );
    success(&output);
    let report: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(report["total_references"], 3);
    assert_eq!(report["references"].as_array().unwrap().len(), 1);
    assert_eq!(report["truncated"], true);
    let output = run(&database, &["usages", "--symbol", symbol]);
    success(&output);
    let terminal = String::from_utf8(output.stdout)?;
    assert!(terminal.contains("Observed references: 3; distinct repositories: 1"));
    assert!(terminal.contains("src/index.ts\":3:23-3:30 [utf-16]"));
    assert!(terminal.contains("do not prove safety"));
    let unknown = symbol.replace(" 1.0.0 ", " 2.0.0 ");
    let output = run(&database, &["usages", "--symbol", &unknown, "--json"]);
    success(&output);
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout)?["total_references"],
        0
    );
    let output = run(&database, &["usages", "--symbol", "local 1", "--json"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let output = run(&database, &["usages", "--symbol", symbol, "--limit", "0"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    Ok(())
}

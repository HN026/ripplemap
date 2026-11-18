use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    process::Command,
};

use prost::Message;
use ripplemap::{
    api::{self, ApiSnapshot, Extraction, Status},
    scip,
    store::{Snapshot, SnapshotInput, Store},
};
use serde_json::Value;
use tempfile::{TempDir, tempdir};

const LEGACY: &str =
    "scip-typescript npm @ripplemap/packaged 1.0.0 dist/`functions.d.ts`/removed().";
const RETAINED: &str =
    "scip-typescript npm @ripplemap/packaged 1.0.0 dist/`functions.d.ts`/retained().";

fn fixture(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/semantic/packaged")
        .join(path)
}

fn input(repository: &str, commit: &str) -> SnapshotInput {
    SnapshotInput {
        repository: repository.into(),
        commit: commit.into(),
        indexed_at: "2026-11-18T18:16:17.300Z".into(),
    }
}

fn setup() -> anyhow::Result<(TempDir, Store, Vec<ApiSnapshot>, Vec<Snapshot>)> {
    let directory = tempdir()?;
    let mut store = Store::open(&directory.path().join("index.sqlite3"))?;
    let (mut apis, mut consumers) = (Vec::new(), Vec::new());
    for version in [1, 2] {
        let commit = format!("{version:040x}");
        let source = store.ingest(
            &fixture(&format!("library-v{version}/source.scip")),
            &input("source", &commit),
        )?;
        let types = store.ingest(
            &fixture(&format!("library-v{version}/declarations.scip")),
            &input("types", &commit),
        )?;
        let evidence: Extraction =
            api::files::read(&fixture(&format!("library-v{version}/exports.json")))?;
        apis.push(api::bind(&store, evidence, &source, &types)?);
        consumers.push(store.ingest(
            &fixture(&format!("consumer-v{version}/index.scip")),
            &input("consumer", &commit),
        )?);
    }
    // Another indexed commit has the same raw v1 symbol; selected-snapshot queries must exclude it.
    store.ingest(
        &fixture("consumer-v1/index.scip"),
        &input("consumer", &"f".repeat(40)),
    )?;
    Ok((directory, store, apis, consumers))
}

#[test]
fn packaged_api_resolution_preserves_aliases_versions_and_observed_locations() -> anyhow::Result<()>
{
    let (_directory, mut store, apis, consumers) = setup()?;
    let result = api::resolve(&store, &apis[0], &consumers[0], LEGACY, 100)?;
    assert_eq!(result.status, Status::Resolved);
    assert_eq!(result.candidates.len(), 1);
    assert_eq!(result.candidates[0].name, "legacy");
    assert!(result.candidates[0].producer_symbols[0].contains("src/`functions.ts`/removed()."));
    let usages = result.usages.unwrap();
    assert_eq!(
        (
            usages.total_references,
            usages.observed_repositories,
            usages.indexed_snapshots
        ),
        (2, 1, 1)
    );
    assert_eq!(
        (
            usages.references[1].start.line,
            usages.references[1].start.column
        ),
        (4, 23)
    );
    assert_eq!(
        api::resolve(&store, &apis[0], &consumers[0], RETAINED, 100)?.status,
        Status::Ambiguous
    );
    let candidates = api::resolve(&store, &apis[0], &consumers[0], RETAINED, 1)?;
    assert_eq!(
        candidates
            .candidates
            .iter()
            .map(|m| m.name.as_str())
            .collect::<Vec<_>>(),
        ["kept", "retained"]
    );
    assert!(candidates.usages.unwrap().truncated);
    let other = "scip-typescript npm @ripplemap/library 1.0.0 src/`index.ts`/retained().";
    let unrelated = api::resolve(&store, &apis[0], &consumers[0], other, 100)?;
    assert_eq!(unrelated.status, Status::Missing);
    assert!(unrelated.candidates.is_empty());
    assert_eq!(unrelated.usages.unwrap().total_references, 3);
    let unsupported =
        "scip-typescript npm @ripplemap/packaged 1.0.0 dist/`index.d.ts`/unsupportedValue.";
    assert_eq!(
        api::resolve(&store, &apis[0], &consumers[0], unsupported, 100)?.status,
        Status::Unsupported
    );
    assert_eq!(
        api::resolve(&store, &apis[0], &consumers[0], "local 2", 100)?.status,
        Status::Unsupported
    );
    assert_eq!(
        api::resolve(&store, &apis[0], &consumers[1], LEGACY, 100)?.status,
        Status::Missing
    );
    let v2_symbol = &apis[1]
        .mappings
        .iter()
        .find(|m| m.name == "retained")
        .unwrap()
        .declaration_symbols[0];
    assert!(v2_symbol.contains(" 2.0.0 dist/`api.d.ts`/stable()."));
    assert_eq!(
        api::resolve(&store, &apis[0], &consumers[1], v2_symbol, 100)?.status,
        Status::Missing
    );
    let linked = api::bridge(&store, &apis[0], &apis[1], "retained")?;
    assert_eq!(linked.status, Status::Resolved);
    assert_ne!(
        linked.base_mapping.unwrap().declaration_symbols,
        linked.head_mapping.unwrap().declaration_symbols
    );
    assert!(linked.explanation.contains("does not establish"));
    assert_eq!(
        api::bridge(&store, &apis[0], &apis[1], "legacy")?.status,
        Status::Missing
    );
    assert_eq!(
        api::bridge(&store, &apis[0], &apis[1], "unsupportedValue")?.status,
        Status::Unsupported
    );
    // Changed index bytes invalidate the original API artifact even for the same supplied commit.
    store.ingest(
        &fixture("library-v2/source.scip"),
        &input("source", &apis[0].producer.commit),
    )?;
    assert!(api::validate(&store, &apis[0]).is_err());
    Ok(())
}

#[test]
fn api_evidence_does_not_guess_missing_or_conflicting_definition_sites() -> anyhow::Result<()> {
    let (directory, mut store, apis, consumers) = setup()?;
    let original = &apis[0];
    let mut tampered = original.clone();
    tampered.mappings[0].declaration_symbols = vec![LEGACY.into()];
    assert!(api::validate(&store, &tampered).is_err());
    let mut duplicate = original.evidence.clone();
    duplicate.exports.push(duplicate.exports[0].clone());
    assert!(
        api::bind(
            &store,
            duplicate,
            &original.producer,
            &original.declarations
        )
        .is_err()
    );
    let mut absent = original.evidence.clone();
    absent
        .exports
        .iter_mut()
        .find(|e| e.name == "legacy")
        .unwrap()
        .source
        .as_mut()
        .unwrap()
        .range = [99, 0, 99, 7];
    let missing = api::bind(&store, absent, &original.producer, &original.declarations)?;
    assert_eq!(
        missing
            .mappings
            .iter()
            .find(|m| m.name == "legacy")
            .unwrap()
            .status,
        Status::Missing
    );
    assert!(api::files::read::<ApiSnapshot>(&directory.path().join("absent.json")).is_err());
    let mut no_snapshot = original.producer.clone();
    no_snapshot.repository = "absent".into();
    assert!(
        api::bind(
            &store,
            original.evidence.clone(),
            &no_snapshot,
            &original.declarations
        )
        .is_err()
    );

    // Deliberately create conflicting index evidence at one real compiler definition site.
    let mut reader = scip::Reader::new(Cursor::new(fs::read(fixture("library-v1/source.scip"))?));
    let mut bytes = Vec::new();
    while let Some((tag, message)) = reader.read_field()? {
        prost::encoding::encode_varint(u64::from(tag << 3 | 2), &mut bytes);
        if tag == 2 {
            let mut document = scip::Document::decode(message.as_slice())?;
            if document.relative_path == "src/functions.ts" {
                let mut conflict = document
                    .occurrences
                    .iter()
                    .find(|o| o.symbol.ends_with("/removed().") && o.symbol_roles & 1 != 0)
                    .unwrap()
                    .clone();
                conflict.symbol = conflict.symbol.replace("/removed().", "/different().");
                document.occurrences.push(conflict);
            }
            document.encode_length_delimited(&mut bytes)?;
        } else {
            prost::encoding::encode_varint(message.len() as u64, &mut bytes);
            bytes.extend_from_slice(&message);
        }
    }
    let path = directory.path().join("conflicting.scip");
    fs::write(&path, bytes)?;
    let producer = store.ingest(&path, &input("source", &original.producer.commit))?;
    let ambiguous = api::bind(
        &store,
        original.evidence.clone(),
        &producer,
        &original.declarations,
    )?;
    let mapping = ambiguous
        .mappings
        .iter()
        .find(|m| m.name == "legacy")
        .unwrap();
    assert_eq!(mapping.status, Status::Ambiguous);
    assert_eq!(mapping.producer_symbols.len(), 2);
    assert_eq!(
        api::resolve(&store, &ambiguous, &consumers[0], LEGACY, 100)?.status,
        Status::Ambiguous
    );
    Ok(())
}

#[test]
fn api_cli_reports_versioned_evidence_without_node_or_index_generation() -> anyhow::Result<()> {
    let (directory, store, apis, consumers) = setup()?;
    let database = directory.path().join("index.sqlite3");
    let base = directory.path().join("base.json");
    let head = directory.path().join("head.json");
    api::files::write(&base, &apis[0])?;
    api::files::write(&head, &apis[1])?;
    drop(store);
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_ripplemap"))
            .arg("--database")
            .arg(&database)
            .arg("api")
            .args(args)
            .output()
    };
    let output = run(&[
        "resolve",
        "--api",
        base.to_str().unwrap(),
        "--consumer-repository",
        "consumer",
        "--consumer-commit",
        &consumers[0].commit,
        "--symbol",
        LEGACY,
        "--json",
    ])?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let result: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(result["status"], "resolved");
    assert_eq!(result["usages"]["total_references"], 2);
    let output = run(&[
        "bridge",
        "--base",
        base.to_str().unwrap(),
        "--head",
        head.to_str().unwrap(),
        "--export",
        "retained",
        "--json",
    ])?;
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout)?["status"],
        "resolved"
    );
    let output = run(&[
        "bridge",
        "--base",
        base.to_str().unwrap(),
        "--head",
        head.to_str().unwrap(),
        "--export",
        "legacy",
    ])?;
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Missing"));
    let output = run(&[
        "bridge",
        "--base",
        base.to_str().unwrap(),
        "--head",
        directory.path().join("missing.json").to_str().unwrap(),
        "--export",
        "legacy",
        "--json",
    ])?;
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    Ok(())
}

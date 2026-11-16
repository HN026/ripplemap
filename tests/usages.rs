use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
};

use prost::Message;
use ripplemap::{
    impact,
    scip::{
        self, Document, Metadata, Occurrence, SingleLineRange, SymbolInformation, ToolInfo,
        TypedRange,
    },
    store::{SnapshotInput, Store},
};
use tempfile::tempdir;

const REMOVED: &str = "scip-typescript npm @ripplemap/library 1.0.0 src/`index.ts`/removed().";
const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

fn fixture(package: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/semantic")
        .join(package)
        .join("index.scip")
}

fn input(repository: &str) -> SnapshotInput {
    SnapshotInput {
        repository: repository.into(),
        commit: COMMIT.into(),
        indexed_at: "2026-11-11T17:43:32Z".into(),
    }
}

fn field(tag: u32, message: &impl Message, output: &mut Vec<u8>) {
    prost::encoding::encode_varint(u64::from(tag << 3 | 2), output);
    message.encode_length_delimited(output).unwrap();
}

#[test]
fn exact_usages_exclude_other_versions_and_names_and_count_repositories_distinctly()
-> anyhow::Result<()> {
    let dir = tempdir()?;
    let mut store = Store::open(&dir.path().join("index.sqlite3"))?;
    let empty = impact::usages(&store, REMOVED, 100)?;
    assert_eq!(empty.indexed_snapshots, 0);
    assert!(empty.coverage_note.contains("do not prove safety"));
    for package in ["library", "retained-consumer", "removed-consumer"] {
        store.ingest(&fixture(package), &input(package))?;
    }
    let report = impact::usages(&store, REMOVED, 100)?;
    assert_eq!(
        (report.total_references, report.observed_repositories),
        (3, 1)
    );
    assert_eq!(report.package.version.as_deref(), Some("1.0.0"));
    assert_eq!(
        report
            .references
            .iter()
            .map(|r| (r.start.line, r.start.column, r.end.line, r.end.column))
            .collect::<Vec<_>>(),
        [(1, 10, 1, 17), (1, 21, 1, 28), (3, 23, 3, 30)]
    );
    assert!(
        report
            .references
            .iter()
            .all(|r| r.repository == "removed-consumer" && r.position_encoding == "utf-16")
    );
    let unrelated = REMOVED.replace("@ripplemap/library", "@ripplemap/removed-consumer");
    assert_eq!(impact::usages(&store, &unrelated, 100)?.total_references, 1);

    // Mutate decoded fixture symbols to create real second-version lookup evidence.
    let mut reader = scip::Reader::new(Cursor::new(fs::read(fixture("removed-consumer"))?));
    let mut version_two = Vec::new();
    while let Some((tag, bytes)) = reader.read_field()? {
        if tag == 2 {
            let mut document = Document::decode(bytes.as_slice())?;
            for occurrence in &mut document.occurrences {
                occurrence.symbol = occurrence.symbol.replace(" 1.0.0 ", " 2.0.0 ");
            }
            for symbol in &mut document.symbols {
                symbol.symbol = symbol.symbol.replace(" 1.0.0 ", " 2.0.0 ");
            }
            field(tag, &document, &mut version_two);
        } else {
            field(tag, &Metadata::decode(bytes.as_slice())?, &mut version_two);
        }
    }
    let path = dir.path().join("version-two.scip");
    fs::write(&path, version_two)?;
    store.ingest(&path, &input("version-two"))?;
    assert_eq!(impact::usages(&store, REMOVED, 100)?.total_references, 3);
    assert_eq!(
        impact::usages(&store, &REMOVED.replace(" 1.0.0 ", " 2.0.0 "), 100)?.total_references,
        3
    );
    assert_eq!(
        impact::usages(&store, &REMOVED.replace(" 1.0.0 ", " 3.0.0 "), 100)?.total_references,
        0
    );
    let mut second_commit = input("removed-consumer");
    second_commit.commit = "a".repeat(40);
    store.ingest(&fixture("removed-consumer"), &second_commit)?;
    let report = impact::usages(&store, REMOVED, 1)?;
    assert_eq!(
        (
            report.total_references,
            report.observed_repositories,
            report.references.len()
        ),
        (6, 1, 1)
    );
    assert!(report.truncated);
    assert_eq!(report.references[0].commit, COMMIT);
    store.ingest(&fixture("removed-consumer"), &input("another-consumer"))?;
    let report = impact::usages(&store, REMOVED, 100)?;
    assert_eq!(
        (report.total_references, report.observed_repositories),
        (9, 2)
    );
    assert!(!report.truncated);
    assert_eq!(report.references[0].repository, "another-consumer");
    assert_eq!(report, impact::usages(&store, REMOVED, 100)?);

    let mut broken = fs::read(fixture("retained-consumer"))?;
    broken.extend_from_slice(&[0x12, 5, 0]);
    fs::write(&path, broken)?;
    assert!(store.ingest(&path, &input("removed-consumer")).is_err());
    assert_eq!(report, impact::usages(&store, REMOVED, 100)?);
    assert!(impact::usages(&store, "local 1", 100).is_err());
    assert!(impact::usages(&store, REMOVED, 0).is_err());
    assert!(impact::usages(&store, REMOVED, impact::MAX_LIMIT + 1).is_err());
    Ok(())
}

#[test]
fn document_scopes_external_symbols_and_unknown_versions_are_preserved() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let database = dir.path().join("index.sqlite3");
    let mut store = Store::open(&database)?;
    let unknown_version = REMOVED.replace(" 1.0.0 ", " . ");
    let mut bytes = Vec::new();
    field(
        1,
        &Metadata {
            version: 0,
            tool_info: Some(ToolInfo {
                name: "test".into(),
                version: "1".into(),
            }),
            project_root: "file:///fixture".into(),
            text_document_encoding: 1,
        },
        &mut bytes,
    );
    let external = "scip-typescript npm external 1.0.0 index/f().";
    field(
        3,
        &SymbolInformation {
            symbol: external.into(),
        },
        &mut bytes,
    );
    for path in ["src/a.ts", "src/b.ts"] {
        let reference = Occurrence {
            symbol: unknown_version.clone(),
            typed_range: Some(TypedRange::Single(SingleLineRange {
                line: 0,
                start_character: 3,
                end_character: 6,
            })),
            ..Default::default()
        };
        field(
            2,
            &Document {
                relative_path: path.into(),
                position_encoding: 2,
                occurrences: vec![
                    Occurrence {
                        symbol: "local 1".into(),
                        range: vec![0, 0, 1],
                        symbol_roles: 1,
                        ..Default::default()
                    },
                    reference.clone(),
                    reference,
                ],
                ..Default::default()
            },
            &mut bytes,
        );
    }
    let path = dir.path().join("scoped.scip");
    fs::write(&path, bytes)?;
    let snapshot = store.ingest(&path, &input("consumer"))?;
    assert_eq!((snapshot.definitions, snapshot.references), (2, 2));
    let report = impact::usages(&store, &unknown_version, 100)?;
    assert_eq!(report.package.version, None);
    assert_eq!(report.references[0].start.column, 4);
    assert_eq!(report.references[0].position_encoding, "utf-16");
    assert_eq!(report.references[1].path, "src/b.ts");
    assert_eq!(impact::usages(&store, external, 100)?.total_references, 0);
    let connection = rusqlite::Connection::open(database)?;
    assert_eq!(
        connection.query_row(
            "SELECT count(*) FROM symbols WHERE symbol = 'local 1'",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        2
    );
    assert_eq!(
        connection.query_row(
            "SELECT count(*) FROM symbols WHERE symbol = ?1",
            [external],
            |r| r.get::<_, i64>(0)
        )?,
        1
    );
    Ok(())
}

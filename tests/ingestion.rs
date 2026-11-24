use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
};

use prost::Message;
use ripplemap::{
    scip::{
        self, Document, Metadata, MultiLineRange, Occurrence, SingleLineRange, ToolInfo, TypedRange,
    },
    store::{SnapshotInput, Store},
};
use sha2::{Digest, Sha256};
use tempfile::tempdir;

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

/**
 * Locate a checked-in semantic fixture relative to the Cargo package.
 */
fn fixture(package: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/semantic")
        .join(package)
        .join("index.scip")
}

/**
 * Build explicit snapshot metadata for the isolated fixture database.
 */
fn input(repository: &str) -> SnapshotInput {
    SnapshotInput {
        repository: repository.into(),
        commit: COMMIT.into(),
        indexed_at: "2026-11-11T17:43:32Z".into(),
    }
}

#[test]
/**
 * Verify imports persist and replacement is atomic and idempotent.
 */
fn imports_persist_and_replacement_is_atomic_and_idempotent() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let database = dir.path().join("index.sqlite3");
    let mut store = Store::open(&database)?;
    let first = store.ingest(&fixture("removed-consumer"), &input("consumer"))?;
    assert_eq!(
        (first.documents, first.definitions, first.references),
        (1, 5, 6)
    );
    assert_eq!(first.indexer_version, "0.6.2");
    assert_eq!(
        first.index_digest,
        format!(
            "{:x}",
            Sha256::digest(fs::read(fixture("removed-consumer"))?)
        )
    );
    store.ingest(&fixture("removed-consumer"), &input("consumer"))?;
    assert_eq!(store.snapshots()?, vec![first.clone()]);

    // A valid document is written before the trailing malformed field is read.
    let mut broken = fs::read(fixture("retained-consumer"))?;
    broken.extend_from_slice(&[0x12, 5, 0]);
    let broken_path = dir.path().join("broken.scip");
    fs::write(&broken_path, broken)?;
    assert!(store.ingest(&broken_path, &input("consumer")).is_err());
    assert!(store.ingest(&broken_path, &input("new-repo")).is_err());
    assert_eq!(store.snapshots()?, vec![first]);

    let replaced = store.ingest(&fixture("retained-consumer"), &input("consumer"))?;
    assert_eq!(
        (
            replaced.documents,
            replaced.definitions,
            replaced.references
        ),
        (1, 2, 3)
    );
    let mut another_commit = input("consumer");
    another_commit.commit = "a".repeat(40);
    store.ingest(&fixture("library"), &another_commit)?;
    store.ingest(&fixture("library"), &input("different-repo"))?;
    drop(store);
    let snapshots = Store::open_read_only(&database)?.snapshots()?;
    assert_eq!(snapshots.len(), 3);
    let connection = rusqlite::Connection::open(&database)?;
    let rows: i64 = connection.query_row("SELECT count(*) FROM occurrences", [], |r| r.get(0))?;
    assert_eq!(rows, 5 + 7 + 7);
    assert_eq!(
        connection.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))?,
        "ok"
    );
    Ok(())
}

/**
 * Encode one length-delimited Protobuf field for an intentionally constructed index.
 */
fn field(tag: u8, message: &impl Message, output: &mut Vec<u8>) {
    output.push((tag << 3) | 2);
    message.encode_length_delimited(output).unwrap();
}

/**
 * Encode a small synthetic SCIP index for metadata and range validation checks.
 */
fn index(document: &Document) -> Vec<u8> {
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
    field(2, document, &mut bytes);
    bytes
}

#[test]
/**
 * Verify invalid metadata and unsupported documents leave no snapshot.
 */
fn invalid_metadata_and_unsupported_documents_leave_no_snapshot() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let mut store = Store::open(&dir.path().join("index.sqlite3"))?;
    let path = dir.path().join("invalid.scip");
    let document = Document {
        relative_path: "src/index.ts".into(),
        position_encoding: 2,
        ..Default::default()
    };
    fs::write(&path, index(&document))?;
    for (commit, timestamp) in [
        ("main", "2026-11-11T17:43:32Z"),
        (COMMIT, "yesterday"),
        (COMMIT, "2026-11-11T17:43:32"),
    ] {
        let mut metadata = input("consumer");
        metadata.commit = commit.into();
        metadata.indexed_at = timestamp.into();
        assert!(store.ingest(&path, &metadata).is_err());
    }
    for document in [
        Document {
            relative_path: "../outside.ts".into(),
            ..document.clone()
        },
        Document {
            position_encoding: 0,
            ..document.clone()
        },
        Document {
            position_encoding: 99,
            ..document.clone()
        },
        Document {
            occurrences: vec![Occurrence {
                symbol: "local 1".into(),
                range: vec![1, 3],
                ..Default::default()
            }],
            ..document.clone()
        },
    ] {
        fs::write(&path, index(&document))?;
        assert!(store.ingest(&path, &input("consumer")).is_err());
    }
    let mut duplicate = index(&document);
    field(2, &document, &mut duplicate);
    fs::write(&path, duplicate)?;
    assert!(store.ingest(&path, &input("consumer")).is_err());
    assert!(store.snapshots()?.is_empty());
    assert!(Store::open_read_only(&dir.path().join("missing.sqlite3")).is_err());
    assert!(!dir.path().join("missing.sqlite3").exists());
    let unrelated = dir.path().join("unrelated.sqlite3");
    rusqlite::Connection::open(&unrelated)?.execute("CREATE TABLE personal_data (id)", [])?;
    assert!(Store::open(&unrelated).is_err());
    Ok(())
}

#[test]
/**
 * Verify ranges paths and package headers preserve scip semantics.
 */
fn ranges_paths_and_package_headers_preserve_scip_semantics() -> anyhow::Result<()> {
    let mut occurrence = Occurrence {
        range: vec![2, 22, 29],
        ..Default::default()
    };
    assert_eq!(occurrence.position()?, [2, 22, 2, 29]);
    occurrence.range = vec![2, 22, 3, 0];
    assert_eq!(occurrence.position()?, [2, 22, 3, 0]);
    occurrence.range = vec![-1, 0, 1];
    assert!(occurrence.position().is_err());
    occurrence.range = vec![1, 5, 3];
    assert!(occurrence.position().is_err());
    occurrence.typed_range = Some(TypedRange::Single(SingleLineRange {
        line: 4,
        start_character: 1,
        end_character: 4,
    }));
    assert_eq!(occurrence.position()?, [4, 1, 4, 4]);
    occurrence.typed_range = Some(TypedRange::Multi(MultiLineRange {
        start_line: 1,
        start_character: 4,
        end_line: 2,
        end_character: 0,
    }));
    assert_eq!(occurrence.position()?, [1, 4, 2, 0]);
    let package =
        scip::package("scip-typescript npm @a/package 1.2.3 src/`index.ts`/removed().")?.unwrap();
    assert_eq!(package.name.as_deref(), Some("@a/package"));
    assert_eq!(package.version.as_deref(), Some("1.2.3"));
    let escaped = scip::package("scheme my  manager package  name . file/f().")?.unwrap();
    assert_eq!(escaped.manager.as_deref(), Some("my manager"));
    assert_eq!(escaped.name.as_deref(), Some("package name"));
    assert_eq!(escaped.version, None);
    assert_eq!(scip::package("local 1")?, None);
    assert!(scip::package("local ").is_err());
    assert!(scip::package("missing header").is_err());
    for path in [
        "",
        "/absolute",
        "../outside",
        "a//b",
        "a/./b",
        "a\\b",
        "C:/outside",
    ] {
        assert!(scip::validate_path(path).is_err());
    }
    scip::validate_path("src/a.ts")?;
    Ok(())
}

#[test]
/**
 * Verify streaming reader rejects truncation unknown fields and oversized messages.
 */
fn streaming_reader_rejects_truncation_unknown_fields_and_oversized_messages() {
    for bytes in [
        vec![0x80],
        vec![0xff; 10],
        vec![0x0a],
        vec![0x0a, 3, 0],
        vec![0x22, 0],
        vec![0x08, 0],
    ] {
        assert!(scip::Reader::new(Cursor::new(bytes)).read_field().is_err());
    }
    let mut oversized = vec![0x12];
    prost::encoding::encode_varint((scip::MAX_MESSAGE_BYTES + 1) as u64, &mut oversized);
    assert!(
        scip::Reader::new(Cursor::new(oversized))
            .read_field()
            .is_err()
    );
    assert!(
        scip::Reader::new(Cursor::new([]))
            .read_field()
            .unwrap()
            .is_none()
    );
}

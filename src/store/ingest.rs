use std::{fs::File, path::Path};

use anyhow::{Context, Result, ensure};
use prost::Message;
use rusqlite::{Transaction, TransactionBehavior, params};

use super::{Snapshot, SnapshotInput, Store, documents, snapshots};
use crate::scip::{self, Document, Metadata, SymbolInformation};

impl Store {
    pub fn ingest(&mut self, path: &Path, input: &SnapshotInput) -> Result<Snapshot> {
        self.ingest_inner(path, input).with_context(|| {
            format!(
                "ingest {} for repository {:?} at {}",
                path.display(),
                input.repository,
                input.commit
            )
        })
    }

    fn ingest_inner(&mut self, path: &Path, input: &SnapshotInput) -> Result<Snapshot> {
        input.validate()?;
        let (mut reader, metadata) = open_index(path)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = snapshots::replace(&tx, input, &metadata)?;
        import_fields(&tx, id, &mut reader)?;
        tx.execute(
            "UPDATE snapshots SET index_digest = ?1 WHERE id = ?2",
            params![reader.finish(), id],
        )?;
        let snapshot = Self::select_snapshot(&tx, id)?;
        tx.commit().context("commit snapshot replacement")?;
        Ok(snapshot)
    }
}

fn open_index(path: &Path) -> Result<(scip::Reader<File>, Metadata)> {
    let file = File::open(path).with_context(|| format!("open SCIP file {}", path.display()))?;
    ensure!(
        file.metadata()?.is_file(),
        "SCIP input must be a regular file"
    );
    let mut reader = scip::Reader::new(file);
    let (tag, bytes) = reader
        .read_field()?
        .context("empty SCIP index: missing metadata")?;
    ensure!(tag == 1, "SCIP metadata must be the first field");
    let metadata = Metadata::decode(bytes.as_slice()).context("decode SCIP metadata")?;
    drop(bytes);
    ensure!(
        metadata.version == 0,
        "unsupported SCIP protocol version {}",
        metadata.version
    );
    ensure!(
        !metadata.project_root.is_empty(),
        "missing SCIP project root"
    );
    Ok((reader, metadata))
}

fn import_fields(tx: &Transaction<'_>, id: i64, reader: &mut scip::Reader<File>) -> Result<()> {
    while let Some((tag, bytes)) = reader.read_field()? {
        match tag {
            1 => anyhow::bail!("duplicate SCIP metadata"),
            2 => {
                let document =
                    Document::decode(bytes.as_slice()).context("decode SCIP document")?;
                drop(bytes);
                documents::insert_document(tx, id, &document)
                    .with_context(|| format!("document {:?}", document.relative_path))?;
            }
            3 => {
                let symbol = SymbolInformation::decode(bytes.as_slice())
                    .context("decode external symbol")?;
                ensure!(
                    !symbol.symbol.starts_with("local "),
                    "external symbol cannot be local"
                );
                documents::insert_symbol(tx, id, "", &symbol.symbol)?;
            }
            _ => unreachable!(),
        }
    }
    Ok(())
}

use std::io::{BufReader, Read};

use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};

// Bound both the input buffer and decoded document size, independent of total index size.
pub const MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;

/// A SCIP Index is a stream of top-level fields, not a length-prefixed Index.
/// Memory holds one bounded field and its decoded message, never the whole index.
pub struct Reader<R> {
    input: BufReader<R>,
    digest: Sha256,
}

impl<R: Read> Reader<R> {
    /**
     * Initialize a streaming index reader and its incremental SHA-256 digest.
     */
    pub fn new(input: R) -> Self {
        Self {
            input: BufReader::new(input),
            digest: Sha256::new(),
        }
    }

    /**
     * Read one bounded Protobuf varint, distinguishing clean EOF from truncation.
     */
    fn varint(&mut self) -> Result<Option<u64>> {
        let mut value = 0;
        for shift in (0..70).step_by(7) {
            let mut byte = [0];
            if self.input.read(&mut byte)? == 0 {
                ensure!(shift == 0, "truncated Protobuf varint");
                return Ok(None);
            }
            self.digest.update(byte);
            ensure!(shift < 63 || byte[0] <= 1, "Protobuf varint overflow");
            value |= u64::from(byte[0] & 0x7f) << shift;
            if byte[0] & 0x80 == 0 {
                return Ok(Some(value));
            }
        }
        bail!("Protobuf varint overflow")
    }

    /**
     * Read one supported bounded SCIP field without accumulating the full index.
     */
    pub fn read_field(&mut self) -> Result<Option<(u32, Vec<u8>)>> {
        let Some(key) = self.varint()? else {
            return Ok(None);
        };
        let tag = key >> 3;
        ensure!(
            (1..=3).contains(&tag) && key & 7 == 2,
            "unsupported SCIP Index field key {key}"
        );
        let size = self.varint()?.context("missing Protobuf message length")?;
        ensure!(
            size <= MAX_MESSAGE_BYTES as u64,
            "SCIP message exceeds the {MAX_MESSAGE_BYTES}-byte ingestion limit"
        );
        let mut bytes = vec![0; size as usize];
        self.input
            .read_exact(&mut bytes)
            .context("truncated SCIP message")?;
        self.digest.update(&bytes);
        Ok(Some((tag as u32, bytes)))
    }

    /**
     * Return the SHA-256 digest of the consumed SCIP bytes.
     */
    pub fn finish(self) -> String {
        format!("{:x}", self.digest.finalize())
    }
}

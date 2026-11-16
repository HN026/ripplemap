//! Wire-compatible fields from SCIP v0.10.0, using prost without a protoc build dependency.
//! Schema: https://github.com/scip-code/scip/blob/v0.10.0/scip.proto
//! Documentation, highlighting and relationships are not used for exact occurrence lookup.

use std::io::{BufReader, Read};

use anyhow::{Context, Result, bail, ensure};
use prost::Message;
use sha2::{Digest, Sha256};

// Bound both the input buffer and decoded document size, independent of total index size.
pub const MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;
pub const DEFINITION: i32 = 1;

#[derive(Clone, PartialEq, Message)]
pub struct Metadata {
    #[prost(int32, tag = "1")]
    pub version: i32,
    #[prost(message, optional, tag = "2")]
    pub tool_info: Option<ToolInfo>,
    #[prost(string, tag = "3")]
    pub project_root: String,
    #[prost(int32, tag = "4")]
    pub text_document_encoding: i32,
}

#[derive(Clone, PartialEq, Message)]
pub struct ToolInfo {
    #[prost(string, tag = "1")]
    pub name: String,
    #[prost(string, tag = "2")]
    pub version: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct Document {
    #[prost(string, tag = "1")]
    pub relative_path: String,
    #[prost(message, repeated, tag = "2")]
    pub occurrences: Vec<Occurrence>,
    #[prost(message, repeated, tag = "3")]
    pub symbols: Vec<SymbolInformation>,
    #[prost(string, tag = "4")]
    pub language: String,
    #[prost(int32, tag = "6")]
    pub position_encoding: i32,
}

#[derive(Clone, PartialEq, Message)]
pub struct SymbolInformation {
    #[prost(string, tag = "1")]
    pub symbol: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct Occurrence {
    #[prost(int32, repeated, packed = "true", tag = "1")]
    pub range: Vec<i32>,
    #[prost(string, tag = "2")]
    pub symbol: String,
    #[prost(int32, tag = "3")]
    pub symbol_roles: i32,
    #[prost(oneof = "TypedRange", tags = "8, 9")]
    pub typed_range: Option<TypedRange>,
}

#[derive(Clone, PartialEq, prost::Oneof)]
pub enum TypedRange {
    #[prost(message, tag = "8")]
    Single(SingleLineRange),
    #[prost(message, tag = "9")]
    Multi(MultiLineRange),
}

#[derive(Clone, PartialEq, Message)]
pub struct SingleLineRange {
    #[prost(int32, tag = "1")]
    pub line: i32,
    #[prost(int32, tag = "2")]
    pub start_character: i32,
    #[prost(int32, tag = "3")]
    pub end_character: i32,
}

#[derive(Clone, PartialEq, Message)]
pub struct MultiLineRange {
    #[prost(int32, tag = "1")]
    pub start_line: i32,
    #[prost(int32, tag = "2")]
    pub start_character: i32,
    #[prost(int32, tag = "3")]
    pub end_line: i32,
    #[prost(int32, tag = "4")]
    pub end_character: i32,
}

impl Occurrence {
    pub fn position(&self) -> Result<[i32; 4]> {
        let position = match &self.typed_range {
            Some(TypedRange::Single(r)) => [r.line, r.start_character, r.line, r.end_character],
            Some(TypedRange::Multi(r)) => {
                [r.start_line, r.start_character, r.end_line, r.end_character]
            }
            None => match self.range.as_slice() {
                [line, start, end] => [*line, *start, *line, *end],
                [line, start, end_line, end] => [*line, *start, *end_line, *end],
                _ => bail!("unsupported occurrence range: expected 3 or 4 coordinates"),
            },
        };
        ensure!(
            position.iter().all(|n| *n >= 0),
            "negative source coordinate"
        );
        ensure!(
            (position[0], position[1]) <= (position[2], position[3]),
            "source range ends before it starts"
        );
        Ok(position)
    }
}

pub fn validate_path(path: &str) -> Result<()> {
    ensure!(
        !path.is_empty()
            && !path.contains(['\\', ':', '\0'])
            && !path.split('/').any(|part| matches!(part, "" | "." | "..")),
        "invalid SCIP relative document path: {path:?}"
    );
    Ok(())
}

#[derive(Debug, PartialEq)]
pub struct Package {
    pub scheme: String,
    pub manager: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
}

// Only parse the package header. The complete raw symbol remains the identity.
pub fn package(symbol: &str) -> Result<Option<Package>> {
    ensure!(!symbol.contains('\0'), "NUL in SCIP symbol");
    if let Some(id) = symbol.strip_prefix("local ") {
        ensure!(
            !id.is_empty() && !id.contains(char::is_whitespace),
            "invalid local symbol"
        );
        return Ok(None);
    }
    let mut remaining = symbol;
    let mut components = Vec::with_capacity(4);
    for _ in 0..4 {
        let bytes = remaining.as_bytes();
        let mut i = 0;
        loop {
            ensure!(i < bytes.len(), "invalid SCIP symbol header: {symbol:?}");
            if bytes[i] == b' ' {
                if bytes.get(i + 1) == Some(&b' ') {
                    i += 2;
                    continue;
                }
                ensure!(i > 0, "empty SCIP symbol component");
                components.push(remaining[..i].replace("  ", " "));
                remaining = &remaining[i + 1..];
                break;
            }
            i += 1;
        }
    }
    ensure!(!remaining.is_empty(), "missing SCIP symbol descriptor");
    let mut parts = components.into_iter();
    let scheme = parts.next().unwrap();
    ensure!(!scheme.starts_with("local"), "invalid global SCIP scheme");
    let value = |part: String| (part != ".").then_some(part);
    Ok(Some(Package {
        scheme,
        manager: value(parts.next().unwrap()),
        name: value(parts.next().unwrap()),
        version: value(parts.next().unwrap()),
    }))
}

/// A SCIP Index is a stream of top-level fields, not a length-prefixed Index.
/// Memory holds one bounded field and its decoded message, never the whole index.
pub struct Reader<R> {
    input: BufReader<R>,
    digest: Sha256,
}

impl<R: Read> Reader<R> {
    pub fn new(input: R) -> Self {
        Self {
            input: BufReader::new(input),
            digest: Sha256::new(),
        }
    }

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

    pub fn finish(self) -> String {
        format!("{:x}", self.digest.finalize())
    }
}

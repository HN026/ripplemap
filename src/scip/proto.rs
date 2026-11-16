//! Wire-compatible fields from SCIP v0.10.0, using prost without a protoc build dependency.
//! Schema: https://github.com/scip-code/scip/blob/v0.10.0/scip.proto
//! Documentation, highlighting and relationships are not used for exact occurrence lookup.

use prost::Message;

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

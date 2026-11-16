CREATE TABLE snapshots (
    id INTEGER PRIMARY KEY,
    repository TEXT NOT NULL,
    commit_id TEXT NOT NULL,
    indexed_at TEXT NOT NULL,
    indexer_name TEXT NOT NULL,
    indexer_version TEXT NOT NULL,
    index_digest TEXT NOT NULL,
    project_root TEXT NOT NULL,
    text_encoding INTEGER NOT NULL,
    documents INTEGER NOT NULL DEFAULT 0,
    definitions INTEGER NOT NULL DEFAULT 0,
    references_count INTEGER NOT NULL DEFAULT 0,
    symbol_less_occurrences INTEGER NOT NULL DEFAULT 0,
    UNIQUE(repository, commit_id)
);
CREATE TABLE documents (
    id INTEGER PRIMARY KEY,
    snapshot_id INTEGER NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    language TEXT NOT NULL,
    position_encoding INTEGER NOT NULL,
    UNIQUE(snapshot_id, path)
);
CREATE TABLE symbols (
    id INTEGER PRIMARY KEY,
    snapshot_id INTEGER NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
    scope TEXT NOT NULL,
    symbol TEXT NOT NULL,
    scheme TEXT,
    package_manager TEXT,
    package_name TEXT,
    package_version TEXT,
    UNIQUE(snapshot_id, scope, symbol)
);
CREATE INDEX symbols_lookup ON symbols(symbol) WHERE scope = '';
CREATE TABLE occurrences (
    document_id INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    symbol_id INTEGER NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
    symbol_roles INTEGER NOT NULL,
    start_line INTEGER NOT NULL,
    start_character INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    end_character INTEGER NOT NULL,
    UNIQUE(document_id, symbol_id, symbol_roles, start_line, start_character, end_line, end_character)
);
CREATE INDEX occurrences_symbol ON occurrences(symbol_id);
CREATE VIEW definitions AS SELECT * FROM occurrences WHERE (symbol_roles & 1) != 0;
CREATE VIEW symbol_references AS SELECT * FROM occurrences WHERE (symbol_roles & 1) = 0;

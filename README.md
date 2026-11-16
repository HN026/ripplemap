# Ripplemap

Ripplemap is a local developer tool for investigating the **observed** downstream
impact of library API changes using SCIP semantic indexes. Symbol identities
include package names and exact versions, so references are matched by semantic
identity rather than function names.

The Rust CLI imports pre-generated SCIP indexes into a local SQLite database.
Each snapshot records its repository, exact commit, successful index-generation
time, indexer version, and SHA-256 digest. Index generation is an explicit external
step. Ripplemap does not run an indexer during ingestion.

## Install and import an index

Requires Rust 1.95 or later and a C compiler for bundled SQLite. Node is needed
only for the optional TypeScript fixture tooling.

```sh
cargo install --path . --locked
ripplemap --help
```

Import a SCIP index with the exact source commit and the recorded time its
successful generation completed. Branch names and abbreviated commits are rejected.
For the checked-in fixture:

```sh
fixture_commit="$(git rev-parse 8d9a3d00c39a3ea089825b9b3085b3b94c131164)"
ripplemap ingest --scip fixtures/semantic/removed-consumer/index.scip \
  --repository removed-consumer --commit "$fixture_commit" \
  --indexed-at 2026-11-11T17:43:32Z
```

The default database is `ripplemap.sqlite3` in the current directory. Use
`--database /path/to/index.sqlite3` with any command to select another database.
Reimporting the same repository/commit atomically replaces that snapshot; other
repositories and commits remain distinct. A failed import preserves the previous
snapshot. Identical occurrence rows are deduplicated.

Ingestion reads one Protobuf field at a time, capped at **8 MiB encoded bytes per
field** (including an entire document), and decodes one document at a time.
Memory use includes the decoded document and SQLite's bounded page cache; 8 MiB
is an input-message limit, not a claim about peak process memory. Prepared writes
are batched in one transaction per snapshot, without accumulating the whole index.
Oversized fields fail with an error rather than being skipped. Database schema
version is **1**; unsupported schema versions are rejected without migration.

## Inspect imported snapshots

```sh
ripplemap snapshots
ripplemap snapshots --json
```

Snapshots are ordered by repository and commit. JSON is an array containing
`id`, `repository`, `commit`, `indexed_at` (UTC), `indexer_name`, `indexer_version`,
`index_digest` (hex SHA-256), and counts for `documents`, `definitions`,
`references`, and `symbol_less_occurrences`. The last count records occurrences
without a symbol (for example, highlighting); these are excluded from usages.
Rows marked as definitions by SCIP are kept separately from references through
SQLite views over the occurrence store. Snapshot IDs are local database handles;
repository, commit, and digest describe the evidence.

Listing opens an existing database read-only; a missing database is an error.

## Check the Rust implementation

```sh
cargo test --locked
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
```

Tests use the tiny checked-in SCIP files; they need no Node installation, indexer,
or network after Cargo dependencies are installed.

## Validate the semantic fixture

Prerequisite: npm and a Node version capable of installing the pinned tooling.
`npm ci` installs Node 24.22.2 locally along with the locked indexer and compiler.
From the repository root:

```sh
npm ci --registry=https://registry.npmjs.org
npm run check:semantic
```

The check decodes the three checked-in real SCIP files. It asserts an exact join
between the library definition and the importing consumer, including an aliased
call, while excluding the retained-only consumer and an unrelated same-named
function. It also checks the expected source ranges and version-specific identity.
No network or index generation is involved in the check itself after installation.

## Compile the fixture and regenerate indexes

From the repository root, after `npm ci`:

```sh
for package in library removed-consumer retained-consumer; do
  node_modules/node/bin/node node_modules/typescript/bin/tsc \
    -p "fixtures/semantic/$package" || exit
  node_modules/node/bin/node node_modules/@sourcegraph/scip-typescript/dist/src/main.js \
    index --cwd "fixtures/semantic/$package" --no-progress-bar \
    --output index.scip || exit
done
npm run check:semantic
shasum -a 256 fixtures/semantic/*/index.scip
```

Each project is indexed in a separate invocation with its own project root.
Npm workspaces supply the exact `@ripplemap/library@1.0.0` dependency via a local
symlink; no TypeScript `paths` alias or name-based join is used.
Indexing is never a hidden side effect of a lookup or check.

See [semantic findings and provenance](docs/semantic-foundation.md) for raw symbols,
ranges, external-symbol observations, and limitations.

## Limitations

Zero observed references are never proof that a change is safe. This fixture
proves identity joining only; it does not establish complete consumer coverage,
public API extraction, signature compatibility, or transitive impact.

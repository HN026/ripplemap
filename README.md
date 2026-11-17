# Ripplemap

Ripplemap is a local developer tool for investigating the **observed** downstream
impact of library API changes using SCIP semantic indexes. Symbol identities
include package names and exact versions, so references are matched by semantic
identity rather than function names.

The Rust CLI imports pre-generated SCIP indexes into a local SQLite database.
Use `ingest`, `snapshots`, and `usages` to import and inspect evidence.
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

## Register local repositories

Registration records explicit local Git checkout or workspace paths in a JSON
manifest. It does not generate a SCIP index, install packages, or run project code.

```sh
ripplemap repositories register --repository removed-consumer \
  --path fixtures/semantic/removed-consumer
ripplemap repositories list --json
```

The default manifest is `ripplemap.json` (ignored by Git). Use `--manifest PATH`
with either repository command to select another collection. Registration writes
canonical absolute paths atomically. Repeating the same ID/path is idempotent;
conflicting IDs and duplicate paths are errors. A hand-written manifest can use
paths relative to its own directory:

```json
{
  "schema_version": 1,
  "repositories": [
    { "id": "consumer", "path": "./consumer" }
  ]
}
```

Registered paths must exist in a Git checkout. IDs are explicit identifiers and
should match the repository identifiers supplied when importing SCIP indexes.

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

## Find exact-symbol usages

Quote the full SCIP symbol, including its package version and descriptors:

```sh
ripplemap usages \
  --symbol 'scip-typescript npm @ripplemap/library 1.0.0 src/`index.ts`/removed().'
ripplemap usages \
  --symbol 'scip-typescript npm @ripplemap/library 1.0.0 src/`index.ts`/removed().' \
  --json
```

With the imported `removed-consumer` fixture this returns **three references in
one repository**: the imported name, its alias binding, and the aliased call.
The call starts at `src/index.ts:3:23`. Definitions, the unrelated same-named
function, and references to a different package version are excluded. Import
occurrences count as references, not as calls.

Lookup searches **all stored commits**, not just each repository's newest one.
Reference counts include separate snapshots of the same repository; repository
counts are distinct. Results are ordered by repository, commit, path, range, and
symbol roles. The default display limit is 100 references; `--limit` accepts 1–1000.
Totals remain exact when the displayed list is truncated.

Package metadata comes from the SCIP symbol header, not a manifest or lockfile;
it does not independently verify the installed dependency version. Unknown
package components are JSON `null`. Global symbol strings are compared exactly,
without version stripping or fuzzy matching. Local SCIP IDs are stored with their
document scope and cannot be queried by the global-symbol command.

### JSON and locations

`usages --json` returns one object:

| Field | Meaning |
| --- | --- |
| `symbol`, `package` | Raw identity and parsed `scheme`, `manager`, `name`, `version` |
| `indexed_snapshots` | Total stored snapshots searched, including those without matches |
| `total_references`, `observed_repositories` | Full matching reference count and distinct repository count |
| `references` | Displayed locations with repository, commit, timestamp, digest, and snapshot ID |
| `truncated` | Whether the display limit omitted matching references |
| `coverage_note` | Scope of the static evidence and its safety limitation |

Each reference has `path`, `start`, `end`, `position_encoding`, and `symbol_roles`.
Start/end positions contain `line` and `column`: both are **one-based**, with an
**exclusive end**. Columns count code units in the document's declared encoding
(`utf-8`, `utf-16`, or `utf-32`), not rendered screen columns. For example, a UTF-16
surrogate pair occupies two column units. SQLite preserves the original zero-based
coordinates. A reference also carries its exact `commit`, `indexed_at`, and
`index_digest`; no current checkout is substituted or source file opened.

Machine-readable JSON goes only to stdout. Import confirmations and errors go
to stderr. A command exits **0** when it completes, including a successful lookup
with zero references; **1** for database, ingestion, decoding, lookup, or output
failure; **2** for invalid CLI arguments. A reference count is not an impact exit
status or proof of compatibility.

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

Only exact static references in imported snapshots are supported. SCIP protocol
version 0 is accepted, with legacy three/four-coordinate ranges and typed
single/multiline ranges. Unspecified/unknown position encodings, malformed ranges,
unsafe document paths, duplicate documents, and unknown top-level fields fail the
whole import. Optional documentation, diagnostics, highlighting, and symbol
relationships are not analyzed. An empty `external_symbols` table is valid;
occurrences still retain full identities. External symbol information without an
occurrence does not create a usage.

Repository/commit/time metadata is supplied by the caller; Ripplemap does not
verify it against a checkout. The source-workspace fixture is validated; packaged
declaration-file identities and cross-version API mappings are not. References
need not have a matching producer definition in the supplied database.

Zero observed references are never proof that a change is safe. Results do not
establish complete consumer coverage, freshness, public API compatibility, or
transitive impact.

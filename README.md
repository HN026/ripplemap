# Ripplemap

Ripplemap is a local developer tool for investigating the **observed** downstream
impact of library API changes using SCIP semantic indexes. Symbol identities
include package names and exact versions, so references are matched by semantic
identity rather than function names.

The Rust CLI imports pre-generated SCIP indexes into a local SQLite database.
Use `ingest`, `snapshots`, and `usages` to import and inspect semantic evidence.
Use `repositories` and `dependencies` to register local paths and inspect committed
npm dependency evidence. Use `api extract`, `api resolve`, and `api bridge` to
connect supported packaged public exports to indexed consumers. `analyze` compares
explicit base/head API snapshots, reports observed consumer impact, and saves the
run for later inspection with `runs`. `health` checks recorded repository
compatibility within explicit dependency or TypeScript check profiles.
Each snapshot records its repository, exact commit, successful index-generation
time, indexer version, and SHA-256 digest. Index generation is an explicit external
step. Ripplemap does not run an indexer during ingestion.

## Install and import an index

Requires Rust 1.95 or later and a C compiler for bundled SQLite. Git is required
for repository registration, dependency inspection, and tests. Node is needed only
for TypeScript API extraction, the optional compiler health profile, and fixture
tooling.

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
version is **3**; unsupported schema versions are rejected without migration.
Databases created with schema 1 or 2 are left untouched. Select a fresh
file with `--database`, reimport the SCIP files, and rescan dependency evidence.

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
canonical absolute paths atomically; concurrent registration writers are not supported. Repeating the same ID/path is idempotent;
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

## Inspect npm dependencies

`dependencies scan` requires a registered path and an already imported snapshot
with the same repository ID and exact commit. It reads committed Git blobs from
that revision, including the nearest npm lockfile in the path or its Git
ancestors. It does not read dependency declarations from the working tree, install
packages, run project scripts, or generate an index.

```sh
# Choose the full commit recorded when your SCIP index was generated.
# indexed_commit must contain that exact Git object ID.
ripplemap dependencies scan --repository removed-consumer \
  --commit "$indexed_commit" --json
ripplemap dependencies list --repository removed-consumer \
  --commit "$indexed_commit" --json
```

Use `--manifest PATH` with `dependencies scan` to select another repository
collection. `dependencies list` reads saved evidence without requiring the
checkout or manifest. Scanning replaces that snapshot's dependency evidence
transactionally; failed scans preserve previous evidence. Reimporting its SCIP
snapshot invalidates the old dependency evidence and requires another scan.

### Run the dependency demo

The checked-in collection `fixtures/semantic/repositories.json` registers both
consumers with paths relative to that manifest. From this checkout, using a fresh
database:

```sh
demo_commit="$(git rev-parse HEAD)"
cargo run --locked -- --database ripplemap-m3.sqlite3 ingest \
  --scip fixtures/semantic/removed-consumer/index.scip \
  --repository removed-consumer --commit "$demo_commit" \
  --indexed-at 2026-11-17T15:58:29Z
cargo run --locked -- --database ripplemap-m3.sqlite3 dependencies scan \
  --manifest fixtures/semantic/repositories.json \
  --repository removed-consumer --commit "$demo_commit"
cargo run --locked -- --database ripplemap-m3.sqlite3 dependencies list \
  --repository removed-consumer --commit "$demo_commit" --json
```

The timestamp above records the shipped fixture's successful regeneration (see
provenance below). The demo explicitly selects this checkout's commit because
its fixture sources were verified. For another repository, supply its recorded
index commit and actual successful generation time; do not substitute a branch
or assume the current checkout matches an older index.

The report resolves `@ripplemap/library@1.0.0` as a direct production dependency,
`@ripplemap/retained-consumer@1.0.0` as a direct development dependency, and
`typescript@6.0.3` as an optional peer. It also records the retained consumer's
transitive declarations, including the cycle back to the removed consumer.
A declared dependency does not imply an API reference: the retained consumer's
source still only calls the retained library function.

### Dependency evidence format and scope

Terminal output shows versions, relationship/kind, resolution status, file
pointers, warnings, and SHA-256 digests. JSON contains `snapshot` (the exact stored
SCIP snapshot metadata) and `evidence`:

| Field | Meaning |
| --- | --- |
| `files` | Committed evidence paths and SHA-256 digests |
| `packages` | Package names, versions, workspace flags, and lockfile locations |
| `dependencies` | Requested names/ranges, declaring package, kind, direct flag, resolved target, status, and evidence pointers |
| `issues` | Missing or unsupported evidence and workspace identity discrepancies |

`declared_at` and every `resolved_at` entry use a Git-relative `file` and an RFC 6901
JSON `pointer`. Resolved targets refer to `packages[].location`, retaining nested
installation paths and distinct versions. Workspace resolution records both the
installation link and its target. Aliases retain the requested name separately
from the actual locked package name.

`direct: true` means declared by the registered package, or by the root/workspace
packages when registering a whole npm workspace root. Other edges are transitive.
`kind` is `production`, `development`, `peer`, or `optional`, as declared by the
edge's parent package; it does not classify every downstream edge as production
or development reachability. Optional-peer metadata is retained separately.
Development declarations of installed third-party packages are not traversed.
Optional declarations override production declarations with the same name.

Supported inputs are npm package-lock and npm-shrinkwrap versions **2 and 3**, using
the `packages` section and nearest installation-path resolution. Shrinkwrap takes
precedence over package-lock in the same directory. Workspace patterns support
literal path segments and whole-segment `*`; workspace membership comes from the
locked paths. Other patterns, package links outside declared workspaces, npm v1,
Yarn, and pnpm are unsupported. A stale lockfile can omit an unlocked wildcard
workspace; this inspection does not establish exhaustive workspace coverage.

Resolution status is `resolved`, `unresolved`, or `unsupported`. A missing lockfile
retains known declarations as unresolved and records missing transitive/workspace
coverage. Missing/unsupported evidence is report data and exits 0 when inspection
completes; malformed JSON, an unavailable exact commit, missing snapshot, Git
failure, or storage failure exits 1. No report is saved on execution failure.
Dependency files must be regular committed files and are capped at 32 MiB each.

A resolved dependency is a lockfile fact, not proof of installed contents, version
constraint satisfaction, peer compatibility, complete freshness, or API use.
These dependency inspection commands do not produce compatibility status; use
`health` for the supported consistency and compatibility checks.

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

## Resolve packaged public APIs

`api extract` reads a registered package at an **explicit full Git commit**. It
runs the supplied TypeScript compiler helper against committed files, verifies
that checked-in declarations equal an in-memory declaration emit, and joins
compiler-derived definition ranges to imported source and declaration SCIP
snapshots. It does not build the checkout, install dependencies, execute project
scripts, or generate indexes. Install the pinned tooling with `npm ci` first.
The helper path is trusted executable code; use the supplied local helper.

Prepare the library's source and generated declaration indexes separately. For
the packaged fixture, explicit generation commands are in
`scripts/regenerate-packaged.mjs`. Register the library package directory and
import its two indexes with distinct repository IDs, recording the **same exact
source commit**, actual generation time, and package version. Import consumer
indexes separately with their own exact commits. For example, after these imports:

```sh
ripplemap repositories register --repository library-source --path /path/to/library
ripplemap api extract --repository library-source --commit "$library_commit" \
  --declarations-repository library-types --source-entry src/index.ts \
  --helper scripts/extract-api.mjs --node node_modules/node/bin/node \
  --output api-base.json
ripplemap api resolve --api api-base.json \
  --consumer-repository consumer --consumer-commit "$consumer_commit" \
  --symbol 'scip-typescript npm @ripplemap/packaged 1.0.0 dist/`functions.d.ts`/removed().' \
  --json
```

Source and declaration indexes use paths relative to the registered package.
Both must describe the committed inputs; caller-supplied index provenance is
trusted, as with ingestion. Missing commits, files, or snapshots are errors.
Extraction writes the output artifact atomically after validation succeeds.
`--manifest PATH` selects the registered collection. Extraction uses at most 256
committed input files totaling 8 MiB: package/config files and TypeScript sources,
including declarations. API JSON files are limited to 8 MiB.

Resolution uses the exact declaration symbol and one explicitly selected consumer
snapshot. Raw source and declaration symbols are retained. The fixture's `legacy`
export maps to the `removed` implementation and two consumer references, including
the call at `src/index.ts:4:23`. Its original import token has only a local SCIP
identity, so that token is not included in global usage results. Unrelated packages
and different versions are excluded from the resolved mapping.

Public `retained` and `kept` aliases share one declaration target. Their raw
consumer symbol cannot distinguish which public binding was used, so Ripplemap
returns both candidates with `ambiguous` status rather than choosing one.

### Explicit cross-version binding

Extract a second artifact using explicitly supplied head source/declaration
snapshots, then select a public binding to connect:

```sh
ripplemap api bridge --base api-base.json --head api-head.json \
  --export retained --json
```

A resolved bridge requires the same npm package, root entry point, and
compiler-verified named public export in both supplied artifacts, with unique
SCIP definition mappings. Internal function names and paths can differ. This
links a public contract identity across the two explicit versions; it does not
establish signature, behavior, or implementation compatibility. Versions and raw
symbols remain separate, and consumer usage sets are never combined implicitly.
A missing artifact or stored snapshot is an execution error, never an empty diff.

### API evidence and supported scope

All mappings use these statuses:

| Status | Meaning |
| --- | --- |
| `resolved` | Supported compiler sites join to unique exact-version SCIP definitions; resolution also has observed consumer references |
| `ambiguous` | Multiple definition identities or public aliases prevent selecting one mapping |
| `unsupported` | The construct or symbol scheme is outside the supported subset |
| `missing` | Required mapping, public binding, or observed reference is absent |

Supported packages have a self-contained TypeScript root entry point, a root
`types` declaration entry, and named exported function declarations,
including in-package aliases and re-exports. A simple root `exports` declaration
with `types` and `default` is accepted. TypeScript **6.0.3** and UTF-16 SCIP
coordinates are required. Values, default exports, overloads, and merged
constructs are recorded as unsupported. External re-exports, project references,
extended configs, path mappings, custom type roots, `typesVersions`, subpath
exports, and complex conditional exports are outside this subset. Unsupported
project layouts fail extraction with an explanation. The API identity commands do not classify changes; `analyze` supports the change
types described below.

API artifacts contain `producer` and `declarations` snapshot metadata, `evidence`
(package identity, compiler version, entry points, file digests, public export
sites and optional minimum-argument evidence), and `mappings` (statuses,
explanations, raw symbols). Compiler sites are
zero-based UTF-16 ranges with exclusive ends. Displayed usages retain the
one-based position format documented above. Treat artifacts as trusted local
evidence, not authenticated attestations. Resolve/bridge revalidate stored index
metadata, digests, and definition joins; they do not rerun the compiler or inspect
the current working tree. Replacing an index invalidates artifacts when its
recorded metadata changes; extract again.

`api resolve --json` returns status, explanation, raw symbol, package, entry point,
producer/declaration/consumer snapshots, candidate mappings, and a bounded `usages`
report (or null for local symbols). `api bridge --json` returns status, explanation,
public binding, both package identities, producer snapshots, and both mappings.
JSON goes to stdout; diagnostics go to stderr. Completed evidence results,
including ambiguous/unsupported/missing, exit **0**; execution/output failures exit
**1**, and invalid CLI arguments exit **2**. No impact-specific exit code is
produced. Zero observed references never establish safety.

Run the full manual CLI acceptance flow against the exact committed fixture
revision (temporary database and artifacts are removed afterward):

```sh
node scripts/check-api-cli.mjs "$(git rev-parse 5bdb6b2)"
```

This builds the Rust binary, imports all six real indexes, extracts committed
APIs, resolves consumers, checks alias ambiguity and an explicit version bridge,
and verifies that a failed extraction preserves its previous artifact. Normal
Rust tests use checked-in compiler evidence and require no Node or indexer.

## Compare library APIs and inspect consumer impact

Prepare `api-base.json` and `api-head.json` with `api extract`, using their exact
library revisions and matching source/declaration indexes. Both must describe the
same npm package and root entry point; versions stay explicit. Select the
**expected consumers and exact indexed commits** in a separate analysis manifest:

```json
{
  "schema_version": 1,
  "consumers": [
    { "repository": "consumer", "commit": "0123456789abcdef0123456789abcdef01234567" }
  ]
}
```

Replace the example commit with the consumer's actual full indexed Git object ID.
This manifest selects evidence; it does not register paths, discover repositories,
install packages, or index anything. It supports up to 256 consumers, one commit
per repository. Missing selected indexes are reported as `missing_index`; another
stored commit is never substituted. Repositories outside the manifest are not
searched, including other historical commits of a selected consumer.

```sh
ripplemap analyze --base api-base.json --head api-head.json \
  --consumers consumers.json --limit 100 --max-index-age-days 30
ripplemap analyze --base api-base.json --head api-head.json \
  --consumers consumers.json --json > analysis.json
# Use the run ID printed in the report:
ripplemap runs show --id 1 --json
ripplemap runs list --limit 100
```

Every successful analysis is saved in SQLite, including runs with evidence gaps
or no supported changes. Saving verifies that the input snapshots have not changed
during analysis. Execution failures do not create a partial run. Saved reports
contain the full API artifacts, selected consumer commits, index digests and
metadata, compiler-derived changes, bounded reference locations, options, analysis
version, and evaluation time. Replacing indexes does not rewrite saved results.
`runs show` reads the original report without requiring checkouts or rerunning the
analysis. The run list is ordered newest first and limited explicitly.

### Supported changes and matching rules

- `removed_export`: a supported named function binding exists in the base public
  exports and is absent from the explicit head public exports.
- `required_parameters_added`: a retained supported function's minimum positional
  argument count increased. This checks ordinary identifier parameters, accounting
  for optional/default parameters. Rest parameters, generic functions,
  destructuring, and explicit `this` parameters have no arity evidence. Artifacts
  made before arity extraction remain usable for removals; missing arity evidence
  is reported as unsupported for retained bindings.

The second classification identifies a concrete API change, not a proven compiler
failure at every reference: import occurrences and calls supplying enough arguments
can still be reported. Parameter types, return types, overload compatibility,
assignability, and behavior changes are unchecked. A retained binding is never
labelled fully compatible.

Consumer lookup uses **exact baseline declaration identities**, following the
compiler-to-SCIP mapping. It does not strip versions or automatically apply a
bridge to a different dependency version. Other-version reference counts and
observed versions are recorded separately as deferred evidence. Versions come
from semantic symbols, not an independently verified installation or lockfile.
If multiple public aliases share the baseline identity, the report shows their
names and places references under candidates, excluding them from the definite
observed-impact counts. Repository counts are distinct across all changes;
per-change reference counts can overlap when multiple bindings share a symbol.

### Coverage, output, and exit codes

The terminal report and JSON include the APIs before/after, classification,
explanation, raw identities, observed and candidate repository counts, locations,
consumer commits, index times/digests, missing indexes, unsupported mappings, and
truncation. Ordering is deterministic for the same supplied evidence. Run IDs and
evaluation times change on each run.

`--limit` (1–1000) bounds **all displayed references across the run**, ordered by
export, repository, and location. Full counts remain accurate when the display
is truncated. Each coverage record lists up to 64 sorted observed package versions
with a separate `versions_truncated` flag. `local_references` counts every local
symbol reference in that consumer; these cannot be attributed specifically to
this library. Known global import-token gaps are not filled with text matching.

Freshness is the caller-supplied index-generation time compared to the recorded
`evaluated_at`, using `--max-index-age-days` (default 30). Labels are
`within_age_threshold`, `stale`, `future_timestamp`, or `missing_index`. This is
an age check, not verification that a checkout, dependency lockfile, or installed
package still matches the index. Zero references do not establish safety, even
with every supplied consumer indexed.

`analyze --json` produces `{ "id": ..., "report": ... }`. The report contains
`format_version` (currently 1), `analysis_version`, `evaluated_at`, `options`,
full `base`/`head` API artifacts, normalized `consumers`, `coverage`, `comparison`
(`changes`, `issues`, `retained_exports`), `findings`, distinct
`observed_repositories`/`candidate_repositories`, `truncated`, and `scope_notes`.
Findings link to changes by export name and separate observed/candidate counts.
Locations use the one-based format documented for usages. JSON is stdout only;
errors are stderr only.

| Exit | `analyze` result |
| --- | --- |
| 0 | No supported observed impact or recorded evidence gaps within this narrow check scope; never a safety verdict |
| 1 | Analysis, input, storage, or output execution failed; missing base/head files or snapshots are errors |
| 2 | Invalid command-line arguments |
| 3 | Definite mapped baseline references were observed for a supported change; this takes precedence over evidence gaps |
| 4 | No definite observed impact, but candidate references or evidence gaps remain (including an empty consumer selection) |

Unsupported exports, missing indexes, stale/future consumer indexes, unmapped
baseline references, and deferred other-version references count as gaps.
`runs show`/`runs list` exit 0 when reading succeeds, regardless of a saved
analysis's outcome. An output failure can occur after a run has been saved; inspect
`runs list` before repeating an analysis if its output was interrupted.

The reproducible CLI check also runs this workflow against the shipped packaged
fixture and verifies exact saved-result replay:

```sh
node scripts/check-api-cli.mjs "$(git rev-parse 5bdb6b2)"
```

## Check repository compatibility health

`health` validates a registered repository at one explicitly selected indexed
commit. First import its SCIP index and run `dependencies scan` for that same
repository ID and full commit. No current/default branch or latest snapshot is
substituted.

```sh
ripplemap health --repository consumer --commit "$consumer_commit"
ripplemap health --repository consumer --commit "$consumer_commit" --json
```

The default registered collection is `ripplemap.json`; use `--manifest PATH` to
select another. To check a registered collection, supply an explicit selection
using the same schema as the analysis consumer manifest:

```json
{
  "schema_version": 1,
  "consumers": [
    { "repository": "consumer", "commit": "0123456789abcdef0123456789abcdef01234567" }
  ]
}
```

Replace the illustrative commit with the actual full indexed commit. Select
1–256 registered repositories, one commit each:

```sh
ripplemap health --manifest ripplemap.json --selection health-inputs.json --json
```

### Health profiles and outcomes

Every report names its profile and lists enabled/disabled checks. **Healthy is
limited to that profile and recorded evidence**, not all API or runtime behavior.

| Status | Meaning |
| --- | --- |
| `healthy` | All required, enabled checks passed for the recorded snapshot |
| `incompatible` | A supported check found a concrete violation |
| `unknown` | Required evidence is missing, stale, inconsistent, ambiguous, or unsupported |
| `check_failed` | A required validation could not execute or complete |

`dependencies-v1` enables:

- npm version-range checks for reachable production, development, optional, and
  peer declarations in the saved dependency graph. Named npm aliases with an
  explicit version range are supported. Prerelease matching follows npm range
  semantics. File/git/workspace protocols, tags, npm overrides, and unparsable
  requirements produce Unknown rather than a guessed match.
- Required peer compatibility using the recorded parent installation environment
  for installed third-party packages. A package's own nested copy cannot satisfy
  its peer. Optional dependencies/peers may be absent; if present, supported ranges
  must still match. Missing required installation paths are concrete violations
  only with otherwise supported, complete lockfile evidence.
- Evidence consistency: reinspection of exact committed dependency files must
  match saved evidence. Manifest identities/declarations are compared with locked
  metadata. Indexed document paths must exist at the selected revision, and
  referenced npm package identities/versions must appear in the reachable graph.
  Missing identity/version matches are Unknown, with an example location.
- Index age: the recorded generation timestamp must not be in the future or older
  than `--max-index-age-days` (default 30, configurable 0–36500).

This profile does not type-check source or establish that public exports exist.
It reads committed files even if the checkout is currently dirty or at another
revision. SCIP commit/timestamp metadata remains caller-attested: the check does
not authenticate indexed source contents or prove complete reference coverage.
Installed packages are not inspected by the dependency profile.

Old saved dependency evidence may differ after inspection rules change. Run
`dependencies scan` again for the exact snapshot to refresh it; health does not
silently rewrite saved evidence. Health checks are read-only and use schema 3;
health results are returned directly, rather than persisted as analysis runs.

### Explicit TypeScript compiler validation

Install the pinned tooling with `npm ci` first. To enable `typescript-v1`,
explicitly supply the trusted helper and Node executable:

```sh
ripplemap health --repository consumer --commit "$consumer_commit" \
  --compiler-helper scripts/check-typescript.mjs \
  --node node_modules/node/bin/node --compiler-timeout-seconds 60 --json
```

Supplying the helper authorizes its execution. The shipped helper uses TypeScript
6.0.3's compiler API with no emission, and does not execute project scripts,
plugins, an indexer, or package installation. This profile adds compiler errors
and supported missing-export diagnostics (`TS2305` and `TS2724`) to the dependency
checks. Diagnostics include the dependency when it can be identified from the
compiler's import/export declaration, file, one-based UTF-16 position, explanation,
and TypeScript code. Other compiler errors remain ordinary compiler diagnostics;
they are not automatically attributed to a dependency upgrade.

The whole Git checkout must be clean, including untracked files, and its HEAD
must equal the exact requested commit. Ignored installations are allowed. A dirty
checkout or another HEAD produces Unknown without running the helper. Install
dependencies yourself before checking: required package metadata must match locked
names/versions, including workspace identities. Missing or mismatched installation
evidence produces Unknown, never a misleading compiler pass. Matching metadata
is not proof of package integrity.

The compiler records its command, process exit code, TypeScript version, error
count, and hashes of decoded input files. Files are checked for changes during
validation; Git state and stored evidence are rechecked afterward. These are
consistency checks, not filesystem isolation. TypeScript still honors source
suppression directives. Project references, compiler plugins, `noResolve`, and
unchecked JavaScript inputs produce Unknown. `noCheck` and library-skip options
are disabled so they cannot turn the enabled validation into a silent pass.

The helper reads at most 4096 files totaling 64 MiB, returns at most 1000 compiler
diagnostics, and cannot emit files. Execution defaults to 60 seconds (1–300),
with an 8 MiB stdout and 64 KiB stderr limit. Timeouts, malformed results, missing
runtimes, and execution errors produce Check failed. The helper path itself is
trusted executable code; these limits are not a sandbox for arbitrary helpers.

### Health output and exit codes

`health --json` emits `{ "format_version": 1, "status": ..., "reports": [...] }`
for both single-repository and collection checks. Each report includes the exact
commit/snapshot, evaluation time, profile, enabled/disabled checks, per-dependency
resolution and status, structured diagnostics and evidence pointers, committed
file hashes, optional compiler evidence, and truncation metadata. Repository
ordering follows the explicit selection sorted by ID. No JSON diagnostics are
mixed into stderr; configuration/storage errors before checks start use stderr.

`--limit` (1–1000, default 100) bounds displayed diagnostics per repository, with
an exact `diagnostics_total` and `truncated` flag. Full per-dependency statuses and
overall status are computed before truncation. Compiler errors beyond its separate
1000-error display cap are recorded in `compiler.diagnostics_total`. Compiler
findings link to per-dependency statuses by package name. When multiple
installation paths have that name, individual copies remain Unknown candidates;
the repository still retains the concrete compiler violation. Per-dependency
constraint results remain subject to repository-level evidence gaps such as age
or missing source evidence.

Exit 0 means Healthy within the named profile; 1 means Check failed or command
execution failure; 2 means invalid CLI arguments; 3 means Incompatible; 4 means
Unknown. Collection precedence is Check failed, Incompatible, Unknown, Healthy.
All other findings remain visible, even when a higher-priority outcome determines
the exit code. No numerical risk score or complete safety claim is produced.

Run the manual compiler acceptance check after installing the pinned tooling:

```sh
node scripts/check-health-cli.mjs
```

It copies the existing small fixture into a disposable local Git repository,
links already installed tooling, verifies a healthy compiler check, removes an
export, checks the exact missing-export diagnostic, and checks execution failure,
timeout, and dirty-checkout handling. It performs no installation, index generation,
or modification to the original fixture. Normal Rust tests need no Node/indexer.

## Check the Rust implementation

```sh
cargo test --locked
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
```

Tests use the tiny checked-in fixture and temporary local Git repositories.
They require Git but no Node installation, indexer, or network after Cargo
dependencies are installed.

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

## Validate packaged-library evidence

The packaged fixture under `fixtures/semantic/packaged` includes two library
versions, generated JavaScript/declarations, and consumers installed from real
npm tarballs. Its public functions include aliases and re-exports. The second
version moves and renames an implementation while retaining its public export.

```sh
npm run check:packaged
# Explicit manual regeneration: compile, pack, install without scripts, and index.
node scripts/regenerate-packaged.mjs
```

The check uses TypeScript 6.0.3's compiler API, validates that packaged declarations
match source emission, and joins compiler-derived definition ranges to real SCIP
occurrences. `provenance.json` records the generation time and index hashes.
Regeneration changes machine-specific SCIP project-root metadata when the
checkout path differs. Generated tarballs and installations are ignored by Git.

Producer symbols use `src/*.ts`; consumer symbols use `dist/*.d.ts`. Declaration
indexes provide the exact versioned identities referenced by the consumers.
Public aliases can share one raw target symbol: the index alone then cannot tell
which public alias was used. A re-exported import token can also be recorded as a
local symbol even when its alias binding and call have global identities. The
fixture preserves these limits rather than filling them with text matches.

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
verify it against a checkout during ingestion. API extraction separately reads an
explicit committed revision and verifies compiler emission; it cannot attest
that a caller supplied the correct index for that revision. Raw `usages` lookups
do not require a matching producer definition. Packaged API resolution supports
only the subset and explicit version-binding rules described above.

Zero observed references are never proof that a change is safe. Results do not
establish complete consumer coverage, freshness, public API compatibility, or
transitive impact.

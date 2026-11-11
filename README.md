# Ripplemap

A local developer tool for reporting the **observed** downstream impact of library
API changes using semantic indexes. Initial ecosystem: TypeScript; implementation:
Rust. This repository currently contains the Milestone 1 semantic foundation proof.
The Rust library and CLI begin in Milestone 2.

## Reproduce the semantic proof

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
Normal Rust tests in later milestones will use these same checked-in indexes.

## Regenerate indexes (explicit manual acceptance)

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

## Milestones

- [x] 1. Validate semantic joining with a real, reproducible SCIP fixture.
- [ ] 2. Rust package, transactional SQLite ingestion, exact-symbol usages.
- [ ] 3. Removed-symbol impact reports and evidence quality.
- [ ] 4. Detect removed public exports from explicit base/head snapshots.
- [ ] 5. Complete CLI documentation, demo, exit codes, and minimal CI.

Zero observed references are never proof that a change is safe. This fixture
proves identity joining only; it does not establish complete consumer coverage,
public API extraction, signature compatibility, or transitive impact.

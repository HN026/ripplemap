# Ripplemap

Ripplemap is a local developer tool for investigating the **observed** downstream
impact of library API changes using SCIP semantic indexes. Symbol identities
include package names and exact versions, so references are matched by semantic
identity rather than function names.

The included TypeScript demo checks a library and two consumers. It shows which
consumer references a particular library function, with source locations, while
excluding an unrelated function with the same name.

## Run the demo

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

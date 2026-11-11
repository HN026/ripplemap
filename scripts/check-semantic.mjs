// Fixture acceptance check only; Ripplemap's implementation will be Rust.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fromBinary } from '@bufbuild/protobuf';
import { IndexSchema, PositionEncoding, SymbolRole } from '@scip-code/scip';

const packages = ['library', 'removed-consumer', 'retained-consumer'];
const indexes = Object.fromEntries(packages.map(name => [name, fromBinary(
  IndexSchema,
  readFileSync(new URL(`../fixtures/semantic/${name}/index.scip`, import.meta.url)),
)]));
const removed = 'scip-typescript npm @ripplemap/library 1.0.0 src/`index.ts`/removed().';
const retained = 'scip-typescript npm @ripplemap/library 1.0.0 src/`index.ts`/retained().';
const unrelated = 'scip-typescript npm @ripplemap/removed-consumer 1.0.0 src/`index.ts`/removed().';
const definition = occurrence => (occurrence.symbolRoles & SymbolRole.Definition) !== 0;
const occurrences = name => indexes[name].documents.flatMap(document =>
  document.occurrences.map(occurrence => ({ path: document.relativePath, ...occurrence })));
const references = (name, symbol) => occurrences(name)
  .filter(occurrence => !definition(occurrence) && occurrence.symbol === symbol);

for (const index of Object.values(indexes)) {
  assert.equal(index.metadata.toolInfo.name, 'scip-typescript');
  assert.equal(index.metadata.toolInfo.version, '0.6.2');
  assert.equal(index.documents.length, 1);
  assert.equal(index.documents[0].relativePath, 'src/index.ts');
  assert.equal(index.documents[0].positionEncoding, PositionEncoding.UTF16CodeUnitOffsetFromLineStart);
  assert.deepEqual(index.externalSymbols, []);
  assert.ok(index.documents[0].occurrences.every(occurrence => occurrence.diagnostics.length === 0));
}
assert.deepEqual(occurrences('library').filter(o => definition(o) && o.symbol === removed)
  .map(o => o.range), [[0, 16, 23]]);
assert.deepEqual(occurrences('library').filter(o => definition(o) && o.symbol === retained)
  .map(o => o.range), [[4, 16, 24]]);
assert.deepEqual(references('removed-consumer', removed).map(o => o.range), [
  [0, 9, 16], // imported name
  [0, 20, 27], // alias binding
  [2, 22, 29], // aliased call
]);
assert.deepEqual(references('retained-consumer', removed), []);
assert.deepEqual(references('retained-consumer', retained).map(o => o.range), [[0, 9, 17], [2, 22, 30]]);
assert.notEqual(unrelated, removed);
assert.deepEqual(occurrences('removed-consumer').filter(o => definition(o) && o.symbol === unrelated)
  .map(o => o.range), [[4, 9, 16]]);
assert.deepEqual(references('removed-consumer', unrelated).map(o => o.range), [[7, 25, 32]]);
assert.deepEqual(references('removed-consumer', removed.replace(' 1.0.0 ', ' 2.0.0 ')), []);

console.log(JSON.stringify({
  symbol: removed,
  definition: { repository: 'library', path: 'src/index.ts', range: [0, 16, 23] },
  affectedRepositories: ['removed-consumer'],
  references: references('removed-consumer', removed).map(({ path, range }) => ({ path, range })),
  unrelatedSymbol: unrelated,
  note: 'Exact semantic identity join passed. Ranges are zero-based UTF-16, end-exclusive.',
}, null, 2));

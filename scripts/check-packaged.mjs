import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { fromBinary } from '@bufbuild/protobuf';
import { IndexSchema } from '@scip-code/scip';
import { extract } from './extract-api.mjs';

const fixture = new URL('../fixtures/semantic/packaged/', import.meta.url);
const read = name => readFileSync(new URL(name, fixture));
const index = name => fromBinary(IndexSchema, read(name));
const range = r => r.length === 3 ? [r[0], r[1], r[0], r[2]] : r;
function definition(index, site) {
  return index.documents.find(d => d.relativePath === site.path).occurrences
    .filter(o => (o.symbolRoles & 1) !== 0 && JSON.stringify(range(o.range)) === JSON.stringify(site.range)).map(o => o.symbol);
}
const maps = [];
for (const version of [1, 2]) {
  const evidence = JSON.parse(read(`library-v${version}/exports.json`));
  const files = Object.fromEntries(evidence.files.map(file => [file.path, read(`library-v${version}/${file.path}`).toString()]));
  assert.deepEqual(extract({ files, source_entry: 'src/index.ts' }), evidence);
  const producer = index(`library-v${version}/source.scip`);
  const declarations = index(`library-v${version}/declarations.scip`);
  const mapping = new Map();
  for (const entry of evidence.exports) {
    if (entry.status === 'unsupported') continue;
    const source = definition(producer, entry.source);
    const types = definition(declarations, entry.declaration);
    assert.equal(source.length, 1);
    assert.equal(types.length, 1);
    assert.notEqual(source[0], types[0]);
    mapping.set(entry.name, { source: source[0], types: types[0] });
  }
  assert.equal(evidence.exports.find(e => e.name === 'unsupportedValue').status, 'unsupported');
  assert.deepEqual(mapping.get('kept'), mapping.get('retained'));
  assert.throws(() => extract({ files: { ...files, 'dist/index.d.ts': 'export {};\n' }, source_entry: 'src/index.ts' }), /does not match/);
  maps.push(mapping);
}
const consumer = index('consumer-v1/index.scip');
const legacy = consumer.documents[0].occurrences.filter(o => o.symbol === maps[0].get('legacy').types);
assert.deepEqual(legacy.map(o => o.range), [[0, 19, 26], [3, 22, 29]]);
assert.ok(consumer.documents[0].occurrences.some(o => o.symbol.startsWith('local ') && o.range[0] === 0));
assert.ok(consumer.documents[0].occurrences.some(o => o.symbol.includes('@ripplemap/library 1.0.0')));
assert.notEqual(maps[0].get('retained').types, maps[1].get('retained').types);
assert.ok(maps[1].get('retained').source.endsWith('/stable().'));
assert.equal(maps[1].has('legacy'), false);
const provenance = JSON.parse(read('provenance.json'));
for (const [name, digest] of Object.entries(provenance.indexes)) assert.equal(createHash('sha256').update(read(name)).digest('hex'), digest);
console.log('Packaged semantic proof passed: compiler-verified declarations, exact semantic sites, aliases, version separation, and local-token coverage limit.');

// Manual acceptance only: Rust CLI + compiler helper against a supplied fixture commit.
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const commit = process.argv[2];
if (!/^[a-fA-F0-9]{40}$|^[a-fA-F0-9]{64}$/.test(commit ?? '')) throw new Error('Supply the exact fixture Git commit as the only argument');
const built = spawnSync('cargo', ['build', '--locked', '--target-dir', join(root, 'target')], { cwd: root, stdio: 'inherit' });
if (built.status !== 0) throw new Error('Cargo build failed');
const temporary = mkdtempSync(join(tmpdir(), 'ripplemap-api-demo-'));
const binary = join(root, 'target/debug/ripplemap');
const fixture = join(root, 'fixtures/semantic/packaged');
const manifest = join(temporary, 'repositories.json');
const provenance = JSON.parse(readFileSync(join(fixture, 'provenance.json'), 'utf8'));
const run = (args, succeeds = true) => {
  const result = spawnSync(binary, ['--database', join(temporary, 'index.sqlite3'), ...args], { cwd: root, encoding: 'utf8' });
  if (result.error) throw result.error;
  assert.equal(result.status, succeeds ? 0 : 1, result.stderr);
  return result;
};
const extraction = version => ['api', 'extract', '--manifest', manifest,
  '--repository', `source-v${version}`, '--declarations-repository', `types-v${version}`,
  '--commit', commit, '--source-entry', 'src/index.ts', '--helper', 'scripts/extract-api.mjs',
  '--node', 'node_modules/node/bin/node', '--output', join(temporary, `api-v${version}.json`)];
try {
  writeFileSync(manifest, JSON.stringify({ schema_version: 1, repositories: [1, 2].map(v => ({ id: `source-v${v}`, path: join(fixture, `library-v${v}`) })) }));
  for (const version of [1, 2]) {
    for (const [repository, index] of [
      [`source-v${version}`, `library-v${version}/source.scip`],
      [`types-v${version}`, `library-v${version}/declarations.scip`],
      [`consumer-v${version}`, `consumer-v${version}/index.scip`],
    ]) run(['ingest', '--scip', join(fixture, index), '--repository', repository, '--commit', commit, '--indexed-at', provenance.generated_at]);
    run(extraction(version));
  }
  const legacy = 'scip-typescript npm @ripplemap/packaged 1.0.0 dist/`functions.d.ts`/removed().';
  const retained = 'scip-typescript npm @ripplemap/packaged 1.0.0 dist/`functions.d.ts`/retained().';
  const lookup = symbol => JSON.parse(run(['api', 'resolve', '--api', join(temporary, 'api-v1.json'), '--consumer-repository', 'consumer-v1', '--consumer-commit', commit, '--symbol', symbol, '--json']).stdout);
  const resolved = lookup(legacy);
  assert.equal(resolved.status, 'resolved');
  assert.equal(resolved.usages.total_references, 2);
  assert.equal(resolved.candidates[0].name, 'legacy');
  assert.equal(lookup(retained).status, 'ambiguous');
  const bridged = JSON.parse(run(['api', 'bridge', '--base', join(temporary, 'api-v1.json'), '--head', join(temporary, 'api-v2.json'), '--export', 'retained', '--json']).stdout);
  assert.equal(bridged.status, 'resolved');
  assert.notDeepEqual(bridged.base_mapping.producer_symbols, bridged.head_mapping.producer_symbols);
  const before = readFileSync(join(temporary, 'api-v1.json'));
  const invalid = extraction(1);
  invalid[invalid.indexOf('--source-entry') + 1] = 'src/missing.ts';
  const failed = run(invalid, false);
  assert.match(failed.stderr, /Missing explicit source entry/);
  assert.deepEqual(readFileSync(join(temporary, 'api-v1.json')), before);
  console.log('API CLI acceptance passed: exact committed extraction, packaged resolution, alias ambiguity, explicit version bridge, and atomic failure preservation.');
} finally {
  rmSync(temporary, { recursive: true, force: true });
}

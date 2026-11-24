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
/**
 * Execute the CLI against the temporary database and require the expected exit code.
 */
const run = (args, expectedExit = 0) => {
  const result = spawnSync(binary, ['--database', join(temporary, 'index.sqlite3'), ...args], { cwd: root, encoding: 'utf8' });
  if (result.error) throw result.error;
  assert.equal(result.status, expectedExit, result.stderr);
  return result;
};
/**
 * Build explicit API extraction arguments for one packaged fixture version.
 */
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
  /**
   * Resolve an exact declaration symbol in the explicitly selected consumer snapshot.
   */
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
  const failed = run(invalid, 1);
  assert.match(failed.stderr, /Missing explicit source entry/);
  assert.deepEqual(readFileSync(join(temporary, 'api-v1.json')), before);
  const consumers = join(temporary, 'consumers.json');
  writeFileSync(consumers, JSON.stringify({ schema_version: 1, consumers: [
    { repository: 'consumer-v1', commit },
    { repository: 'consumer-v2', commit },
    { repository: 'missing-consumer', commit },
  ] }));
  const analyze = ['analyze', '--base', join(temporary, 'api-v1.json'), '--head', join(temporary, 'api-v2.json'), '--consumers', consumers, '--json'];
  const analysis = run(analyze, 3);
  const saved = JSON.parse(analysis.stdout);
  assert.equal(saved.report.comparison.changes[0].export, 'legacy');
  assert.equal(saved.report.comparison.changes[0].kind, 'removed_export');
  assert.equal(saved.report.observed_repositories, 1);
  assert.equal(saved.report.findings[0].observed_references, 2);
  assert.equal(saved.report.coverage.find(c => c.requested.repository === 'missing-consumer').freshness, 'missing_index');
  assert.ok(saved.report.coverage.find(c => c.requested.repository === 'consumer-v2').other_version_references > 0);
  assert.equal(run(['runs', 'show', '--id', String(saved.id), '--json']).stdout, analysis.stdout);
  const unchanged = [...analyze];
  unchanged[unchanged.indexOf('--head') + 1] = join(temporary, 'api-v1.json');
  assert.equal(JSON.parse(run(unchanged, 4).stdout).report.comparison.changes.length, 0);
  const missingHead = [...analyze];
  missingHead[missingHead.indexOf('--head') + 1] = join(temporary, 'missing.json');
  assert.equal(run(missingHead, 1).stdout, '');
  assert.equal(JSON.parse(run(['runs', 'list', '--json']).stdout).length, 2);
  console.log('API CLI acceptance passed: committed extraction, packaged resolution, alias ambiguity, explicit version bridge, removal impact, honest coverage, saved replay, and atomic failure preservation.');
} finally {
  rmSync(temporary, { recursive: true, force: true });
}

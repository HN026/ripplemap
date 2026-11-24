// Manual acceptance: reuses the semantic fixture; no installs, indexing, or changes to the original checkout.
import assert from 'node:assert/strict';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const temporary = mkdtempSync(join(tmpdir(), 'ripplemap-health-'));
const checkout = join(temporary, 'checkout');
const binary = join(root, 'target/debug/ripplemap');
const node = join(root, 'node_modules/node/bin/node');
const helper = join(root, 'scripts/check-typescript.mjs');
const manifest = join(temporary, 'repositories.json');
function command(executable, args, cwd = root, expected = 0) {
  const result = spawnSync(executable, args, { cwd, encoding: 'utf8' });
  if (result.error) throw result.error;
  assert.equal(result.status, expected, result.stdout + result.stderr);
  return result;
}
function git(args) {
  return command('git', ['-c', 'user.name=Ripplemap fixture', '-c', 'user.email=test@example.invalid', '-c', 'commit.gpgsign=false', '-c', 'core.hooksPath=/dev/null', ...args], checkout).stdout.trim();
}
function commit() { git(['add', '.']); git(['commit', '-m', 'fixture']); return git(['rev-parse', 'HEAD']); }
const cli = (args, expected = 0) => command(binary, ['--database', join(temporary, 'index.sqlite3'), ...args], root, expected);
function ingest(commit) {
  cli(['ingest', '--scip', join(checkout, 'fixtures/semantic/removed-consumer/index.scip'), '--repository', 'consumer', '--commit', commit, '--indexed-at', new Date().toISOString()]);
  cli(['dependencies', 'scan', '--manifest', manifest, '--repository', 'consumer', '--commit', commit]);
}
const health = (commit, expected = 0, extra = []) => JSON.parse(cli(['health', '--manifest', manifest, '--repository', 'consumer', '--commit', commit, ...(extra.includes('--compiler-helper') ? [] : ['--compiler-helper', helper]), ...(extra.includes('--node') ? [] : ['--node', node]), '--json', ...extra], expected).stdout);
try {
  command('cargo', ['build', '--locked']);
  mkdirSync(checkout);
  for (const file of ['package.json', 'package-lock.json']) copyFileSync(join(root, file), join(checkout, file));
  for (const pkg of ['library', 'removed-consumer', 'retained-consumer']) {
    const relative = `fixtures/semantic/${pkg}`;
    mkdirSync(join(checkout, relative, 'src'), { recursive: true });
    for (const file of ['package.json', 'tsconfig.json', 'src/index.ts', 'index.scip']) copyFileSync(join(root, relative, file), join(checkout, relative, file));
  }
  writeFileSync(join(checkout, '.gitignore'), 'node_modules/\n');
  mkdirSync(join(checkout, 'node_modules/@ripplemap'), { recursive: true });
  for (const pkg of ['library', 'removed-consumer', 'retained-consumer']) symlinkSync(join(checkout, 'fixtures/semantic', pkg), join(checkout, 'node_modules/@ripplemap', pkg), 'dir');
  symlinkSync(join(root, 'node_modules/typescript'), join(checkout, 'node_modules/typescript'), 'dir');
  git(['init', '-b', 'main']);
  let revision = commit();
  writeFileSync(manifest, JSON.stringify({ schema_version: 1, repositories: [{ id: 'consumer', path: join(checkout, 'fixtures/semantic/removed-consumer') }] }));
  ingest(revision);
  const healthy = health(revision);
  assert.equal(healthy.status, 'healthy');
  assert.equal(healthy.reports[0].profile, 'typescript-v1');
  assert.ok(healthy.reports[0].compiler.files.length > 0);
  // Explicit source change: consumer source and checked-in index stay unchanged.
  writeFileSync(join(checkout, 'fixtures/semantic/library/src/index.ts'), 'export function retained(value: number): number { return value * 2; }\n');
  revision = commit();
  ingest(revision);
  const incompatible = health(revision, 3);
  const diagnostic = incompatible.reports[0].diagnostics.find(d => d.check === 'missing_exports');
  assert.ok(diagnostic, JSON.stringify(incompatible));
  assert.equal(diagnostic.dependency, '@ripplemap/library');
  assert.equal(diagnostic.path, 'src/index.ts');
  assert.equal(diagnostic.start.line, 1);
  assert.equal(incompatible.reports[0].dependencies.find(d => d.dependency.name === '@ripplemap/library' && d.dependency.direct).status, 'incompatible');
  // The compiler names a package, but cannot identify every installed copy by name alone.
  const nested = 'fixtures/semantic/retained-consumer/node_modules/@ripplemap/library';
  const lock = JSON.parse(readFileSync(join(checkout, 'package-lock.json'), 'utf8'));
  lock.packages[nested] = { version: '1.0.0' };
  writeFileSync(join(checkout, 'package-lock.json'), JSON.stringify(lock));
  mkdirSync(join(checkout, nested), { recursive: true });
  copyFileSync(join(checkout, 'fixtures/semantic/library/package.json'), join(checkout, nested, 'package.json'));
  revision = commit();
  ingest(revision);
  const ambiguous = health(revision, 3);
  assert.ok(ambiguous.reports[0].dependencies.filter(d => d.dependency.name === '@ripplemap/library').every(d => d.status === 'unknown'));
  const failed = health(revision, 1, ['--node', join(temporary, 'missing-node')]);
  assert.equal(failed.status, 'check_failed');
  const hanging = join(temporary, 'hanging.mjs');
  writeFileSync(hanging, 'setInterval(() => {}, 1000);\n');
  assert.equal(health(revision, 1, ['--compiler-helper', hanging, '--compiler-timeout-seconds', '1']).status, 'check_failed');
  const malformed = join(temporary, 'malformed.mjs');
  writeFileSync(malformed, 'console.log("invalid JSON");\n');
  assert.equal(health(revision, 1, ['--compiler-helper', malformed]).status, 'check_failed');
  writeFileSync(join(checkout, 'fixtures/semantic/removed-consumer/src/index.ts'), '// dirty source\n');
  assert.equal(health(revision, 4).status, 'unknown');
  assert.equal(git(['rev-parse', 'HEAD']), revision);
  console.log('Health CLI acceptance passed: healthy compiler profile, missing export with source/dependency evidence, execution failure, timeout, and dirty-checkout Unknown.');
} finally { rmSync(temporary, { recursive: true, force: true }); }

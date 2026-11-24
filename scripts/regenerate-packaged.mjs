// Explicit manual acceptance: builds/installs this fixture and runs the external indexer.
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { extract } from './extract-api.mjs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const fixture = resolve(root, 'fixtures/semantic/packaged');
const node = resolve(root, 'node_modules/node/bin/node');
const tsc = resolve(root, 'node_modules/typescript/bin/tsc');
const indexer = resolve(root, 'node_modules/@sourcegraph/scip-typescript/dist/src/main.js');
/**
 * Execute one explicit fixture generation command with the pinned local Node toolchain.
 */
function run(command, args, cwd) {
  const result = spawnSync(command, args, { cwd, stdio: 'inherit', env: { ...process.env, PATH: `${dirname(node)}:${process.env.PATH}` } });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} exited ${result.status}`);
}
mkdirSync(resolve(fixture, 'artifacts'), { recursive: true });
for (const version of [1, 2]) {
  const library = resolve(fixture, `library-v${version}`);
  const consumer = resolve(fixture, `consumer-v${version}`);
  run(node, [tsc, '-p', '.'], library);
  run(node, [indexer, 'index', '--no-progress-bar', '--output', 'source.scip'], library);
  run(node, [indexer, 'index', 'tsconfig.declarations.json', '--no-progress-bar', '--output', 'declarations.scip'], library);
  run('npm', ['pack', '--ignore-scripts', '--pack-destination', '../artifacts'], library);
  run('npm', ['install', '--ignore-scripts', '--registry=https://registry.npmjs.org'], consumer);
  run(node, [tsc, '-p', '.'], consumer);
  run(node, [indexer, 'index', '--no-progress-bar'], consumer);
}

for (const version of [1, 2]) {
  const library = resolve(fixture, `library-v${version}`);
  const files = {};
  for (const directory of ['src', 'dist']) {
    for (const name of readdirSync(resolve(library, directory))) {
      if (name.endsWith('.ts')) files[`${directory}/${name}`] = readFileSync(resolve(library, directory, name), 'utf8');
    }
  }
  for (const name of ['package.json', 'tsconfig.json']) files[name] = readFileSync(resolve(library, name), 'utf8');
  writeFileSync(resolve(library, 'exports.json'), JSON.stringify(extract({ files, source_entry: 'src/index.ts' }), null, 2) + '\n');
}
const indexes = {};
for (const version of [1, 2]) {
  for (const name of [`library-v${version}/source.scip`, `library-v${version}/declarations.scip`, `consumer-v${version}/index.scip`]) {
    indexes[name] = createHash('sha256').update(readFileSync(resolve(fixture, name))).digest('hex');
  }
}
writeFileSync(resolve(fixture, 'provenance.json'), JSON.stringify({
  generated_at: new Date().toISOString(), indexer: 'scip-typescript', indexer_version: '0.6.2',
  typescript_version: '6.0.3', command: 'node scripts/regenerate-packaged.mjs', indexes,
}, null, 2) + '\n');
run(node, [resolve(root, 'scripts/check-packaged.mjs')], root);

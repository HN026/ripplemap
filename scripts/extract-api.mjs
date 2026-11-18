// Explicit compiler helper. Input is a committed file map; no project code or indexer runs.
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import ts from 'typescript';
import { virtualProject, checkDiagnostics } from './typescript/host.mjs';
import { publicExports } from './typescript/exports.mjs';

export function extract(input) {
  const { root, files, host } = virtualProject(input);
  const read = name => {
    const text = files.get(path.resolve(root, name));
    if (text === undefined) throw new Error(`Missing committed file: ${name}`);
    return text;
  };
  const pkg = JSON.parse(read('package.json'));
  if (typeof pkg.name !== 'string' || !pkg.name || typeof pkg.version !== 'string' || !pkg.version) throw new Error('Package requires an exact name and version');
  if (pkg.typesVersions) throw new Error('Unsupported package layout: typesVersions');
  const declarationEntry = typeof pkg.types === 'string' ? pkg.types.replace(/^\.\//, '') : pkg.types;
  if (typeof declarationEntry !== 'string' || !declarationEntry.endsWith('.d.ts')) throw new Error('Unsupported package layout: a types entry ending in .d.ts is required');
  if (pkg.exports) {
    if (Object.keys(pkg.exports).join() !== '.' || typeof pkg.exports['.']?.types !== 'string' || pkg.exports['.'].types.replace(/^\.\//, '') !== declarationEntry) throw new Error('Unsupported package exports: only a root exports.types matching package.types is supported');
    if (Object.keys(pkg.exports['.']).some(k => !['types', 'default'].includes(k)) || (pkg.exports['.'].default !== undefined && typeof pkg.exports['.'].default !== 'string')) throw new Error('Unsupported conditional package exports');
  }
  const parsed = ts.parseConfigFileTextToJson('tsconfig.json', read('tsconfig.json'));
  if (parsed.error) throw new Error(ts.flattenDiagnosticMessageText(parsed.error.messageText, '\n'));
  if (parsed.config.extends || parsed.config.references) throw new Error('Unsupported project layout: extends/references');
  const configured = parsed.config.compilerOptions ?? {};
  if (['plugins', 'paths', 'baseUrl', 'rootDirs', 'typeRoots'].some(k => configured[k] !== undefined) || (configured.types?.length ?? 0) !== 0) throw new Error('Unsupported compiler resolution customization');
  const converted = ts.convertCompilerOptionsFromJson(configured, root);
  if (converted.errors.length) throw new Error(converted.errors.map(d => ts.flattenDiagnosticMessageText(d.messageText, '\n')).join('\n'));
  const options = { ...converted.options, declaration: true, noEmit: false, emitDeclarationOnly: true, incremental: false, composite: false, types: [] };
  const sourceEntry = input.source_entry;
  if (typeof sourceEntry !== 'string' || !files.has(path.resolve(root, sourceEntry))) throw new Error('Missing explicit source entry');
  const sourceProgram = ts.createProgram([path.resolve(root, sourceEntry)], options, host);
  checkDiagnostics(sourceProgram);
  const emitted = new Map();
  const result = sourceProgram.emit(undefined, (name, text) => {
    if (name.endsWith('.d.ts')) emitted.set(name, text);
  });
  if (result.emitSkipped || result.diagnostics.some(d => d.category === ts.DiagnosticCategory.Error)) throw new Error('Declaration emission failed');
  if (!emitted.has(path.resolve(root, declarationEntry))) throw new Error('Declared types entry was not emitted from the selected source entry');
  for (const [name, text] of emitted) {
    if (files.get(name) !== text) throw new Error(`Declaration does not match compiler emission: ${path.relative(root, name)}`);
  }
  const declarationProgram = ts.createProgram([path.resolve(root, declarationEntry)], { ...options, noEmit: true }, host);
  checkDiagnostics(declarationProgram);
  const hashes = [...files].map(([name, text]) => ({ path: path.relative(root, name).split(path.sep).join('/'), sha256: createHash('sha256').update(text).digest('hex') })).sort((a, b) => a.path.localeCompare(b.path, 'en'));
  return {
    format_version: 1, typescript_version: ts.version,
    package: { manager: 'npm', name: pkg.name, version: pkg.version },
    entrypoint: '.', source_entry: sourceEntry, declaration_entry: declarationEntry,
    files: hashes,
    exports: publicExports(sourceProgram, path.resolve(root, sourceEntry), declarationProgram, path.resolve(root, declarationEntry), root),
  };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { console.log(JSON.stringify(extract(JSON.parse(readFileSync(0, 'utf8'))), null, 2)); }
  catch (error) { console.error(`API extraction failed: ${error.message}`); process.exitCode = 1; }
}

// Explicitly enabled compiler check: reads sources/declarations, never emits or runs project code.
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import ts from 'typescript';

const input = JSON.parse(readFileSync(0, 'utf8'));
const hashes = new Map();
let totalBytes = 0;
function read(file) {
  const text = ts.sys.readFile(file);
  if (text !== undefined && !hashes.has(file)) {
    totalBytes += Buffer.byteLength(text);
    if (hashes.size >= 4096 || totalBytes > 64 * 1024 * 1024) throw new Error('Compiler input exceeds 4096 files or 64 MiB');
    hashes.set(file, createHash('sha256').update(text).digest('hex'));
  }
  return text;
}
function unknown(message) { return { status: 'unknown', message, typescript_version: ts.version, diagnostics: [], diagnostics_total: 0, files: [] }; }
function inspect() {
  if (ts.version !== '6.0.3') return unknown('Only TypeScript 6.0.3 is supported');
  for (const pkg of input.packages) {
    const file = path.join(input.lock_root, pkg.location, 'package.json');
    const text = read(file);
    if (text === undefined) return unknown(`Required installed package metadata unavailable: ${file}`);
    const actual = JSON.parse(text);
    if (actual.name !== pkg.name || actual.version !== pkg.version) return unknown(`Installed package identity differs from lockfile: ${file}`);
  }
  const config = path.join(input.project, 'tsconfig.json');
  if (!ts.sys.fileExists(config)) return unknown('No tsconfig.json at the registered path');
  const parsed = ts.getParsedCommandLineOfConfigFile(config, { noEmit: true, incremental: false, composite: false, noCheck: false, skipLibCheck: false, skipDefaultLibCheck: false }, {
    ...ts.sys, readFile: read, onUnRecoverableConfigFileDiagnostic: d => { throw new Error(ts.flattenDiagnosticMessageText(d.messageText, '\n')); },
  });
  if (!parsed) throw new Error('Could not parse TypeScript configuration');
  if (parsed.projectReferences?.length || parsed.options.plugins?.length) return unknown('Project references and compiler plugins are unsupported');
  if (parsed.options.noResolve || (parsed.options.allowJs && !parsed.options.checkJs)) return unknown('noResolve and unchecked JavaScript inputs are unsupported');
  if (!parsed.fileNames.length) return unknown('No TypeScript input files in this project');
  if (parsed.errors.length) throw new Error(parsed.errors.map(d => ts.flattenDiagnosticMessageText(d.messageText, '\n')).join('\n'));
  const host = ts.createCompilerHost(parsed.options);
  host.readFile = read;
  host.getSourceFile = (file, version) => {
    const text = read(file);
    return text === undefined ? undefined : ts.createSourceFile(file, text, version, true);
  };
  host.writeFile = () => { throw new Error('Unexpected compiler output'); };
  const program = ts.createProgram(parsed.fileNames, parsed.options, host);
  const diagnostics = ts.getPreEmitDiagnostics(program).filter(d => d.category === ts.DiagnosticCategory.Error);
  const records = diagnostics.slice(0, 1000).map(d => {
    const start = d.file && d.start !== undefined ? d.file.getLineAndCharacterOfPosition(d.start) : null;
    let dependency = null;
    if (d.file && d.start !== undefined) {
      const visit = node => {
        if (d.start < node.getFullStart() || d.start >= node.getEnd()) return;
        if ((ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) && node.moduleSpecifier && ts.isStringLiteral(node.moduleSpecifier)) {
          const specifier = node.moduleSpecifier.text;
          if (!specifier.startsWith('.') && !path.isAbsolute(specifier)) dependency = specifier.startsWith('@') ? specifier.split('/').slice(0, 2).join('/') : specifier.split('/')[0];
        }
        ts.forEachChild(node, visit);
      };
      visit(d.file);
    }
    return { code: d.code, dependency, path: d.file ? path.relative(input.project, d.file.fileName).split(path.sep).join('/') : null,
      start: start ? { line: start.line + 1, column: start.character + 1 } : null,
      message: ts.flattenDiagnosticMessageText(d.messageText, '\n') };
  });
  for (const [file, digest] of hashes) {
    const text = ts.sys.readFile(file);
    if (text === undefined || createHash('sha256').update(text).digest('hex') !== digest) return unknown('Compiler inputs changed during checking');
  }
  return { status: 'completed', message: 'TypeScript no-emit diagnostics completed against recorded input hashes', typescript_version: ts.version,
    diagnostics: records, diagnostics_total: diagnostics.length,
    files: [...hashes].sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0).map(([file, sha256]) => ({ path: file, sha256 })) };
}
try { console.log(JSON.stringify(inspect())); }
catch (error) { console.error(`TypeScript check failed: ${error.message}`); process.exitCode = 1; }

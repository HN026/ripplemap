import path from 'node:path';
import ts from 'typescript';
import { trackedInputs } from './inputs.mjs';
import { compilerDiagnostic } from './diagnostics.mjs';

/**
 * Coordinate installed identity checks, no-emit compilation, and stable input evidence.
 */
export function inspectProject(input) {
  const files = trackedInputs();
  if (ts.version !== '6.0.3') return unknown('Only TypeScript 6.0.3 is supported');
  const installationIssue = installedPackageIssue(input, files.read);
  if (installationIssue) return unknown(installationIssue);
  const config = path.join(input.project, 'tsconfig.json');
  if (!ts.sys.fileExists(config)) return unknown('No tsconfig.json at the registered path');
  const parsed = ts.getParsedCommandLineOfConfigFile(config, { noEmit: true, incremental: false, composite: false, noCheck: false, skipLibCheck: false, skipDefaultLibCheck: false }, {
    ...ts.sys, readFile: files.read, onUnRecoverableConfigFileDiagnostic: d => { throw new Error(ts.flattenDiagnosticMessageText(d.messageText, '\n')); },
  });
  if (!parsed) throw new Error('Could not parse TypeScript configuration');
  const issue = configurationIssue(parsed);
  if (issue) return unknown(issue);
  if (parsed.errors.length) throw new Error(parsed.errors.map(d => ts.flattenDiagnosticMessageText(d.messageText, '\n')).join('\n'));
  const program = createProgram(parsed, files.read);
  const diagnostics = ts.getPreEmitDiagnostics(program).filter(d => d.category === ts.DiagnosticCategory.Error);
  const records = diagnostics.slice(0, 1000).map(d => compilerDiagnostic(d, input.project));
  if (!files.unchanged()) return unknown('Compiler inputs changed during checking');
  return { status: 'completed', message: 'TypeScript no-emit diagnostics completed against recorded input hashes', typescript_version: ts.version,
    diagnostics: records, diagnostics_total: diagnostics.length, files: files.evidence() };
}

/**
 * Identify the first missing or mismatched installed package identity.
 */
function installedPackageIssue(input, read) {
  for (const pkg of input.packages) {
    const file = path.join(input.lock_root, pkg.location, 'package.json');
    const text = read(file);
    if (text === undefined) return `Required installed package metadata unavailable: ${file}`;
    const actual = JSON.parse(text);
    if (actual.name !== pkg.name || actual.version !== pkg.version) return `Installed package identity differs from lockfile: ${file}`;
  }
}

/**
 * Describe unsupported or empty project configurations without treating them as
 * healthy.
 */
function configurationIssue(parsed) {
  if (parsed.projectReferences?.length || parsed.options.plugins?.length) return 'Project references and compiler plugins are unsupported';
  if (parsed.options.noResolve || (parsed.options.allowJs && !parsed.options.checkJs)) return 'noResolve and unchecked JavaScript inputs are unsupported';
  if (!parsed.fileNames.length) return 'No TypeScript input files in this project';
}

/**
 * Create a read-tracked TypeScript program whose host rejects emitted files.
 */
function createProgram(parsed, read) {
  const host = ts.createCompilerHost(parsed.options);
  host.readFile = read;
  host.getSourceFile = (file, version) => {
    const text = read(file);
    return text === undefined ? undefined : ts.createSourceFile(file, text, version, true);
  };
  host.writeFile = () => { throw new Error('Unexpected compiler output'); };
  return ts.createProgram(parsed.fileNames, parsed.options, host);
}

/**
 * Build the structured incomplete-evidence response for an unsupported compiler input.
 */
function unknown(message) {
  return { status: 'unknown', message, typescript_version: ts.version, diagnostics: [], diagnostics_total: 0, files: [] };
}

import path from 'node:path';
import ts from 'typescript';

/**
 * Create a compiler host limited to supplied committed inputs and trusted standard
 * libraries.
 */
export function virtualProject(input) {
  const root = path.resolve('ripplemap-virtual-snapshot');
  const files = new Map();
  for (const [name, text] of Object.entries(input.files)) {
    if (!name || path.isAbsolute(name) || name.split('/').some(p => !p || p === '.' || p === '..') || /[\\:\0]/.test(name)) throw new Error(`Invalid snapshot path: ${name}`);
    files.set(path.resolve(root, name), text);
  }
  const lib = path.dirname(ts.getDefaultLibFilePath({}));
  /**
   * Allow only standard TypeScript declaration library files in the trusted library
   * directory.
   */
  const isLib = name => path.dirname(name) === lib && /^lib(?:\..*)?\.d\.ts$/.test(path.basename(name));
  /**
   * Read virtual committed inputs or a permitted compiler standard library.
   */
  const readFile = name => files.get(path.resolve(name)) ?? (isLib(name) ? ts.sys.readFile(name) : undefined);
  const host = {
    ...ts.createCompilerHost({}),
    getCurrentDirectory: () => root,
    readFile,
    fileExists: name => readFile(name) !== undefined,
    directoryExists: directory => directory === lib || [...files.keys()].some(name => name.startsWith(`${directory}${path.sep}`)),
    getDirectories: () => [],
    realpath: name => name,
    getSourceFile: (name, version) => {
      const text = readFile(name);
      return text === undefined ? undefined : ts.createSourceFile(name, text, version, true);
    },
    writeFile: () => { throw new Error('Unexpected write'); },
  };
  return { root, files, host };
}

/**
 * Reject compiler programs containing errors with readable flattened diagnostic
 * messages.
 */
export function checkDiagnostics(program) {
  const errors = ts.getPreEmitDiagnostics(program).filter(d => d.category === ts.DiagnosticCategory.Error);
  if (errors.length) throw new Error(errors.map(d => ts.flattenDiagnosticMessageText(d.messageText, '\n')).join('\n'));
}

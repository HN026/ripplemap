import path from 'node:path';
import ts from 'typescript';

/**
 * Convert a TypeScript error to its stable dependency, location, and message fields.
 */
export function compilerDiagnostic(diagnostic, project) {
  const start = diagnostic.file && diagnostic.start !== undefined ? diagnostic.file.getLineAndCharacterOfPosition(diagnostic.start) : null;
  return {
    code: diagnostic.code,
    dependency: referencedDependency(diagnostic),
    path: diagnostic.file ? path.relative(project, diagnostic.file.fileName).split(path.sep).join('/') : null,
    start: start ? { line: start.line + 1, column: start.character + 1 } : null,
    message: ts.flattenDiagnosticMessageText(diagnostic.messageText, '\n'),
  };
}

/**
 * Read the package name from the compiler AST around the diagnostic location.
 */
function referencedDependency(diagnostic) {
  let dependency = null;
  if (diagnostic.file && diagnostic.start !== undefined) {
    /**
     * Inspect only the AST path containing the diagnostic and retain its import/export
     * package.
     */
    function visit(node) {
      if (diagnostic.start < node.getFullStart() || diagnostic.start >= node.getEnd()) return;
      if ((ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) && node.moduleSpecifier && ts.isStringLiteral(node.moduleSpecifier)) {
        const specifier = node.moduleSpecifier.text;
        if (!specifier.startsWith('.') && !path.isAbsolute(specifier)) dependency = specifier.startsWith('@') ? specifier.split('/').slice(0, 2).join('/') : specifier.split('/')[0];
      }
      ts.forEachChild(node, visit);
    }
    visit(diagnostic.file);
  }
  return dependency;
}

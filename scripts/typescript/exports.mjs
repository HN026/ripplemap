import path from 'node:path';
import ts from 'typescript';

function exportsOf(program, entry) {
  const source = program.getSourceFile(entry);
  if (!source) throw new Error(`Entry point missing from program: ${entry}`);
  const checker = program.getTypeChecker();
  const module = checker.getSymbolAtLocation(source);
  if (!module) throw new Error('Entry point must be an external TypeScript module');
  return new Map(checker.getExportsOfModule(module).map(symbol => {
    const target = symbol.flags & ts.SymbolFlags.Alias ? checker.getAliasedSymbol(symbol) : symbol;
    return [symbol.name, target.declarations ?? []];
  }));
}

function site(declarations, root) {
  if (declarations.length !== 1) return null;
  const declaration = declarations[0];
  if (!ts.isFunctionDeclaration(declaration) || !declaration.name) return null;
  const source = declaration.getSourceFile();
  const relative = path.relative(root, source.fileName).split(path.sep).join('/');
  if (relative.startsWith('../') || path.isAbsolute(relative)) return null;
  const start = source.getLineAndCharacterOfPosition(declaration.name.getStart(source));
  const end = source.getLineAndCharacterOfPosition(declaration.name.getEnd());
  return { path: relative, range: [start.line, start.character, end.line, end.character] };
}

export function publicExports(sourceProgram, sourceEntry, declarationProgram, declarationEntry, root) {
  const source = exportsOf(sourceProgram, sourceEntry);
  const declarations = exportsOf(declarationProgram, declarationEntry);
  const names = [...new Set([...source.keys(), ...declarations.keys()])].sort();
  return names.map(name => {
    const sourceSite = site(source.get(name) ?? [], root);
    const declarationSite = site(declarations.get(name) ?? [], root);
    const supported = name !== 'default' && sourceSite !== null && declarationSite !== null;
    return {
      name, status: supported ? 'resolved' : 'unsupported',
      explanation: supported ? 'Compiler-resolved named function export; declaration emission verified.' : 'Only single named function declarations and their in-package aliases/re-exports are supported; default exports, values, overloads, and merged declarations are excluded.',
      source: sourceSite, declaration: declarationSite,
    };
  });
}

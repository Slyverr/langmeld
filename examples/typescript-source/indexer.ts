import ts from "typescript";
import { createInterface } from "node:readline";

function parse(source: string) {
  const file = ts.createSourceFile(
    "input.ts",
    source,
    ts.ScriptTarget.Latest,
    true,
    ts.ScriptKind.TS,
  );
  const byteOffsets = new Uint32Array(source.length + 1);
  let utf16Offset = 0;
  let utf8Offset = 0;
  for (const character of source) {
    const units = character.length;
    const bytes = Buffer.byteLength(character, "utf8");
    for (let unit = 0; unit < units; unit += 1)
      byteOffsets[utf16Offset + unit] = utf8Offset;
    utf16Offset += units;
    utf8Offset += bytes;
    byteOffsets[utf16Offset] = utf8Offset;
  }

  function byteOffset(offset: number): number {
    return byteOffsets[offset] ?? utf8Offset;
  }

  function span(node: ts.Node): [number, number] {
    return [byteOffset(node.getStart(file)), byteOffset(node.getEnd())];
  }

  const errors = file.parseDiagnostics.map((syntaxError) => {
    const start = syntaxError.start ?? 0;
    const length = syntaxError.length ?? 1;
    return {
      message: ts.flattenDiagnosticMessageText(syntaxError.messageText, " "),
      span: [byteOffset(start), byteOffset(start + length)],
    };
  });

  const imports = file.statements.flatMap((statement) => {
    if (!ts.isImportDeclaration(statement) || !statement.importClause)
      return [];
    const records = [];
    if (statement.importClause.name) {
      records.push({
        module_specifier: statement.moduleSpecifier.text,
        module_span: span(statement.moduleSpecifier),
        imported_name: "default",
        local_name: statement.importClause.name.text,
        imported_span: span(statement.importClause.name),
      });
    }
    const bindings = statement.importClause.namedBindings;
    if (bindings && ts.isNamedImports(bindings)) {
      records.push(
        ...bindings.elements.map((element) => ({
          module_specifier: statement.moduleSpecifier.text,
          module_span: span(statement.moduleSpecifier),
          imported_name: element.propertyName?.text ?? element.name.text,
          local_name: element.name.text,
          imported_span: span(element.propertyName ?? element.name),
        })),
      );
    }
    return records;
  });

  const declarations = file.statements.filter(
    (statement): statement is ts.FunctionDeclaration =>
      ts.isFunctionDeclaration(statement) &&
      statement.body !== undefined &&
      statement.name !== undefined,
  );
  const localNames = new Set(
    declarations.map((declaration) => declaration.name!.text),
  );
  const importedNames = new Map<
    string,
    { name: string; moduleSpecifier: string }
  >();
  for (const statement of file.statements) {
    if (!ts.isImportDeclaration(statement) || !statement.importClause) continue;
    const bindings = statement.importClause.namedBindings;
    if (statement.importClause.name) {
      importedNames.set(statement.importClause.name.text, {
        name: "default",
        moduleSpecifier: statement.moduleSpecifier.text,
      });
    }
    if (bindings && ts.isNamedImports(bindings)) {
      for (const element of bindings.elements) {
        importedNames.set(element.name.text, {
          name: element.propertyName?.text ?? element.name.text,
          moduleSpecifier: statement.moduleSpecifier.text,
        });
      }
    }
  }

  function callsIn(functionNode: ts.FunctionDeclaration) {
    const calls: Array<{
      target: string;
      target_span: [number, number];
      local_name: string;
      span: [number, number];
      module_specifier?: string;
    }> = [];
    const visit = (node: ts.Node): void => {
      if (
        node !== functionNode &&
        (ts.isFunctionLike(node) || ts.isClassLike(node))
      )
        return;
      if (
        ts.isCallExpression(node) &&
        ts.isIdentifier(node.expression) &&
        (localNames.has(node.expression.text) ||
          importedNames.has(node.expression.text))
      ) {
        const imported = importedNames.get(node.expression.text);
        calls.push({
          target: imported?.name ?? node.expression.text,
          target_span: span(node.expression),
          local_name: node.expression.text,
          span: span(node),
          ...(imported ? { module_specifier: imported.moduleSpecifier } : {}),
        });
      }
      ts.forEachChild(node, visit);
    };
    if (functionNode.body) visit(functionNode.body);
    return calls.sort((left, right) => left.span[0] - right.span[0]);
  }

  const functions = declarations.map((declaration) => {
    const parameters = declaration.parameters.map((parameter) => ({
      name: parameter.name.getText(file),
      name_span: span(parameter.name),
      span: span(parameter),
      ...(parameter.type
        ? {
            annotation: parameter.type.getText(file),
            annotation_span: span(parameter.type),
          }
        : {}),
    }));
    return {
      name: declaration.name!.text,
      exported:
        declaration.modifiers?.some(
          (modifier) => modifier.kind === ts.SyntaxKind.ExportKeyword,
        ) ?? false,
      default_exported:
        declaration.modifiers?.some(
          (modifier) => modifier.kind === ts.SyntaxKind.DefaultKeyword,
        ) ?? false,
      name_span: span(declaration.name!),
      span: span(declaration),
      kind: declaration.modifiers?.some(
        (modifier) => modifier.kind === ts.SyntaxKind.AsyncKeyword,
      )
        ? "async"
        : "sync",
      parameters,
      calls: callsIn(declaration),
      ...(declaration.type
        ? {
            return_type: declaration.type.getText(file),
            return_type_span: span(declaration.type),
          }
        : {}),
    };
  });

  return { errors, functions, imports };
}

const input = createInterface({ input: process.stdin, crlfDelay: Infinity });
process.stdout.write('{"ready":true}\n');
for await (const line of input) {
  try {
    const request: unknown = JSON.parse(line);
    if (
      typeof request !== "object" ||
      request === null ||
      !("source" in request) ||
      typeof request.source !== "string"
    ) {
      throw new Error("request must contain a string source field");
    }
    process.stdout.write(`${JSON.stringify(parse(request.source))}\n`);
  } catch (error) {
    process.stdout.write(
      `${JSON.stringify({ frontend_error: error instanceof Error ? error.message : String(error) })}\n`,
    );
  }
}

// What a person gets in Visual Studio Code for `.langmeld` files: the problems of the
// grammar, an outline, and go-to-definition on the name of a rule.
import * as assert from "node:assert/strict";
import * as vscode from "vscode";
import { code, open, problems } from "./shared";

export async function run(): Promise<void> {
  const [document] = await open("words.langmeld");

  const found = await problems(document.uri, (list) => list.some((d) => code(d) === "LM-G001"));
  const unknown = found.find((d) => code(d) === "LM-G001");
  assert.ok(unknown);
  assert.equal(unknown.range.start.line, 5);

  const symbols = await vscode.commands.executeCommand<vscode.DocumentSymbol[]>(
    "vscode.executeDocumentSymbolProvider",
    document.uri,
  );
  assert.deepEqual(symbols.map((symbol) => symbol.name), ["Words", "s", "name", "File", "Word", "Other"]);

  // `Word` in `rule File = Word*` leads to `rule Word = name`.
  const use = new vscode.Position(3, document.lineAt(3).text.indexOf("Word") + 1);
  const where = await vscode.commands.executeCommand<vscode.Location[]>(
    "vscode.executeDefinitionProvider",
    document.uri,
    use,
  );
  assert.equal(where.length, 1, JSON.stringify(where));
  assert.equal(where[0].range.start.line, 4);
  assert.equal(document.getText(where[0].range), "Word");
}

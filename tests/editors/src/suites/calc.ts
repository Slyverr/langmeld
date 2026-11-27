// What a person gets in Visual Studio Code from the extension written for Calc: problems
// while typing, an outline, folding, colours and keyword completion.
import * as assert from "node:assert/strict";
import * as vscode from "vscode";
import { code, open, problems } from "./shared";

export async function run(): Promise<void> {
  const [document, editor] = await open("broken.calc");

  // A mistake in the file is shown, with the code and place the server gave it.
  const first = await problems(document.uri, (found) => found.some((d) => code(d) === "LM-P001"));
  const mistake = first.find((d) => code(d) === "LM-P001");
  assert.ok(mistake);
  assert.equal(mistake.severity, vscode.DiagnosticSeverity.Error);
  assert.equal(mistake.range.start.line, 3);

  // Mending it while typing clears it, and the check of the language itself appears.
  await editor.edit((edit) => {
    edit.replace(new vscode.Range(3, 0, 3, document.lineAt(3).text.length), "let area = square(12) / 0;");
  });
  const mended = await problems(document.uri, (found) => found.some((d) => code(d) === "CALC010"));
  assert.ok(!mended.some((d) => code(d) === "LM-P001"), JSON.stringify(mended));

  const symbols = await vscode.commands.executeCommand<vscode.DocumentSymbol[]>(
    "vscode.executeDocumentSymbolProvider",
    document.uri,
  );
  assert.deepEqual(symbols.map((symbol) => symbol.name), ["square", "area"]);

  const folds = await vscode.commands.executeCommand<vscode.FoldingRange[]>(
    "vscode.executeFoldingRangeProvider",
    document.uri,
  );
  assert.ok(folds.some((fold) => fold.start === 0 && fold.end === 1), JSON.stringify(folds));

  const colours = await vscode.commands.executeCommand<vscode.SemanticTokens>(
    "vscode.provideDocumentSemanticTokens",
    document.uri,
  );
  assert.ok(colours && colours.data.length > 0, "the server colours tokens");

  // Typing the start of a keyword offers it.
  await editor.edit((edit) => edit.insert(new vscode.Position(document.lineCount, 0), "\nle"));
  const where = new vscode.Position(document.lineCount - 1, 2);
  const offered = await vscode.commands.executeCommand<vscode.CompletionList>(
    "vscode.executeCompletionItemProvider",
    document.uri,
    where,
  );
  const labels = offered.items.map((item) => (typeof item.label === "string" ? item.label : item.label.label));
  assert.ok(labels.includes("let"), JSON.stringify(labels));
}

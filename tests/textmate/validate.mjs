import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { readFile } from "node:fs/promises";

import oniguruma from "vscode-oniguruma";
import textmate from "vscode-textmate";

const grammarDefinition = JSON.parse(readFileSync(0, "utf8"));
const wasm = await readFile(
  new URL("./node_modules/vscode-oniguruma/release/onig.wasm", import.meta.url),
);
const wasmBytes = wasm.buffer.slice(
  wasm.byteOffset,
  wasm.byteOffset + wasm.byteLength,
);
const onigLib = oniguruma.loadWASM(wasmBytes).then(() => ({
  createOnigScanner: (patterns) => new oniguruma.OnigScanner(patterns),
  createOnigString: (value) => new oniguruma.OnigString(value),
}));
const registry = new textmate.Registry({
  onigLib,
  loadGrammar: async () => null,
});
const grammar = await registry.addGrammar(grammarDefinition);

function tokenizeLine(line, previous = textmate.INITIAL) {
  const result = grammar.tokenizeLine(line, previous);
  assert.equal(result.stoppedEarly, false);
  return result;
}

function scopesAt(line, tokens, text) {
  const start = line.indexOf(text);
  assert.notEqual(start, -1, `fixture does not contain ${JSON.stringify(text)}`);
  const token = tokens.find(
    ({ startIndex, endIndex }) => startIndex <= start && start < endIndex,
  );
  assert.ok(token, `no TextMate token covers ${JSON.stringify(text)}`);
  return token.scopes;
}

function validateOverlappingComments() {
  const openingLine = "## begin block";
  const opening = tokenizeLine(openingLine);
  assert.ok(
    scopesAt(openingLine, opening.tokens, "begin").includes("comment.block.overlap"),
  );

  const continuation = tokenizeLine("continued !! program", opening.ruleStack);
  assert.ok(
    scopesAt("continued !! program", continuation.tokens, "continued").includes(
      "comment.block.overlap",
    ),
  );
  assert.ok(
    scopesAt("continued !! program", continuation.tokens, "program").includes(
      "keyword.other.overlap",
    ),
  );

  const lineComment = "# line comment program";
  assert.ok(
    scopesAt(lineComment, tokenizeLine(lineComment).tokens, "program").includes(
      "comment.line.overlap",
    ),
  );

  console.log("Overlapping TextMate comment tokenization passed.");
}

function validateExampleGrammar() {
  const source = 'program { let title = "Hello"; draw circle; }';
  const line = tokenizeLine(source);
  assert.ok(scopesAt(source, line.tokens, "program").includes("keyword.other.widgetscript"));
  assert.ok(scopesAt(source, line.tokens, "let").includes("keyword.other.widgetscript"));
  assert.ok(scopesAt(source, line.tokens, "draw").includes("keyword.other.widgetscript"));
  assert.ok(scopesAt(source, line.tokens, "circle").includes("keyword.other.widgetscript"));
  assert.ok(scopesAt(source, line.tokens, "{").includes("punctuation.widgetscript"));
  assert.ok(scopesAt(source, line.tokens, "Hello").includes("string.quoted.double.widgetscript"));
  assert.ok(!scopesAt(source, line.tokens, "title").includes("keyword.other.widgetscript"));

  const numberLine = "let count = 42;";
  assert.ok(
    scopesAt(numberLine, tokenizeLine(numberLine).tokens, "42").includes(
      "constant.numeric.widgetscript",
    ),
  );

  const escapedStringLine = String.raw`let text = "a\"b";`;
  const escapedString = tokenizeLine(escapedStringLine);
  assert.ok(
    scopesAt(escapedStringLine, escapedString.tokens, String.raw`\"`).includes(
      "constant.character.escape.widgetscript",
    ),
  );

  console.log("TextMate grammar tokenization passed.");
}

if (process.argv.includes("--overlapping-comments")) {
  validateOverlappingComments();
} else {
  validateExampleGrammar();
}

// Drives the server built for the browser, from Node, without a browser.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

import init, { CalcServer } from "./www/pkg/calc_web.js";

await init({ module_or_path: await readFile(new URL("./www/pkg/calc_web_bg.wasm", import.meta.url)) });
const server = new CalcServer();
const send = (message) =>
  server.handle(JSON.stringify({ jsonrpc: "2.0", ...message })).map((reply) => JSON.parse(reply));
const uri = "file:///main.calc";

const [started] = send({ id: 1, method: "initialize", params: { capabilities: {} } });
assert.equal(started.result.capabilities.positionEncoding, "utf-16");

const [published] = send({
  method: "textDocument/didOpen",
  params: {
    textDocument: { uri, languageId: "calc", version: 1, text: 'fn f() {\r\n  "😀" 1 / 0;\r\n}\r\n' },
  },
});
const [problem] = published.params.diagnostics;
assert.equal(problem.code, "LM-P001");
assert.deepEqual(problem.range.start, { line: 1, character: 7 });

send({
  method: "textDocument/didChange",
  params: { textDocument: { uri, version: 2 }, contentChanges: [{ text: "fn f() {\n  1 / 0;\n}\nle" }] },
});
const [symbols] = send({ id: 2, method: "textDocument/documentSymbol", params: { textDocument: { uri } } });
assert.deepEqual(symbols.result.map((s) => s.name), ["f"]);
const [folds] = send({ id: 3, method: "textDocument/foldingRange", params: { textDocument: { uri } } });
assert.deepEqual(folds.result.map((f) => [f.startLine, f.endLine]), [[0, 1]]);
const [completion] = send({
  id: 4,
  method: "textDocument/completion",
  params: { textDocument: { uri }, position: { line: 3, character: 2 } },
});
assert.ok(completion.result.some((item) => item.label === "let"), JSON.stringify(completion));
console.log("the browser server answers as expected");

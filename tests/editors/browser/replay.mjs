// Runs the browser example in a headless browser: the recorded editor sessions go to the
// web worker, whose replies must be those the native server gave, and the Monaco page
// shows problems from the same worker while text is edited.
//
// CHROME names the browser to start (`/usr/bin/google-chrome` by default). The example
// has to be built into examples/web/www/pkg first, see examples/web/README.md.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createServer } from "node:http";
import { extname, join } from "node:path";
import { fileURLToPath } from "node:url";

import puppeteer from "puppeteer-core";

const here = fileURLToPath(new URL(".", import.meta.url));
const www = join(here, "../../../examples/web/www");
const sessions = join(here, "../../../crates/langmeld-ide/tests/sessions");
const types = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm" };

const server = createServer(async (request, response) => {
  const path = request.url === "/" ? "/index.html" : request.url.split("?")[0];
  try {
    const body =
      path === "/blank.html" ? "<!doctype html><title>blank</title>" : await readFile(join(www, path));
    response.writeHead(200, { "content-type": types[extname(path)] ?? "text/plain" });
    response.end(body);
  } catch {
    response.writeHead(404).end();
  }
});
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const origin = `http://127.0.0.1:${server.address().port}`;

const browser = await puppeteer.launch({
  executablePath: process.env.CHROME ?? "/usr/bin/google-chrome",
  headless: true,
  args: ["--no-sandbox", "--disable-gpu"],
});
let failed = false;
try {
  const page = await browser.newPage();
  page.on("pageerror", (error) => console.error("page error:", error));

  // The same sessions as the native tests, through a real worker.
  await page.goto(`${origin}/blank.html`);
  for (const encoding of ["utf-8", "utf-16"]) {
    const transcript = JSON.parse(await readFile(join(sessions, `calc-${encoding}.json`), "utf8"));
    const answers = await page.evaluate(async (steps) => {
      const worker = new Worker("/worker.js", { type: "module" });
      const received = [];
      worker.onmessage = ({ data }) => received.push(data);
      const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
      const out = [];
      for (const step of steps) {
        const before = received.length;
        worker.postMessage(step.send);
        // The worker is still starting for the first messages: wait for the replies a
        // step is known to have, or a moment if it has none.
        const deadline = Date.now() + 30000;
        while (received.length - before < step.replies.length && Date.now() < deadline) {
          await wait(10);
        }
        if (step.replies.length === 0) {
          await wait(100);
        }
        out.push(received.splice(before));
      }
      worker.terminate();
      return out;
    }, transcript);
    transcript.forEach((step, index) => {
      const expected = step.replies.map((reply) => {
        const { jsonrpc, ...rest } = reply;
        return rest;
      });
      const got = answers[index].map((reply) => {
        const { jsonrpc, ...rest } = reply;
        return rest;
      });
      assert.deepEqual(got, expected, `${encoding}: step ${index} (${step.send.method})`);
    });
    console.log(`the ${encoding} session gives the same answers through the web worker`);
  }

  // Monaco, with the worker behind it.
  await page.goto(`${origin}/index.html`);
  await page.waitForFunction(
    () => window.monaco && window.monaco.editor.getModelMarkers({}).length > 0,
    { timeout: 90000 },
  );
  const first = await page.evaluate(() =>
    window.monaco.editor.getModelMarkers({}).map((marker) => marker.code),
  );
  assert.ok(first.includes("CALC010"), JSON.stringify(first));
  await page.evaluate(() => {
    const model = window.monaco.editor.getModels()[0];
    model.applyEdits([{ range: new window.monaco.Range(7, 1, 7, 1), text: "let = 1;\n" }]);
  });
  await page.waitForFunction(
    () =>
      window.monaco.editor.getModelMarkers({}).some((marker) => marker.code === "LM-P001"),
    { timeout: 30000 },
  );
  console.log("the Monaco page shows the problems of the worker while text changes");
} catch (error) {
  failed = true;
  console.error(error);
} finally {
  await browser.close();
  server.close();
}
process.exit(failed ? 1 : 0);

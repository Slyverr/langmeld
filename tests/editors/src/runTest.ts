// Starts Visual Studio Code with an extension that `langmeld package vscode` wrote, and
// runs one suite of checks inside it. The suite is named by SUITE (`calc` or `grammar`),
// the extension's folder by EXTENSION and the folder to open by WORKSPACE.
import * as path from "node:path";
import { runTests } from "@vscode/test-electron";

function required(name: string): string {
  const value = process.env[name];
  if (!value) {
    throw new Error(`set ${name}`);
  }
  return path.resolve(value);
}

async function main() {
  const suite = process.env.SUITE ?? "calc";
  await runTests({
    extensionDevelopmentPath: required("EXTENSION"),
    extensionTestsPath: path.resolve(__dirname, "suites", suite),
    launchArgs: [required("WORKSPACE"), "--no-sandbox", "--disable-gpu"],
  });
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});

import * as vscode from "vscode";

/** The problems of a document, once `ready` accepts them or the time is up. */
export async function problems(
  uri: vscode.Uri,
  ready: (found: vscode.Diagnostic[]) => boolean,
  seconds = 90,
): Promise<vscode.Diagnostic[]> {
  const deadline = Date.now() + seconds * 1000;
  for (;;) {
    const found = vscode.languages.getDiagnostics(uri);
    if (ready(found)) {
      return found;
    }
    if (Date.now() > deadline) {
      throw new Error(
        `no problems of the right kind for ${uri.toString()} in ${seconds} seconds; ` +
          `there are ${JSON.stringify(found.map((d) => [d.code, d.message]))}`,
      );
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
}

export function code(found: vscode.Diagnostic): string {
  const code = found.code;
  return typeof code === "object" ? String(code.value) : String(code);
}

export async function open(name: string): Promise<[vscode.TextDocument, vscode.TextEditor]> {
  const folder = vscode.workspace.workspaceFolders?.[0];
  if (!folder) {
    throw new Error("no folder is open");
  }
  const document = await vscode.workspace.openTextDocument(vscode.Uri.joinPath(folder.uri, name));
  return [document, await vscode.window.showTextDocument(document)];
}

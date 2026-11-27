// A small client of the protocol between Monaco and the server in the worker.
const PROGRAM = `// Errors show as you type. Try Ctrl+Space on an empty line.
fn square(n) {
  n * n
}

let area = square(12);
print(area / 0);
`;

const worker = new Worker(new URL("./worker.js", import.meta.url), { type: "module" });
const pending = new Map();
let requests = 0;
let onDiagnostics = () => {};

worker.onmessage = ({ data }) => {
  if (data.method === "textDocument/publishDiagnostics") {
    onDiagnostics(data.params.diagnostics);
  } else if (pending.has(data.id)) {
    pending.get(data.id)(data.result);
    pending.delete(data.id);
  }
};

const notify = (method, params) => worker.postMessage({ jsonrpc: "2.0", method, params });
const request = (method, params) =>
  new Promise((resolve) => {
    const id = ++requests;
    pending.set(id, resolve);
    worker.postMessage({ jsonrpc: "2.0", id, method, params });
  });

// Monaco counts lines and columns from one, the protocol from zero.
const toRange = ({ start, end }) => ({
  startLineNumber: start.line + 1,
  startColumn: start.character + 1,
  endLineNumber: end.line + 1,
  endColumn: end.character + 1,
});
const toPosition = (position) => ({
  line: position.lineNumber - 1,
  character: position.column - 1,
});

window.require.config({ paths: { vs: "https://cdn.jsdelivr.net/npm/monaco-editor@0.52.2/min/vs" } });
window.require(["vs/editor/editor.main"], async () => {
  const monaco = window.monaco;
  const started = await request("initialize", {
    capabilities: { general: { positionEncodings: ["utf-16"] } },
  });
  notify("initialized", {});
  monaco.languages.register({ id: "calc", extensions: [".calc"] });
  const uri = "file:///main.calc";
  const model = monaco.editor.createModel(PROGRAM, "calc", monaco.Uri.parse(uri));
  const textDocument = { uri };

  onDiagnostics = (diagnostics) => {
    monaco.editor.setModelMarkers(
      model,
      "calc",
      diagnostics.map((d) => ({
        ...toRange(d.range),
        message: d.message,
        code: d.code,
        severity: d.severity === 2 ? monaco.MarkerSeverity.Warning : monaco.MarkerSeverity.Error,
      })),
    );
    document.getElementById("status").textContent = `${diagnostics.length} problem(s)`;
  };

  monaco.languages.registerCompletionItemProvider("calc", {
    async provideCompletionItems(_, position) {
      const items = await request("textDocument/completion", {
        textDocument,
        position: toPosition(position),
      });
      return {
        suggestions: (items ?? []).map((item) => ({
          label: item.label,
          kind: monaco.languages.CompletionItemKind.Keyword,
          insertText: item.textEdit.newText,
          range: toRange(item.textEdit.range),
        })),
      };
    },
  });

  const symbol = (s) => ({
    name: s.name,
    detail: "",
    // The protocol numbers kinds from one, Monaco from zero.
    kind: s.kind - 1,
    tags: [],
    range: toRange(s.range),
    selectionRange: toRange(s.selectionRange),
    children: (s.children ?? []).map(symbol),
  });
  monaco.languages.registerDocumentSymbolProvider("calc", {
    async provideDocumentSymbols() {
      const symbols = await request("textDocument/documentSymbol", { textDocument });
      return (symbols ?? []).map(symbol);
    },
  });

  monaco.languages.registerFoldingRangeProvider("calc", {
    async provideFoldingRanges() {
      const folds = await request("textDocument/foldingRange", { textDocument });
      return (folds ?? []).map((f) => ({ start: f.startLine + 1, end: f.endLine + 1 }));
    },
  });

  monaco.languages.registerSelectionRangeProvider("calc", {
    async provideSelectionRanges(_, positions) {
      const found = await request("textDocument/selectionRange", {
        textDocument,
        positions: positions.map(toPosition),
      });
      return (found ?? []).map((first) => {
        const ranges = [];
        for (let next = first; next; next = next.parent) ranges.push({ range: toRange(next.range) });
        return ranges;
      });
    },
  });

  const legend = started.capabilities.semanticTokensProvider.legend;
  monaco.languages.registerDocumentSemanticTokensProvider("calc", {
    getLegend: () => legend,
    async provideDocumentSemanticTokens() {
      const tokens = await request("textDocument/semanticTokens/full", { textDocument });
      return { data: new Uint32Array(tokens?.data ?? []) };
    },
    releaseDocumentSemanticTokens() {},
  });

  notify("textDocument/didOpen", {
    textDocument: { uri, languageId: "calc", version: model.getVersionId(), text: model.getValue() },
  });
  model.onDidChangeContent((event) => {
    notify("textDocument/didChange", {
      textDocument: { uri, version: event.versionId },
      // Monaco lists the changes from the end of the text, so they apply in order.
      contentChanges: event.changes.map((change) => ({
        range: {
          start: { line: change.range.startLineNumber - 1, character: change.range.startColumn - 1 },
          end: { line: change.range.endLineNumber - 1, character: change.range.endColumn - 1 },
        },
        text: change.text,
      })),
    });
  });

  monaco.editor.create(document.getElementById("editor"), {
    model,
    "semanticHighlighting.enabled": true,
    automaticLayout: true,
  });
});

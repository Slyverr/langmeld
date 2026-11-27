# Calc in the browser

The Calc language server compiled to WebAssembly, run in a web worker behind the
[Monaco](https://microsoft.github.io/monaco-editor/) editor: problems while typing,
colours, outline, folding, selection ranges and keyword completion. The page posts each
message of the protocol to the worker, and the worker posts back what
`Server::handle_json` answers.

From the root of the repository, in the development shell:

```sh
examples/web/build.sh                           # writes examples/web/www/pkg
node examples/web/test.mjs                      # checks the server without a browser
python3 -m http.server --directory examples/web/www 8000
```

Then open <http://localhost:8000>. The editor itself is loaded from a public CDN, so the
page needs a network connection. The version of `wasm-bindgen` in `Cargo.toml` has to
match the command's.

The pull request workflow also replays recorded editor sessions through the worker in a
headless browser, and checks that the answers are the ones of the native server; see
`tests/editors`.

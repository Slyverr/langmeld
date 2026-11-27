# Editor tests

Checks of what a person sees in an editor, run by the pull request workflow
(`.github/workflows/ci.yml`, jobs `vscode` and `browser`); they need a graphical or
headless browser, so `cargo xtask check` leaves them out. Node comes from the development
shell, and `package-lock.json` pins every package they use.

| Folder | Holds |
|--------|-------|
| `src/` | The Visual Studio Code tests: `runTest.ts` starts the editor with an extension that `langmeld package vscode` wrote, and `suites/` are the checks run inside it |
| `fixtures/` | The files those tests open |
| `browser/` | The test of `examples/web` in headless Chrome |

To run the Visual Studio Code test for Calc by hand, from the root of the repository:

```sh
cargo build -p langmeld-cli -p calc
export PATH="$PWD/target/debug:$PATH"
langmeld package vscode examples/calc --out target/editors/calc
cd tests/editors && npm ci && npm run compile
ln -s "$PWD/node_modules" ../../target/editors/calc/node_modules
(cd ../../target/editors/calc && npx tsc -p .)
SUITE=calc EXTENSION=../../target/editors/calc WORKSPACE=fixtures/calc npm run test:vscode
```

For `.langmeld` files, make the extension with `langmeld package vscode --langmeld --out
target/editors/grammar`, and run `SUITE=grammar` with `WORKSPACE=fixtures/grammar`.

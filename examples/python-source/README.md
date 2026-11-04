# Python source index

This standalone Langmeld consumer uses CPython's standard-library `ast` and
`tokenize` modules to index top-level synchronous and asynchronous functions,
parameters, annotations, direct calls, and a bounded form of relative sibling
imports. It does not import, compile, or execute source files. Python 3.11 or
newer is required; set `LANGMELD_PYTHON` to select a different `python3`
executable.

The Rust adapter starts one isolated `python3 -I -S -u indexer.py` worker and
reuses it for parsing updates. Isolated mode disables environment and user-site
customization, while `-S` skips site-package initialization. Source requests
are limited to 4 MiB and responses to 32 MiB. The worker communicates through
newline-delimited JSON on stdin and stdout; stderr is reserved for process
failures.

## Try it

```sh
cargo run --manifest-path examples/python-source/Cargo.toml -- source examples/python-source/main.py
cargo run --manifest-path examples/python-source/Cargo.toml -- project examples/python-source/project
```

The second command indexes the manifest-selected project. Its `recovery.py`
fixture contains an intentional syntax error; valid top-level function siblings
remain available and the command reports the error.

## Editor integration

Start the LSP for one open file:

```sh
cargo run --manifest-path examples/python-source/Cargo.toml --bin python-source-lsp
```

Or preload a project snapshot and resolve direct relative imports across the
project's `.py` files:

```sh
cargo run --manifest-path examples/python-source/Cargo.toml --bin python-source-lsp -- --project-root examples/python-source/project
```

The `.langmeld` file declares the indexed constructs and generated typed AST. The
Python parser supplies the source spans and generic nodes; Langmeld supplies
workspace symbol resolution and the LSP navigation surface.

## Supported source subset

- Top-level `def` and `async def` declarations, positional-only and ordinary
  parameters, variadic parameters, keyword-only parameters, annotations, and
  return annotations.
- Direct calls to functions declared in the same module or imported with
  `from .module import name [as alias]`.
- Relative imports of direct sibling `.py` modules selected by the project's
  explicit sources or source roots. Project paths come from the bounded
  `langmeld.json` loader; there is no second filesystem scanner.
- `__init__.py` files selected by the manifest are indexed as ordinary Python
  modules. Package re-exports and `from . import name` resolution are not
  implemented.
- Best-effort recovery of complete top-level function declarations and simple
  `from .module import ...` statements when another top-level function has a
  syntax error.

It does not resolve absolute or multi-level imports, package re-exports,
dynamic imports, classes, methods, arbitrary incomplete statements, or Python
types. It is a source index, not a type checker, interpreter, or compiler. The
opaque `.langmeld` lexer treats each Python file as one host-parsed region, so the
generic LSP does not emit Python semantic highlighting and the generated
lexical editor grammar is not a Python syntax highlighter. A host lexer must
emit real Python token spans to provide token-level highlighting.

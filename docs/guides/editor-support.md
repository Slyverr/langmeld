# Editor support

A language gets errors while typing, colours, an outline, folding, selection ranges and
keyword completion in any editor that speaks the Language Server Protocol, from its
grammar alone. This guide covers what you get, how to start the server, how to make a
Visual Studio Code extension, how to run the same server in a browser, and how to add
checks of your own.

## What you get

| Feature | Comes from |
|---------|------------|
| Syntax errors, as you type | The parser, with the text and fix of every diagnostic |
| Colours (semantic tokens) | The kind of each token: keywords, comments, strings, numbers, operators and names |
| Outline | `symbol` lines of the `editor` block |
| Folding | `fold` lines of the `editor` block; without any, whatever stands in braces, and every comment of several lines |
| Selection ranges | The tree: a token, then each node around it |
| Keyword completion | What the parser would accept next, from the tokens it looked for at that place |

The server counts columns as the editor asks: in UTF-8 bytes if the editor offers them, and
otherwise in UTF-16 units. Emoji, CJK and `\r\n` line ends give the same answers either
way. A change that arrives with an older version than the text holds is ignored, so the
problems an editor shows are always those of the latest text.

## Say what editors show

The `editor` block of the grammar names the nodes the outline lists and the nodes that fold:

```langmeld
langmeld 1
language Shapes { extensions "shapes" }
tokens {
    skip space = /[ \t\r\n]+/
    word       = preset ident(xid)
}
rule File  = Item*
rule Item  = Group | Let
rule Group = "group" name:Name Body
rule Body  = "{" Item* "}"
rule Let   = "let" name:Name "=" Name ";"
rule Name  = word

editor {
    symbol Group kind module   name name
    symbol Let   kind variable name name
    fold   Body
}
```

`symbol NODE kind KIND name LABEL` lists each `NODE` in the outline under the text of its
part labelled `LABEL`. The kinds are `file`, `module`, `namespace`, `package`, `class`,
`method`, `property`, `field`, `constructor`, `enum`, `interface`, `function`, `variable`,
`constant`, `string`, `number`, `boolean`, `array`, `object`, `key`, `enum_member`,
`struct`, `event`, `operator` and `type`; any other word is read as `variable`. Symbols
nest as their nodes do.

A line that names a node the language does not have (`LM-F016`), or a label the node does
not give (`LM-F017`), is refused when the grammar is loaded, so a misspelt name is an
error and never a silent gap in the outline.

## Start the server

A program made with `langmeld::cli::App` gets the command from `standard()`, with the
`lsp` feature of `langmeld`:

```toml
langmeld = { version = "0.6", features = ["cli", "kit", "lsp"] }
```

```rust,ignore
let app = App::new(language())
    .editor(&loaded().editor)
    .check_with(my_checks)
    .standard();
```

`mylang lsp` then serves the language on standard input and output. `.editor(...)` gives
the rules of the `editor` block, which the generated `syntax::loaded()` holds with the
language. `check_with` gives the checks of your own: the server shows them with the parser's
problems, on text that parses without problems, as `mylang check` does.

A project made with `langmeld init` already has all of this.

To serve a language from your own code, give a `Server` each message, or let `serve` read
them from the standard input:

```rust
use langmeld::ide::{LanguageHooks, Server};
use langmeld::{Diagnostic, Parse, Source, SourceFile, load_language};

/// What this language checks besides its grammar.
struct Checks;

impl LanguageHooks for Checks {
    fn diagnostics(&self, file: &SourceFile, parse: &Parse) -> Vec<Diagnostic> {
        let _ = (file, parse);
        Vec::new()
    }
}

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let loaded = load_language(Source::text(
    "tiny.langmeld",
    "langmeld 1\nlanguage Tiny { extensions \"tiny\" }\nrule File = \"!\"*\n",
))?;
let mut server = Server::new(&loaded.language, &loaded.editor, Checks);
let replies = server.handle_json(
    r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}"#,
);
assert!(replies[0].contains("Tiny language server"));
# Ok(())
# }
```

`LanguageHooks` has a method for each thing a language adds: `diagnostics` (problems of its
own), `completions` (words of its own to offer) and `provides_definitions` with
`definition` (where the name at a place is declared in the same file). Each has a default
that adds nothing. A method that panics is logged to the editor and counts as having added
nothing; the server goes on.

A document over 16 MiB (change it with `Server::limit`) is not parsed. It gets one problem,
`LM-E001`, in its place.

## A Visual Studio Code extension

```sh
langmeld package vscode            # in the folder of the project
cd editors/vscode
npm install && npm run compile
```

The command writes an extension from the grammar and `langmeld.toml`:

- `language-configuration.json`: comment markers, bracket pairs, auto-closing pairs and a
  word pattern, all read from the grammar.
- A TextMate grammar, for colours before the server starts. The server's semantic tokens
  take over once it runs.
- `src/extension.ts`, which starts the server with `vscode-languageclient`.

The server is started with the command of `[bins] server` in `langmeld.toml`, or else
`mylang lsp` (`mylangc lsp` for the `split` layout), which has to be on the path; the
setting `mylang.server.path` of the editor overrides it. `[package.vscode]` sets the
display name, publisher, icon and the folder written; see [`langmeld.toml`](../reference/langmeld-toml.md).
Press F5 in the folder to try it, and `npx @vscode/vsce package` to make a package.

`langmeld package vscode --langmeld` makes the extension for `.langmeld` files themselves.

## In the browser

`Server::handle_json` takes a message of the protocol as JSON text and gives the replies as
JSON text, which is all a web worker needs: the page posts each message to the worker, and
the worker posts back what the server answers. The server and the language compile to
WebAssembly with nothing to change in the language (`langmeld-ide` builds for
`wasm32-unknown-unknown`); `examples/web` does it for Calc, behind the Monaco editor, with
about 150 lines of JavaScript. Panics do not unwind in WebAssembly, so a failure inside a
hook stops that worker instead of being logged.

## Grammar files

`langmeld lsp` serves `.langmeld` files with the same machinery: syntax errors, the
problems of the grammar (as `langmeld check` shows them, for the whole project the file
is in, with the text the editor holds for each open file), an outline of the tokens, rules
and expressions, folding, and go-to-definition on the names of rules, expressions and
tokens within the file.

## The limits

- The server parses the whole document again after each change. A file of 10,000 lines
  takes well under 50 ms; the nightly run measures it.
- Names across files, rename, hover and references arrive with binding, in a later release.

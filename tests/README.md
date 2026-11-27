# Tests

| Folder | Contains |
|--------|----------|
| `corpus/` | Framework behavior: grammar errors, error recovery, every diagnostic code |
| `real-code/` | Syntax that real languages use, one topic per file |
| `stress/` | A small invented language, `pipes.langmeld`, and its corpus: valid and broken text for every hard feature of the format, held to at most two tokens of lookahead |
| `real-code/tokens/` | The same kind of cases at token level: the expected output lists every token |

Token cases run a small test lexer instead of a parser. Each token is one line,
`KIND "text"`, with `joint` after it when nothing separates it from the next token, and
each diagnostic follows as `! CODE start..end message`. `tokens/terminated/` uses a lexer
whose line endings end statements, and `tokens/interpolated/` one whose strings hold
`{}` holes.

Run them with `cargo nextest run -p langmeld-syntax --test corpus`.

The syntax cases of `real-code/` run against `examples/handwritten-calc` (`cargo nextest run -p handwritten-calc`) and `examples/calc`. The files about syntax that Calc does not have (`generics.txt`, `operators.txt`, `characters.txt`) run instead against small grammars declared in `crates/langmeld-syntax/tests/support/real_code.rs`, by `cargo nextest run -p langmeld-syntax --test corpus`.

## File format

A corpus file is a `.txt` file holding any number of cases:

```text
=== let with a missing semicolon
let x = 1
let y = 2;
---
File
  LetStmt "let" Name("x") "=" Literal(1) !error(expected ";")
```

- `=== ` starts a case. The rest of the line is its name, unique within the file.
- The source runs up to the first line that is exactly `---`, so a source cannot contain
  such a line. Its bytes are kept, including `\r\n`. The line ending just before `---` is
  not part of the source; add an empty line to end the source with one.
- The expected output runs up to the next `=== ` line. Its line endings and trailing empty
  lines are ignored.
- A name followed by `[expected-failure]` marks a case that cannot pass yet. The run fails
  if such a case starts to pass, so that the marker gets removed.

Until a case has a grammar that parses it, mark it `[expected-failure]`; its expected tree is a
sketch to be confirmed once the case passes. A tree is written one node or token per line,
without trivia; a node holding a single token takes one line.

The list of cases and their outcomes is snapshotted with
[insta](https://insta.rs/). After a deliberate change, review it with `cargo insta review`.

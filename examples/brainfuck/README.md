# Brainfuck

This is a complete interpreter for the eight-command Brainfuck language. A
custom lexer ignores non-command characters, a Rust parser builds a nested AST,
and a Rust interpreter executes it. Brainfuck's `[` token is both a command and
the opening delimiter for a loop; Langmeld's generic block grammar expects a
construct prefix before its block delimiter, so the custom parser preserves
Brainfuck's actual syntax without adding a fake keyword.

This example defines a concrete dialect: 30,000 cells, wrapping 8-bit cell
arithmetic, a bounded tape pointer, zero on input exhaustion, and a ten-million
instruction limit. These policies belong to this interpreter, not Langmeld.

Run the checked-in Hello World program or provide a source file. The interpreter
reads bytes from standard input only when the program executes `,`:

```sh
cargo run -p langmeld-brainfuck-example --locked
cargo run -p langmeld-brainfuck-example --locked -- path/to/program.bf
```

The example also exercises source-located diagnostics for unmatched loops,
tape-boundary violations, and exhausted execution budgets. The language
specific parser and runtime stay in this example; they do not add Brainfuck
semantics to Langmeld itself.

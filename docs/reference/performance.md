# Performance

Benchmarks use criterion and run with `cargo bench -p langmeld-syntax --locked` and
`cargo bench -p json --locked`. The nightly CI benchmark measures them again on each run
and prints the results in the run summary. Numbers are only comparable when they come
from the same machine, so this document keeps none.

## Lexer

The lexer benchmark measures throughput in bytes per second, on 1 MiB of repeated input.
It runs two kinds of input:

- Data, JSON-like.
- Code, with comments.

Each input is lexed twice: with a language that declares 5 words, and with one that
declares 300. "Words" are the reserved words and the extra operators the language declares.

## Grammar

Two benchmarks measure parsing, from text to tree, in bytes per second.

- `grammar` in `langmeld-syntax` parses about 1 MiB of statements with a grammar of 5, 50
  and 300 rules, each kind of statement starting with a word of its own. It does so for
  input that uses only five of the rules and for input that uses every rule. The speed
  should not depend on how many rules there are to choose from: the bar is that 300 rules
  take at most twice as long as 5.
- `throughput` in `examples/json` parses about 1 MiB of JSON, and lexes it alone for
  comparison. The bar is at least 20 MB/s. It also parses the same document with the
  JSON grammar grown to 300 rules.

## Language server

`langmeld-ide` has a benchmark of the time from an edit to the problems published, for a
Calc file of 10,000 lines, and of colouring the whole file, with
`cargo bench -p langmeld-ide --features lsp`. The bar is 50 ms from edit to problems. The
nightly run measures it and prints it in the run summary with the bar.

The bars are read as a trend. A slow shared runner can miss them without anything being
wrong, so the nightly run reports them and never fails because of them.

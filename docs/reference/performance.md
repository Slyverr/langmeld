# Performance

Benchmarks use criterion and run with `cargo bench -p langmeld-syntax --locked`. The nightly
CI benchmark measures them again on each run. Numbers are only comparable when they come
from the same machine, so this document keeps none.

## Lexer

The lexer benchmark measures throughput in bytes per second, on 1 MiB of repeated input.
It runs two kinds of input:

- Data, JSON-like.
- Code, with comments.

Each input is lexed twice: with a language that declares 5 words, and with one that
declares 300. "Words" are the reserved words and the extra operators the language declares.

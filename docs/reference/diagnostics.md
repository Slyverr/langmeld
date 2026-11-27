# Diagnostic codes

Every problem Langmeld reports has a code that keeps its meaning between releases.

## Problems in source text

| Code | Meaning |
|------|---------|
| `LM-L001` | Characters that no token matches |
| `LM-L002` | A string is not closed before the end of its line |
| `LM-L003` | A block comment is not closed |
| `LM-L004` | A character literal is not closed |
| `LM-L005` | A character literal does not hold exactly one character |
| `LM-L006` | A number has a base prefix and no digits |
| `LM-L007` | The file is larger than the size limit |
| `LM-L008` | The file has more tokens than the limit |
| `LM-P001` | Something else was expected |
| `LM-P002` | A reserved word is used as a name |
| `LM-P003` | The source is nested more deeply than the limit |
| `LM-P004` | A parser stopped taking tokens |

## Problems in token declarations

These are returned when a lexer is built, each with advice on fixing it.

| Code | Meaning |
|------|---------|
| `LM-G007` | A token can match empty text |
| `LM-G008` | A keyword is not matched as an identifier |
| `LM-G009` | A token can never match because another has the same text |
| `LM-L101` | A pattern is invalid or uses an unsupported feature |
| `LM-L102` | A keyword is declared and no token is an identifier |
| `LM-L103` | The patterns are too large to compile |
| `LM-L104` | A string token enables no kind of quote |

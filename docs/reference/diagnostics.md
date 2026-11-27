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
| `LM-P006` | Operators that do not chain are chained, such as `a < b < c` |
| `LM-P007` | The source needs more backtracking than the limit allows |
| `LM-S001` | A grammar's own code built a node its schema does not allow |

## Problems in token declarations

These are returned when a lexer is built, each with advice on fixing it. A grammar's
tokens are checked the same way, so `LM-G007` to `LM-G009` also come from
`LanguageBuilder::build`.

| Code | Meaning |
|------|---------|
| `LM-G007` | A token can match empty text |
| `LM-G008` | A keyword is not matched as an identifier |
| `LM-G009` | A token can never match because another has the same text |
| `LM-L101` | A pattern is invalid or uses an unsupported feature |
| `LM-L102` | A keyword is declared and no token is an identifier |
| `LM-L103` | The patterns are too large to compile |
| `LM-L104` | A string token enables no kind of quote |

## Problems in grammars

These are returned by `LanguageBuilder::build` together, so that one build shows every
problem at once, each with a note that starts "To fix this". A grammar with any error is
refused. A warning leaves the grammar usable and is listed by `Language::warnings`.

| Code | Severity | Meaning |
|------|----------|---------|
| `LM-G001` | Error | A rule or token is used and nothing declares it; the note suggests the closest name |
| `LM-G002` | Error | A rule can start with itself, so parsing it would never take a token |
| `LM-G003` | Error | Two choices start with the same three tokens, so the next tokens cannot tell them apart |
| `LM-G004` | Error | An optional or repeated part could start the same way as what follows it |
| `LM-G005` | Error | A repeated part can match nothing, so the repetition could go on forever |
| `LM-G006` | Warning | A rule is never used |
| `LM-G010` | Error | Two operators of an expression have the same symbol, or an operator could also continue what follows the expression |
| `LM-G011` | Error | A name is declared twice, or a word is both reserved and contextual |
| `LM-G012` | Error | The language has no rules |
| `LM-G013` | Error | A postfix operator does not start with the symbol that marks it |
| `LM-G014` | Error | The language has more tokens, rules and literals than a kind number can count |
| `LM-G015` | Warning | A custom rule does not say what it can start with, so it may start with anything and choices around it cannot be checked |
| `LM-G016` | Error | A choice needs to look further ahead than can be checked for a grammar this large |

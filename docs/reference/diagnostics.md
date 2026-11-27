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

## Problems in `.langmeld` files

These are found while a file is read, before the grammar is checked, and each has a note
that starts "To fix this". Mistakes of the text itself are reported with the codes
`LM-L001`, `LM-L002` and `LM-P001` above. See the
[format reference](langmeld-format.md).

| Code | Meaning |
|------|---------|
| `LM-F001` | The file does not start with its format version line |
| `LM-F002` | The file is written for a format version this release does not read |
| `LM-F003` | A rule, expression, operand or operator name is not in PascalCase; the fix renames it everywhere |
| `LM-F004` | A token or label name is not in snake_case |
| `LM-F005` | A token uses a preset that does not exist |
| `LM-F006` | A preset has an option it does not have, or `interpolate` is used outside `string` |
| `LM-F007` | An imported file, or the language file, cannot be read |
| `LM-F008` | Imports go in a circle |
| `LM-F009` | An `extern` rule or token has no Rust code |
| `LM-F010` | A quoted string uses an escape the format does not know |
| `LM-F011` | A precedence is not a number from 1 to 65535 |
| `LM-F012` | A custom rule is parsed without its Rust code, so it is read as one error |
| `LM-F013` | A second `language` block |
| `LM-F014` | No `language` block |
| `LM-F015` | A compiled language cannot be read: it is damaged, was compiled by another version, or lacks the code of a custom rule |
| `LM-F016` | An `editor` line names a rule or expression the language does not have |
| `LM-F017` | A `symbol` line names a part the node does not label |

Problems found by checking the grammar (`LM-G001` and the others above) are reported at
the rule they are about in the `.langmeld` file; a name that is not declared is
reported at the place it is used.

## Problems in `langmeld.toml`

| Code | Meaning |
|------|---------|
| `LM-M001` | The file is not valid TOML |
| `LM-M002` | A setting that does not exist |
| `LM-M003` | A field of the `language` block is repeated; it is written in the grammar only |
| `LM-M004` | A setting has a value of the wrong kind, or one that is not allowed |


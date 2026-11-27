# The `.langmeld` format

A `.langmeld` file describes a language: its words and symbols, its rules and its
operators. Langmeld reads it, checks it, and parses files of that language with it. This
page is the whole syntax. For a walk through writing one, see
[Write a `.langmeld` file](../guides/write-a-langmeld-file.md).

## A whole file

```langmeld
langmeld 1

language Greeting {
    extensions "hi"
    comments { line "#" }
}

tokens {
    skip space = /[ \t\r\n]+/
    name       = preset ident(ascii)
}

rule File     = greetings:Greeting*
rule Greeting = "hello" who:name
```

A file is, in this order:

1. The version line `langmeld 1`. It is required, and a file written for another version
   is refused with a message that says so (`LM-F001`, `LM-F002`).
2. Declarations, in any order: `language`, `tokens`, `rule`, `expr`, `extern`, `bind`,
   `editor` and `import`.

Comments start with `//` and run to the end of the line. Spaces and line endings only
separate words; a rule may be written over several lines.

## Names

| Names of | Written | Example | Becomes |
|----------|---------|---------|---------|
| Rules, expressions, operands, operators, `extern` declarations | `PascalCase` | `LetStmt` | A node kind, and later a type |
| Tokens and labels | `snake_case` | `ident`, `then` | A token kind, and later a method |

A name against the convention is refused with the name it should have, and the fix renames it
everywhere it is written (`LM-F003`, `LM-F004`). A label may be a Rust keyword such as `type`
or `match`; code generation escapes it.

Quoted text uses double quotes and the escapes `\"`, `\\`, `\n`, `\r` and `\t`
(`LM-F010`). A pattern is written between slashes, `/[a-z]+/`, in the syntax of the
[`regex` crate](https://docs.rs/regex) without anchors or word boundaries; write `\/` for a
slash in it.

### Reserved and contextual words

These words end or start a declaration, so they are never names: `atom`, `extern`,
`import`, `infix`, `postfix`, `prec`, `prefix`, `recover`, `scope`, `sep`, `sep_strict` and
`try`.

The other words of the format have a meaning only where they are expected, and are names
everywhere else: a label can be called `kind` or `name`, and a rule can use a token called
`rule`. Which declaration a word starts is decided by the words after it, at most three tokens
ahead.

## `language`

```langmeld
langmeld 1

language Calc {
    extensions "calc" "clc"
    comments {
        line "//"
        doc "///"
        block "/*" "*/" nested
    }
    terminators { newline ";" }
    continue_inside { "(" ")" "[" "]" "{" "}" }
}

tokens { ident = preset ident(xid) }

rule File = ident*
```

| Setting | Meaning |
|---------|---------|
| `extensions "a" "b"` | The file extensions of the language, without dots |
| `comments { ... }` | `line "//"` to the end of the line, `doc "///"` the same but kept for documentation, `block "/*" "*/"` with `nested` if they nest |
| `terminators { newline ";" }` | Line endings end statements, and so does each text listed after `newline`; see below |
| `continue_inside { ... }` | Pairs of brackets inside which a line ending never ends a statement; by default `( )` and `[ ]` |

With `terminators`, the lexer makes a token of each line ending that ends a statement.
Runs of blank lines and comments count as one. A line ending is ignored inside the
brackets of `continue_inside`, and after a binary operator or a comma, so a statement may
continue on the next line. A rule asks for the end of a statement with `terminated(...)`.

Exactly one `language` block is allowed in the files of a language (`LM-F013`, `LM-F014`).
Its settings are written here and nowhere else: [`langmeld.toml`](langmeld-toml.md) may not
repeat them.

## `tokens`

```langmeld
langmeld 1

language Tokens { }

tokens {
    skip space = /[ \t\r\n]+/
    ident      = preset ident(xid)
    int        = preset int(dec, hex, bin, oct, underscores)
    float      = preset float(exponent, underscores)
    string     = preset string(double, single, raw, interpolate "{" "}" with Expr)
    letter     = preset char
    word       = /[a-z]+(-[a-z]+)*/
    keywords   { let fn }
    contextual { from }
}

rule File = Expr*
rule Expr = ident | int | float | string | letter | word | "let" "fn" "from"
```

| Declaration | Meaning |
|-------------|---------|
| `name = /pattern/` | Text that matches the pattern |
| `skip name = ...` | Text that rules never see, such as spaces; the tree keeps it |
| `name = preset kind(options)` | A ready-made token |
| `keywords { a b }` | Reserved words: they cannot be names |
| `contextual { a }` | Words that are names until a rule asks for them |

Where several tokens could match, the longest wins, and between equals a fixed text
wins over a pattern. Words written between quotes in rules need no declaration, so
`keywords` is only for words no rule mentions.

| Preset | Options |
|--------|---------|
| `int` | `dec`, `hex`, `bin`, `oct`, `underscores` |
| `float` | `exponent`, `underscores`; a digit is required after the `.` |
| `string` | `double`, `single`, `raw`, and `interpolate "{" "}" with Rule` |
| `char` | none |
| `ident` | `xid` (Unicode, the default) or `ascii` |

A string with `interpolate` has holes between the two texts; each hole is parsed by the
rule or expression after `with`. A rule that uses the string gets a node of the token's name
holding the quotes, the text and each hole's contents. A string that is not closed before
the end of its line is one mistake (`LM-L002`). An unknown preset or option is refused
(`LM-F005`, `LM-F006`).

## `rule`

```langmeld
langmeld 1

language Parts { }

tokens {
    skip space = /[ \t\r\n]+/
    ident      = preset ident(ascii)
}

rule File    = items:Item*
rule Item    = Let | Call | Block
rule Let     = "let" name:ident "=" value:ident ";"
rule Call    = callee:ident "(" args:sep(ident, ",") ")" ";"
rule Block   = try ("{" Item* "}") | "{" "}"
rule Guarded = !"let" ident &";" recover ";" "}"
```

| Write | Meaning |
|-------|---------|
| `"text"` | Exactly this token. A word is reserved unless declared `contextual` |
| `token` | A declared token |
| `Rule` | Another rule or expression |
| `label:X` | A labelled part: it becomes a typed accessor and an entry of the node's schema |
| `X*` `X+` `X?` | Any number, at least one, or at most one |
| `( ... )` | Grouping |
| `A \| B` | One of them, chosen by the next tokens |
| `sep(X, ",")` | Items separated by the text, with a trailing separator allowed |
| `sep_strict(X, ",")` | The same, without a trailing separator |
| `terminated(X)` | `X` followed by the end of a line or a terminator, or before `}` or the end of the file |
| `&X` and `!X` | Look at what comes next without taking it: it must, or must not, be `X` |
| `try ( ... )` | Parse the part, and go back if it has mistakes; explicit and rare |
| `recover "a" "b"` | After a rule, where reading resumes after a mistake; otherwise worked out |

Every rule becomes a node of its name, except a rule that only chooses between other rules.
Parts are written one after another; `|` binds loosest, then juxtaposition, then the
suffixes `*` `+` `?`.

The grammar is checked when it is loaded. A choice is made by looking at up to three tokens;
a grammar that needs more is refused, with the two ways that cannot be told apart and a way to
fix it (`LM-G003`, `LM-G004`). `langmeld check --explain` shows where a grammar looks more
than one token ahead. See the [diagnostics](diagnostics.md) for every check.

## `expr`

```langmeld
langmeld 1

language Sums { }

tokens {
    skip space = /[ \t\r\n]+/
    int        = preset int
    ident      = preset ident(ascii)
}

rule File = Expr

expr Expr {
    atom    Number = int
    atom    Name   = ident
    atom    Paren  = "(" inner:Expr ")"
    prefix  Neg    "-"            prec 9
    infix   Mul    "*" | "/"      prec 6 left
    infix   Add    "+" | "-"      prec 5 left
    infix   Cmp    "==" | "<"     prec 4 none
    infix   Pow    "^"            prec 7 right
    postfix Call   "(" args:sep(Expr, ",") ")" prec 10
}
```

An expression is never written with left recursion. It has:

- `atom Name = parts`: what an operand can be. Its parts are those of a rule.
- `prefix Name "symbol" prec N`: an operator before its operand.
- `infix Name "a" | "b" prec N left|right|none`: an operator between two operands. `left`
  groups `a - b - c` as `(a - b) - c`, `right` as `a - (b - c)`, and `none` makes a chain an
  error (`LM-P006`).
- `postfix Name parts prec N`: an operator after its operand, starting with the symbol that
  marks it.

A larger precedence binds tighter, and may be from 1 to 65535 (`LM-F011`). The name of each
operand and operator becomes the node kind it makes. Two operators with the same symbol, or
one that could also start what follows the expression, are refused (`LM-G010`).

## `extern`

```langmeld
langmeld 1

language Arrows { }

tokens {
    skip space = /[ \t\r\n]+/
    name       = preset ident(ascii)
}

extern token RawText                         // read by Rust: its end needs counting
extern rule ArrowOrParen -> Expr first "("   // parsed by Rust: it needs more lookahead

rule File = Expr*

expr Expr {
    atom Name  = name
    atom Fancy = ArrowOrParen
}
```

- `extern token Name` is a token whose text Rust code recognises.
- `extern rule Name -> Kind first "(" ...` is a rule that Rust code parses. `-> Kind` names the
  node it makes, and `first` lists the tokens it can start with, so that choices around it
  can still be checked. Without `first` it may start with anything, which is a warning
  (`LM-G015`).

The code is given by name when the language is loaded. A missing piece is refused at its
declaration (`LM-F009`). With no Rust code at all, as in `langmeld parse --grammar`, a
custom rule is read as one error node, reported once (`LM-F012`).

## `bind`

Binding declares which nodes define names, which refer to them, and the scopes between.
The language server and `langmeld::bind` use it to report names that nothing defines, and
to go to definitions, find uses, rename and complete names, across files. See
[Names and projects](../guides/names-and-projects.md) for how to use it.

```langmeld
langmeld 1

language Bound { extensions "bound" }

tokens {
    skip space = /[ \t\r\n]+/
    ident      = preset ident(ascii)
    string     = preset string(double)
}

rule File    = Item*
rule Item    = Let | Fn | Use | Stage | Get
rule Let     = "let" name:Name ";"
rule Fn      = "fn" name:Name "{" Item* "}"
rule Use     = "use" path:string ";"
rule Stage   = "stage" name:Name "for" target:NameRef body:Block
rule Block   = "{" Item* "}"
rule Get     = object:NameRef "." member:ident ";"
rule Name    = ident
rule NameRef = ident

bind {
    scope File, Fn, Block
    Let.name  defines value after
    Fn.name   defines value hoisted in parent
    Use.path  imports value from Use.path
    NameRef   refers value
    Stage.body includes members of Stage.target
    Get.member refers member of Get.object
}
```

| Statement | Meaning |
|-----------|---------|
| `scope A, B` | These nodes open a scope, nested as the tree is |
| `A.field defines ns modes` | The text at the field defines a name in the namespace `ns` |
| `A refers ns` | The node refers to a name of `ns`, found from the innermost scope outwards |
| `A.field imports ns from A.path` | The names of the file whose path is written at `A.path`, which is a quoted path relative to the importing file |
| `A.field includes members of A.other` | The scope of `field` also sees the members of what `other` refers to, in this file or another |
| `A.field refers member of A.other` | The text at `field` names a member, as the `b` in `a.b`, which code of the language looks up |

Modes after `defines`: `after` (visible after its node ends), `hoisted` (the whole scope),
`in parent` (the scope around), `shadowing` (the default: a name hides an earlier one),
`unique` (a duplicate in the scope is an error), `global` (shared by all files, with no
import, and a duplicate anywhere is an error), and `computed` (named by Rust code).
`after` and `hoisted`, `shadowing` and `unique`, and `global` and `in parent` contradict
each other, which is an error (`LM-F020`). A path in these lines is a node, or a node and the
label of one of its parts, and what it names must exist (`LM-F018`, `LM-F019`).

## `editor`

```langmeld
langmeld 1

language Edited { }

tokens {
    skip space = /[ \t\r\n]+/
    ident      = preset ident(ascii)
}

rule File = Fn*
rule Fn   = "fn" name:ident Body
rule Body = "{" "}"

editor {
    symbol Fn kind function name name
    fold   Body
    highlight Fn by_binding
}
```

| Statement | Meaning |
|-----------|---------|
| `symbol Node kind k name field` | The node is shown in the outline as a symbol of kind `k`, named by the field |
| `fold A, B` | These nodes can be folded |
| `highlight Node by_binding` | The node is coloured by what its name refers to |

Like `bind`, it is read and kept.

## `import`

```text
import "parts/expressions.langmeld"
```

An import names a file relative to the file that holds it, and brings in its declarations:
all files of a language share one set of names. A file is read once however often it is
imported. Importing in a circle (`LM-F008`) or naming a file that cannot be read
(`LM-F007`) is refused, and a mistake in an imported file names that file. The first
rule of the first file is where parsing starts.

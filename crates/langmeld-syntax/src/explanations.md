## LM-L001
Characters that no token matches.

The lexer reads the longest declared token at each place, and here none matches. Remove
the characters, or declare a token for them: literals written in rules are tokens already,
and `tokens` declares the rest.

## LM-L002
A string is not closed before the end of its line.

Strings end on the line where they start. Add the closing quote, or write a line break
inside the string as an escape such as `\n`.

## LM-L003
A block comment is not closed.

The comment opened here runs to the end of the file. Add its closing delimiter. In a
language with nested comments, every opening inside it needs a closing of its own.

## LM-L004
A character literal is not closed.

A character literal ends with a single quote on the line where it starts.

## LM-L005
A character literal does not hold exactly one character.

Write one character or one escape between the quotes. For text of any length, use a
string.

## LM-L006
A number has a base prefix and no digits.

A prefix such as `0x` or `0b` needs at least one digit of its base after it.

## LM-L007
The file is larger than the size limit.

Lexers refuse files above a limit, so that a mistake cannot take all memory. Split the
file, or raise the limit of the language with `lex_limits`.

## LM-L008
The file has more tokens than the limit.

Lexers stop at a limit on the number of tokens. Split the file, or raise the limit of the
language with `lex_limits`.

## LM-L101
A pattern is invalid or uses an unsupported feature.

Patterns are regular expressions without anchors, word boundaries or look-around, so that
all of them compile into one automaton. Rewrite the pattern without these, or read the
token with Rust code as an `extern token`.

## LM-L102
A keyword is declared and no token is an identifier.

Keywords are recognised among identifiers, so a grammar that uses words needs a token for
names, such as `name = preset ident(xid)`.

## LM-L103
The patterns are too large to compile.

The token patterns together need a larger automaton than allowed. Large repetitions such
as `a{1000}` and big Unicode classes are the usual cause. Simplify them, or use a preset.

## LM-L104
A string token enables no kind of quote.

A string preset needs `double`, `single` or both.

## LM-P001
Something else was expected.

The source does not follow the grammar here. The message names what could come, and what
came instead. Parsing goes on after it, so later problems are reported too.

## LM-P002
A reserved word is used as a name.

Keywords cannot name things. Choose another name, or declare the word as contextual so
that it is a name wherever the word itself means nothing.

## LM-P003
The source is nested more deeply than the limit.

Parsing stops at a depth limit, so that no input can overflow the stack. Reduce the
nesting, or raise the limit of the language with `parse_limits`.

## LM-P004
A parser stopped taking tokens.

A parser looked at the same place too often without moving on. In a hand-written parser or
custom rule, make every loop take at least one token or end.

## LM-P006
Operators that do not chain are chained.

Operators declared `none`, such as comparisons, cannot follow one another, as in
`a < b < c`. Add parentheses around one of the operations.

## LM-P007
Parsing needs too much backtracking.

Attempts parse a part and undo it if that fails. Their total work is limited. Make the
part decide with lookahead instead, or move it into a custom rule.

## LM-S001
A grammar's own code built a node that its schema does not allow.

Code that parses a custom rule must only build the nodes the grammar says it makes, in the
places the schema allows. Declare the node in the grammar, or build one it already
describes. This is checked in debug builds only.

## LM-G001
A name is neither a rule nor a token.

A rule refers to something that is not declared. Check the spelling, or declare the rule or
token.

## LM-G002
A rule can start with itself (left recursion).

Reading such a rule would begin by reading it again, forever. Write operators in an `expr`
block, which handles precedence, or require a token before the rule refers to itself.

## LM-G003
Two ways through a rule start the same way for more than three tokens.

The parser chooses a way by looking at most three tokens ahead. Move the common start
before the choice, make the ways start differently, or use `try` or an `extern rule` for
the part that needs more.

## LM-G004
An optional or repeated part cannot be told from what follows it.

The parser cannot tell whether the part is there, or whether what follows has begun. Make
the part start differently from what can follow it, for example with a separator or a
keyword.

## LM-G005
A repeated part can match nothing.

Repeating something that takes no token would never end. Require at least one token inside
the repetition.

## LM-G006
A rule is never used.

No path from the first rule reaches this one. Use it or remove it.

## LM-G007
A token can match empty text.

Every token must take at least one character. Use `+` rather than `*` in patterns, and do
not write empty text.

## LM-G008
A keyword is not matched as an identifier.

A reserved word is found among identifiers, so the identifier token must match the whole
word. Widen the identifier, or write the word as punctuation.

## LM-G009
A token can never match because another has the same text.

Two declarations read exactly the same text, so one would always win. Remove one of them.

## LM-G010
An operator could mean two things.

The same symbol continues an expression in two ways. Use different symbols, or merge the
operators.

## LM-G011
A name is declared twice.

Rules, expressions, operators and tokens share one set of names, and a word is either
reserved or contextual. Rename or remove one declaration.

## LM-G012
The language has no rules.

Declare at least one rule. The first one is where a file starts.

## LM-G013
A postfix operator does not start with a literal.

The parser finds an operator by its symbol, so its part must begin with one, such as `(`
or `[`.

## LM-G014
The language has too many kinds of token and node.

Kinds are numbered with 16 bits. Split the language, or merge tokens and nodes.

## LM-G015
An extern rule does not say what it can start with.

Without its first tokens, choices and error messages cannot take the rule into account.
List them with `first`.

## LM-G016
The grammar is too large to look as far ahead as a choice needs.

Looking two or three tokens ahead costs more the larger a grammar is. Factor out the
common start of the choice so that fewer tokens decide it, or use `try` or an
`extern rule`.

## LM-F001
A file does not start with `langmeld 1`.

The first line names the version of the format, so that later versions can read old files.

## LM-F002
A file declares a version of the format that is not known.

This version of Langmeld reads `langmeld 1`. Use that, or a newer Langmeld.

## LM-F003
A rule, expression or operator name is not in PascalCase.

Rules become types in Rust, so their names start with a capital letter and have no
underscores. The suggested fix renames it; rename references to it too.

## LM-F004
A label is not in snake_case.

Labels become methods in Rust, so they are written in lower case with underscores.

## LM-F005
A token uses a preset that does not exist.

The message lists the presets there are, and the closest one when a name is nearly right.

## LM-F006
A preset has an option it does not have, or `interpolate` is used outside `string`.

The message lists the options of the preset. Only `string` can have holes.

## LM-F007
A file cannot be read.

Check the path. Imports are relative to the file that imports them, and use `/`.

## LM-F008
A file imports itself through other files.

Remove one of the imports of the cycle. Each file only needs to be imported once.

## LM-F009
An `extern` rule or token has no Rust code.

A program that reads the grammar gives the code of each `extern` declaration with
`Source::extern_rule` or `Source::extern_token`. Tools that read a grammar alone, such as
`langmeld check`, do not need it.

## LM-F010
An escape in quoted text is not known.

The escapes are `\"`, `\\`, `\n`, `\r` and `\t`.

## LM-F011
A precedence is not a number from 1 to 65535.

A larger precedence binds tighter.

## LM-F012
Source text reached an `extern` rule whose code is not loaded.

Without the Rust code, a rule that custom code parses is read as one error that covers a
token. Run the program of the language, which gives the code, to parse such files.

## LM-F013
There is more than one `language` block.

A language has one block of settings. Keep one, in any of its files, and move the settings of
the others into it.

## LM-F014
No file says which language it describes.

Add a block such as `language Name { extensions "ext" }` to the first file.

## LM-M001
`langmeld.toml` is not valid TOML.

The message says what is wrong at the place marked.

## LM-M002
`langmeld.toml` has a setting that does not exist.

The message lists the settings there are. Check the spelling.

## LM-F015
A compiled language cannot be read.

The bytes are damaged, were compiled by another version of Langmeld, or lack the code of a
custom rule. Compile the grammar again with this version, and give the code of every
`extern` declaration.

## LM-F016
An `editor` line names something the language does not have.

A line such as `symbol Function kind function name name` or `fold Block` names a rule or an
expression. Check the spelling against the declaration in the file; a token is not a node.

## LM-F017
A `symbol` line names a part the node does not label.

`symbol Function kind function name title` lists each `Function` by the part labelled
`title`. Put the label in the rule, as in `title:ident`, or name a label the rule has.

## LM-F018
A `bind` line names something the language does not have.

A line such as `scope Block` or `Let.name defines value` names a rule or an expression.
Check the spelling against the declaration in the file; a token is not a node.

## LM-F019
A `bind` line names a part that the node does not label.

`Let.name defines value` takes the name from the part labelled `name`. Put the label in the
rule, as in `name:ident`, or name a label the rule has. A path is a node, or a node and one
label.

## LM-F020
Words after `defines` contradict each other.

`after` and `hoisted` say when a name becomes visible, `shadowing` and `unique` say what a
second name of the same kind does, and `global` and `in parent` say where the name lives.
Keep one of each pair.

## LM-F021
A `from` or `of` part belongs to another node than the line's.

`Use.path imports value from Use.target` takes both parts from the same node. Name a part of
the node the line is about, or write only the label of the part.

## LM-M003
A field of the language is written in `langmeld.toml`.

The name, extensions, comments and terminators of a language are written once, in the
`language` block of its grammar. Remove the repeat from `langmeld.toml`.

## LM-M004
A setting has a value of the wrong kind, or one that is not allowed.

The message says what the setting takes. Fix the value at the place marked.

## LM-M005
The project file of a language's users cannot be read.

The file that holds the settings of a project, such as `mylang.toml`, is not valid TOML, has
a setting the language does not know, or has a value of the wrong kind. The message says
which, and the place marked is where. Check the spelling of the key and the kind of its value.

## LM-B001
A name is used that nothing visible there defines.

Check the spelling, define the name before it is used, or import the file that defines it.
Where names are visible comes from the `bind` rules of the language. When a defined name is
close to the one written, the note suggests it, and the editor offers to use it.

## LM-B002
A name that must be the only one is defined twice.

Rename one of the two definitions, or remove one. The place shown with the problem is the
first definition. A name defined `global` is the only one in the whole project, whatever file
it is in.

## LM-B003
An imported file cannot be read.

Imports name a file relative to the file that imports it, written with `/`. The file must
exist and be written in the language. A project also holds only so many files: the problem
says so if that is why, and the fix is to import fewer files.

## LM-B004
A name is defined and nothing uses it.

Use it or remove it. A language reports this only if it asks to: it is a warning that
`Bindings::unused` gives, and a language shows it from its `project_diagnostics`.

## LM-B005
A member is named that what it belongs to does not have.

In `a.b`, `b` is a member of `a`. Check the spelling of the member, or define it. Only the
code of the language knows what members something has, so it is the one that reports this.

## LM-E001
A file is too large for the language server.

The server parses the whole text of a file after every change, so it stops at 16 MiB (a
program that serves a language can change that) and shows no other problem for the file
until it is smaller. Split the file into smaller ones.

# Generated code

`langmeld::build::generate()` in a build script checks the grammar and writes two files
into `OUT_DIR`: `language.bin`, the compiled language, and `syntax.rs`, the Rust types of
its trees. `langmeld::include_syntax!()` includes `syntax.rs` in a module. The output is
never committed, is formatted as `rustfmt` writes it, and is written again whenever a
`.langmeld` file or `langmeld.toml` that the build read changes.

```rust,ignore
// build.rs
fn main() -> Result<(), langmeld::build::BuildError> {
    langmeld::build::generate()
}

// src/lib.rs
pub mod syntax {
    langmeld::include_syntax!();
}
```

A grammar that does not check, or whose names cannot be written in Rust, stops the build
with every problem, each with its place.

## What is written

| Grammar | Rust |
|---------|------|
| A rule or an expression's atom, prefix, infix or postfix operator, `Let` | A struct `Let` over a `SyntaxNode`, implementing `AstNode` |
| A choice of rules, `rule Item = Let \| Print` | An enum `Item` with a variant of each rule's name |
| An `expr Expr { … }` | An enum `Expr` with a variant for each atom and operator |
| A token declared by name, `number` | A struct `NumberToken` over a `SyntaxToken`, with `cast`, `text`, `range` and `syntax` |
| A string with holes | The node that its rule is, like any other |
| A part labelled `value` | A method `value()` on the node |
| A part that can be one of several rules | An enum named for the node and the label, such as `LetValue` |
| The whole grammar | `trait Visitor`, `fn visit`, `fn source()`, `fn loaded()` and `fn language()` |

Every node type has `can_cast(kind)`, `cast(node)`, `syntax()` and `range()` from
`langmeld::ast::AstNode`, which must be in scope to call them. Types derive `Clone`,
`Debug`, `PartialEq`, `Eq` and `Hash`.

## Parts

A method is named for its label, and its result says how often the part occurs:

| The part is | The method gives |
|-------------|------------------|
| Always there, or optional | `Option<T>`, `None` when the source lacks it, so broken files never panic |
| Repeated, `items:Item*` | An iterator of `T` |
| One of several tokens, such as the `operator` of `Add` | `Option<SyntaxToken>` |
| One token that has a type, `radius:number` | `Option<NumberToken>` |

Operators have parts of their own: a prefix has `operator` and `operand`, and an infix has
`lhs`, `operator` and `rhs`.

A label that is a word of Rust gets a name that works: `type` is `r#type()`, and `syntax`,
`cast`, `range`, `can_cast`, `self`, `crate` and `super` get a `_` after them. Two parts
that would get one name are an error.

A tree does not record labels, so a part is found by what comes before it: after a
literal such as `else`, before one such as `)`, or the second node of its kind. When the
grammar lets two parts of the same kind swap places, as an optional `x:Name?` followed by
`y:Name`, no rule can tell them apart. The part gets no method, and the build prints a
warning that names it. A literal between the parts, or a rule of its own for one of them,
solves it.

## The visitor

`trait Visitor` has `type Output` and a method `visit_<node>` for each node, with no default
methods, so a visitor stops compiling when the grammar gains a node. `visit(&mut visitor,
node)` gives a node to the method for its kind, and gives `None` for kinds that are not
nodes of the grammar, such as an error.

`langmeld check --exhaustive` reads the code in a project's `src` folder and lists the nodes
that it never names, by type, by variant of its enum or by visitor method.

## The language

`syntax::source()` is a `langmeld::Source` for the compiled language,
`syntax::loaded()` is the language read from it once, with what it tells editors
(`loaded().editor`, which the command line's `lsp` command serves), and `syntax::language()`
is its `language` field. A language with `extern` rules or tokens has neither, because
their code must be given first:

```rust,ignore
let loaded = langmeld::load_language(
    syntax::source().extern_rule("Raw", parse_raw),
)?;
```

## Names

Rust types are named as the grammar names them, in PascalCase. These clash with names that
the generated code needs and are refused with the two parts named: `Visitor`, a node and a
token type `<Token>Token` of the same name, and two labels that make one method. Rename one
of the two parts.

## Writing the code yourself

`langmeld::codegen::generate(&language)` is what `build::generate` calls, for tools that
write the code somewhere else, and `langmeld::ast` has what generated code stands on:
`AstNode`, and the functions `after`, `before`, `within` and `between` that find the
children of a node between two tokens.

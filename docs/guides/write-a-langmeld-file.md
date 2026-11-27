# Write a `.langmeld` file

Describe a language in a file, and Langmeld parses it: a tree that keeps every byte, one
diagnostic for each mistake, and no parsing code. This guide builds a small language for
shopping lists and runs it. The [format reference](../reference/langmeld-format.md) has the
whole syntax.

## The file

Lists look like this, one item to a line, with an optional amount:

```text
# for the weekend
milk 2
eggs 12; bread
```

```langmeld
langmeld 1

language Shopping {
    extensions "list"
    comments { line "#" }
    terminators { newline ";" }
}

tokens {
    skip space = /[ \t]+/
    word       = preset ident(xid)
    amount     = preset int(underscores)
}

rule List = items:terminated(Item)*
rule Item = name:word amount:amount?
```

The first line is the version of the format. `language` names the language and what
files it reads. `tokens` declares the words and numbers, and `rule` how they fit together.
`terminators` makes the end of a line end an item, and so does `;`.

## Run it

Check it, which reads the file and reports every mistake at its place:

```sh
langmeld check shopping.langmeld
```

```text
shopping.langmeld: language Shopping is valid (2 nodes, 0 warnings)
```

Parse a file with it, without writing any Rust:

```sh
langmeld parse --grammar shopping.langmeld weekend.list
```

In a program, load the file, or have the build script compile it:

```rust
use langmeld::{FileId, Source, SourceFile, load_language, parse};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let loaded = load_language(Source::text(
        "shopping.langmeld",
        "langmeld 1\n\
         language Shopping { terminators { newline \";\" } }\n\
         tokens { skip space = /[ \\t]+/\n\
                  word = preset ident(xid)\n\
                  amount = preset int }\n\
         rule List = items:terminated(Item)*\n\
         rule Item = name:word amount:amount?\n",
    ))?;
    let list = SourceFile::new(FileId::new(0), "weekend.list", "milk 2\neggs 12; bread\n")?;
    let parsed = parse(&loaded.language, &list);
    assert!(parsed.diagnostics.is_empty());
    assert_eq!(parsed.syntax().children().count(), 3);
    Ok(())
}
```

For a build script, `langmeld::build::embed()?` compiles the language, and the program includes
it with `Source::embedded(include_bytes!(concat!(env!("OUT_DIR"), "/language.bin")))`.

## When the file is wrong

A mistake is shown where it is written, with how to fix it:

```text
error[LM-F003]: the rule name `shopping_list` should be written like `ShoppingList`
 --> shopping.langmeld:12:6
   |
12 | rule shopping_list = items:terminated(Item)*
   |      ^^^^^^^^^^^^^
   = note: To fix this, rename it to `ShoppingList`; rule names become types, so they start with a capital and have no underscores.
```

The checks of the grammar itself are named in the [diagnostics](../reference/diagnostics.md).
When two choices cannot be told apart within three tokens, the grammar is refused rather than
read one way silently.

## Reload without building

`load_language(Source::path("shopping.langmeld"))` reads the file when it is called, so
editing the file changes how the next file parses, with no rebuild.

## Next

- A [project manifest](../reference/langmeld-toml.md) names the root file and the binaries.
- Operators with precedence go in an `expr` block; parts that need Rust go in `extern`.
- The [stress-test language](../../tests/stress/pipes.langmeld) uses most of the format.

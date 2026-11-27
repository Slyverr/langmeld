# `langmeld.toml`

The manifest of a language project. It is optional, lives next to the language's files, and
is read by `build.rs` and by every `langmeld` command. It holds only what the `.langmeld`
files do not: repeating a field of the `language` block, such as `extensions`, is an
error (`LM-M003`), so that each fact is written in one place.

```toml
root = "language/main.langmeld"       # the default

[bins]
layout = "single"                     # single | split | multicall
server = ["mylang", "lsp"]            # editor packages launch this

[project]
manifest = "mylang.toml"              # marks the root of a user's project

[package.vscode]
display-name = "MyLang"
publisher = "your-publisher-id"
icon = "assets/icon.png"
out = "../../editors/vscode"
```

| Key | Meaning | Default |
|-----|---------|---------|
| `root` | The root `.langmeld` file, relative to the manifest | `language/main.langmeld` |
| `bins.layout` | One binary (`single`), a compiler, a language server and a runner (`split`), or one binary that acts as the others by its name (`multicall`) | `single` |
| `bins.server` | The command that starts the language server | none |
| `project.manifest` | The name of the file that marks the root of a project written in the language | none |
| `package.vscode.display-name`, `publisher`, `icon`, `out` | What the Visual Studio Code extension is made with | none |

Mistakes are reported with the place in the file and a note that starts "To fix this":
text that is not TOML (`LM-M001`), a key that does not exist (`LM-M002`), a field of the
`language` block repeated (`LM-M003`), and a value of the wrong kind or not allowed
(`LM-M004`).

`langmeld check` and `langmeld test` take a folder with a `langmeld.toml`, or a `.langmeld`
file directly. A build script compiles the language with `langmeld::build::embed()`, which
finds the root the same way.

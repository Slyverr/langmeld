# The `langmeld` command and language programs

Run `langmeld` from a release, with `nix run github:Slyverr/langmeld`, or from a checkout
with `cargo run -p langmeld-cli -- <command>`.

## The `langmeld` command

| Command | Does |
|---------|------|
| `init NAME [--bins LAYOUT]` | Writes a new language project: grammar, interpreter, command line and corpus |
| `init --lib PATH [--bins LAYOUT]` | Adds a language crate at `PATH` to the workspace around it, and to its list of members |
| `generate rule NAME [--path PATH]` | Adds a rule to the grammar |
| `generate handler NODE [--path PATH]` | Adds a function that handles a node to `src/handlers.rs` |
| `check [PATH] [--explain] [--exhaustive]` | Shows the problems of a grammar. `--explain` lists the choices that look more than one token ahead; `--exhaustive` lists the nodes that the project's code in `src` never handles |
| `parse --grammar PATH FILE` | Prints the tree of a file, and its problems with their source |
| `test [PATH] [--update]` | Runs the corpus files in `corpus/` of a language project; `--update` rewrites what failing cases expect |
| `explain CODE` | Says what a diagnostic code means and how to solve it |
| `--help`, `--version` | Describes the commands, or names the version |

`LAYOUT` is `single` (one program with every command, the default), `split` (one program
that runs programs and one with the other commands) or `multicall` (one program that acts
as `name-command` when started under that name). A new project depends on the release of
Langmeld that made it, and `--lib` touches nothing but the new crate and the workspace's
list of members.

A command that takes a grammar takes a `.langmeld` file or a folder, the current one by
default. In a folder it reads [`langmeld.toml`](langmeld-toml.md), and otherwise
`language/main.langmeld`. Commands read grammars without the code of their `extern`
declarations.

## The programs of languages

`langmeld::cli::App` (see [Make a program](../guides/make-a-program.md)) gives a language's
program these commands, and room for its own:

| Command | Does |
|---------|------|
| `check [PATH]... [--format text\|json]` | Parses the files named, or the files with the language's extensions in the folders named, and runs the language's own checks on files that parse without problems |
| `test [--corpus DIR] [--update]` | Runs the corpus files under `DIR`, `corpus` by default |
| `explain CODE` | Explains a code of Langmeld or of the language |

### JSON

`check --format json` writes one object to the output, and nothing to the error output:

```json
{
  "files": 1, "errors": 1, "warnings": 0,
  "diagnostics": [{
    "code": "LM-P001",
    "severity": "error",
    "message": "expected an expression, found `;`",
    "place": { "file": "a.calc",
               "start": { "offset": 8, "line": 1, "column": 9 },
               "end": { "offset": 9, "line": 1, "column": 10 } },
    "labels": [{ "message": "…", "place": { … } }],
    "notes": ["…"],
    "fixes": [{ "title": "…", "edits": [{ "place": { … }, "replacement": "…" }] }]
  }]
}
```

Lines and columns count from one, and columns count characters. `severity` is `error`,
`warning` or `info`. The example is spread over lines to read; the program writes one line
with the keys in alphabetical order. Diagnostics are ordered by file, place and code, so
output is the same from run to run.

## Exit codes

| Code | Meaning |
|------|---------|
| 0 | Nothing is wrong |
| 1 | Problems were found in a grammar, a source file or a corpus case, and shown |
| 2 | The command could not do its work: wrong arguments, or a file or folder it needs is missing |

Problems are written to the error output; what a command reports about its work, such as
`checked 3 files, 1 error, 0 warnings`, goes to the output.

A program started under the name `program-command`, such as a link to it, runs `command`.

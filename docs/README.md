# Documentation

In-depth documentation for this project. For a quick introduction, install and basic
usage, see the [project README](../README.md). For how to contribute, see
[CONTRIBUTING.md](../CONTRIBUTING.md).

## Structure

Folders and files are created only when they get content; some may not exist yet.

| Path | Contains | Read when |
|------|----------|-----------|
| `architecture.md` | How the system works: components, data flow, key design | Changing how something works |
| `guides/` | Step-by-step how-tos for specific tasks | Doing a specific task |
| `reference/` | Exact facts: API, CLI, configuration, file formats | Looking up details |
| `troubleshooting.md` | Known problems, error messages and their fixes | Something fails |
| `glossary.md` | Project-specific terms and what they mean | Meeting an unfamiliar term |
| `assets/` | Images and diagrams used by these docs | Adding a visual to a doc |

## Contents

Every document is listed here with a one-line summary, grouped by the folders above.
Update this list when adding, renaming or removing a document.

### Top level

- [dogfood-log.md](dogfood-log.md): problems met while building real languages with Langmeld
- [learning/notes.md](learning/notes.md): the maintainer's own explanations of the ideas the parser rests on

### Guides

- [guides/write-a-langmeld-file.md](guides/write-a-langmeld-file.md): describing a language in a `.langmeld` file, checking it, running it and reloading it
- [guides/declare-a-grammar.md](guides/declare-a-grammar.md): declaring a language as a grammar with the builder, and what happens to a grammar that is refused
- [guides/make-a-program.md](guides/make-a-program.md): from a grammar to a running language: typed trees, an interpreter, a command line, and where it goes in a project
- [guides/editor-support.md](guides/editor-support.md): errors, colours, outline, folding and completion in editors from the grammar, a Visual Studio Code extension, and the same server in a browser
- [guides/names-and-projects.md](guides/names-and-projects.md): saying how names bind, and getting unknown-name errors, go to definition, references, rename and completion across files, with a resolver and the project file of a language's users
- [guides/hand-written-parser.md](guides/hand-written-parser.md): writing a parser by hand with the syntax crate's primitives

### Reference

- [reference/langmeld-format.md](reference/langmeld-format.md): every part of a `.langmeld` file
- [reference/cli.md](reference/cli.md): the `langmeld` command, the commands of a language's own program, the JSON of `check` and the exit codes
- [reference/generated-code.md](reference/generated-code.md): the Rust types written from a grammar, how parts are found, and the visitor
- [reference/langmeld-toml.md](reference/langmeld-toml.md): the manifest of a language project
- [reference/diagnostics.md](reference/diagnostics.md): the code of every problem Langmeld reports, in source text, token declarations and grammars, and what it means
- [reference/performance.md](reference/performance.md): what the benchmarks measure, the speeds they are held to, and where they run

## Rules

- **One fact, one place.** The project README owns the intro, install and quickstart;
  these docs own depth. Link instead of copying.
- **Docs change with the code.** A change to behavior covered here updates the doc in the
  same change.
- **File names** are lowercase kebab-case: `guides/deploy-to-production.md`.

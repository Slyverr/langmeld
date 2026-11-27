# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project uses [Semantic Versioning](https://semver.org/). How to add entries and cut
releases is described in [CONTRIBUTING.md](CONTRIBUTING.md#changelog).

## [Unreleased]

### Added

- Names across files, from the `bind` block of a grammar: `langmeld::bind` (feature `bind`)
  binds the files of a `Project`, read through a `FileProvider` (`DiskFiles`, `MemoryFiles`
  or an `Overlay`), and answers where a name is defined, what uses it and what is visible
  (`Bindings`). Scopes, namespaces, `after`, `hoisted`, `in parent`, `shadowing`, `unique`,
  `global`, `computed`, `imports ... from` and `includes members of` work, and
  `refers member of` marks uses of a member for a `Resolver` to look up. A `Resolver` also
  names what is there without being written and the names a node stands for.
- Problems of names, each told once with how to solve it: a name that nothing defines, with
  the closest defined name as a suggestion and a quick fix (`LM-B001`); a name defined twice,
  showing the first even in another file (`LM-B002`); an import that cannot be read
  (`LM-B003`); names nothing uses, as warnings from `Bindings::unused` (`LM-B004`); and a
  member that does not exist (`LM-B005`).
- `Project::rename` gives the edits that rename a name in every file, and refuses text that
  is not a name or is a keyword, a name that is computed, and a rename that would make
  another name mean something else.
- The language server goes to definition (also from the path of an import to its file),
  finds references, renames, completes the names visible at a place, hovers, lists the
  symbols of the workspace, offers the fixes of problems as quick fixes, colours names by
  what they mean, and shows the problems of every open file again when another file changes.
  It reads the files of the workspace and those they import, and the files that change on
  disk. `LanguageHooks` gains `resolver`, `project_diagnostics` and `hover`; `Server::files`
  and `Server::project_file` say where files come from and which project file to check.
- `ProjectManifest<T>` finds the project file of a language's users (such as `mylang.toml`)
  above a file, reads it into a type the language chooses, and marks mistakes inside it
  (`LM-M005`); `keys` lists the keys a table takes. `App::project_file` shows these problems
  in the editor and completes the keys, and `package vscode` has the extension send files of
  the name `[project] manifest` gives to the server.
- `mylang check` binds names across the files it checks and those they import, and
  `App::resolver` gives it and the server a `Resolver`. `check_files_with` does the same from
  code.
- A `bind` line that names a rule, expression or part the language does not have
  (`LM-F018`, `LM-F019`), words that contradict each other (`LM-F020`) and a part of another
  node (`LM-F021`) are refused when the grammar is loaded, each once.
- Calc has `use "file.calc";`, which runs another file and brings in its names, a `Builtins`
  resolver for `print`, `sqrt` and `abs`, and `calc::interp::run_project`; `calc run` and
  `calc check` follow imports.
- `closest` suggests the name nearest to one that is not known.
- Fuzzing of binding (`binding`), property tests of renaming and of edited projects, scripted
  editor sessions of names in UTF-8 and UTF-16, and a corpus of the stress-test language's
  names: global pipelines across files, stages that see the tasks of their pipeline, names a
  repetition makes, and names in the hole of a text.

### Changed

- The language and what it tells editors travel together: `ide::serve(language, hooks)` and
  `Server::new(language, hooks)` take a language read from a file as it is (`&Loaded`, such
  as the generated `syntax::loaded()`), which brings its `editor` rules, or a `&Language`,
  or a `Served` with rules given to it by `Served::editor`. This replaces the three
  arguments of 0.6.0. `App::new` takes the same, so `App::new(syntax::loaded())` replaces
  `App::new(syntax::language()).editor(...)`; `App::editor` stays for a language with a
  parser of its own.
- The declared rules of a `bind` block are `BindRules`. The hooks of a language server see a
  file named by its path when the document's address names a file, and
  `LanguageHooks::closed` is given that name. `check_files` takes a `Served` as well as a
  `Language`.

## [0.6.0] - 2026-11-27

### Added

- A language server for any language, from its grammar alone, in `langmeld::ide` (feature
  `lsp`): errors while typing, colours, an outline, folding, selection ranges and keyword
  completion. Columns are counted in UTF-8 or UTF-16 as the editor asks, a change older than
  the text is ignored, a failure in one request is logged and answered with nothing, and a
  document over 16 MiB gets one problem (`LM-E001`) instead of being parsed. The same
  server runs in a web worker: `Server::handle_json` takes and gives JSON text.
- `mylang lsp`: programs made with `langmeld::cli::App` get the command, with
  `App::editor` giving the `editor` block and `check_with` the language's own checks.
  `langmeld init` projects have it.
- `langmeld package vscode` writes a Visual Studio Code extension from the grammar and
  `langmeld.toml`, with its language configuration (comments, brackets, word pattern) taken
  from the grammar.
- `langmeld lsp` serves `.langmeld` files: syntax errors, the problems of the grammar for
  the whole project, outline, folding, colours and go-to-definition on rule names.
- The `editor` block of a grammar is checked: a line that names a rule the language does
  not have (`LM-F016`) or a label a node does not give (`LM-F017`) is refused.
- `Language::expected` says what may come after some text, `Language::token_class` what
  sort of token a kind is, and `Language::field` finds a labelled part of a node.
- Calc has its own check, `CALC010`, for a division by a literal zero, and an example of
  its server in a browser, behind Monaco, in `examples/web`.
- `syntax::loaded()`, written in the generated code, gives the language and what it tells
  editors.
- `Source::reading` says where a grammar's imports are read from, as an editor needs to
  read the files it has open.
- Fuzzing of the language servers (`lsp_session`), the edit latency of a file of 10,000
  lines in the nightly run's summary with its bar of 50 ms, and tests that run the generated
  extension in Visual Studio Code and the browser example in a headless browser.

### Changed

- The `lsp` feature of `langmeld` (and of `langmeld-cli`, on by default for the `langmeld`
  command) adds the server; a program that does not use it builds as before.
- Calc's program is behind the `cli` feature, on by default, so that its language can be
  built for the browser without a command line.

## [0.5.0] - 2026-11-27

### Added

- `langmeld init NAME` writes a language that runs at once: a grammar, an interpreter, a
  command line and cases to test it, laid out as one program (`--bins single`), a program
  that runs programs and one for the tools (`split`), or one that acts as the command it is
  named for (`multicall`). `init --lib PATH` adds a language crate to the workspace around
  it and changes nothing else but the list of members. `generate rule` and `generate
  handler` add to it.
- Rust types for the trees of a language, written when the program is built:
  `build::generate` in a build script and `include_syntax!` where they are used. Every node
  is a type with a method for each label, every choice is an enum, tokens have types, and a
  visitor has a method for each node, so that code stops compiling when the grammar
  gains something it does not handle. `langmeld check --exhaustive` lists the nodes the
  code never names.
- `langmeld::cli::App`, behind the `cli` feature: a command line for a language with
  `check` (also as JSON, with exit codes 0, 1 and 2), `test` and `explain`, and commands of
  its own.
- `langmeld::kit::interp`, behind the `kit` feature: scopes, values and budgets for
  interpreters, and a budget that runs out is a diagnostic.
- `Diagnostics`, which collects problems and gives them back in a fixed order, and a long
  explanation of every diagnostic code, from `langmeld explain CODE` and `explain`.
- A language knows its file extensions (`Language::extensions`).
- Calc is a program that runs: `calc run`, `calc eval`, `calc check`, `calc test` and
  `calc explain`, with its own diagnostics.

### Fixed

- A rule with nothing after its `=` is reported as having no body, instead of as a missing
  colon.
- A file that cannot be read is reported without the number of the system's error.

## [0.4.0] - 2026-11-27

### Added

- JSON and Calc are also written as `.langmeld` files, and tests check that both give the
  same trees as their builder versions on every corpus file, whether loaded at run time
  or compiled by the build script.
- The `langmeld` command: `check` validates a language file or project and, with `--explain`,
  shows where it looks more than one token ahead; `test` runs the corpus files of a
  language project and `--update` rewrites what they expect; `parse --grammar` parses a
  file with a `.langmeld` file alone, reading custom rules as one error each.
- A language can be compiled ahead of time from a build script (`build::embed`) and a program
  includes the result, so it starts without reading or checking the grammar. A language
  declared with the builder can be written as bytes and read back too.
- A language can be written in a `.langmeld` file and loaded with `load_language`. The
  file is checked, and every mistake is one diagnostic at its place in the file, with a
  note that says how to fix it. Files can import others, and custom rules and tokens get
  their Rust code by name.
- `langmeld.toml`, the file of a language project, is read with every mistake reported at
  its place: the root file, how binaries are laid out, the project file and the editor
  package. It cannot repeat what the `language` block of the grammar already says.
- Strings with holes can be declared in a grammar (`Token::interpolated`): the quotes, the
  text and each hole's expression become one node.
- Grammar problems can point at the place a rule or name was written
  (`LanguageBuilder::locate`, `Pattern::located`), and `Language::lookahead` lists the
  choices that need more than one token to decide.

## [0.3.0] - 2026-11-27

### Added

- Declare a language as a grammar with a builder: rules, repetition, choices, operators
  with precedence, fields, and custom rules written in Rust. The grammar is checked when
  it is built, and each problem comes with advice on how to fix it.
- A parser that runs a declared grammar and recovers from mistakes, giving a lossless
  syntax tree and one diagnostic per mistake.
- Hand-written parsers can call rules by name, try a part and go back, and look ahead.
- A description of the nodes a grammar produces and the fields each one has.
- An example that declares JSON as a grammar, and another that declares Calc and is checked
  to give the same trees as its hand-written twin.

### Fixed

- A parser that had to leave out the root of a very deep tree could give a tree one level
  taller than its depth limit.

## [0.2.0] - 2026-11-27

### Added

- A lexer built from declared tokens: patterns and presets for numbers, strings, characters
  and identifiers, reserved and contextual keywords, nested comments, newline-ended
  statements, string holes, and tokens lexed by your own code.
- Building blocks for a hand-written parser that give a lossless syntax tree and one
  diagnostic per mistake, with limits that report instead of crashing.

## [0.1.0] - 2026-11-27

### Added

- Core types for languages: source files with identities, byte ranges, line and column
  positions in UTF-8 and UTF-16, diagnostics with labels, notes and fixes, and syntax
  kinds for `rowan` trees.

# Names and projects

Say how names bind, and `check` and the editor report names that nothing defines, go to
definitions, find uses, rename safely and complete names, across files.

## Declare how names bind

The `bind` block of a grammar says which nodes define names, which use them, and where a
name can be seen from:

```langmeld
bind {
    scope File, Block, FnDecl
    LetStmt.name defines value after              // visible after the statement
    FnDecl.name  defines value hoisted in parent  // visible in all of the scope around
    Param.name   defines value
    NameRef      refers  value
    UseStmt      imports value from UseStmt.path
}
```

A `scope` is a node that holds names, nested as the tree is. A definition and a use match
only within one kind of name, such as `value`, which you choose. A use is looked up from its
scope outwards. Each word after `defines` changes where the name can be seen:

| Word | The name is |
|------|-------------|
| none, or `shadowing` | visible after its own text, and hides an earlier one of the same name |
| `after` | visible after the node that defines it ends, so not inside it |
| `hoisted` | visible in the whole scope, before and after it |
| `in parent` | put in the scope around the node's own, as a function's name is |
| `unique` | an error to define twice in one scope |
| `global` | shared by every file with no import, and an error to define twice anywhere |
| `computed` | worked out by your code from the node, such as `worker_1` and `worker_2` |

Three more lines reach further:

- `A imports ns from A.path` brings in the names of the file whose path is written at
  `path`, in quotes, relative to the importing file.
- `A.body includes members of A.target` lets the scope of `body` also see the names inside
  what `target` refers to, in this file or another, as `stage canary for pipeline deploy {
  ... }` sees the tasks of `deploy`.
- `A.member refers member of A.object` marks uses such as the `b` in `a.b`. What `a` holds
  needs types, so your code looks the member up; see the resolver below.

A line that names a node or a part the language does not have, or words that contradict
each other, is refused when the grammar is loaded (`LM-F018` to `LM-F021`).

## What you get

- Names that nothing defines are problems (`LM-B001`), with a suggestion when a name that is
  defined is close. The editor offers the suggestion as a quick fix.
- Names that must be the only one and are defined twice are problems (`LM-B002`) that show
  where the first is, even in another file.
- Imports of files that cannot be read are problems (`LM-B003`).
- In the editor: go to definition (also from the path of an import to its file), find uses,
  rename, completion of the names that are visible where you write, hover, symbols of the
  whole workspace, and colours by what a name means.
- Rename is refused, with a reason, if the new text is not a name or is a keyword, if the
  name is computed, or if some other name would then mean something else.

## Use it from code

A `Project` holds the files of a language, and `Resolver::resolve` binds them. `Project::new`
takes a loaded language, which brings its rules:

```rust
use langmeld::bind::{Project, Resolver};
use langmeld::project::MemoryFiles;
use langmeld::{Source, load_language};

/// What the language knows without being told.
struct Builtins;

impl Resolver for Builtins {
    fn predefined(&self) -> &[(&str, &str)] {
        &[("value", "print")]
    }
}

let loaded = load_language(Source::text(
    "tiny.langmeld",
    r#"langmeld 1
language Tiny { extensions "tiny" }
tokens {
    skip ws = /[ \n]+/
    name    = preset ident(ascii)
}
rule File    = Item*
rule Item    = Let | Call
rule Let     = "let" name:Name ";"
rule Call    = target:NameRef "(" ")" ";"
rule Name    = name
rule NameRef = name
bind {
    scope File
    Let.name defines value
    NameRef  refers  value
}
"#,
))?;
let mut files = MemoryFiles::default();
files.insert("main.tiny", "let f; f(); print(); g();");
let mut project = Project::new(&loaded);
project.open("main.tiny", &files);

let bindings = Builtins.resolve(&project);
// Only `g` is unknown: `f` is defined, and `print` is predefined.
assert_eq!(bindings.diagnostics().len(), 1);
assert_eq!(bindings.diagnostics()[0].message, "`g` is not defined");
# Ok::<(), Box<dyn std::error::Error>>(())
```

`Bindings` answers what is defined and used where: `definitions`, `references`,
`definition_at`, `references_to`, `visible` and `members_of`. `Bindings::unused` gives a
warning for every name that nothing uses (`LM-B004`), for a language that wants them.
`Project::rename` and `Project::rename_with` give the edits that rename a name in every
file, or say why not.

## Add what the rules cannot say

A `Resolver` has four things to say, each with a default that says nothing:

- `predefined` names what is there without being written, such as built-in functions.
- `computed_names` gives the names a node declared `computed` defines, with where each is
  written. A task `worker[i]` inside `repeat 1..=3 as i` defines `worker_1`, `worker_2` and
  `worker_3`. Their uses resolve, and renaming them is refused.
- `member` says what a use declared `refers member of` means, and reports a member that does
  not exist (`LM-B005`) with `Member::Missing`.
- `resolve` binds in a way of your own: call `Project::bind_declared`, then add definitions
  with `Bindings::define`, point uses at them with `Bindings::retarget`, or report with
  `Bindings::report`.

Give the resolver to `App::resolver` in a program made with
[`langmeld::cli`](make-a-program.md), so that `check` and `lsp` use it, and return it from
`LanguageHooks::resolver` in a server of your own.

## Project files

A language can have a project file of its users, such as `mylang.toml`, declared as a Rust
type. Langmeld finds the nearest one above a file, reads it, and marks problems inside it:

```rust
use langmeld::project::{MemoryFiles, ProjectManifest};

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    name: String,
}

let mut files = MemoryFiles::default();
files.insert("/work/mylang.toml", "name = \"demo\"");
let found = ProjectManifest::<Settings>::discover(&files, "/work/src/main.tiny", "mylang.toml");
let manifest = found.expect("there is one")?;
assert_eq!(manifest.value.name, "demo");
assert_eq!(manifest.folder(), "/work");
# Ok::<(), Box<dyn std::error::Error>>(())
```

A file that is not TOML, has a key the type does not know or a value of the wrong kind has
one problem, `LM-M005`, marked in the file. `App::project_file::<Settings>("mylang.toml")`,
or `Server::project_file` in a server of your own, shows these problems in the editor and
completes the keys. Name the file in `[project]` of
[`langmeld.toml`](../reference/langmeld-toml.md) so that the editor extension sends it to
the server.

## Where files come from

Binding reads files through a `FileProvider`: `DiskFiles`, `MemoryFiles`, or an `Overlay`
of one over another, as an editor's unsaved texts over the disk. The language server reads
the files of the workspace and the files they import, and reads them again when they
change. `Server::files` gives it another provider, as a browser needs.

A project holds at most 10,000 files, and a file of at most 16 MiB; `Project::most_files`
and `Project::largest_file` change that. What does not fit is not added, and an import of it
is a problem (`LM-B003`) that says so.

//! `langmeld init`, which writes a new language project.

use std::path::{Component, Path, PathBuf};

use langmeld_cli::{Failure, line, read};

/// Which programs a new project has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum Layout {
    /// One program with every command.
    Single,
    /// A program that runs programs, and one with the other commands.
    Split,
    /// One program that behaves by the name it is started under.
    Multicall,
}

impl Layout {
    fn name(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Split => "split",
            Self::Multicall => "multicall",
        }
    }
}

/// Names a package cannot have.
const RESERVED: [&str; 12] = [
    "build",
    "core",
    "crate",
    "deps",
    "langmeld",
    "proc-macro",
    "self",
    "std",
    "super",
    "test",
    "alloc",
    "lib",
];

/// The parts of the templates that depend on the project.
struct Names {
    /// The package, such as `shapes-lang`.
    package: String,
    /// The package as Rust code names it.
    module: String,
    /// The language, such as `Shapes`.
    language: String,
    /// The name of the program and the file extension, such as `shapes`.
    short: String,
    /// The prefix of the language's diagnostic codes.
    codes: String,
}

fn names(package: &str) -> Result<Names, Failure> {
    let mut chars = package.chars();
    let valid = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !valid || RESERVED.contains(&package) {
        return Err(Failure::Unable(format!(
            "`{package}` cannot name a package: use letters, digits, `-` and `_`, starting \
             with a letter"
        )));
    }
    let stem = package
        .strip_suffix("-lang")
        .or_else(|| package.strip_suffix("_lang"))
        .unwrap_or(package);
    let language = stem
        .split(['-', '_'])
        .flat_map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_ascii_uppercase())
                .into_iter()
                .chain(chars)
        })
        .collect::<String>();
    let short: String = stem
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    Ok(Names {
        package: package.to_owned(),
        module: package.replace('-', "_"),
        codes: short.to_ascii_uppercase(),
        language,
        short,
    })
}

fn fill(template: &str, names: &Names, source: &str) -> String {
    template
        .replace("{{name}}", &names.package)
        .replace("{{crate}}", &names.module)
        .replace("{{Name}}", &names.language)
        .replace("{{extension}}", &names.short)
        .replace("{{binary}}", &names.short)
        .replace("{{CODE}}", &names.codes)
        .replace("{{source}}", source)
}

/// A program of the project: its file, the function of `cli` it runs and the suffix
/// of its name.
fn program(names: &Names, function: &str, suffix: &str) -> String {
    fill(include_str!("../templates/main.rs"), names, "")
        .replace("{{function}}", function)
        .replace("{{suffix}}", suffix)
}

/// The README, which tells how to run the programs that the layout makes.
fn readme(names: &Names, layout: Layout, library: bool) -> String {
    if library {
        return fill(include_str!("../templates/README.lib.md"), names, "");
    }
    let (run, tools) = match layout {
        Layout::Split => (
            format!("cargo run --bin {} --", names.short),
            format!("cargo run --bin {}c --", names.short),
        ),
        Layout::Single | Layout::Multicall => {
            ("cargo run --".to_owned(), "cargo run --".to_owned())
        }
    };
    fill(include_str!("../templates/README.md"), names, "")
        .replace("{{run}}", &run)
        .replace("{{tools}}", &tools)
}

/// The files of a project, by path.
fn files(names: &Names, source: &str, layout: Layout, library: bool) -> Vec<(String, String)> {
    let text = |template: &str| fill(template, names, source);
    let mut manifest = text(include_str!("../templates/Cargo.toml.template"));
    if !library && layout != Layout::Split {
        // The program is called after the language, not after the package.
        manifest.push_str(&text(
            "\n[[bin]]\nname = \"{{binary}}\"\npath = \"src/main.rs\"\n",
        ));
    }
    if !library {
        // A project of its own, even inside another workspace.
        manifest.push_str("\n[workspace]\n");
    }
    let mut files = vec![
        ("Cargo.toml".to_owned(), manifest),
        (
            "build.rs".to_owned(),
            text(include_str!("../templates/build.rs")),
        ),
        (
            "langmeld.toml".to_owned(),
            format!("[bins]\nlayout = \"{}\"\n", layout.name()),
        ),
        (
            "language/main.langmeld".to_owned(),
            text(include_str!("../templates/main.langmeld")),
        ),
        (
            "src/lib.rs".to_owned(),
            text(include_str!("../templates/lib.rs")),
        ),
        (
            "src/cli.rs".to_owned(),
            text(include_str!("../templates/cli.rs")),
        ),
        (
            "src/interp.rs".to_owned(),
            text(include_str!("../templates/interp.rs")),
        ),
        (
            "corpus/basics.txt".to_owned(),
            text(include_str!("../templates/corpus.txt")),
        ),
        (
            format!("examples/hello.{}", names.short),
            text(include_str!("../templates/hello.txt")),
        ),
        ("README.md".to_owned(), readme(names, layout, library)),
        (
            ".gitignore".to_owned(),
            text(include_str!("../templates/gitignore.template")),
        ),
    ];
    match layout {
        _ if library => {}
        Layout::Single | Layout::Multicall => {
            files.push(("src/main.rs".to_owned(), program(names, "app", "")));
        }
        Layout::Split => {
            let short = &names.short;
            files.push((format!("src/bin/{short}.rs"), program(names, "runner", "")));
            files.push((format!("src/bin/{short}c.rs"), program(names, "tools", "c")));
        }
    }
    files
}

fn write(project: &Path, files: &[(String, String)]) -> Result<(), Failure> {
    let occupied = std::fs::read_dir(project).is_ok_and(|mut entries| entries.next().is_some());
    if occupied || project.is_file() {
        return Err(Failure::Unable(format!(
            "{} is not empty",
            project.display()
        )));
    }
    for (path, text) in files {
        let path = project.join(path);
        let unable =
            |problem: std::io::Error| Failure::Unable(format!("{}: {problem}", path.display()));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(unable)?;
        }
        std::fs::write(&path, text).map_err(unable)?;
    }
    tidy(project, files);
    Ok(())
}

/// Lays out the code as `rustfmt` would, since the length of names changes it. Without
/// `rustfmt` the code stays as it was written, which compiles all the same.
fn tidy(project: &Path, files: &[(String, String)]) {
    let code = files
        .iter()
        .filter(|(path, _)| path.ends_with(".rs"))
        .map(|(path, _)| project.join(path));
    let _ = std::process::Command::new("rustfmt")
        .args(["--edition", "2024", "--quiet"])
        .args(code)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// The workspace a library joins: the folder of its manifest, and the path of the
/// library from there.
fn workspace_of(project: &Path) -> Result<(PathBuf, String), Failure> {
    let here = std::env::current_dir()
        .map_err(|problem| Failure::Unable(format!("no current folder: {problem}")))?;
    let full = here.join(project);
    if full.components().any(|part| part == Component::ParentDir) {
        return Err(Failure::Unable("name the folder without `..`".to_owned()));
    }
    for folder in full.ancestors().skip(1) {
        let manifest = folder.join("Cargo.toml");
        if !manifest.is_file() {
            continue;
        }
        let table: toml::Table = toml::from_str(&read(&manifest)?)
            .map_err(|problem| Failure::Unable(format!("{}: {problem}", manifest.display())))?;
        if table.contains_key("workspace") {
            let relative = full
                .strip_prefix(folder)
                .map_err(|problem| Failure::Unable(problem.to_string()))?
                .components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            return Ok((folder.to_owned(), relative));
        }
    }
    Err(Failure::Unable(
        "there is no workspace around that folder; `langmeld init NAME` makes a project of \
         its own"
            .to_owned(),
    ))
}

/// Whether the members of a workspace already include `path`.
fn is_member(manifest: &str, path: &str) -> bool {
    let table: toml::Table = toml::from_str(manifest).unwrap_or_default();
    let members = table
        .get("workspace")
        .and_then(|workspace| workspace.get("members"))
        .and_then(toml::Value::as_array);
    let parent = path.rsplit_once('/').map_or("", |(parent, _)| parent);
    members.into_iter().flatten().any(|member| {
        member.as_str().is_some_and(|member| {
            let member = member.trim_start_matches("./");
            member == path
                || member
                    .strip_suffix('*')
                    .is_some_and(|prefix| prefix.trim_end_matches('/') == parent)
        })
    })
}

/// The manifest with `path` among the members, keeping its layout. `None` if it has
/// no list of members to add to.
fn with_member(manifest: &str, path: &str) -> Option<String> {
    let start = manifest.find("members")?;
    let open = start + manifest.get(start..)?.find('[')?;
    let close = open + manifest.get(open..)?.find(']')?;
    let list = manifest.get(open + 1..close)?;
    let kept = list.trim_end();
    let comma = if kept.is_empty() || kept.ends_with(',') {
        ""
    } else {
        ","
    };
    let added = if list.contains('\n') {
        format!("{kept}{comma}\n    \"{path}\",\n")
    } else if kept.is_empty() {
        format!("\"{path}\"")
    } else {
        format!("{kept}{comma} \"{path}\"")
    };
    let updated = format!(
        "{}{added}{}",
        manifest.get(..=open)?,
        manifest.get(close..)?
    );
    is_member(&updated, path).then_some(updated)
}

/// Writes a new project named `name`, or a library at `library` inside a workspace.
pub(crate) fn run(
    name: Option<&str>,
    library: Option<&Path>,
    layout: Layout,
    local: Option<&str>,
) -> Result<(), Failure> {
    let source = match local {
        Some(path) => format!("path = \"{}\"", path.replace('\\', "/")),
        None => format!("version = \"{}\"", env!("CARGO_PKG_VERSION")),
    };
    if let Some(project) = library {
        let package = project
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let names = names(&package)?;
        let (root, relative) = workspace_of(project)?;
        write(project, &files(&names, &source, layout, true))?;
        let path = root.join("Cargo.toml");
        let manifest = read(&path)?;
        if !is_member(&manifest, &relative) {
            match with_member(&manifest, &relative) {
                Some(updated) => {
                    std::fs::write(&path, updated).map_err(|problem| {
                        Failure::Unable(format!("{}: {problem}", path.display()))
                    })?;
                    line(&format!("Added {relative} to the workspace."));
                }
                None => line(&format!(
                    "Add \"{relative}\" to the workspace members in {}.",
                    path.display()
                )),
            }
        }
        line(&format!(
            "Created the {} language in {}. Its programs come from `{}::cli`.",
            names.language,
            project.display(),
            names.module
        ));
        return Ok(());
    }
    let name = name.ok_or_else(|| Failure::Unable("name the project".to_owned()))?;
    let names = names(name)?;
    write(Path::new(name), &files(&names, &source, layout, false))?;
    let run = match layout {
        Layout::Split => format!("cargo run --bin {} -- run", names.short),
        Layout::Single | Layout::Multicall => "cargo run -- run".to_owned(),
    };
    line(&format!(
        "Created the {} language in {name}. Try it:\n\n    cd {name}\n    {run} \
         examples/hello.{}\n\nREADME.md says what to change.",
        names.language, names.short
    ));
    Ok(())
}

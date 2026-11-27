//! The project files of a language's users.

use langmeld_ide::project::{MemoryFiles, ProjectManifest, find_up, keys};
use langmeld_syntax::{FileId, SourceFile};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct App {
    name: String,
    #[serde(default)]
    build: Option<Build>,
    #[serde(default)]
    targets: Vec<Target>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct Build {
    #[serde(default)]
    out_dir: String,
    #[serde(default)]
    jobs: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Target {
    path: String,
}

fn file(text: &str) -> SourceFile {
    SourceFile::new(FileId::new(7), "app.toml", text.to_owned()).expect("small")
}

#[test]
fn a_project_file_is_read_into_the_language_type() {
    let manifest = ProjectManifest::<App>::parse(file(
        "name = \"demo\"\n[build]\njobs = 4\n[[targets]]\npath = \"a\"\n",
    ))
    .expect("valid");
    assert_eq!(manifest.value.name, "demo");
    let build = manifest.value.build.expect("given");
    assert_eq!((build.jobs, build.out_dir.as_str()), (4, ""));
    assert_eq!(manifest.value.targets[0].path, "a");
}

#[test]
fn problems_point_into_the_project_file() {
    let problem = ProjectManifest::<App>::parse(file("name = \"demo\"\nnmae = 1\n"))
        .expect_err("unknown key");
    let found = &problem.diagnostic;
    assert_eq!(found.code, "LM-M005");
    assert_eq!(found.file, FileId::new(7));
    assert_eq!(usize::from(found.range.start()), 14);
    assert_eq!(usize::from(found.range.end()), 18);
    assert_eq!(
        found.message,
        "unknown field `nmae`, expected one of `name`, `build`, `targets`"
    );
    assert_eq!(
        found.notes,
        [
            "To fix this, correct the file at the place marked: check the spelling of the key \
             and the kind of its value."
        ]
    );
    assert_eq!(
        problem.to_string(),
        "app.toml:2: unknown field `nmae`, expected one of `name`, `build`, `targets`"
    );
    // A file that is not TOML at all has its mistake pointed at too.
    let broken = ProjectManifest::<App>::parse(file("name = ")).expect_err("no value");
    assert_eq!(broken.diagnostic.code, "LM-M005");
    assert_eq!(
        broken.diagnostic.message,
        "string values must be quoted, expected literal string"
    );
}

#[test]
fn the_nearest_project_file_above_is_found() {
    let files: MemoryFiles = [
        ("/p/app.toml", "name = \"outer\""),
        ("/p/sub/app.toml", "name = \"inner\""),
    ]
    .into_iter()
    .map(|(path, text)| (path.to_owned(), text.to_owned()))
    .collect();
    let found = |from| find_up(&files, from, "app.toml");
    assert_eq!(
        found("/p/sub/deep/er/main.x").as_deref(),
        Some("/p/sub/app.toml")
    );
    assert_eq!(found("/p/main.x").as_deref(), Some("/p/app.toml"));
    assert_eq!(found("/q/main.x"), None);
    assert_eq!(found("main.x"), None);
    let manifest = ProjectManifest::<App>::discover(&files, "/p/sub/main.x", "app.toml")
        .expect("found")
        .expect("valid");
    assert_eq!(manifest.value.name, "inner");
    assert_eq!(manifest.folder(), "/p/sub");
}

#[test]
fn the_keys_of_each_table_are_known() {
    assert_eq!(keys::<App>(&[]), ["name", "build", "targets"]);
    assert_eq!(keys::<App>(&["build"]), ["out-dir", "jobs"]);
    assert_eq!(keys::<App>(&["targets"]), ["path"]);
    assert!(keys::<App>(&["name"]).is_empty());
    assert!(keys::<App>(&["missing", "deeper"]).is_empty());
}

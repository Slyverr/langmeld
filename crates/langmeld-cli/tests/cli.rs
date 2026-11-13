use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("langmeld-cli-test-{}-{id}", std::process::id()));
        fs::create_dir_all(&path).expect("test directory should be created");
        Self(path)
    }

    fn path(&self) -> &PathBuf {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write_widget_project(project: &Path) {
    fs::create_dir_all(project.join("language/constructs")).unwrap();
    fs::write(
        project.join("langmeld.toml"),
        "format = 1\ndefinition = \"language/language.langmeld\"\n",
    )
    .unwrap();
    fs::write(
        project.join("language/language.langmeld"),
        "language \"Widget\" { import \"constructs/statements.langmeld\"; construct \"program\" { syntax { literal \"program\"; } block \"{\" \"}\" { allow \"print\"; } } }",
    )
    .unwrap();
    fs::write(
        project.join("language/constructs/statements.langmeld"),
        "construct \"print\" { syntax { literal \"print\"; identifier \"name\"; literal \";\"; } }",
    )
    .unwrap();
}

#[path = "cli/check_format.rs"]
mod check_format;
#[path = "cli/generate.rs"]
mod generate;
#[path = "cli/help.rs"]
mod help;
#[path = "cli/init.rs"]
mod init;
#[path = "cli/lsp.rs"]
mod lsp;

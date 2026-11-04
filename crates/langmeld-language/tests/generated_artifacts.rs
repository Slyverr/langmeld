use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use langmeld_language::{
    GeneratedArtifactWrite, check_generated_artifacts, write_generated_artifacts,
};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

struct TempProject(PathBuf);

impl TempProject {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "lm-artifacts-{}-{}",
            std::process::id(),
            NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("create isolated project directory");
        Self(path)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }

    fn generated(&self) -> PathBuf {
        self.0.join(".langmeld/generated")
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn artifacts() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("README.md".to_owned(), "# Generated language\n".to_owned()),
        ("nested/metadata.json".to_owned(), "{}\n".to_owned()),
    ])
}

#[test]
fn write_generated_artifacts_creates_a_complete_tree_and_reports_up_to_date() {
    let project = TempProject::new();
    let expected = artifacts();

    let result = write_generated_artifacts(project.path(), &expected)
        .expect("first generation writes the complete set");

    assert_eq!(result, GeneratedArtifactWrite::Updated { file_count: 2 });
    check_generated_artifacts(project.path(), &expected)
        .expect("the generated set matches the expected artifacts");
    assert_eq!(
        write_generated_artifacts(project.path(), &expected)
            .expect("writing an up-to-date set succeeds"),
        GeneratedArtifactWrite::UpToDate { file_count: 2 }
    );
}

#[test]
fn check_generated_artifacts_reports_stale_files_without_rewriting_them() {
    let project = TempProject::new();
    let expected = artifacts();
    write_generated_artifacts(project.path(), &expected).expect("initial generation succeeds");
    let generated_file = project.generated().join("README.md");
    fs::write(&generated_file, "user changed this\n").expect("change generated file");

    let error = check_generated_artifacts(project.path(), &expected)
        .expect_err("check detects generated content drift");

    assert!(error.contains("README.md"));
    assert_eq!(
        fs::read_to_string(generated_file).expect("read unchanged stale content"),
        "user changed this\n"
    );
}

#[test]
fn write_generated_artifacts_replaces_only_its_owned_directory() {
    let project = TempProject::new();
    fs::write(project.path().join("README.md"), "authored\n").expect("write authored README");
    let mut prior = artifacts();
    prior.insert("removed.txt".to_owned(), "stale\n".to_owned());
    write_generated_artifacts(project.path(), &prior).expect("initial generation succeeds");

    let result = write_generated_artifacts(project.path(), &artifacts())
        .expect("new generated set replaces the prior set");

    assert_eq!(result, GeneratedArtifactWrite::Updated { file_count: 2 });
    assert!(!project.generated().join("removed.txt").exists());
    assert_eq!(
        fs::read_to_string(project.path().join("README.md")).expect("read authored README"),
        "authored\n"
    );
}

#[test]
fn write_generated_artifacts_rejects_paths_that_escape_the_owned_directory() {
    let project = TempProject::new();
    let expected = BTreeMap::from([("../escaped.txt".to_owned(), "unsafe".to_owned())]);

    let error = write_generated_artifacts(project.path(), &expected)
        .expect_err("parent traversal is rejected before writing");

    assert!(error.contains("unsafe"));
    assert!(!project.path().join("escaped.txt").exists());
}

#[test]
fn write_generated_artifacts_rejects_noncanonical_and_conflicting_paths() {
    let project = TempProject::new();
    let noncanonical = BTreeMap::from([("nested//file.txt".to_owned(), "unsafe".to_owned())]);
    let conflict = BTreeMap::from([
        ("node".to_owned(), "file".to_owned()),
        ("node/child.txt".to_owned(), "child".to_owned()),
    ]);

    let error = write_generated_artifacts(project.path(), &noncanonical)
        .expect_err("repeated separators are rejected");
    assert!(error.contains("unsafe"));

    let error = write_generated_artifacts(project.path(), &conflict)
        .expect_err("a file cannot also be a parent directory");
    assert!(error.contains("conflict"));
    assert!(!project.generated().exists());
}

#[test]
fn write_generated_artifacts_rejects_an_empty_set() {
    let project = TempProject::new();

    let error = write_generated_artifacts(project.path(), &BTreeMap::new())
        .expect_err("an empty set cannot erase a previous generated tree");

    assert!(error.contains("empty"));
    assert!(!project.generated().exists());
}

#[cfg(unix)]
#[test]
fn check_generated_artifacts_rejects_a_nested_symlink() {
    use std::os::unix::fs::symlink;

    let project = TempProject::new();
    let outside = TempProject::new();
    let expected = artifacts();
    write_generated_artifacts(project.path(), &expected).expect("initial generation succeeds");
    let nested = project.generated().join("nested");
    fs::remove_file(nested.join("metadata.json")).expect("remove generated nested file");
    fs::remove_dir(&nested).expect("remove generated nested directory");
    symlink(outside.path(), &nested).expect("link generated content outside the project");

    let error = check_generated_artifacts(project.path(), &expected)
        .expect_err("nested output symlinks are rejected");

    assert!(error.contains("symlink"));
    assert!(
        fs::read_dir(outside.path())
            .expect("inspect outside directory")
            .next()
            .is_none()
    );
}

#[cfg(unix)]
#[test]
fn write_generated_artifacts_rejects_symlinked_state_and_output_roots() {
    use std::os::unix::fs::symlink;

    let project = TempProject::new();
    let outside = TempProject::new();
    let expected = artifacts();

    symlink(outside.path(), project.path().join(".langmeld"))
        .expect("link the state directory outside the project");
    let error = write_generated_artifacts(project.path(), &expected)
        .expect_err("a symlinked state directory is rejected");
    assert!(error.contains("plain directory"));
    assert!(
        fs::read_dir(outside.path())
            .expect("inspect outside directory")
            .next()
            .is_none()
    );

    fs::remove_file(project.path().join(".langmeld")).expect("remove state symlink");
    fs::create_dir(project.path().join(".langmeld")).expect("create real state directory");
    symlink(outside.path(), project.generated())
        .expect("link generated output outside the project");
    let error = write_generated_artifacts(project.path(), &expected)
        .expect_err("a symlinked generated directory is rejected");
    assert!(error.contains("plain directory"));
    assert!(
        fs::read_dir(outside.path())
            .expect("inspect outside directory again")
            .next()
            .is_none()
    );
}

#[cfg(unix)]
#[test]
fn check_generated_artifacts_rejects_special_files() {
    use std::os::unix::net::UnixListener;

    let project = TempProject::new();
    let expected = artifacts();
    write_generated_artifacts(project.path(), &expected).expect("initial generation succeeds");
    let socket_path = project.generated().join("s");
    fs::remove_file(project.generated().join("nested/metadata.json"))
        .expect("remove regular generated file");
    let _listener = UnixListener::bind(&socket_path).expect("create a special file");

    let error = check_generated_artifacts(project.path(), &expected)
        .expect_err("special output files are rejected");

    assert!(error.contains("special file"));
}

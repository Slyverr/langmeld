use super::*;
use std::process::Command;

fn shared_target_dir(workspace_root: &Path) -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .map(|target_dir| {
            if target_dir.is_absolute() {
                target_dir
            } else {
                workspace_root.join(target_dir)
            }
        })
        .unwrap_or_else(|| workspace_root.join("target"))
}

#[test]
fn init_generates_a_modular_declarative_project_without_overwriting_files() {
    let parent = TestDirectory::new();
    let project = parent.path().join("widget-language");
    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "init",
            project.to_str().expect("temporary path is UTF-8"),
            "--name",
            "widget-language",
            "--language-name",
            r#"Widget "Script""#,
            "--non-interactive",
        ])
        .output()
        .expect("CLI should start");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("langmeld lsp`"));
    assert!(project.join("Cargo.toml").is_file());
    assert_eq!(
        fs::read_to_string(project.join("LICENSE-MIT")).unwrap(),
        include_str!("../../templates/LICENSE-MIT")
    );
    assert_eq!(
        fs::read_to_string(project.join("LICENSE-APACHE")).unwrap(),
        include_str!("../../templates/LICENSE-APACHE")
    );
    assert!(project.join("langmeld.toml").is_file());
    assert!(!project.join(".cargo/config.toml").exists());
    assert!(project.join("src/lib.rs").is_file());
    assert!(project.join("src/main.rs").is_file());
    assert!(project.join("src/runtime.rs").is_file());
    assert!(project.join("src/constructs/mod.rs").is_file());
    assert!(project.join("src/constructs/let_handler.rs").is_file());
    assert!(project.join("src/constructs/print.rs").is_file());
    let let_handler = fs::read_to_string(project.join("src/constructs/let_handler.rs"))
        .expect("declarative let handler should be generated");
    assert!(let_handler.contains("const CONSTRUCT: &'static str = \"let\";"));
    assert!(let_handler.contains("context.field_text_capture(\"value\")?"));
    assert!(project.join("tests/language.rs").is_file());
    assert!(project.join("language/main.langmeld").is_file());
    assert!(!project.join("AGENTS.md").exists());
    assert!(!project.join(".agents").exists());
    assert!(!project.join("language/language.langmeld").exists());
    assert!(
        project
            .join("language/constructs/statements.langmeld")
            .is_file()
    );
    let main = fs::read_to_string(project.join("src/main.rs")).expect("CLI should be generated");
    assert!(main.contains("Cli::parse()"));
    assert!(main.contains("fs::read_to_string"));
    let library = fs::read_to_string(project.join("src/lib.rs"))
        .expect("reusable language loader should be generated");
    assert!(!library.contains('\r'));
    assert!(library.ends_with("}\n"));
    assert!(!library.ends_with("}\n\n"));
    assert!(
        library.contains("pub use langmeld_project::LanguageSpecLoadError as LanguageLoadError;")
    );
    assert!(library.contains("pub fn load_from(project_root: impl AsRef<Path>)"));
    assert!(library.contains("load_language_spec(project_root)?"));
    assert!(main.contains("error.diagnostics()"));
    assert!(main.contains("error.sources()"));
    let manifest =
        fs::read_to_string(project.join("Cargo.toml")).expect("manifest should be generated");
    assert!(manifest.contains("[workspace]"));
    assert!(manifest.contains("license = \"MIT OR Apache-2.0\""));
    assert!(manifest.contains("clap = { version = \"4.6\", features = [\"derive\"] }"));
    let readme = fs::read_to_string(project.join("README.md")).expect("README should be generated");
    assert!(readme.contains("## License"));
    assert!(readme.contains("language/main.langmeld"));
    assert!(readme.contains("src/constructs/"));
    assert!(readme.contains("langmeld generate construct repeat"));
    assert!(readme.contains("generated handler reports a diagnostic"));
    assert!(!readme.contains("{{TYPED_BINDINGS_GUIDANCE}}"));
    let manifest = fs::read_to_string(project.join("langmeld.toml"))
        .expect("Langmeld manifest should be generated");
    assert_eq!(manifest, "format = 1\n");
    let check = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .current_dir(&project)
        .arg("check")
        .output()
        .expect("CLI should start");
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
    let definition = fs::read_to_string(project.join("language/main.langmeld"))
        .expect("definition should be generated");
    assert!(definition.starts_with(r#"language "Widget \"Script\"""#));
    assert!(definition.contains("import \"constructs/statements.langmeld\";"));
    assert!(definition.contains("root \"program\";"));
    let test_source = fs::read_to_string(project.join("tests/language.rs"))
        .expect("behavior tests should be generated");
    assert!(test_source.contains("assert_eq!(report.output, \"Hello\\n\")"));
    assert!(test_source.contains("failure.diagnostics"));
    assert!(test_source.contains("failure.partial_output"));
    assert!(test_source.contains("unknown variable `missing`"));
}

#[test]
fn init_can_generate_typed_capture_bindings_for_declarative_projects() {
    let parent = TestDirectory::new();
    let project = parent.path().join("typed-language");
    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "init",
            project.to_str().expect("temporary path is UTF-8"),
            "--name",
            "typed-language",
            "--language-name",
            "TypedLanguage",
            "--non-interactive",
            "--typed-bindings",
            "--local-crates",
        ])
        .output()
        .expect("CLI should start");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let manifest = fs::read_to_string(project.join("langmeld.toml"))
        .expect("Langmeld manifest should be generated");
    assert_eq!(manifest, "format = 1\nrust_bindings = \"typed\"\n");

    let cargo_manifest =
        fs::read_to_string(project.join("Cargo.toml")).expect("Cargo manifest should be generated");
    assert!(cargo_manifest.contains(&format!(
        "[build-dependencies]\nlangmeld-build = \"{}\"",
        env!("CARGO_PKG_VERSION")
    )));
    let cargo_config = fs::read_to_string(project.join(".cargo/config.toml"))
        .expect("local Langmeld crate overrides should be generated");
    assert!(cargo_config.contains("langmeld-build = { path = "));
    assert!(project.join("build.rs").is_file());
    assert_eq!(
        fs::read_to_string(project.join("build.rs")).unwrap(),
        include_str!("../../templates/declarative/build.rs")
    );

    let library = fs::read_to_string(project.join("src/lib.rs"))
        .expect("language library should be generated");
    assert!(library.contains("pub mod generated"));
    assert!(library.contains("/langmeld_bindings.rs"));

    let let_handler = fs::read_to_string(project.join("src/constructs/let_handler.rs"))
        .expect("typed let handler should be generated");
    assert!(
        let_handler
            .contains("const CONSTRUCT: &'static str = crate::generated::constructs::Let::NAME;")
    );
    assert!(!let_handler.contains("const CONSTRUCT: &'static str = \"let\";"));
    assert!(let_handler.contains("context.typed::<crate::generated::constructs::Let>()?"));
    assert!(let_handler.contains("syntax.name()?"));
    assert!(let_handler.contains("syntax.value()?"));
    assert!(!let_handler.contains("field_text_capture"));

    let print_handler = fs::read_to_string(project.join("src/constructs/print.rs"))
        .expect("typed print handler should be generated");
    assert!(
        print_handler
            .contains("const CONSTRUCT: &'static str = crate::generated::constructs::Print::NAME;")
    );
    assert!(print_handler.contains("context.typed::<crate::generated::constructs::Print>()?"));
    assert!(print_handler.contains("syntax.name()?"));
    assert!(!print_handler.contains("field_identifier_capture"));

    let statements = project.join("language/constructs/statements.langmeld");
    let mut source = fs::read_to_string(&statements).expect("construct definitions should exist");
    source.push_str(
        "\nconstruct \"parent\" { syntax { literal \"parent\"; zero_or_more \"children\" { literal \"child\"; } } implements \"statement\"; }\n",
    );
    fs::write(&statements, source).expect("named group should be added");

    let collision = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .current_dir(&project)
        .args([
            "generate",
            "construct",
            "parent-children",
            "--literal",
            "parent_children",
            "--category",
            "statement",
        ])
        .output()
        .expect("CLI should start");
    assert!(!collision.status.success());
    assert!(
        String::from_utf8_lossy(&collision.stderr).contains(
            "multiple language constructs or groups map to the Rust type `ParentChildren`"
        ),
        "{}",
        String::from_utf8_lossy(&collision.stderr)
    );
    assert!(
        !statements
            .parent()
            .unwrap()
            .join("constructs/parent-children.langmeld")
            .exists()
    );
    assert!(!project.join("src/constructs/parent_children.rs").exists());

    let generated = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .current_dir(&project)
        .args([
            "generate",
            "construct",
            "repeat",
            "--literal",
            "repeat",
            "--category",
            "statement",
        ])
        .output()
        .expect("CLI should start");
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let repeat_handler = fs::read_to_string(project.join("src/constructs/repeat.rs"))
        .expect("typed construct handler should be generated");
    assert!(
        repeat_handler.contains(
            "const CONSTRUCT: &'static str = crate::generated::constructs::Repeat::NAME;"
        )
    );
    assert!(repeat_handler.contains("context.typed::<crate::generated::constructs::Repeat>()?"));

    let readme = fs::read_to_string(project.join("README.md")).expect("README should exist");
    assert!(readme.contains("Typed capture accessors are generated"));
    assert!(!readme.contains("{{TYPED_BINDINGS_GUIDANCE}}"));

    let check = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .current_dir(&project)
        .arg("check")
        .output()
        .expect("CLI should start");
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );

    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let cargo_test = Command::new("cargo")
        .current_dir(&project)
        .args(["test", "--offline", "--manifest-path"])
        .arg("Cargo.toml")
        .arg("--target-dir")
        .arg(shared_target_dir(&workspace_root))
        .output()
        .expect("generated project should compile and run its tests");
    assert!(
        cargo_test.status.success(),
        "{}{}",
        String::from_utf8_lossy(&cargo_test.stdout),
        String::from_utf8_lossy(&cargo_test.stderr)
    );
    assert!(
        String::from_utf8_lossy(&cargo_test.stdout)
            .contains("test runs_a_program_from_the_project_definition ... ok"),
        "{}",
        String::from_utf8_lossy(&cargo_test.stdout)
    );
}

#[test]
fn init_rejects_typed_bindings_for_custom_parser_projects() {
    let parent = TestDirectory::new();
    let project = parent.path().join("custom-language");
    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "init",
            project.to_str().expect("temporary path is UTF-8"),
            "--template",
            "custom-parser",
            "--typed-bindings",
            "--non-interactive",
        ])
        .output()
        .expect("CLI should start");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("requires the declarative"));
    assert!(!project.exists());
}

#[test]
fn init_uses_ignored_local_crate_overrides_for_both_templates() {
    let parent = TestDirectory::new();
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|path| path.join("crates/langmeld-core/Cargo.toml").is_file())
        .expect("CLI should be built from the Langmeld workspace")
        .canonicalize()
        .expect("workspace root should be canonicalized");
    let templates = [
        (
            "declarative",
            None,
            ["langmeld-core", "langmeld-language", "langmeld-project"].as_slice(),
            false,
            Some("runs_a_program_from_the_project_definition"),
        ),
        (
            "typed-declarative",
            None,
            [
                "langmeld-core",
                "langmeld-language",
                "langmeld-project",
                "langmeld-build",
            ]
            .as_slice(),
            true,
            None,
        ),
        (
            "custom-parser",
            Some("custom-parser"),
            ["langmeld-core", "langmeld-language", "langmeld-lsp"].as_slice(),
            false,
            Some("parses_a_valid_say_statement"),
        ),
    ];

    for (project_name, template, crates, typed_bindings, expected_test) in templates {
        let project = parent.path().join(project_name);
        let mut command = Command::new(env!("CARGO_BIN_EXE_langmeld"));
        command.args([
            "init",
            project.to_str().expect("temporary path is UTF-8"),
            "--name",
            project_name,
            "--language-name",
            "ExampleLanguage",
            "--non-interactive",
            "--local-crates",
        ]);

        if let Some(template) = template {
            command.args(["--template", template]);
        }
        if typed_bindings {
            command.arg("--typed-bindings");
        }

        let output = command.output().expect("CLI should start");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let config_path = project.join(".cargo/config.toml");
        let config_source = fs::read_to_string(&config_path).expect("local Cargo config exists");
        assert!(config_source.starts_with("[patch.crates-io]\n"));

        for crate_name in crates {
            assert!(config_source.contains(&format!("{crate_name} = {{ path = \"")));
            assert!(
                workspace_root
                    .join("crates")
                    .join(crate_name)
                    .join("Cargo.toml")
                    .is_file()
            );
        }

        let gitignore =
            fs::read_to_string(project.join(".gitignore")).expect("generated ignore file exists");
        assert!(gitignore.lines().any(|line| line == "/.cargo/config.toml"));

        let manifest =
            fs::read_to_string(project.join("Cargo.toml")).expect("generated manifest exists");
        assert!(manifest.contains(&format!(
            "langmeld-core = \"{}\"",
            env!("CARGO_PKG_VERSION")
        )));
        assert!(!manifest.contains(workspace_root.to_str().unwrap()));

        let metadata = Command::new("cargo")
            .args([
                "metadata",
                "--format-version",
                "1",
                "--no-deps",
                "--offline",
            ])
            .current_dir(&project)
            .output()
            .expect("Cargo should start");
        assert!(
            metadata.status.success(),
            "{}",
            String::from_utf8_lossy(&metadata.stderr)
        );

        let Some(expected_test) = expected_test else {
            continue;
        };
        let output = Command::new("cargo")
            .current_dir(&project)
            .args(["test", "--offline", "--manifest-path", "Cargo.toml"])
            .arg("--target-dir")
            .arg(shared_target_dir(&workspace_root))
            .output()
            .expect("generated project tests should run");
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout)
                .contains(&format!("test {expected_test} ... ok")),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}

#[test]
fn init_can_include_optional_agent_instructions_and_skill() {
    let parent = TestDirectory::new();
    let project = parent.path().join("widget-language");
    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "init",
            project.to_str().expect("temporary path is UTF-8"),
            "--non-interactive",
            "--agents-md",
            "--agent-skill",
        ])
        .output()
        .expect("CLI should start");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(project.join("AGENTS.md")).unwrap(),
        include_str!("../../templates/AGENTS.md")
    );
    assert_eq!(
        fs::read_to_string(project.join(".agents/skills/langmeld-language/SKILL.md")).unwrap(),
        include_str!("../../templates/langmeld-language/SKILL.md")
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("created `AGENTS.md`"));
    assert!(stdout.contains("created `.agents/skills/langmeld-language/SKILL.md`"));
}

#[test]
fn init_normalizes_directory_names_into_valid_package_defaults() {
    let parent = TestDirectory::new();
    let project = parent.path().join("_widget-language_");
    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "init",
            project.to_str().expect("temporary path is UTF-8"),
            "--non-interactive",
        ])
        .output()
        .expect("CLI should start");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let manifest = fs::read_to_string(project.join("Cargo.toml"))
        .expect("generated project manifest should exist");
    assert!(manifest.contains("name = \"widget-language\""));
    assert!(manifest.contains("clap = { version = \"4.6\", features = [\"derive\"] }"));
}

#[cfg(unix)]
#[test]
fn init_rolls_back_files_and_directories_after_nested_symlink_failure() {
    let parent = TestDirectory::new();
    let outside = TestDirectory::new();
    let project = parent.path().join("symlinked-language");
    fs::create_dir_all(project.join("language"))
        .expect("language definition directory should be created");
    std::os::unix::fs::symlink(outside.path(), project.join("language/constructs"))
        .expect("test symlink should be created");

    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "init",
            project.to_str().expect("temporary path is UTF-8"),
            "--non-interactive",
        ])
        .output()
        .expect("CLI should start");

    assert!(!output.status.success());
    assert!(!project.join("Cargo.toml").exists());
    assert!(!project.join("README.md").exists());
    assert!(!project.join("language/main.langmeld").exists());
    assert!(!project.join("src").exists());
    assert!(
        fs::symlink_metadata(project.join("language/constructs"))
            .expect("user-owned symlink should remain")
            .file_type()
            .is_symlink()
    );
    assert!(!outside.path().join("main.rs").exists());
}

#[test]
fn init_refuses_to_overwrite_existing_project_files() {
    let parent = TestDirectory::new();
    let project = parent.path().join("widget-language");
    fs::create_dir_all(&project).expect("project directory should be created");
    fs::write(project.join("Cargo.toml"), "user content").expect("file should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "init",
            project.to_str().expect("temporary path is UTF-8"),
            "--non-interactive",
        ])
        .output()
        .expect("CLI should start");

    assert!(!output.status.success());
    assert_eq!(
        fs::read_to_string(project.join("Cargo.toml")).expect("original file should remain"),
        "user content"
    );
    assert!(!project.join("src").exists());
}

#[cfg(unix)]
#[test]
fn init_rejects_dangling_generated_file_symlinks_during_preflight() {
    use std::os::unix::fs::symlink;

    let parent = TestDirectory::new();
    let project = parent.path().join("widget-language");
    let definition_directory = project.join("language");
    fs::create_dir_all(&definition_directory).expect("definition directory should be created");

    let definition_path = definition_directory.join("main.langmeld");
    let missing_target = parent.path().join("missing-main.langmeld");
    symlink(&missing_target, &definition_path).expect("test symlink should be created");

    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "init",
            project.to_str().expect("temporary path is UTF-8"),
            "--non-interactive",
        ])
        .output()
        .expect("CLI should start");

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("already exists; no files were changed")
    );
    assert!(
        fs::symlink_metadata(&definition_path)
            .expect("dangling symlink should remain")
            .file_type()
            .is_symlink()
    );
    assert!(!missing_target.exists());
    assert!(!project.join("Cargo.toml").exists());
    assert!(!project.join("README.md").exists());
}

#[test]
fn help_lists_init_and_the_non_interactive_mode() {
    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args(["init", "--help"])
        .output()
        .expect("CLI should start");

    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("Create a starter language project"));
    assert!(help.contains("--non-interactive"));
    assert!(help.contains("--typed-bindings"));
    assert!(help.contains("--local-crates"));
    assert!(help.contains("--agents-md"));
    assert!(help.contains("--agent-skill"));

    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args(["lsp", "--help"])
        .output()
        .expect("CLI should start");

    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(
        help.contains("Serve editor support for a language project or `.langmeld` definitions")
    );
    assert!(help.contains("--manifest-path"));
    assert!(help.contains("--definitions"));

    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args(["format", "--help"])
        .output()
        .expect("CLI should start");

    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("Format a language project's `.langmeld` definitions"));
    assert!(help.contains("--check"));

    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args(["generate", "textmate", "--help"])
        .output()
        .expect("CLI should start");

    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("Generate a lexical TextMate grammar"));
    assert!(help.contains("--scope-name"));
}

#[test]
fn init_can_scaffold_a_host_owned_rust_parser() {
    let parent = TestDirectory::new();
    let project = parent.path().join("custom-language");
    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "init",
            project.to_str().expect("temporary path is UTF-8"),
            "--template",
            "custom-parser",
            "--name",
            "custom-language",
            "--language-name",
            "Custom Language",
            "--non-interactive",
        ])
        .output()
        .expect("CLI should start");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("cargo run --bin language-lsp"));
    assert!(project.join("Cargo.toml").is_file());
    assert!(project.join("src/lib.rs").is_file());
    assert!(project.join("src/main.rs").is_file());
    assert!(project.join("src/bin/language-lsp.rs").is_file());
    assert!(project.join("tests/parser.rs").is_file());
    assert!(!project.join("langmeld.toml").exists());
    assert!(!project.join("language").exists());
    let manifest =
        fs::read_to_string(project.join("Cargo.toml")).expect("manifest should be generated");
    assert!(manifest.contains("default-run = \"custom-language\""));
    assert!(manifest.contains("clap = { version = \"4.6\", features = [\"derive\"] }"));
    assert!(!manifest.contains("langmeld-project"));
    assert!(manifest.contains("langmeld-lsp = "));
    let readme = fs::read_to_string(project.join("README.md")).expect("README should be generated");
    assert!(readme.contains("## License"));
    assert!(readme.contains("LICENSE-MIT"));
    let parser =
        fs::read_to_string(project.join("src/lib.rs")).expect("host parser should be generated");
    assert!(parser.contains("impl LanguageParser for SayParser"));
    assert!(parser.contains("split_inclusive('\\n')"));
    let binary =
        fs::read_to_string(project.join("src/main.rs")).expect("binary should be generated");
    assert!(binary.contains("Custom Language"));
    assert!(binary.contains("Cli::parse()"));
    assert!(binary.contains("fs::read_to_string"));
    let lsp = fs::read_to_string(project.join("src/bin/language-lsp.rs"))
        .expect("language server entry point should be generated");
    assert!(lsp.contains("HostLanguageBackend::new(SayParser)"));
}

use crate::TestDirectory;
use std::{fs, process::Command};

#[test]
fn generate_construct_adds_import_syntax_and_registered_handler() {
    let parent = TestDirectory::new();
    let project = parent.path().join("widget-language");
    let init = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "init",
            project.to_str().expect("temporary path is UTF-8"),
            "--non-interactive",
        ])
        .output()
        .expect("CLI should start");
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );

    let manifest = project.join("langmeld.toml");
    let missing_name = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "construct",
            "--manifest-path",
            manifest.to_str().expect("temporary path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");
    assert!(!missing_name.status.success());
    assert!(
        String::from_utf8_lossy(&missing_name.stderr)
            .contains("provide a construct name or run this command in an interactive terminal")
    );

    let root_path = project.join("language/main.langmeld");
    let mut root = fs::read_to_string(&root_path).expect("root definition should be readable");
    root = root.replace('\n', "\r\n");
    root = root
        .replace("\r\n        ", "\r\n\t\t")
        .replace("\r\n    ", "\r\n\t");
    let closing_brace = root
        .rfind("}\r\n")
        .expect("root should end with a closing brace");
    root.insert(closing_brace, '\t');
    fs::write(&root_path, root).expect("indented root should be written");

    let registry_path = project.join("src/constructs/mod.rs");
    let registry = fs::read_to_string(&registry_path).expect("registry should be generated");
    fs::write(&registry_path, registry.replace('\n', "\r\n"))
        .expect("registry should use CRLF line endings");

    let generated = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "construct",
            "trace",
            "--manifest-path",
            manifest.to_str().expect("temporary path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );

    let root = fs::read_to_string(&root_path).expect("root definition should be readable");
    assert!(root.contains("import \"constructs/trace.langmeld\";"));
    assert!(
        root.lines()
            .any(|line| line == "\timport \"constructs/trace.langmeld\";")
    );
    assert!(!root.contains("\t\timport \"constructs/trace.langmeld\";"));
    assert!(root.contains("\r\n\timport \"constructs/trace.langmeld\";\r\n\t}\r\n"));
    let syntax = fs::read_to_string(project.join("language/constructs/trace.langmeld"))
        .expect("construct syntax should be generated");
    assert!(syntax.contains("literal \"trace\";"));
    assert!(syntax.contains("implements \"statement\";"));
    let handler = fs::read_to_string(project.join("src/constructs/trace.rs"))
        .expect("construct handler should be generated");
    assert!(handler.contains("const CONSTRUCT: &'static str = \"trace\";"));
    let registry = fs::read_to_string(&registry_path).expect("handler registry should be readable");
    assert!(registry.contains("mod trace;"));
    assert!(registry.contains("handlers.register(trace::TraceHandler)?;"));
    assert!(registry.contains("\r\nmod trace;\r\n"));
    assert!(
        !registry.replace("\r\n", "").contains('\n'),
        "managed updates should not introduce LF-only lines into a CRLF registry"
    );

    let checked = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "check",
            "--manifest-path",
            manifest.to_str().expect("temporary path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );

    let repeated = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "construct",
            "trace",
            "--manifest-path",
            manifest.to_str().expect("temporary path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");
    assert!(!repeated.status.success());
    assert_eq!(
        fs::read_to_string(project.join("language/main.langmeld")).unwrap(),
        root
    );

    let keyword = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "construct",
            "loop",
            "--manifest-path",
            manifest.to_str().expect("temporary path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");
    assert!(
        keyword.status.success(),
        "{}",
        String::from_utf8_lossy(&keyword.stderr)
    );
    assert!(project.join("language/constructs/loop.langmeld").is_file());
    assert!(project.join("src/constructs/loop_handler.rs").is_file());
    let registry = fs::read_to_string(project.join("src/constructs/mod.rs"))
        .expect("keyword handler should be registered");
    assert!(registry.contains("mod loop_handler;"));
    assert!(registry.contains("handlers.register(loop_handler::LoopHandler)?;"));

    let generated_function = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "construct",
            "function-declaration",
            "--literal",
            "fn",
            "--manifest-path",
            manifest.to_str().expect("temporary path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");
    assert!(
        generated_function.status.success(),
        "{}",
        String::from_utf8_lossy(&generated_function.stderr)
    );
    let function_syntax =
        fs::read_to_string(project.join("language/constructs/function-declaration.langmeld"))
            .expect("semantic construct name should determine the definition filename");
    assert!(function_syntax.contains("construct \"function-declaration\""));
    assert!(function_syntax.contains("literal \"fn\";"));
    assert!(
        project
            .join("src/constructs/function_declaration.rs")
            .is_file()
    );

    let checked_function = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "check",
            "--manifest-path",
            manifest.to_str().expect("temporary path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");
    assert!(
        checked_function.status.success(),
        "{}",
        String::from_utf8_lossy(&checked_function.stderr)
    );

    let empty_literal = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "construct",
            "empty-literal",
            "--literal",
            "",
            "--manifest-path",
            manifest.to_str().expect("temporary path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");
    assert!(!empty_literal.status.success());
    assert!(
        !project
            .join("language/constructs/empty-literal.langmeld")
            .exists()
    );
}

#[test]
fn generate_construct_places_import_on_its_own_line_for_compact_root() {
    let parent = TestDirectory::new();
    let project = parent.path().join("compact-language");
    let init = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "init",
            project.to_str().expect("temporary path is UTF-8"),
            "--non-interactive",
        ])
        .output()
        .expect("CLI should start");
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );

    let root_path = project.join("language/main.langmeld");
    fs::write(
        &root_path,
        r#"language "Widget" { import "constructs/statements.langmeld"; root "program"; construct "program" { syntax { literal "program"; } block "{" "}" { allow_category "statement"; } } }"#,
    )
    .expect("compact root definition should be written");

    let manifest = project.join("langmeld.toml");
    let generated = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "construct",
            "trace",
            "--manifest-path",
            manifest.to_str().expect("temporary path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );

    let root = fs::read_to_string(&root_path).expect("updated root should be readable");
    assert_eq!(
        root.lines().nth(1),
        Some("    import \"constructs/trace.langmeld\";")
    );
    assert_eq!(root.lines().last(), Some("}"));

    let checked = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "check",
            "--manifest-path",
            manifest.to_str().expect("temporary path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
}

#[test]
fn generate_construct_escapes_quoted_syntax_literals() {
    let parent = TestDirectory::new();
    let project = parent.path().join("quoted-language");
    let init = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "init",
            project.to_str().expect("temporary path is UTF-8"),
            "--non-interactive",
        ])
        .output()
        .expect("CLI should start");
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );

    let manifest = project.join("langmeld.toml");
    let generated = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "construct",
            "quoted",
            "--literal",
            r#""hello \"world\"""#,
            "--manifest-path",
            manifest.to_str().expect("temporary path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );

    let syntax = fs::read_to_string(project.join("language/constructs/quoted.langmeld"))
        .expect("generated syntax should be readable");
    assert!(syntax.contains(r#"literal "\"hello \\\"world\\\"\"";"#));

    let checked = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "check",
            "--manifest-path",
            manifest.to_str().expect("temporary path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
}

#[test]
fn generate_construct_rolls_back_files_when_registry_update_fails() {
    let parent = TestDirectory::new();
    let project = parent.path().join("widget-language");
    let init = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "init",
            project.to_str().expect("temporary path is UTF-8"),
            "--non-interactive",
        ])
        .output()
        .expect("CLI should start");
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );

    let root_path = project.join("language/main.langmeld");
    let registry_path = project.join("src/constructs/mod.rs");
    let original_root = fs::read_to_string(&root_path).expect("root definition should be readable");
    let original_registry =
        fs::read_to_string(&registry_path).expect("handler registry should be readable");
    let original_permissions = fs::metadata(&registry_path)
        .expect("handler registry metadata should be readable")
        .permissions();
    let mut readonly_permissions = original_permissions.clone();
    readonly_permissions.set_readonly(true);
    fs::set_permissions(&registry_path, readonly_permissions)
        .expect("handler registry should become read-only");

    let generated = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "construct",
            "trace",
            "--manifest-path",
            project
                .join("langmeld.toml")
                .to_str()
                .expect("path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");

    fs::set_permissions(&registry_path, original_permissions)
        .expect("handler registry permissions should be restored");

    assert!(!generated.status.success());
    assert!(String::from_utf8_lossy(&generated.stderr).contains("read-only"));
    assert_eq!(
        fs::read_to_string(&root_path).expect("root definition should remain readable"),
        original_root
    );
    assert_eq!(
        fs::read_to_string(&registry_path).expect("handler registry should remain readable"),
        original_registry
    );
    assert!(!project.join("language/constructs/trace.langmeld").exists());
    assert!(!project.join("src/constructs/trace.rs").exists());
}

#[test]
fn generate_construct_explains_handler_module_collisions_without_partial_files() {
    let parent = TestDirectory::new();
    let project = parent.path().join("colliding-language");
    let init = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "init",
            project.to_str().expect("temporary path is UTF-8"),
            "--non-interactive",
        ])
        .output()
        .expect("CLI should start");
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );

    let statements = project.join("language/constructs/statements.langmeld");
    let mut source = fs::read_to_string(&statements).expect("constructs should be generated");
    source.push_str(
        "\nconstruct \"foo-bar\" { syntax { literal \"foo\"; literal \";\"; } implements \"statement\"; }\n",
    );
    fs::write(&statements, source).expect("the existing construct should be added");

    let manifest = project.join("langmeld.toml");
    let existing_handler = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "handler",
            "foo-bar",
            "--manifest-path",
            manifest.to_str().expect("manifest path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");
    assert!(
        existing_handler.status.success(),
        "{}",
        String::from_utf8_lossy(&existing_handler.stderr)
    );

    let root_path = project.join("language/main.langmeld");
    let original_root = fs::read_to_string(&root_path).expect("root definition should be readable");
    let registry_path = project.join("src/constructs/mod.rs");
    let original_registry =
        fs::read_to_string(&registry_path).expect("handler registry should be readable");
    let handler_path = project.join("src/constructs/foo_bar.rs");
    let original_handler =
        fs::read_to_string(&handler_path).expect("first handler should be generated");

    let generated = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "construct",
            "foo_bar",
            "--literal",
            "foo_bar",
            "--category",
            "statement",
            "--manifest-path",
            manifest.to_str().expect("manifest path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");

    assert!(!generated.status.success());
    let collision_message = String::from_utf8_lossy(&generated.stderr);
    assert!(
        collision_message.contains("foo_bar` and `foo-bar`"),
        "{collision_message}"
    );
    assert!(
        collision_message.contains("both map to Rust module `foo_bar`"),
        "{collision_message}"
    );
    assert_eq!(fs::read_to_string(&root_path).unwrap(), original_root);
    assert_eq!(
        fs::read_to_string(&registry_path).unwrap(),
        original_registry
    );
    assert_eq!(fs::read_to_string(&handler_path).unwrap(), original_handler);
    assert!(
        !project
            .join("language/constructs/foo_bar.langmeld")
            .exists()
    );
}

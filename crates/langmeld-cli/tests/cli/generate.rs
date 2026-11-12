use super::*;
use serde_json::{Value, json};
use std::process::Command;

#[test]
fn generate_handler_adds_a_stub_and_managed_registration_without_overwriting() {
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

    let statements = project.join("language/constructs/statements.langmeld");
    let mut source = fs::read_to_string(&statements).expect("constructs should be generated");
    source.push_str(
        "\nconstruct \"trace\" { syntax { literal \"trace\"; literal \";\"; } implements \"statement\"; }\n\nconstruct \"move\" { syntax { literal \"move\"; literal \";\"; } implements \"statement\"; }\n\nconstruct \"bad/name\" { syntax { literal \"bad\"; literal \";\"; } implements \"statement\"; }\n\nconstruct \"foo-bar\" { syntax { literal \"foo\"; literal \";\"; } implements \"statement\"; }\n\nconstruct \"foo_bar\" { syntax { literal \"bar\"; literal \";\"; } implements \"statement\"; }\n",
    );
    fs::write(&statements, source).expect("new constructs should be written");

    let manifest = project.join("langmeld.toml");
    let registry_path = project.join("src/constructs/mod.rs");
    let mut registry = fs::read_to_string(&registry_path).expect("registry should be generated");
    registry.push_str("\n// User-owned source outside managed sections.\n");
    fs::write(&registry_path, registry).expect("user-owned source should be preserved");

    let existing = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "handler",
            "let",
            "--manifest-path",
            manifest.to_str().expect("temporary path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");
    assert!(!existing.status.success());

    let generated = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "handler",
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

    let handler = project.join("src/constructs/trace.rs");
    let handler_source = fs::read_to_string(&handler).expect("handler should be generated");
    assert!(handler_source.contains("const CONSTRUCT: &'static str = \"trace\";"));
    assert!(handler_source.contains("context.construct().span"));
    assert!(!handler_source.contains("ParsedConstruct"));
    assert!(handler_source.contains("implement this construct's language behavior"));
    let registry_path = project.join("src/constructs/mod.rs");
    let registry = fs::read_to_string(&registry_path).expect("registry should be updated");
    assert!(registry.contains("mod trace;"));
    assert!(registry.contains("handlers.register(trace::TraceHandler)?;"));
    assert!(registry.contains("    // <langmeld:end registrations>"));
    assert!(registry.contains("// User-owned source outside managed sections."));

    let original_handler = handler_source;
    let original_registry = registry;
    let repeated = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "handler",
            "trace",
            "--manifest-path",
            manifest.to_str().expect("temporary path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");
    assert!(!repeated.status.success());
    assert_eq!(fs::read_to_string(handler).unwrap(), original_handler);
    assert_eq!(
        fs::read_to_string(registry_path).unwrap(),
        original_registry
    );

    let keyword = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "handler",
            "move",
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
    assert!(project.join("src/constructs/move_handler.rs").is_file());
    let registry = fs::read_to_string(project.join("src/constructs/mod.rs"))
        .expect("keyword handler should be registered");
    assert!(registry.contains("mod move_handler;"));
    assert!(registry.contains("handlers.register(move_handler::MoveHandler)?;"));

    let invalid_name = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "handler",
            "bad/name",
            "--manifest-path",
            manifest.to_str().expect("temporary path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");
    assert!(!invalid_name.status.success());
    assert!(
        String::from_utf8_lossy(&invalid_name.stderr).contains("cannot map to a Rust module name")
    );

    let first_colliding_name = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "handler",
            "foo-bar",
            "--manifest-path",
            manifest.to_str().expect("temporary path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");
    assert!(
        first_colliding_name.status.success(),
        "{}",
        String::from_utf8_lossy(&first_colliding_name.stderr)
    );

    let second_colliding_name = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "handler",
            "foo_bar",
            "--manifest-path",
            manifest.to_str().expect("temporary path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");
    assert!(!second_colliding_name.status.success());
    assert!(String::from_utf8_lossy(&second_colliding_name.stderr).contains("already exists"));
}

#[test]
fn generate_handler_escapes_additional_reserved_module_keywords() {
    let parent = TestDirectory::new();
    let project = parent.path().join("keyword-language");
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
    for construct in ["become", "box", "union"] {
        source.push_str(&format!(
            "\nconstruct \"{construct}\" {{ syntax {{ literal \"{construct}\"; literal \";\"; }} implements \"statement\"; }}\n"
        ));
    }
    fs::write(&statements, source).expect("keyword constructs should be written");

    let manifest = project.join("langmeld.toml");
    let registry_path = project.join("src/constructs/mod.rs");
    for (construct, handler) in [
        ("become", "BecomeHandler"),
        ("box", "BoxHandler"),
        ("union", "UnionHandler"),
    ] {
        let generated = Command::new(env!("CARGO_BIN_EXE_langmeld"))
            .args([
                "generate",
                "handler",
                construct,
                "--manifest-path",
                manifest.to_str().expect("path is UTF-8"),
            ])
            .output()
            .expect("CLI should start");
        assert!(
            generated.status.success(),
            "{}",
            String::from_utf8_lossy(&generated.stderr)
        );

        assert!(
            project
                .join(format!("src/constructs/{construct}_handler.rs"))
                .is_file()
        );

        let registry = fs::read_to_string(&registry_path).expect("registry should be readable");
        assert!(registry.contains(&format!("mod {construct}_handler;")));
        assert!(registry.contains(&format!(
            "handlers.register({construct}_handler::{handler})?;"
        )));
    }
}

#[test]
fn generate_handler_removes_new_file_when_registry_update_fails() {
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

    let statements = project.join("language/constructs/statements.langmeld");
    let mut source = fs::read_to_string(&statements).expect("constructs should be generated");
    source.push_str(
        "\nconstruct \"trace\" { syntax { literal \"trace\"; literal \";\"; } implements \"statement\"; }\n",
    );
    fs::write(&statements, source).expect("new construct should be written");

    let registry_path = project.join("src/constructs/mod.rs");
    let original_registry =
        fs::read_to_string(&registry_path).expect("registry should be generated");
    let original_permissions = fs::metadata(&registry_path)
        .expect("registry metadata should be readable")
        .permissions();
    let mut readonly_permissions = original_permissions.clone();
    readonly_permissions.set_readonly(true);
    fs::set_permissions(&registry_path, readonly_permissions)
        .expect("registry should become read-only");

    let generated = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "handler",
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
        .expect("registry permissions should be restored");

    assert!(!generated.status.success());
    assert!(String::from_utf8_lossy(&generated.stderr).contains("read-only"));
    assert_eq!(
        fs::read_to_string(&registry_path).expect("registry should remain readable"),
        original_registry
    );
    assert!(!project.join("src/constructs/trace.rs").exists());
}

#[cfg(unix)]
#[test]
fn generate_handler_rejects_dangling_source_symlinks_cleanly() {
    use std::os::unix::fs::symlink;

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

    let statements = project.join("language/constructs/statements.langmeld");
    let mut source = fs::read_to_string(&statements).expect("constructs should be generated");
    source.push_str(
        "\nconstruct \"trace\" { syntax { literal \"trace\"; literal \";\"; } implements \"statement\"; }\n",
    );
    fs::write(&statements, source).expect("new construct should be written");

    let handler_path = project.join("src/constructs/trace.rs");
    let missing_target = parent.path().join("missing-handler.rs");
    symlink(&missing_target, &handler_path).expect("dangling handler symlink should be created");

    let registry_path = project.join("src/constructs/mod.rs");
    let original_registry =
        fs::read_to_string(&registry_path).expect("handler registry should be generated");
    let generated = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "handler",
            "trace",
            "--manifest-path",
            project
                .join("langmeld.toml")
                .to_str()
                .expect("path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");

    assert!(!generated.status.success());
    assert!(String::from_utf8_lossy(&generated.stderr).contains("handler source"));
    assert!(String::from_utf8_lossy(&generated.stderr).contains("already exists"));
    assert!(
        fs::symlink_metadata(&handler_path)
            .expect("handler symlink should remain")
            .file_type()
            .is_symlink()
    );
    assert!(!missing_target.exists());
    assert_eq!(
        fs::read_to_string(registry_path).expect("registry should remain readable"),
        original_registry
    );
}

#[test]
fn generate_typed_handler_uses_the_generated_construct_identity() {
    let parent = TestDirectory::new();
    let project = parent.path().join("typed-language");
    let init = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "init",
            project.to_str().expect("temporary path is UTF-8"),
            "--non-interactive",
            "--typed-bindings",
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
        "\nconstruct \"trace\" { syntax { literal \"trace\"; literal \";\"; } implements \"statement\"; }\n",
    );
    fs::write(&statements, source).expect("new construct should be written");

    let manifest = project.join("langmeld.toml");
    let generated = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "handler",
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

    let handler = fs::read_to_string(project.join("src/constructs/trace.rs"))
        .expect("typed handler should be generated");
    assert!(
        handler
            .contains("const CONSTRUCT: &'static str = crate::generated::constructs::Trace::NAME;")
    );
    assert!(!handler.contains("const CONSTRUCT: &'static str = \"trace\";"));
}

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
fn generate_textmate_uses_imported_definitions_and_writes_json_to_stdout() {
    let parent = TestDirectory::new();
    let project = parent.path();
    write_widget_project(project);

    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "textmate",
            "--manifest-path",
            project.join("langmeld.toml").to_str().unwrap(),
            "--scope-name",
            "source.widget",
            "--file-type",
            "widget",
        ])
        .output()
        .expect("CLI should start");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let grammar: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(grammar["name"], "Widget");
    assert_eq!(grammar["scopeName"], "source.widget");
    assert_eq!(grammar["fileTypes"], json!(["widget"]));
    assert!(
        grammar["patterns"]
            .as_array()
            .unwrap()
            .iter()
            .any(|pattern| {
                pattern["name"] == "keyword.other.widget"
                    && pattern["match"].as_str().unwrap().contains("print")
            })
    );
}

#[test]
fn generate_tree_sitter_writes_a_grammar_js_source_file_to_stdout() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/tree-sitter-language/langmeld.toml");

    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "tree-sitter",
            "--manifest-path",
            manifest.to_str().unwrap(),
            "--grammar-name",
            "widget_script",
        ])
        .output()
        .expect("CLI should start");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let grammar = String::from_utf8(output.stdout).expect("grammar source should be UTF-8");
    assert!(grammar.contains("module.exports = grammar({"));
    assert!(grammar.contains("name: \"widget_script\""));
    assert!(grammar.contains("source_file: $ => repeat(choice($.construct_"));
    assert!(grammar.contains("identifier: _ =>"));
    assert!(grammar.contains("optional(seq(\"to\", field(\"target\", $.identifier)))"));
    assert!(grammar.contains("repeat(seq(\"with\", field(\"label\", $.string)))"));
    assert!(grammar.contains("choice(seq(\"say\","));
    assert!(grammar.contains("field(\"value\", $.token)"));
    assert!(
        grammar.contains("token: $ => choice($.identifier, $.integer, $.string, $.punctuation)")
    );
}

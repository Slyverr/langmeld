use crate::TestDirectory;
use std::{fs, process::Command};

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
        "\nconstruct \"trace\" { syntax { literal \"trace\"; literal \";\"; } implements \"statement\"; }\n\nconstruct \"move\" { syntax { literal \"move\"; literal \";\"; } implements \"statement\"; }\n\nconstruct \"bad/name\" { syntax { literal \"bad\"; literal \";\"; } implements \"statement\"; }\n\nconstruct \"foo-bar\" { syntax { literal \"foo\"; literal \";\"; } implements \"statement\"; }\n\nconstruct \"foo_bar\" { syntax { literal \"bar\"; literal \";\"; } implements \"statement\"; }\n\nconstruct \"fooBar\" { syntax { literal \"camel\"; literal \";\"; } implements \"statement\"; }\n",
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

    let same_handler_type = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "handler",
            "fooBar",
            "--manifest-path",
            manifest.to_str().expect("temporary path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");
    assert!(
        same_handler_type.status.success(),
        "{}",
        String::from_utf8_lossy(&same_handler_type.stderr)
    );
    assert!(project.join("src/constructs/foobar.rs").is_file());
    let registry = fs::read_to_string(project.join("src/constructs/mod.rs"))
        .expect("registry should contain both module-qualified handlers");
    assert!(registry.contains("handlers.register(foobar::FooBarHandler)?;"));

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
    let collision_message = String::from_utf8_lossy(&second_colliding_name.stderr);
    assert!(collision_message.contains("foo_bar` and `foo-bar`"));
    assert!(collision_message.contains("both map to Rust module `foo_bar`"));
    assert!(collision_message.contains("rename one construct"));
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

use std::process::Command;

fn help_for(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args(args)
        .output()
        .expect("CLI should start");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn init_help_lists_project_options() {
    let help = help_for(&["init", "--help"]);

    assert!(help.contains("Create a starter language project"));
    assert!(help.contains("--non-interactive"));
    assert!(help.contains("--typed-bindings"));
    assert!(help.contains("--local-crates"));
    assert!(help.contains("--agents-md"));
    assert!(help.contains("--agent-skill"));
}

#[test]
fn lsp_help_lists_project_and_definition_modes() {
    let help = help_for(&["lsp", "--help"]);

    assert!(
        help.contains("Serve editor support for a language project or `.langmeld` definitions")
    );
    assert!(help.contains("--manifest-path"));
    assert!(help.contains("--definitions"));
}

#[test]
fn format_help_lists_check_mode() {
    let help = help_for(&["format", "--help"]);

    assert!(help.contains("Format a language project's `.langmeld` definitions"));
    assert!(help.contains("--check"));
}

#[test]
fn textmate_generation_help_lists_scope_name() {
    let help = help_for(&["generate", "textmate", "--help"]);

    assert!(help.contains("Generate a lexical TextMate grammar"));
    assert!(help.contains("--scope-name"));
}

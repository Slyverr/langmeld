use std::{io, path::Path};

use langmeld_language::quote_definition_string;

use super::{AgentGuidance, Template, local_crates};

pub(super) fn project_files(
    package_name: &str,
    language_name: &str,
    template: Template,
    typed_bindings: bool,
    agent_guidance: AgentGuidance,
    local_crate_root: Option<&Path>,
) -> io::Result<Vec<(String, String)>> {
    let version = env!("CARGO_PKG_VERSION");
    let dependencies = match template {
        Template::Declarative => format!(
            "clap = {{ version = \"4.6\", features = [\"derive\"] }}\nlangmeld-core = \"{version}\"\nlangmeld-language = \"{version}\"\nlangmeld-project = \"{version}\"\n"
        ),
        Template::CustomParser => {
            if typed_bindings {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "typed bindings require the declarative `.langmeld` template",
                ));
            }
            format!(
                "clap = {{ version = \"4.6\", features = [\"derive\"] }}\nlangmeld-core = \"{version}\"\nlangmeld-language = \"{version}\"\nlangmeld-lsp = \"{version}\"\n"
            )
        }
    };
    let build_dependencies = if typed_bindings {
        format!("\n[build-dependencies]\nlangmeld-build = \"{version}\"\n")
    } else {
        String::new()
    };
    let library = "\n[lib]\nname = \"language\"\n";
    let manifest = format!(
        "[package]\nname = {package_name:?}\ndefault-run = {package_name:?}\nversion = \"0.1.0\"\nedition = \"2024\"\nrust-version = \"1.88\"\nlicense = \"MIT OR Apache-2.0\"\n{library}\n[dependencies]\n{dependencies}{build_dependencies}\n[workspace]\n"
    );
    let gitignore = if local_crate_root.is_some() {
        "/target/\n/.cargo/config.toml\n"
    } else {
        "/target/\n"
    };
    let mut files = vec![
        ("Cargo.toml".to_owned(), manifest),
        (".gitignore".to_owned(), gitignore.to_owned()),
        (
            "LICENSE-MIT".to_owned(),
            include_str!("../../templates/LICENSE-MIT").to_owned(),
        ),
        (
            "LICENSE-APACHE".to_owned(),
            include_str!("../../templates/LICENSE-APACHE").to_owned(),
        ),
    ];

    if let Some(root) = local_crate_root {
        files.push((
            ".cargo/config.toml".to_owned(),
            local_crates::cargo_config(root, template, typed_bindings)?,
        ));
    }

    match template {
        Template::Declarative => {
            let typed_guidance = if typed_bindings {
                "\nTyped capture accessors are generated during the Cargo build. In handlers, use `context.typed::<crate::generated::constructs::Let>()?` or the matching construct view instead of string-based field lookup. Capture and group names must convert unambiguously to Rust identifiers.\n"
            } else {
                ""
            };
            let mut library = include_str!("../../templates/declarative/src/lib.rs").to_owned();
            if typed_bindings {
                library.push_str(
                    "\npub mod generated {\n    include!(concat!(env!(\"OUT_DIR\"), \"/langmeld_bindings.rs\"));\n}\n",
                );
            }

            files.extend([
                (
                    "README.md".to_owned(),
                    include_str!("../../templates/declarative/README.md")
                        .replace("{{LANGUAGE_NAME}}", language_name)
                        .replace("{{TYPED_BINDINGS_GUIDANCE}}", typed_guidance),
                ),
                (
                    "langmeld.toml".to_owned(),
                    if typed_bindings {
                        "format = 1\nrust_bindings = \"typed\"\n".to_owned()
                    } else {
                        "format = 1\n".to_owned()
                    },
                ),
                (
                    "src/lib.rs".to_owned(),
                    library,
                ),
                (
                    "src/main.rs".to_owned(),
                    include_str!("../../templates/declarative/src/main.rs").to_owned(),
                ),
                (
                    "src/runtime.rs".to_owned(),
                    include_str!("../../templates/declarative/src/runtime.rs").to_owned(),
                ),
                (
                    "src/constructs/mod.rs".to_owned(),
                    include_str!("../../templates/declarative/src/constructs/mod.rs").to_owned(),
                ),
                (
                    "src/constructs/let_handler.rs".to_owned(),
                    include_str!("../../templates/declarative/src/constructs/let_handler.rs")
                        .replace(
                            "{{CAPTURE_ACCESS}}",
                            if typed_bindings {
                                "let syntax = context.typed::<crate::generated::constructs::Let>()?;\n        let name = syntax.name()?.text.to_owned();\n        let value = syntax.value()?;"
                            } else {
                                "let name = context.field_identifier(\"name\")?.to_owned();\n        let value = context.field_text_capture(\"value\")?;"
                            },
                        ),
                ),
                (
                    "src/constructs/print.rs".to_owned(),
                    include_str!("../../templates/declarative/src/constructs/print.rs")
                        .replace(
                            "{{CAPTURE_ACCESS}}",
                            if typed_bindings {
                                "let syntax = context.typed::<crate::generated::constructs::Print>()?;\n        let name = syntax.name()?;"
                            } else {
                                "let name = context.field_identifier_capture(\"name\")?;"
                            },
                        ),
                ),
                (
                    "tests/language.rs".to_owned(),
                    include_str!("../../templates/declarative/tests/language.rs")
                        .replace("{{LANGUAGE_NAME}}", &format!("{language_name:?}")),
                ),
                (
                    "language/main.langmeld".to_owned(),
                    include_str!("../../templates/declarative/language/main.langmeld")
                        .replace("{{LANGUAGE_NAME}}", &quote_definition_string(language_name)),
                ),
                (
                    "language/constructs/statements.langmeld".to_owned(),
                    include_str!(
                        "../../templates/declarative/language/constructs/statements.langmeld"
                    )
                    .to_owned(),
                ),
            ]);

            if typed_bindings {
                files.push((
                    "build.rs".to_owned(),
                    "fn main() {\n    if let Err(error) = langmeld_build::generate_project() {\n        panic!(\"{error}\");\n    }\n}\n".to_owned(),
                ));
            }
        }
        Template::CustomParser => {
            files.extend([
                (
                    "README.md".to_owned(),
                    include_str!("../../templates/custom-parser/README.md")
                        .replace("{{LANGUAGE_NAME}}", language_name),
                ),
                (
                    "src/lib.rs".to_owned(),
                    include_str!("../../templates/custom-parser/src/lib.rs").to_owned(),
                ),
                (
                    "src/main.rs".to_owned(),
                    include_str!("../../templates/custom-parser/src/main.rs")
                        .replace("{{LANGUAGE_NAME}}", &format!("{language_name:?}")),
                ),
                (
                    "src/bin/language-lsp.rs".to_owned(),
                    include_str!("../../templates/custom-parser/src/bin/language-lsp.rs")
                        .to_owned(),
                ),
                (
                    "tests/parser.rs".to_owned(),
                    include_str!("../../templates/custom-parser/tests/parser.rs").to_owned(),
                ),
            ]);
        }
    }

    if agent_guidance.agents_md {
        files.push((
            "AGENTS.md".to_owned(),
            include_str!("../../templates/AGENTS.md").to_owned(),
        ));
    }

    if agent_guidance.skill {
        files.push((
            ".agents/skills/langmeld-language/SKILL.md".to_owned(),
            include_str!("../../templates/langmeld-language/SKILL.md").to_owned(),
        ));
    }

    Ok(files)
}

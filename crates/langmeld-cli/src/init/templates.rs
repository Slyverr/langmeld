use langmeld_language::quote_definition_string;

use super::{AgentGuidance, Template};

pub(super) fn project_files(
    package_name: &str,
    language_name: &str,
    template: Template,
    agent_guidance: AgentGuidance,
) -> Vec<(String, String)> {
    let version = env!("CARGO_PKG_VERSION");
    let dependencies = match template {
        Template::Declarative => format!(
            "clap = {{ version = \"4.6\", features = [\"derive\"] }}\nlangmeld-core = \"{version}\"\nlangmeld-language = \"{version}\"\nlangmeld-project = \"{version}\"\n"
        ),
        Template::CustomParser => {
            format!(
                "clap = {{ version = \"4.6\", features = [\"derive\"] }}\nlangmeld-core = \"{version}\"\nlangmeld-language = \"{version}\"\nlangmeld-lsp = \"{version}\"\n"
            )
        }
    };
    let library = "\n[lib]\nname = \"language\"\n";
    let manifest = format!(
        "[package]\nname = {package_name:?}\ndefault-run = {package_name:?}\nversion = \"0.1.0\"\nedition = \"2024\"\nrust-version = \"1.88\"\nlicense = \"MIT OR Apache-2.0\"\n{library}\n[dependencies]\n{dependencies}\n[workspace]\n"
    );
    let mut files = vec![
        ("Cargo.toml".to_owned(), manifest),
        (".gitignore".to_owned(), "/target/\n".to_owned()),
        (
            "LICENSE-MIT".to_owned(),
            include_str!("../../templates/LICENSE-MIT").to_owned(),
        ),
        (
            "LICENSE-APACHE".to_owned(),
            include_str!("../../templates/LICENSE-APACHE").to_owned(),
        ),
    ];

    match template {
        Template::Declarative => {
            files.extend([
                (
                    "README.md".to_owned(),
                    include_str!("../../templates/declarative/README.md")
                        .replace("{{LANGUAGE_NAME}}", language_name),
                ),
                ("langmeld.toml".to_owned(), "format = 1\n".to_owned()),
                (
                    "src/lib.rs".to_owned(),
                    include_str!("../../templates/declarative/src/lib.rs").to_owned(),
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
                        .to_owned(),
                ),
                (
                    "src/constructs/print.rs".to_owned(),
                    include_str!("../../templates/declarative/src/constructs/print.rs").to_owned(),
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

    files
}

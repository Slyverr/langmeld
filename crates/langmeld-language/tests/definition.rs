use langmeld_core::{Diagnostic, Severity, SourceFile, Span};
use langmeld_language::{
    CapturedValue, CompletionKind, DefinitionResolver, DefinitionResult, DefinitionSource,
    complete_at, decode_definition_string, parse, parse_language_definition,
    parse_language_definition_project, quote_definition_string,
};
use std::collections::HashMap;

#[derive(Default)]
struct Resolver(HashMap<String, DefinitionSource>);

impl DefinitionResolver for Resolver {
    type Error = String;

    fn resolve(&self, _from_key: &str, import: &str) -> Result<DefinitionSource, Self::Error> {
        self.0
            .get(import)
            .cloned()
            .ok_or_else(|| format!("unknown test import `{import}`"))
    }
}

const DEFINITION: &str = r#"
language "Toy" {
    construct "program" {
        syntax {
            literal "program";
        }
        block "{" "}" {
            allow "let";
            allow "print";
        }
    }
    construct "let" {
        syntax {
            literal "let";
            identifier "name";
            literal "=";
            integer "value";
            literal ";";
        }
    }
    construct "print" {
        syntax {
            literal "print";
            identifier "name";
            literal ";";
        }
    }
}
"#;

#[path = "definition/choices.rs"]
mod choices;
#[path = "definition/imports.rs"]
mod imports;
#[path = "definition/lexing.rs"]
mod lexing;
#[path = "definition/roots.rs"]
mod roots;
#[path = "definition/strings.rs"]
mod strings;
#[path = "definition/syntax.rs"]
mod syntax;

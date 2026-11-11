//! Parsing one `.langmeld` source and lowering it into grammar data.

use langmeld_core::{Diagnostic, SourceFile, Span};

use crate::{ConstructSpec, LexicalRules, parse};

use super::{
    decode_definition_string,
    grammar::definition_grammar,
    lower::{lower_construct, lower_import, lower_lexical_rules, string_field},
};

#[derive(Clone)]
pub(super) struct ImportDirective {
    pub(super) path: String,
    pub(super) span: Span,
}

pub(super) struct DefinitionUnit {
    pub(super) language_name: Option<String>,
    pub(super) constructs: Vec<(ConstructSpec, Span)>,
    pub(super) root_constructs: Vec<(String, Span)>,
    pub(super) imports: Vec<ImportDirective>,
    pub(super) lexical_rules: LexicalRules,
    pub(super) lexical_span: Option<Span>,
    pub(super) diagnostics: Vec<Diagnostic>,
}

pub(super) fn parse_definition_unit(source: &SourceFile, is_root: bool) -> DefinitionUnit {
    let parsed = parse(source, definition_grammar());
    let mut unit = DefinitionUnit {
        language_name: None,
        constructs: Vec::new(),
        root_constructs: Vec::new(),
        imports: Vec::new(),
        lexical_rules: LexicalRules::default(),
        lexical_span: None,
        diagnostics: parsed.diagnostics().to_vec(),
    };
    if is_root {
        let language_nodes: Vec<_> = parsed
            .constructs()
            .iter()
            .filter(|node| node.name == "language")
            .collect();
        if language_nodes.is_empty() {
            unit.diagnostics.push(Diagnostic::error(
                "LM-DEF-001",
                "expected one `language` declaration",
                source,
                Span::new(0, 0),
            ));
            return unit;
        }
        for node in language_nodes.iter().skip(1) {
            unit.diagnostics.push(Diagnostic::error(
                "LM-DEF-002",
                "a definition file must contain exactly one `language` declaration",
                source,
                node.span,
            ));
        }
        for node in parsed
            .constructs()
            .iter()
            .filter(|node| node.name != "language")
        {
            unit.diagnostics.push(Diagnostic::error(
                "LM-DEF-013",
                "constructs and imports must be inside the `language` declaration",
                source,
                node.span,
            ));
        }
        let root = language_nodes[0];
        let Some(name) = string_field(root, "name").and_then(decode_definition_string) else {
            unit.diagnostics.push(Diagnostic::error(
                "LM-DEF-003",
                "language declaration requires a valid quoted name",
                source,
                root.span,
            ));
            return unit;
        };
        unit.language_name = Some(name);
        let lexical_nodes = root
            .children
            .iter()
            .filter(|child| child.name == "lexical")
            .collect::<Vec<_>>();
        if lexical_nodes.len() > 1 {
            unit.diagnostics.push(Diagnostic::error(
                "LM-DEF-024",
                "a language may declare only one `lexical` block",
                source,
                lexical_nodes[1].span,
            ));
        }
        if let Some(lexical) = lexical_nodes.first() {
            unit.lexical_span = Some(lexical.span);
            unit.lexical_rules = lower_lexical_rules(lexical, source, &mut unit.diagnostics);
        }
        for node in &root.children {
            match node.name.as_str() {
                "construct" => {
                    if let Some(construct) = lower_construct(node, source, &mut unit.diagnostics) {
                        unit.constructs.push((construct, node.span));
                    }
                }
                "root" => {
                    if let Some(name) =
                        string_field(node, "name").and_then(decode_definition_string)
                    {
                        unit.root_constructs.push((name, node.span));
                    } else {
                        unit.diagnostics.push(Diagnostic::error(
                            "LM-DEF-027",
                            "`root` requires a valid quoted construct name",
                            source,
                            node.span,
                        ));
                    }
                }
                "import" => {
                    if let Some(import) = lower_import(node, source, &mut unit.diagnostics) {
                        unit.imports.push(import);
                    }
                }
                _ => {}
            }
        }
    } else {
        if let Some(node) = parsed
            .constructs()
            .iter()
            .find(|node| node.name == "language")
        {
            unit.diagnostics.push(Diagnostic::error(
                "LM-DEF-014",
                "an imported definition file cannot declare a language root",
                source,
                node.span,
            ));
        }
        for node in parsed.constructs() {
            match node.name.as_str() {
                "construct" => {
                    if let Some(construct) = lower_construct(node, source, &mut unit.diagnostics) {
                        unit.constructs.push((construct, node.span));
                    }
                }
                "import" => {
                    if let Some(import) = lower_import(node, source, &mut unit.diagnostics) {
                        unit.imports.push(import);
                    }
                }
                "language" => {}
                _ => unit.diagnostics.push(Diagnostic::error(
                    "LM-DEF-015",
                    "an imported file may contain only `construct` and `import` declarations",
                    source,
                    node.span,
                )),
            }
        }
        if unit.constructs.is_empty() && unit.imports.is_empty() {
            unit.diagnostics.push(Diagnostic::error(
                "LM-DEF-016",
                "imported definition file contains no declarations",
                source,
                Span::new(0, 0),
            ));
        }
    }
    unit
}

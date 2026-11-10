//! Frontend for the first user-facing `.langmeld` language-definition syntax.

use std::sync::OnceLock;

use langmeld_core::{Diagnostic, SourceFile, Span};

use crate::{
    CapturedValue, ConstructSpec, FieldKind, LanguageSpec, LexicalRules, ParsedConstruct,
    PatternPart, SpecError, parse,
};

/// Result of parsing and validating one language-definition source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefinitionResult {
    /// Validated language when the complete definition has no errors.
    pub language: Option<LanguageSpec>,
    /// Parser and definition diagnostics tied to the source snapshot.
    pub diagnostics: Vec<Diagnostic>,
}

impl DefinitionResult {
    /// Returns whether a valid language was produced.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.language.is_some() && self.diagnostics.is_empty()
    }
}

/// A named `.langmeld` source supplied to a language-definition project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefinitionSource {
    /// Stable resolver key used to detect duplicate imports and cycles.
    pub key: String,
    /// Immutable source snapshot containing the definition text.
    pub source: SourceFile,
}

/// Maximum number of nested definition imports accepted in one project.
pub const MAX_DEFINITION_IMPORT_DEPTH: usize = 64;

impl DefinitionSource {
    /// Creates a source with a stable resolver key.
    #[must_use]
    pub fn new(key: impl Into<String>, source: SourceFile) -> Self {
        Self {
            key: key.into(),
            source,
        }
    }
}

/// Host-controlled loading of explicitly imported `.langmeld` files.
pub trait DefinitionResolver {
    /// Resolves `import` relative to `from_key` and returns a stable source key.
    ///
    /// Implementations own filesystem access, path normalization, and any
    /// project-root restrictions.
    fn resolve(&self, from_key: &str, import: &str) -> Result<DefinitionSource, String>;
}

#[derive(Clone)]
struct ImportDirective {
    path: String,
    span: Span,
}

struct DefinitionUnit {
    language_name: Option<String>,
    constructs: Vec<(ConstructSpec, Span)>,
    root_constructs: Vec<(String, Span)>,
    imports: Vec<ImportDirective>,
    lexical_rules: LexicalRules,
    lexical_span: Option<Span>,
    diagnostics: Vec<Diagnostic>,
}

/// Parses a single language-definition file and validates its declared grammar.
///
/// The bootstrap grammar is authored in Rust and parsed by Langmeld's generic
/// parser. This keeps the definition frontend on the same parser path that
/// user-defined languages use. The accepted subset currently describes
/// keyword-led token patterns and delimited blocks. Files containing imports
/// must use [`parse_language_definition_project`] with a host resolver. Root
/// declarations restrict top-level constructs; without them, every construct
/// remains available at the source root.
#[must_use]
pub fn parse_language_definition(source: &SourceFile) -> DefinitionResult {
    let mut unit = parse_definition_unit(source, true);
    if let Some(import) = unit.imports.first() {
        unit.diagnostics.push(Diagnostic::error(
            "LM-DEF-010",
            "this definition contains imports; use `parse_language_definition_project` with a resolver",
            source,
            import.span,
        ));
    }
    let constructs = unit
        .constructs
        .into_iter()
        .map(|(construct, span)| (construct, source.clone(), span))
        .collect();
    finish_definition(
        unit.language_name,
        constructs,
        unit.root_constructs,
        unit.lexical_rules,
        unit.lexical_span,
        unit.diagnostics,
        source.clone(),
    )
}

/// Returns Langmeld's built-in grammar for `.langmeld` language definitions.
///
/// The grammar accepts root declarations and constrains nested syntax through
/// explicit blocks. It is used by the definition parser and can also drive
/// editor features such as diagnostics and context-aware completion.
#[must_use]
pub fn language_definition_grammar() -> &'static LanguageSpec {
    definition_grammar()
}

/// Loads and composes a language root and its explicitly imported local modules.
///
/// The root file contains one `language` declaration. Imported files contain
/// top-level `construct` and `import` declarations. The resolver owns path
/// handling and must return a stable key for each logical source.
#[must_use]
pub fn parse_language_definition_project(
    root: DefinitionSource,
    resolver: &impl DefinitionResolver,
) -> DefinitionResult {
    let mut unit = parse_definition_unit(&root.source, true);
    let Some(language_name) = unit.language_name.take() else {
        return DefinitionResult {
            language: None,
            diagnostics: unit.diagnostics,
        };
    };
    let mut diagnostics = std::mem::take(&mut unit.diagnostics);
    if root.key.trim().is_empty() {
        diagnostics.push(Diagnostic::error(
            "LM-DEF-019",
            "root definition source key cannot be empty",
            &root.source,
            Span::new(0, 0),
        ));
    }
    let mut constructs = unit
        .constructs
        .into_iter()
        .map(|(construct, span)| (construct, root.source.clone(), span))
        .collect::<Vec<_>>();
    let root_constructs = unit.root_constructs;
    let mut loading = vec![root.key.clone()];
    let mut loaded = std::collections::HashSet::new();
    loaded.insert(root.key.clone());

    load_imports(
        &root,
        unit.imports,
        resolver,
        &mut loading,
        &mut loaded,
        &mut constructs,
        &mut diagnostics,
    );

    finish_definition(
        Some(language_name),
        constructs,
        root_constructs,
        unit.lexical_rules,
        unit.lexical_span,
        diagnostics,
        root.source,
    )
}

fn parse_definition_unit(source: &SourceFile, is_root: bool) -> DefinitionUnit {
    let parsed = parse(source, definition_grammar());
    let mut unit = DefinitionUnit {
        language_name: None,
        constructs: Vec::new(),
        root_constructs: Vec::new(),
        imports: Vec::new(),
        lexical_rules: LexicalRules::default(),
        lexical_span: None,
        diagnostics: parsed.diagnostics,
    };
    if is_root {
        let language_nodes: Vec<_> = parsed
            .constructs
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
            .constructs
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
        let Some(name) = string_field(root, "name").and_then(decode_quoted) else {
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
                    if let Some(name) = string_field(node, "name").and_then(decode_quoted) {
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
            .constructs
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
        for node in &parsed.constructs {
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

fn lower_lexical_rules(
    node: &ParsedConstruct,
    source: &SourceFile,
    diagnostics: &mut Vec<Diagnostic>,
) -> LexicalRules {
    let mut rules = LexicalRules::default();
    for declaration in &node.children {
        match declaration.name.as_str() {
            "line_comment" => {
                if let Some(marker) = string_field(declaration, "marker").and_then(decode_quoted) {
                    rules = rules.line_comment(marker);
                } else {
                    diagnostics.push(Diagnostic::error(
                        "LM-DEF-025",
                        "`line_comment` requires a valid quoted marker",
                        source,
                        declaration.span,
                    ));
                }
            }
            "block_comment" => {
                if let (Some(open), Some(close)) = (
                    string_field(declaration, "open").and_then(decode_quoted),
                    string_field(declaration, "close").and_then(decode_quoted),
                ) {
                    rules = rules.block_comment(open, close);
                } else {
                    diagnostics.push(Diagnostic::error(
                        "LM-DEF-026",
                        "`block_comment` requires quoted opening and closing markers",
                        source,
                        declaration.span,
                    ));
                }
            }
            _ => {}
        }
    }
    rules
}

fn lower_import(
    node: &ParsedConstruct,
    source: &SourceFile,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<ImportDirective> {
    let Some(path) = string_field(node, "path").and_then(decode_quoted) else {
        diagnostics.push(Diagnostic::error(
            "LM-DEF-017",
            "import requires a valid quoted path",
            source,
            node.span,
        ));
        return None;
    };
    if path.trim().is_empty() {
        diagnostics.push(Diagnostic::error(
            "LM-DEF-017",
            "import path cannot be empty",
            source,
            node.span,
        ));
        return None;
    }
    Some(ImportDirective {
        path,
        span: node.span,
    })
}

fn load_imports(
    importer: &DefinitionSource,
    imports: Vec<ImportDirective>,
    resolver: &impl DefinitionResolver,
    loading: &mut Vec<String>,
    loaded: &mut std::collections::HashSet<String>,
    constructs: &mut Vec<(ConstructSpec, SourceFile, Span)>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for import in imports {
        if loading.len() >= MAX_DEFINITION_IMPORT_DEPTH {
            diagnostics.push(Diagnostic::error(
                "LM-DEF-021",
                format!("maximum definition import depth ({MAX_DEFINITION_IMPORT_DEPTH}) exceeded"),
                &importer.source,
                import.span,
            ));
            continue;
        }
        let resolved = match resolver.resolve(&importer.key, &import.path) {
            Ok(resolved) => resolved,
            Err(error) => {
                diagnostics.push(Diagnostic::error(
                    "LM-DEF-018",
                    format!("cannot resolve import `{}`: {error}", import.path),
                    &importer.source,
                    import.span,
                ));
                continue;
            }
        };
        if resolved.key.trim().is_empty() {
            diagnostics.push(Diagnostic::error(
                "LM-DEF-019",
                format!("resolver returned an empty key for `{}`", import.path),
                &importer.source,
                import.span,
            ));
            continue;
        }
        if loading.contains(&resolved.key) {
            diagnostics.push(Diagnostic::error(
                "LM-DEF-020",
                format!("import cycle reaches `{}`", import.path),
                &importer.source,
                import.span,
            ));
            continue;
        }
        if !loaded.insert(resolved.key.clone()) {
            continue;
        }

        loading.push(resolved.key.clone());
        let mut unit = parse_definition_unit(&resolved.source, false);
        diagnostics.append(&mut unit.diagnostics);
        constructs.extend(
            unit.constructs
                .into_iter()
                .map(|(construct, span)| (construct, resolved.source.clone(), span)),
        );
        load_imports(
            &resolved,
            unit.imports,
            resolver,
            loading,
            loaded,
            constructs,
            diagnostics,
        );
        loading.pop();
    }
}

fn finish_definition(
    language_name: Option<String>,
    constructs: Vec<(ConstructSpec, SourceFile, Span)>,
    root_constructs: Vec<(String, Span)>,
    lexical_rules: LexicalRules,
    lexical_span: Option<Span>,
    mut diagnostics: Vec<Diagnostic>,
    fallback_source: SourceFile,
) -> DefinitionResult {
    let Some(language_name) = language_name else {
        return DefinitionResult {
            language: None,
            diagnostics,
        };
    };
    let specs = constructs
        .iter()
        .map(|(construct, _, _)| construct.clone())
        .collect::<Vec<_>>();
    let language = match LanguageSpec::new(language_name, specs)
        .and_then(|language| language.with_lexical_rules(lexical_rules))
        .and_then(|language| {
            if root_constructs.is_empty() {
                Ok(language)
            } else {
                let names = root_constructs
                    .iter()
                    .map(|(name, _)| name.as_str())
                    .collect::<Vec<_>>();
                language.with_root_constructs(&names)
            }
        }) {
        Ok(language) => Some(language),
        Err(error) => {
            let target = match &error {
                SpecError::DuplicateConstruct(name) => constructs
                    .iter()
                    .filter(|(construct, _, _)| construct.name() == name)
                    .nth(1),
                _ => spec_error_construct(&error).and_then(|name| {
                    constructs
                        .iter()
                        .find(|(construct, _, _)| construct.name() == name)
                }),
            };
            let lexical_error = matches!(
                error,
                SpecError::InvalidLineComment(_)
                    | SpecError::InvalidBlockComment { .. }
                    | SpecError::DuplicateCommentStart(_)
            );
            let root_target = match &error {
                SpecError::UnknownRootConstruct(name) => root_constructs
                    .iter()
                    .find(|(root, _)| root == name)
                    .map(|(_, span)| (&fallback_source, *span)),
                SpecError::DuplicateRootConstruct(name) => root_constructs
                    .iter()
                    .filter(|(root, _)| root == name)
                    .nth(1)
                    .map(|(_, span)| (&fallback_source, *span)),
                _ => None,
            };
            let (source, span) = root_target
                .or_else(|| target.map(|(_, source, span)| (source, *span)))
                .unwrap_or_else(|| {
                    (
                        &fallback_source,
                        if lexical_error {
                            lexical_span.unwrap_or_else(|| Span::new(0, 0))
                        } else {
                            Span::new(0, 0)
                        },
                    )
                });
            diagnostics.push(Diagnostic::error(
                "LM-DEF-004",
                error.to_string(),
                source,
                span,
            ));
            None
        }
    };
    let language = language.filter(|_| diagnostics.is_empty());
    DefinitionResult {
        language,
        diagnostics,
    }
}

fn lower_construct(
    node: &ParsedConstruct,
    source: &SourceFile,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<ConstructSpec> {
    let Some(name) = string_field(node, "name").and_then(decode_quoted) else {
        diagnostics.push(Diagnostic::error(
            "LM-DEF-003",
            "construct declaration requires a valid quoted name",
            source,
            node.span,
        ));
        return None;
    };

    let syntax_nodes: Vec<_> = node
        .children
        .iter()
        .filter(|child| child.name == "syntax")
        .collect();
    if syntax_nodes.is_empty() {
        diagnostics.push(Diagnostic::error(
            "LM-DEF-005",
            format!("construct `{name}` must have at least one `syntax` block"),
            source,
            node.span,
        ));
        return None;
    }

    let mut patterns = Vec::new();
    for syntax in syntax_nodes {
        let mut pattern = Vec::new();
        for part in &syntax.children {
            if let Some(pattern_part) = lower_pattern(part, source, diagnostics) {
                pattern.push(pattern_part);
            }
        }
        patterns.push(pattern);
    }

    let block_nodes: Vec<_> = node
        .children
        .iter()
        .filter(|child| child.name == "block")
        .collect();
    if block_nodes.len() > 1 {
        diagnostics.push(Diagnostic::error(
            "LM-DEF-007",
            format!("construct `{name}` may declare only one block"),
            source,
            block_nodes[1].span,
        ));
        return None;
    }

    let mut patterns = patterns.into_iter();
    let mut construct = ConstructSpec::new(name, patterns.next().unwrap_or_default());
    for pattern in patterns {
        construct = construct.variant(pattern);
    }
    let mut implemented_categories = Vec::new();
    for implementation in node
        .children
        .iter()
        .filter(|child| child.name == "implements")
    {
        if let Some(category) = string_field(implementation, "category").and_then(decode_quoted) {
            implemented_categories.push(category);
        } else {
            diagnostics.push(Diagnostic::error(
                "LM-DEF-022",
                "`implements` requires a valid quoted category name",
                source,
                implementation.span,
            ));
        }
    }
    construct = construct.implements(
        &implemented_categories
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    );
    if let Some(block) = block_nodes.first() {
        let (Some(open), Some(close)) = (
            string_field(block, "open").and_then(decode_quoted),
            string_field(block, "close").and_then(decode_quoted),
        ) else {
            diagnostics.push(Diagnostic::error(
                "LM-DEF-008",
                "block requires quoted opening and closing tokens",
                source,
                block.span,
            ));
            return None;
        };
        let mut children = Vec::new();
        let mut categories = Vec::new();
        for child in block.children.iter().filter(|child| child.name == "allow") {
            let name = string_field(child, "name").and_then(decode_quoted);
            if let Some(name) = name {
                children.push(name);
            } else {
                diagnostics.push(Diagnostic::error(
                    "LM-DEF-009",
                    "`allow` requires a valid quoted construct name",
                    source,
                    child.span,
                ));
            }
        }
        for child in block
            .children
            .iter()
            .filter(|child| child.name == "allow_category")
        {
            if let Some(category) = string_field(child, "category").and_then(decode_quoted) {
                categories.push(category);
            } else {
                diagnostics.push(Diagnostic::error(
                    "LM-DEF-023",
                    "`allow_category` requires a valid quoted category name",
                    source,
                    child.span,
                ));
            }
        }
        construct = construct.block_names_with_categories(open, close, children, categories);
    }
    Some(construct)
}

fn lower_pattern(
    node: &ParsedConstruct,
    source: &SourceFile,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<PatternPart> {
    match node.name.as_str() {
        "literal" | "token" | "identifier" | "integer" | "string" => {
            let Some(value) = string_field(node, "value").and_then(decode_quoted) else {
                diagnostics.push(Diagnostic::error(
                    "LM-DEF-006",
                    "syntax item requires a valid quoted value",
                    source,
                    node.span,
                ));
                return None;
            };
            Some(match node.name.as_str() {
                "literal" => PatternPart::literal(value),
                "token" => PatternPart::token(value),
                "identifier" => PatternPart::Field {
                    name: value,
                    kind: FieldKind::Identifier,
                },
                "integer" => PatternPart::Field {
                    name: value,
                    kind: FieldKind::Integer,
                },
                "string" => PatternPart::Field {
                    name: value,
                    kind: FieldKind::String,
                },
                _ => return None,
            })
        }
        "expression" => {
            let (Some(name), Some(terminator)) = (
                string_field(node, "name").and_then(decode_quoted),
                string_field(node, "terminator").and_then(decode_quoted),
            ) else {
                diagnostics.push(Diagnostic::error(
                    "LM-DEF-006",
                    "expression syntax requires a field name and a valid quoted terminator",
                    source,
                    node.span,
                ));
                return None;
            };
            Some(PatternPart::expression(name, terminator))
        }
        "optional" | "zero_or_more" | "one_or_more" => {
            let parts = node
                .children
                .iter()
                .filter_map(|child| lower_pattern(child, source, diagnostics))
                .collect::<Vec<_>>();
            Some(match node.name.as_str() {
                "optional" => PatternPart::optional(parts),
                "zero_or_more" => PatternPart::zero_or_more(parts),
                "one_or_more" => PatternPart::one_or_more(parts),
                _ => return None,
            })
        }
        _ => None,
    }
}

fn string_field<'a>(node: &'a ParsedConstruct, name: &str) -> Option<&'a str> {
    node.fields
        .iter()
        .find(|field| field.name == name)
        .and_then(|field| match &field.value {
            CapturedValue::String(value) => Some(value.as_str()),
            _ => None,
        })
}

fn decode_quoted(value: &str) -> Option<String> {
    let inner = value.strip_prefix('"')?.strip_suffix('"')?;
    let mut decoded = String::with_capacity(inner.len());
    let mut characters = inner.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            decoded.push(character);
            continue;
        }
        match characters.next()? {
            '\\' => decoded.push('\\'),
            '"' => decoded.push('"'),
            'n' => decoded.push('\n'),
            'r' => decoded.push('\r'),
            't' => decoded.push('\t'),
            _ => return None,
        }
    }
    Some(decoded)
}

fn spec_error_construct(error: &SpecError) -> Option<&str> {
    match error {
        SpecError::EmptyConstructName
        | SpecError::NoConstructs
        | SpecError::EmptyLanguageName
        | SpecError::UnknownRootConstruct(_)
        | SpecError::DuplicateRootConstruct(_)
        | SpecError::InvalidLineComment(_)
        | SpecError::InvalidBlockComment { .. }
        | SpecError::DuplicateCommentStart(_) => None,
        SpecError::DuplicateConstruct(name)
        | SpecError::EmptyPattern(name)
        | SpecError::EmptyLiteral(name)
        | SpecError::InvalidBlockDelimiter(name) => Some(name),
        SpecError::InvalidLiteral { construct, .. } | SpecError::EmptyFieldName(construct) => {
            Some(construct)
        }
        SpecError::InvalidExpressionTerminator { construct, .. } => Some(construct),
        SpecError::DuplicateField { construct, .. }
        | SpecError::UnknownChildConstruct { construct, .. }
        | SpecError::DuplicateChildConstruct { construct, .. }
        | SpecError::EmptyCategoryName(construct)
        | SpecError::DuplicateImplementedCategory { construct, .. }
        | SpecError::DuplicateBlockCategory { construct, .. }
        | SpecError::UnknownBlockCategory { construct, .. }
        | SpecError::EmptyPatternGroup(construct)
        | SpecError::EmptyOptional(construct)
        | SpecError::EmptyRepetition(construct)
        | SpecError::AmbiguousPatternBoundary(construct)
        | SpecError::AmbiguousVariants(construct) => Some(construct),
        SpecError::AmbiguousConstructs(left, _) => Some(left),
    }
}

fn definition_grammar() -> &'static LanguageSpec {
    static GRAMMAR: OnceLock<LanguageSpec> = OnceLock::new();
    GRAMMAR.get_or_init(|| {
        LanguageSpec::new(
            "Langmeld definition",
            [
                ConstructSpec::new(
                    "language",
                    [
                        PatternPart::literal("language"),
                        PatternPart::string("name"),
                    ],
                )
                .block("{", "}", &["root", "construct", "import", "lexical"]),
                ConstructSpec::new(
                    "root",
                    [
                        PatternPart::literal("root"),
                        PatternPart::string("name"),
                        PatternPart::literal(";"),
                    ],
                ),
                ConstructSpec::new(
                    "import",
                    [
                        PatternPart::literal("import"),
                        PatternPart::string("path"),
                        PatternPart::literal(";"),
                    ],
                ),
                ConstructSpec::new("lexical", [PatternPart::literal("lexical")]).block(
                    "{",
                    "}",
                    &["line_comment", "block_comment"],
                ),
                ConstructSpec::new(
                    "line_comment",
                    [
                        PatternPart::literal("line_comment"),
                        PatternPart::string("marker"),
                        PatternPart::literal(";"),
                    ],
                ),
                ConstructSpec::new(
                    "block_comment",
                    [
                        PatternPart::literal("block_comment"),
                        PatternPart::string("open"),
                        PatternPart::string("close"),
                        PatternPart::literal(";"),
                    ],
                ),
                ConstructSpec::new(
                    "construct",
                    [
                        PatternPart::literal("construct"),
                        PatternPart::string("name"),
                    ],
                )
                .block("{", "}", &["syntax", "block", "implements"]),
                ConstructSpec::new("syntax", [PatternPart::literal("syntax")]).block(
                    "{",
                    "}",
                    &[
                        "literal",
                        "token",
                        "identifier",
                        "integer",
                        "string",
                        "expression",
                        "optional",
                        "zero_or_more",
                        "one_or_more",
                    ],
                ),
                ConstructSpec::new(
                    "block",
                    [
                        PatternPart::literal("block"),
                        PatternPart::string("open"),
                        PatternPart::string("close"),
                    ],
                )
                .block("{", "}", &["allow", "allow_category"]),
                ConstructSpec::new("optional", [PatternPart::literal("optional")]).block(
                    "{",
                    "}",
                    &[
                        "literal",
                        "token",
                        "identifier",
                        "integer",
                        "string",
                        "expression",
                        "optional",
                        "zero_or_more",
                        "one_or_more",
                    ],
                ),
                ConstructSpec::new("zero_or_more", [PatternPart::literal("zero_or_more")]).block(
                    "{",
                    "}",
                    &[
                        "literal",
                        "token",
                        "identifier",
                        "integer",
                        "string",
                        "expression",
                        "optional",
                        "zero_or_more",
                        "one_or_more",
                    ],
                ),
                ConstructSpec::new("one_or_more", [PatternPart::literal("one_or_more")]).block(
                    "{",
                    "}",
                    &[
                        "literal",
                        "token",
                        "identifier",
                        "integer",
                        "string",
                        "expression",
                        "optional",
                        "zero_or_more",
                        "one_or_more",
                    ],
                ),
                ConstructSpec::new(
                    "literal",
                    [
                        PatternPart::literal("literal"),
                        PatternPart::string("value"),
                        PatternPart::literal(";"),
                    ],
                ),
                ConstructSpec::new(
                    "token",
                    [
                        PatternPart::literal("token"),
                        PatternPart::string("value"),
                        PatternPart::literal(";"),
                    ],
                ),
                ConstructSpec::new(
                    "identifier",
                    [
                        PatternPart::literal("identifier"),
                        PatternPart::string("value"),
                        PatternPart::literal(";"),
                    ],
                ),
                ConstructSpec::new(
                    "integer",
                    [
                        PatternPart::literal("integer"),
                        PatternPart::string("value"),
                        PatternPart::literal(";"),
                    ],
                ),
                ConstructSpec::new(
                    "string",
                    [
                        PatternPart::literal("string"),
                        PatternPart::string("value"),
                        PatternPart::literal(";"),
                    ],
                ),
                ConstructSpec::new(
                    "expression",
                    [
                        PatternPart::literal("expression"),
                        PatternPart::string("name"),
                        PatternPart::literal("until"),
                        PatternPart::string("terminator"),
                        PatternPart::literal(";"),
                    ],
                ),
                ConstructSpec::new(
                    "allow",
                    [
                        PatternPart::literal("allow"),
                        PatternPart::string("name"),
                        PatternPart::literal(";"),
                    ],
                ),
                ConstructSpec::new(
                    "allow_category",
                    [
                        PatternPart::literal("allow_category"),
                        PatternPart::string("category"),
                        PatternPart::literal(";"),
                    ],
                ),
                ConstructSpec::new(
                    "implements",
                    [
                        PatternPart::literal("implements"),
                        PatternPart::string("category"),
                        PatternPart::literal(";"),
                    ],
                ),
            ],
        )
        .and_then(|language| language.with_root_constructs(&["language", "import", "construct"]))
        .and_then(|language| {
            language.with_lexical_rules(LexicalRules::default().line_comment("//"))
        })
        .expect("the built-in language-definition grammar must remain valid")
    })
}

use langmeld_core::{SourceFile, SourceLocation, Span};
use langmeld_language::{CapturedValue, ParseResult, ParsedConstruct, decode_definition_string};

#[derive(Default)]
pub(crate) struct DefinitionIndex {
    constructs: Vec<String>,
    categories: Vec<String>,
    occurrences: Vec<DefinitionOccurrence>,
}

impl DefinitionIndex {
    pub(crate) fn add_parsed_source(&mut self, source: &SourceFile, parsed: &ParseResult) {
        self.add_constructs(source, parsed.constructs());
    }

    pub(crate) fn constructs(&self) -> &[String] {
        &self.constructs
    }

    pub(crate) fn categories(&self) -> &[String] {
        &self.categories
    }

    pub(crate) fn definitions_at(
        &self,
        source: &SourceFile,
        parsed: &ParseResult,
        offset: usize,
    ) -> Vec<SourceLocation> {
        let Some((namespace, name)) = symbol_at(parsed.constructs(), offset) else {
            return Vec::new();
        };
        self.occurrences
            .iter()
            .filter(|occurrence| {
                occurrence.declaration
                    && occurrence.namespace == namespace
                    && occurrence.name == name
            })
            .map(|occurrence| occurrence.location.clone())
            .filter(|location| valid_location(source, location))
            .collect()
    }

    pub(crate) fn references_at(
        &self,
        source: &SourceFile,
        parsed: &ParseResult,
        offset: usize,
        include_declaration: bool,
    ) -> Vec<SourceLocation> {
        let Some((namespace, name)) = symbol_at(parsed.constructs(), offset) else {
            return Vec::new();
        };
        self.occurrences
            .iter()
            .filter(|occurrence| {
                occurrence.namespace == namespace
                    && occurrence.name == name
                    && (include_declaration || !occurrence.declaration)
            })
            .map(|occurrence| occurrence.location.clone())
            .filter(|location| valid_location(source, location))
            .collect()
    }

    fn add_constructs(&mut self, source: &SourceFile, constructs: &[ParsedConstruct]) {
        for construct in constructs {
            match construct.name.as_str() {
                "construct" => {
                    if let Some((name, field)) = string_field(construct, "name") {
                        Self::add_completion(&mut self.constructs, &name);
                        self.add_occurrence(
                            source,
                            IndexNamespace::Construct,
                            name,
                            field.span,
                            true,
                        );
                    }
                }
                "implements" => {
                    if let Some((category, field)) = string_field(construct, "category") {
                        Self::add_completion(&mut self.categories, &category);
                        self.add_occurrence(
                            source,
                            IndexNamespace::Category,
                            category,
                            field.span,
                            true,
                        );
                    }
                }
                "root" | "allow" => {
                    if let Some((name, field)) = string_field(construct, "name") {
                        self.add_occurrence(
                            source,
                            IndexNamespace::Construct,
                            name,
                            field.span,
                            false,
                        );
                    }
                }
                "allow_category" => {
                    if let Some((category, field)) = string_field(construct, "category") {
                        self.add_occurrence(
                            source,
                            IndexNamespace::Category,
                            category,
                            field.span,
                            false,
                        );
                    }
                }
                _ => {}
            }
            self.add_constructs(source, &construct.children);
        }
    }

    fn add_completion(values: &mut Vec<String>, name: &str) {
        if !values.iter().any(|value| value == name) {
            values.push(name.to_owned());
        }
    }

    fn add_occurrence(
        &mut self,
        source: &SourceFile,
        namespace: IndexNamespace,
        name: String,
        span: Span,
        declaration: bool,
    ) {
        self.occurrences.push(DefinitionOccurrence {
            namespace,
            name,
            declaration,
            location: SourceLocation::new(source, span),
        });
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum IndexNamespace {
    Construct,
    Category,
}

#[derive(Clone)]
struct DefinitionOccurrence {
    namespace: IndexNamespace,
    name: String,
    declaration: bool,
    location: SourceLocation,
}

fn symbol_at(constructs: &[ParsedConstruct], offset: usize) -> Option<(IndexNamespace, String)> {
    for construct in constructs {
        for field in &construct.fields {
            if field.span.start <= offset
                && offset <= field.span.end
                && let Some(namespace) =
                    reference_field(construct.name.as_str(), field.name.as_str())
                && let CapturedValue::String(raw_name) = &field.value
            {
                return Some((namespace, decode_definition_string(raw_name)?));
            }
        }
        if let Some(symbol) = symbol_at(&construct.children, offset) {
            return Some(symbol);
        }
    }
    None
}

fn reference_field(construct: &str, field: &str) -> Option<IndexNamespace> {
    match (construct, field) {
        ("construct", "name") | ("root" | "allow", "name") => Some(IndexNamespace::Construct),
        ("implements" | "allow_category", "category") => Some(IndexNamespace::Category),
        _ => None,
    }
}

fn string_field<'a>(
    construct: &'a ParsedConstruct,
    name: &str,
) -> Option<(String, &'a langmeld_language::CapturedField)> {
    let field = construct.fields.iter().find(|field| field.name == name)?;
    let CapturedValue::String(value) = &field.value else {
        return None;
    };
    Some((decode_definition_string(value)?, field))
}

fn valid_location(source: &SourceFile, location: &SourceLocation) -> bool {
    location.source_name() != source.name() || location.is_valid_for(source)
}

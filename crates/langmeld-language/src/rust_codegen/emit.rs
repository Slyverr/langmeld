use std::{
    collections::{BTreeSet, HashMap},
    fmt::Write,
};

use langmeld_core::{FieldConstraints, FieldKind, MapKeyKind, ValidatedLanguageSpec};

use super::types::{EnumDefinition, TypeRegistry, ValueReader};
use super::{pascal_case, rust_keyword_safe, snake_case, unique};

/// Generates deterministic Rust source for typed views of a language AST.
///
/// The generated module embeds the canonical language definition and includes
/// conversions from [`crate::AstNode`] and [`crate::Program`]. Consumers compile
/// the output in a crate that depends on both `langmeld-core` and
/// `langmeld-language`.
#[must_use]
pub fn generate_rust_ast(language: &ValidatedLanguageSpec) -> String {
    let spec = language.spec();
    let mut used_types = BTreeSet::from([
        "AstNode".to_owned(),
        "BTreeMap".to_owned(),
        "Diagnostic".to_owned(),
        "GenericProgram".to_owned(),
        "Option".to_owned(),
        "PhantomData".to_owned(),
        "PropertyMap".to_owned(),
        "Result".to_owned(),
        "Spanned".to_owned(),
        "Severity".to_owned(),
        "SourceFile".to_owned(),
        "Span".to_owned(),
        "String".to_owned(),
        "Vec".to_owned(),
        "Reference".to_owned(),
        "ConversionError".to_owned(),
        "fmt".to_owned(),
    ]);
    let construct_types = spec
        .constructs
        .iter()
        .map(|construct| unique(pascal_case(&construct.id), &mut used_types))
        .collect::<Vec<_>>();
    let child_types = spec
        .constructs
        .iter()
        .enumerate()
        .map(|(index, construct)| {
            construct
                .block
                .as_ref()
                .map(|_| unique(format!("{}Child", construct_types[index]), &mut used_types))
        })
        .collect::<Vec<_>>();
    let registry = TypeRegistry::new(spec, &mut used_types);
    let field_names = spec
        .constructs
        .iter()
        .map(|construct| {
            let mut used = BTreeSet::from([
                "children".to_owned(),
                "syntax_variant".to_owned(),
                "span".to_owned(),
            ]);
            construct
                .fields
                .iter()
                .map(|field| unique(rust_keyword_safe(snake_case(&field.name)), &mut used))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let symbols = spec
        .constructs
        .iter()
        .enumerate()
        .filter_map(|(index, construct)| {
            construct
                .symbol
                .as_ref()
                .map(|symbol| (symbol.kind.as_str(), construct_types[index].as_str()))
        })
        .collect::<HashMap<_, _>>();

    let mut output = String::from(
        "// Generated typed views for a Langmeld language syntax tree.\n\n\
use std::{collections::BTreeMap, fmt, marker::PhantomData};\n\
use langmeld_core::{Diagnostic, Severity, SourceFile, Span};\n\
use langmeld_language::{AstNode, Program as GenericProgram};\n\n",
    );
    let _ = writeln!(output, "pub const LANGUAGE_NAME: &str = {:?};", spec.name);
    let _ = writeln!(
        output,
        "pub const LANGUAGE_VERSION: &str = {:?};\n",
        spec.version
    );
    write_language_spec(&mut output, spec);
    output.push_str(
        r#"/// Captured values accepted by generated property records.
pub type PropertyMap = BTreeMap<String, langmeld_language::CapturedValue>;

#[derive(Clone, Debug, PartialEq)]
pub struct Spanned<T> {
    pub value: T,
    pub span: Span,
}

/// A reference spelling parameterized by the generated declaration type.
#[derive(Clone, Debug, PartialEq)]
pub struct Reference<T> {
    pub name: String,
    _target: PhantomData<fn() -> T>,
}
impl<T> Reference<T> {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), _target: PhantomData }
    }
    #[must_use]
    pub fn as_str(&self) -> &str { &self.name }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConversionError {
    WrongConstruct { expected: &'static str, actual: String },
    MissingField(&'static str),
    WrongFieldKind(&'static str),
    UnknownEnumValue { field: &'static str, value: String },
    UnknownChild { parent: &'static str, child: String },
    UnexpectedChildren { parent: &'static str },
    InvalidSyntaxVariant { construct: &'static str, variant: String },
}
impl fmt::Display for ConversionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongConstruct { expected, actual } => write!(formatter, "expected construct {expected}, got {actual}"),
            Self::MissingField(field) => write!(formatter, "required field {field} is absent"),
            Self::WrongFieldKind(field) => write!(formatter, "field {field} has an unexpected value kind"),
            Self::UnknownEnumValue { field, value } => write!(formatter, "field {field} has unknown enum value {value}"),
            Self::UnknownChild { parent, child } => write!(formatter, "construct {child} is not an allowed child of {parent}"),
            Self::UnexpectedChildren { parent } => write!(formatter, "construct {parent} cannot have children"),
            Self::InvalidSyntaxVariant { construct, variant } => write!(formatter, "construct {construct} has unknown syntax variant {variant}"),
        }
    }
}
impl std::error::Error for ConversionError {}

"#,
    );

    write_enum_types(&mut output, &registry);
    write_collection_aliases(&mut output, &registry, &symbols);
    write_property_records(&mut output, &registry, &symbols);
    write_child_enums(&mut output, spec, &construct_types, &child_types);
    for reader in &registry.readers {
        write_value_reader(&mut output, reader, &registry, &symbols);
    }
    write_construct_types(
        &mut output,
        spec,
        &construct_types,
        &child_types,
        &field_names,
        &registry,
        &symbols,
    );
    write_program_conversion(&mut output, spec, &construct_types);
    output
}

/// Generates Rust source that reconstructs and validates a canonical language specification.
///
/// The returned module uses the public `langmeld` facade and is suitable for an
/// initialized Rust host to own and edit as its base language model.
#[must_use]
pub fn generate_rust_language_spec(language: &ValidatedLanguageSpec) -> String {
    let mut output = String::from(
        "// Initial Rust representation of the canonical Langmeld language model.\n\
         // Edit this model directly; Langmeld does not overwrite authored host source.\n\
         use langmeld::core as langmeld_core;\n\n",
    );
    write_language_spec(&mut output, language.spec());
    output
}

fn write_enum_types(output: &mut String, registry: &TypeRegistry) {
    for definition in &registry.enum_definitions {
        let _ = writeln!(
            output,
            "/// Values accepted by this generated field type.\n#[derive(Clone, Copy, Debug, PartialEq, Eq)]\npub enum {} {{",
            definition.name
        );
        for variant in &definition.variants {
            let _ = writeln!(output, "    {variant},");
        }
        output.push_str("}\n");
        let _ = writeln!(
            output,
            "impl {} {{\n    #[must_use]\n    pub fn as_str(self) -> &'static str {{\n        match self {{",
            definition.name
        );
        for (value, variant) in definition.values.iter().zip(&definition.variants) {
            let _ = writeln!(output, "            Self::{variant} => {value:?},");
        }
        output.push_str("        }\n    }\n}\n\n");
    }
}

fn write_collection_aliases(
    output: &mut String,
    registry: &TypeRegistry,
    symbols: &HashMap<&str, &str>,
) {
    for alias in &registry.collection_aliases {
        let rust_type = match &alias.kind {
            FieldKind::List { item } => format!(
                "Vec<{}>",
                registry.spanned_type(item, &format!("{}[]", alias.path), symbols)
            ),
            FieldKind::Map { value, .. } => format!(
                "BTreeMap<String, {}>",
                registry.spanned_type(value, &format!("{}{{}}", alias.path), symbols)
            ),
            _ => continue,
        };
        let _ = writeln!(
            output,
            "/// Typed collection captured by `{}`.\npub type {} = {};\n",
            alias.path, alias.name, rust_type
        );
    }
}

fn write_property_records(
    output: &mut String,
    registry: &TypeRegistry,
    symbols: &HashMap<&str, &str>,
) {
    for record in &registry.property_records {
        let _ = writeln!(
            output,
            "/// Typed property record captured by `{}`.\n#[derive(Clone, Debug, PartialEq)]\npub struct {} {{",
            record.path, record.name
        );
        for (index, field) in record.fields.iter().enumerate() {
            let path = format!("{}.{}", record.path, field.name);
            let field_type = registry.spanned_type(&field.kind, &path, symbols);
            let field_type = if field.required {
                field_type
            } else {
                format!("Option<{field_type}>")
            };
            let _ = writeln!(
                output,
                "    pub {}: {},",
                record.field_names[index], field_type
            );
        }
        output.push_str("}\n\n");
        let _ = writeln!(
            output,
            "impl TryFrom<&PropertyMap> for {} {{\n    type Error = ConversionError;\n    fn try_from(values: &PropertyMap) -> Result<Self, Self::Error> {{",
            record.name
        );
        for (index, field) in record.fields.iter().enumerate() {
            let name = &record.field_names[index];
            let path = format!("{}.{}", record.path, field.name);
            let reader = &registry.reader_names[&path];
            if field.required {
                let _ = writeln!(
                    output,
                    "        let {name} = {reader}(values.get({:?}).ok_or(ConversionError::MissingField({:?}))?)?;",
                    field.name, field.name
                );
            } else {
                let _ = writeln!(
                    output,
                    "        let {name} = values.get({:?}).map({reader}).transpose()?;",
                    field.name
                );
            }
        }
        output.push_str("        Ok(Self {\n");
        for name in &record.field_names {
            let _ = writeln!(output, "            {name},");
        }
        output.push_str("        })\n    }\n}\n\n");
    }
}

fn write_child_enums(
    output: &mut String,
    spec: &langmeld_core::LanguageSpec,
    construct_types: &[String],
    child_types: &[Option<String>],
) {
    for (index, construct) in spec.constructs.iter().enumerate() {
        let Some(block) = &construct.block else {
            continue;
        };
        let Some(child_type) = &child_types[index] else {
            continue;
        };
        let allowed = allowed_construct_ids(spec, block);
        let _ = writeln!(
            output,
            "/// Child nodes accepted by `{}`.\n#[derive(Clone, Debug, PartialEq)]\npub enum {child_type} {{",
            construct.id
        );
        for child in &allowed {
            if let Some(child_index) = spec.constructs.iter().position(|item| item.id == *child) {
                let type_name = &construct_types[child_index];
                let _ = writeln!(output, "    {type_name}({type_name}),");
            }
        }
        output.push_str("}\n\n");
    }
}

fn allowed_construct_ids(
    spec: &langmeld_core::LanguageSpec,
    block: &langmeld_core::BlockSpec,
) -> Vec<String> {
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    for allowed in &block.allowed {
        if spec
            .constructs
            .iter()
            .any(|construct| construct.id == *allowed)
        {
            if seen.insert(allowed.clone()) {
                result.push(allowed.clone());
            }
        } else {
            for construct in spec
                .constructs
                .iter()
                .filter(|construct| construct.implements.contains(allowed))
            {
                if seen.insert(construct.id.clone()) {
                    result.push(construct.id.clone());
                }
            }
        }
    }
    result
}

fn write_construct_types(
    output: &mut String,
    spec: &langmeld_core::LanguageSpec,
    construct_types: &[String],
    child_types: &[Option<String>],
    field_names: &[Vec<String>],
    registry: &TypeRegistry,
    symbols: &HashMap<&str, &str>,
) {
    for (index, construct) in spec.constructs.iter().enumerate() {
        let type_name = &construct_types[index];
        let _ = writeln!(
            output,
            "/// Typed form of the `{}` construct.\n#[derive(Clone, Debug, PartialEq)]\npub struct {type_name} {{",
            construct.id
        );
        for (field_index, field) in construct.fields.iter().enumerate() {
            let path = format!("{}.{}", construct.id, field.name);
            let field_type = registry.spanned_type(&field.kind, &path, symbols);
            let field_type = if field.required {
                field_type
            } else {
                format!("Option<{field_type}>")
            };
            let _ = writeln!(
                output,
                "    pub {}: {},",
                field_names[index][field_index], field_type
            );
        }
        if let Some(child_type) = &child_types[index] {
            let _ = writeln!(output, "    pub children: Vec<{child_type}>,");
        }
        output.push_str("    pub syntax_variant: String,\n    pub span: Span,\n}\n\n");

        let _ = writeln!(
            output,
            "impl TryFrom<&AstNode> for {type_name} {{\n    type Error = ConversionError;\n    fn try_from(node: &AstNode) -> Result<Self, Self::Error> {{"
        );
        let _ = writeln!(
            output,
            "        if node.construct != {:?} {{ return Err(ConversionError::WrongConstruct {{ expected: {:?}, actual: node.construct.clone() }}); }}",
            construct.id, construct.id
        );
        if !construct.syntax_ids.is_empty() {
            let valid = construct
                .syntax_ids
                .iter()
                .map(|id| format!("{id:?}"))
                .collect::<Vec<_>>()
                .join(" | ");
            let _ = writeln!(
                output,
                "        if !matches!(node.syntax_variant.as_str(), {valid}) {{ return Err(ConversionError::InvalidSyntaxVariant {{ construct: {:?}, variant: node.syntax_variant.clone() }}); }}",
                construct.id
            );
        }
        for (field_index, field) in construct.fields.iter().enumerate() {
            let field_name = &field_names[index][field_index];
            let path = format!("{}.{}", construct.id, field.name);
            let reader = &registry.reader_names[&path];
            if field.required {
                let _ = writeln!(
                    output,
                    "        let {field_name} = {reader}(node.fields.get({:?}).ok_or(ConversionError::MissingField({:?}))?)?;",
                    field.name, field.name
                );
            } else {
                let _ = writeln!(
                    output,
                    "        let {field_name} = node.fields.get({:?}).map({reader}).transpose()?;",
                    field.name
                );
            }
        }
        if let (Some(block), Some(child_type)) = (&construct.block, &child_types[index]) {
            let allowed = allowed_construct_ids(spec, block);
            output.push_str("        let children = node.children.iter().map(|child| match child.construct.as_str() {\n");
            for child in allowed {
                let Some(child_index) = spec.constructs.iter().position(|item| item.id == child)
                else {
                    continue;
                };
                let rust_type = &construct_types[child_index];
                let _ = writeln!(
                    output,
                    "            {child:?} => Ok({child_type}::{rust_type}({rust_type}::try_from(child)?)),"
                );
            }
            let _ = writeln!(
                output,
                "            child => Err(ConversionError::UnknownChild {{ parent: {:?}, child: child.to_owned() }}),",
                construct.id
            );
            output.push_str("        }).collect::<Result<Vec<_>, ConversionError>>()?;\n");
        } else {
            let _ = writeln!(
                output,
                "        if !node.children.is_empty() {{ return Err(ConversionError::UnexpectedChildren {{ parent: {:?} }}); }}",
                construct.id
            );
        }
        output.push_str("        Ok(Self {\n");
        for field_name in &field_names[index] {
            let _ = writeln!(output, "            {field_name},");
        }
        if child_types[index].is_some() {
            output.push_str("            children,\n");
        }
        output.push_str("            syntax_variant: node.syntax_variant.clone(),\n            span: node.span,\n        })\n    }\n}\n\n");
    }
}

fn write_program_conversion(
    output: &mut String,
    spec: &langmeld_core::LanguageSpec,
    construct_types: &[String],
) {
    let Some(root_index) = spec
        .constructs
        .iter()
        .position(|construct| construct.id == spec.root)
    else {
        return;
    };
    let root_type = &construct_types[root_index];
    let _ = writeln!(
        output,
        "/// Converts generic root nodes into generated typed syntax values.\npub fn from_program(program: &GenericProgram) -> Result<Vec<{root_type}>, ConversionError> {{\n    program.nodes.iter().map({root_type}::try_from).collect()\n}}\n"
    );
    output.push_str(
        r#"/// Parses and analyzes source while preserving non-error diagnostics.
pub fn parse_source(source: &SourceFile) -> Result<(Vec<ROOT>, Vec<Diagnostic>), Vec<Diagnostic>> {
    parse_source_with_custom_parsers(source, &[])
}

/// Parses source with the callbacks required by this language definition.
pub fn parse_source_with_custom_parsers(
    source: &SourceFile,
    custom_parsers: &[langmeld_language::CustomConstructParser],
) -> Result<(Vec<ROOT>, Vec<Diagnostic>), Vec<Diagnostic>> {
    let language = validated_language().map_err(|errors| {
        errors.into_iter().map(|error| Diagnostic::spec_error(error.code, error.message)).collect::<Vec<_>>()
    })?;
    let prepared = langmeld_language::PreparedLanguage::new(language.clone())
        .map_err(|error| vec![Diagnostic::spec_error("GEN0002", error.to_string())])?;
    let parser = prepared.with_custom_parsers(custom_parsers.iter().cloned())
        .map_err(|error| vec![Diagnostic::spec_error("GEN0003", error.to_string())])?;
    let parsed = parser.parse(source);
    let mut diagnostics = parsed.diagnostics;
    diagnostics.extend(langmeld_language::analyze(&language, &parsed.program, source));
    if diagnostics.iter().any(|diagnostic| diagnostic.severity == Severity::Error) {
        return Err(diagnostics);
    }
    let roots = from_program(&parsed.program)
        .map_err(|error| vec![Diagnostic::spec_error("GEN0001", error.to_string())])?;
    Ok((roots, diagnostics))
}

"#,
    );
    let placeholder = output
        .rfind("ROOT")
        .expect("parse helper contains root placeholder");
    output.replace_range(placeholder..placeholder + 4, root_type);
    let second = output
        .rfind("ROOT")
        .expect("parse helper has two root placeholders");
    output.replace_range(second..second + 4, root_type);
}

fn write_value_reader(
    output: &mut String,
    reader: &ValueReader,
    registry: &TypeRegistry,
    symbols: &HashMap<&str, &str>,
) {
    let rust_type = registry.field_type(&reader.kind, &reader.path, symbols);
    let _ = writeln!(
        output,
        "fn {}(captured: &langmeld_language::CapturedValue) -> Result<Spanned<{rust_type}>, ConversionError> {{",
        reader.helper
    );
    match &reader.kind {
        FieldKind::Identifier => {
            write_scalar_reader(output, &reader.field, "Identifier", "value.clone()")
        }
        FieldKind::Text => write_scalar_reader(output, &reader.field, "Text", "value.clone()"),
        FieldKind::Number => write_scalar_reader(output, &reader.field, "Number", "*value"),
        FieldKind::Reference { .. } => write_scalar_reader(
            output,
            &reader.field,
            "Reference",
            "Reference::new(value.clone())",
        ),
        FieldKind::Enum { values } => {
            let type_name = registry
                .inline_enums
                .get(&reader.path)
                .expect("inline enum was registered");
            write_enum_reader(
                output,
                &reader.field,
                values,
                type_name,
                &registry.enum_definitions,
            );
        }
        FieldKind::NamedEnum { name, values } => {
            let type_name = registry
                .named_enums
                .get(name)
                .expect("named enum was registered");
            write_enum_reader(
                output,
                &reader.field,
                values,
                type_name,
                &registry.enum_definitions,
            );
        }
        FieldKind::Properties { .. } => {
            let type_name = &registry.property_types[&reader.path];
            let _ = writeln!(
                output,
                "    let values = match &captured.value {{ langmeld_language::Value::Properties(values) => values, _ => return Err(ConversionError::WrongFieldKind({:?})) }};\n    let value = {type_name}::try_from(values)?;",
                reader.field
            );
        }
        FieldKind::List { .. } => {
            let helper = &registry.reader_names[&format!("{}[]", reader.path)];
            let _ = writeln!(
                output,
                "    let values = match &captured.value {{ langmeld_language::Value::List(values) => values, _ => return Err(ConversionError::WrongFieldKind({:?})) }};\n    let value = values.iter().map({helper}).collect::<Result<Vec<_>, ConversionError>>()?;",
                reader.field
            );
        }
        FieldKind::Map { .. } => {
            let helper = &registry.reader_names[&format!("{}{{}}", reader.path)];
            let _ = writeln!(
                output,
                "    let entries = match &captured.value {{ langmeld_language::Value::Map(entries) => entries, _ => return Err(ConversionError::WrongFieldKind({:?})) }};\n    let value = entries.iter().map(|entry| Ok((entry.key.clone(), {helper}(&entry.value)?))).collect::<Result<BTreeMap<_, _>, ConversionError>>()?;",
                reader.field
            );
        }
    }
    output.push_str("    Ok(Spanned { value, span: captured.span })\n}\n\n");
}

fn write_enum_reader(
    output: &mut String,
    field: &str,
    values: &[String],
    type_name: &str,
    definitions: &[EnumDefinition],
) {
    let definition = definitions
        .iter()
        .find(|item| item.name == type_name)
        .expect("enum type has a definition");
    output.push_str("    let value = match &captured.value {\n        langmeld_language::Value::Enum(value) => match value.as_str() {\n");
    for (value, variant) in values.iter().zip(&definition.variants) {
        let _ = writeln!(output, "            {value:?} => {type_name}::{variant},");
    }
    let _ = writeln!(
        output,
        "            other => return Err(ConversionError::UnknownEnumValue {{ field: {field:?}, value: other.to_owned() }}),\n        }},\n        _ => return Err(ConversionError::WrongFieldKind({field:?})),\n    }};"
    );
}

fn write_scalar_reader(output: &mut String, field: &str, variant: &str, expression: &str) {
    let _ = writeln!(
        output,
        "    let value = match &captured.value {{ langmeld_language::Value::{variant}(value) => {expression}, _ => return Err(ConversionError::WrongFieldKind({field:?})) }};"
    );
}

fn write_language_spec(output: &mut String, spec: &langmeld_core::LanguageSpec) {
    output.push_str("/// Reconstructs this module's canonical language definition.\npub fn language_spec() -> langmeld_core::LanguageSpec {\n    langmeld_core::LanguageSpec {\n");
    let _ = writeln!(output, "        schema_version: {},", spec.schema_version);
    let _ = writeln!(output, "        name: {:?}.to_owned(),", spec.name);
    let _ = writeln!(output, "        version: {:?}.to_owned(),", spec.version);
    let _ = writeln!(output, "        root: {:?}.to_owned(),", spec.root);
    write_string_vector(output, "source_extensions", &spec.source_extensions, 8);
    output.push_str("        categories: vec![\n");
    for category in &spec.categories {
        let _ = writeln!(
            output,
            "            langmeld_core::CategorySpec {{ id: {:?}.to_owned() }},",
            category.id
        );
    }
    output.push_str("        ],\n        rules: vec![\n");
    for rule in &spec.rules {
        let _ = writeln!(
            output,
            "            langmeld_core::SyntaxRuleSpec {{ id: {:?}.to_owned(), parameters: {}, pattern: {:?}.to_owned() }},",
            rule.id,
            string_vector(&rule.parameters),
            rule.pattern
        );
    }
    output.push_str("        ],\n        enums: vec![\n");
    for enumeration in &spec.enums {
        let _ = writeln!(
            output,
            "            langmeld_core::EnumSpec {{ id: {:?}.to_owned(), values: {} }},",
            enumeration.id,
            string_vector(&enumeration.values)
        );
    }
    output.push_str("        ],\n        types: vec![\n");
    for record in &spec.types {
        let _ = writeln!(
            output,
            "            langmeld_core::TypeSpec {{ id: {:?}.to_owned(), fields: {} }},",
            record.id,
            field_vector(&record.fields)
        );
    }
    output.push_str("        ],\n        behaviors: vec![\n");
    for behavior in &spec.behaviors {
        let stage = match behavior.stage {
            langmeld_core::BehaviorStage::SemanticAnalysis => "SemanticAnalysis",
            langmeld_core::BehaviorStage::SymbolResolution => "SymbolResolution",
            langmeld_core::BehaviorStage::WorkspaceTypeAnalysis => "WorkspaceTypeAnalysis",
        };
        let _ = writeln!(
            output,
            "            langmeld_core::BehaviorSpec {{ id: {:?}.to_owned(), stage: langmeld_core::BehaviorStage::{stage} }},",
            behavior.id
        );
    }
    output.push_str("        ],\n        constructs: vec![\n");
    for construct in &spec.constructs {
        output.push_str("            langmeld_core::ConstructSpec {\n");
        let _ = writeln!(output, "                id: {:?}.to_owned(),", construct.id);
        write_string_vector(output, "implements", &construct.implements, 16);
        let custom_parser = construct.custom_parser.as_ref().map_or_else(
            || "None".to_owned(),
            |value| format!("Some({value:?}.to_owned())"),
        );
        let _ = writeln!(output, "                custom_parser: {custom_parser},");
        write_string_vector(output, "syntax", &construct.syntax, 16);
        write_string_vector(output, "syntax_ids", &construct.syntax_ids, 16);
        let _ = writeln!(
            output,
            "                fields: {},",
            field_vector(&construct.fields)
        );
        if let Some(block) = &construct.block {
            let _ = writeln!(
                output,
                "                block: Some(langmeld_core::BlockSpec {{ allowed: {} }}),",
                string_vector(&block.allowed)
            );
        } else {
            output.push_str("                block: None,\n");
        }
        if let Some(symbol) = &construct.symbol {
            let scope = if let Some(scope) = &symbol.scope {
                let lookup = match scope.lookup {
                    langmeld_core::ScopeLookup::Exact => "Exact",
                    langmeld_core::ScopeLookup::LexicalParents => "LexicalParents",
                };
                format!(
                    "Some(langmeld_core::ScopeSpec {{ construct: {:?}.to_owned(), name_field: {:?}.to_owned(), lookup: langmeld_core::ScopeLookup::{lookup} }})",
                    scope.construct, scope.name_field
                )
            } else {
                "None".to_owned()
            };
            let _ = writeln!(
                output,
                "                symbol: Some(langmeld_core::SymbolSpec {{ kind: {:?}.to_owned(), name_field: {:?}.to_owned(), scope: {scope} }}),",
                symbol.kind, symbol.name_field
            );
        } else {
            output.push_str("                symbol: None,\n");
        }
        output.push_str("            },\n");
    }
    output.push_str("        ],\n    }\n}\n\n");
    output.push_str("/// Validates the embedded language definition.\npub fn validated_language() -> Result<langmeld_core::ValidatedLanguageSpec, Vec<langmeld_core::SpecError>> {\n    language_spec().validate()\n}\n\n");
}

fn write_string_vector(output: &mut String, name: &str, values: &[String], indent: usize) {
    let padding = " ".repeat(indent);
    let _ = writeln!(output, "{padding}{name}: {},", string_vector(values));
}

fn string_vector(values: &[String]) -> String {
    format!(
        "vec![{}]",
        values
            .iter()
            .map(|value| format!("{value:?}.to_owned()"))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn field_vector(fields: &[langmeld_core::FieldSpec]) -> String {
    format!(
        "vec![{}]",
        fields
            .iter()
            .map(field_expression)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn field_expression(field: &langmeld_core::FieldSpec) -> String {
    format!(
        "langmeld_core::FieldSpec {{ name: {:?}.to_owned(), kind: {}, required: {}, default: {}, constraints: {} }}",
        field.name,
        field_kind_expression(&field.kind),
        field.required,
        field.default.as_ref().map_or_else(
            || "None".to_owned(),
            |value| format!("Some({value:?}.to_owned())")
        ),
        field_constraints_expression(&field.constraints)
    )
}

fn field_kind_expression(kind: &FieldKind) -> String {
    match kind {
        FieldKind::Identifier => "langmeld_core::FieldKind::Identifier".to_owned(),
        FieldKind::Number => "langmeld_core::FieldKind::Number".to_owned(),
        FieldKind::Text => "langmeld_core::FieldKind::Text".to_owned(),
        FieldKind::Enum { values } => format!(
            "langmeld_core::FieldKind::Enum {{ values: {} }}",
            string_vector(values)
        ),
        FieldKind::NamedEnum { name, values } => format!(
            "langmeld_core::FieldKind::NamedEnum {{ name: {:?}.to_owned(), values: {} }}",
            name,
            string_vector(values)
        ),
        FieldKind::Reference { target } => {
            format!("langmeld_core::FieldKind::Reference {{ target: {target:?}.to_owned() }}")
        }
        FieldKind::Properties { fields, name } => format!(
            "langmeld_core::FieldKind::Properties {{ name: {}, fields: {} }}",
            name.as_ref().map_or_else(
                || "None".to_owned(),
                |value| format!("Some({value:?}.to_owned())")
            ),
            field_vector(fields)
        ),
        FieldKind::List { item } => format!(
            "langmeld_core::FieldKind::List {{ item: std::boxed::Box::new({}) }}",
            field_kind_expression(item)
        ),
        FieldKind::Map { key, value } => format!(
            "langmeld_core::FieldKind::Map {{ key: langmeld_core::MapKeyKind::{}, value: std::boxed::Box::new({}) }}",
            match key {
                MapKeyKind::Identifier => "Identifier",
                MapKeyKind::Text => "Text",
            },
            field_kind_expression(value)
        ),
    }
}

fn field_constraints_expression(constraints: &FieldConstraints) -> String {
    let integer = |value: Option<i64>| {
        value.map_or_else(|| "None".to_owned(), |number| format!("Some({number})"))
    };
    let size = |value: Option<usize>| {
        value.map_or_else(|| "None".to_owned(), |number| format!("Some({number})"))
    };
    format!(
        "langmeld_core::FieldConstraints {{ min: {}, max: {}, min_length: {}, max_length: {}, min_items: {}, max_items: {} }}",
        integer(constraints.min),
        integer(constraints.max),
        size(constraints.min_length),
        size(constraints.max_length),
        size(constraints.min_items),
        size(constraints.max_items)
    )
}

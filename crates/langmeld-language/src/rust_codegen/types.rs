use std::collections::{BTreeSet, HashMap};

use langmeld_core::{FieldKind, FieldSpec};

use super::{pascal_case, rust_keyword_safe, snake_case, unique};

pub(super) struct EnumDefinition {
    pub(super) name: String,
    pub(super) values: Vec<String>,
    pub(super) variants: Vec<String>,
}

pub(super) struct PropertyRecord {
    pub(super) path: String,
    pub(super) name: String,
    pub(super) fields: Vec<FieldSpec>,
    pub(super) field_names: Vec<String>,
}

pub(super) struct CollectionAlias {
    pub(super) path: String,
    pub(super) name: String,
    pub(super) kind: FieldKind,
}

pub(super) struct ValueReader {
    pub(super) path: String,
    pub(super) helper: String,
    pub(super) kind: FieldKind,
    pub(super) field: String,
}

pub(super) struct TypeRegistry {
    pub(super) named_enums: HashMap<String, String>,
    pub(super) inline_enums: HashMap<String, String>,
    pub(super) enum_definitions: Vec<EnumDefinition>,
    pub(super) property_types: HashMap<String, String>,
    pub(super) named_records: HashMap<String, String>,
    pub(super) property_records: Vec<PropertyRecord>,
    pub(super) collections: HashMap<String, String>,
    pub(super) collection_aliases: Vec<CollectionAlias>,
    pub(super) readers: Vec<ValueReader>,
    pub(super) reader_names: HashMap<String, String>,
}

impl TypeRegistry {
    pub(super) fn new(spec: &langmeld_core::LanguageSpec, used: &mut BTreeSet<String>) -> Self {
        let mut registry = Self {
            named_enums: HashMap::new(),
            inline_enums: HashMap::new(),
            enum_definitions: Vec::new(),
            property_types: HashMap::new(),
            named_records: HashMap::new(),
            property_records: Vec::new(),
            collections: HashMap::new(),
            collection_aliases: Vec::new(),
            readers: Vec::new(),
            reader_names: HashMap::new(),
        };

        for enumeration in &spec.enums {
            let name = unique(pascal_case(&enumeration.id), used);
            registry
                .named_enums
                .insert(enumeration.id.clone(), name.clone());
            registry
                .enum_definitions
                .push(enum_definition(name, &enumeration.values));
        }
        for record in &spec.types {
            let name = unique(pascal_case(&record.id), used);
            registry
                .named_records
                .insert(record.id.clone(), name.clone());
            registry
                .property_types
                .insert(record.id.clone(), name.clone());
            registry.property_records.push(property_record(
                record.id.clone(),
                name,
                &record.fields,
            ));
        }
        for record in &spec.types {
            for field in &record.fields {
                registry.visit_kind(&format!("{}.{}", record.id, field.name), &field.kind, used);
            }
        }
        for construct in &spec.constructs {
            for field in &construct.fields {
                registry.visit_kind(
                    &format!("{}.{}", construct.id, field.name),
                    &field.kind,
                    used,
                );
            }
        }
        for construct in &spec.constructs {
            for field in &construct.fields {
                registry.register_reader(
                    &format!("{}.{}", construct.id, field.name),
                    &field.kind,
                    &field.name,
                );
            }
        }
        let property_readers = registry
            .property_records
            .iter()
            .flat_map(|record| {
                record.fields.iter().map(|field| {
                    (
                        format!("{}.{}", record.path, field.name),
                        field.kind.clone(),
                        field.name.clone(),
                    )
                })
            })
            .collect::<Vec<_>>();
        for (path, kind, field) in property_readers {
            registry.register_reader(&path, &kind, &field);
        }
        registry
    }

    fn visit_kind(&mut self, path: &str, kind: &FieldKind, used: &mut BTreeSet<String>) {
        match kind {
            FieldKind::Enum { values } => {
                let name = unique(type_name_from_path(path), used);
                self.inline_enums.insert(path.to_owned(), name.clone());
                self.enum_definitions.push(enum_definition(name, values));
            }
            FieldKind::Properties { name: Some(id), .. } => {
                if let Some(name) = self.named_records.get(id) {
                    self.property_types.insert(path.to_owned(), name.clone());
                }
            }
            FieldKind::Properties { fields, name: None } => {
                let name = unique(type_name_from_path(path), used);
                self.property_types.insert(path.to_owned(), name.clone());
                self.property_records
                    .push(property_record(path.to_owned(), name, fields));
                for field in fields {
                    self.visit_kind(&format!("{path}.{}", field.name), &field.kind, used);
                }
            }
            FieldKind::List { item } => {
                self.register_collection(path, kind, used);
                self.visit_kind(&format!("{path}[]"), item, used);
            }
            FieldKind::Map { value, .. } => {
                self.register_collection(path, kind, used);
                self.visit_kind(&format!("{path}{{}}"), value, used);
            }
            _ => {}
        }
    }

    fn register_collection(&mut self, path: &str, kind: &FieldKind, used: &mut BTreeSet<String>) {
        let name = unique(format!("{}Collection", type_name_from_path(path)), used);
        self.collections.insert(path.to_owned(), name.clone());
        self.collection_aliases.push(CollectionAlias {
            path: path.to_owned(),
            name,
            kind: kind.clone(),
        });
    }

    fn register_reader(&mut self, path: &str, kind: &FieldKind, field: &str) {
        if self.reader_names.contains_key(path) {
            return;
        }
        let helper = format!("read_value_{}", self.readers.len());
        self.reader_names.insert(path.to_owned(), helper.clone());
        self.readers.push(ValueReader {
            path: path.to_owned(),
            helper,
            kind: kind.clone(),
            field: field.to_owned(),
        });
        match kind {
            FieldKind::List { item } => self.register_reader(&format!("{path}[]"), item, field),
            FieldKind::Map { value, .. } => {
                self.register_reader(&format!("{path}{{}}"), value, field)
            }
            _ => {}
        }
    }

    pub(super) fn field_type(
        &self,
        kind: &FieldKind,
        path: &str,
        symbols: &HashMap<&str, &str>,
    ) -> String {
        match kind {
            FieldKind::Identifier | FieldKind::Text => "String".to_owned(),
            FieldKind::Number => "f64".to_owned(),
            FieldKind::Enum { .. } => self
                .inline_enums
                .get(path)
                .cloned()
                .unwrap_or_else(|| "GeneratedEnum".to_owned()),
            FieldKind::NamedEnum { name, .. } => self
                .named_enums
                .get(name)
                .cloned()
                .unwrap_or_else(|| "GeneratedEnum".to_owned()),
            FieldKind::Reference { target } => format!(
                "Reference<{}>",
                symbols
                    .get(target.as_str())
                    .copied()
                    .unwrap_or("UnknownTarget")
            ),
            FieldKind::Properties { .. } => self
                .property_types
                .get(path)
                .cloned()
                .unwrap_or_else(|| "GeneratedProperties".to_owned()),
            FieldKind::List { item } => self.collections.get(path).cloned().unwrap_or_else(|| {
                format!(
                    "Vec<{}>",
                    self.spanned_type(item, &format!("{path}[]"), symbols)
                )
            }),
            FieldKind::Map { value, .. } => {
                self.collections.get(path).cloned().unwrap_or_else(|| {
                    format!(
                        "BTreeMap<String, {}>",
                        self.spanned_type(value, &format!("{path}{{}}"), symbols)
                    )
                })
            }
            FieldKind::Expression { .. } => "langmeld::expression::Expression".to_owned(),
        }
    }

    pub(super) fn spanned_type(
        &self,
        kind: &FieldKind,
        path: &str,
        symbols: &HashMap<&str, &str>,
    ) -> String {
        format!("Spanned<{}>", self.field_type(kind, path, symbols))
    }
}

fn enum_definition(name: String, values: &[String]) -> EnumDefinition {
    let mut used = BTreeSet::new();
    let variants = values
        .iter()
        .map(|value| unique(pascal_case(value), &mut used))
        .collect();
    EnumDefinition {
        name,
        values: values.to_vec(),
        variants,
    }
}

fn property_record(path: String, name: String, fields: &[FieldSpec]) -> PropertyRecord {
    let mut used = BTreeSet::from([
        "span".to_owned(),
        "syntax_variant".to_owned(),
        "children".to_owned(),
    ]);
    let field_names = fields
        .iter()
        .map(|field| unique(rust_keyword_safe(snake_case(&field.name)), &mut used))
        .collect();
    PropertyRecord {
        path,
        name,
        fields: fields.to_vec(),
        field_names,
    }
}

fn type_name_from_path(path: &str) -> String {
    path.replace("[]", "Item")
        .replace("{}", "Value")
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(pascal_case)
        .collect()
}

use langmeld_core::{
    BehaviorSpec, BehaviorStage, BlockSeparator, BlockSeparatorPolicy, BlockSpec, ConstructBuilder,
    ConstructSpec, ExpressionAssociativity, ExpressionAtomKind, ExpressionOperatorSpec,
    ExpressionSpec, FieldConstraints, FieldKind, FieldSpec, LanguageBuilder, LanguageSpec,
    LexicalSpec, NamedEnum, NamedRecord, ScopeLookup, ScopeSpec, SymbolPolicySpec, SymbolSpec,
    SyntaxPattern, SyntaxRuleSpec,
};

fn valid_spec() -> LanguageSpec {
    LanguageSpec {
        schema_version: 1,
        source_extensions: Vec::new(),
        lexical: None,
        symbol_policies: Vec::new(),
        name: "Flow".to_owned(),
        version: "0.1.0".to_owned(),
        root: "flow".to_owned(),
        categories: Vec::new(),
        rules: Vec::new(),
        enums: Vec::new(),
        expressions: Vec::new(),
        types: Vec::new(),
        behaviors: Vec::new(),
        constructs: vec![
            ConstructSpec {
                id: "flow".to_owned(),
                implements: Vec::new(),
                custom_parser: None,
                syntax: vec!["flow {name} {".to_owned()],
                syntax_ids: Vec::new(),
                fields: vec![FieldSpec {
                    name: "name".to_owned(),
                    kind: FieldKind::Identifier,
                    required: true,
                    default: None,
                    constraints: FieldConstraints::default(),
                }],
                block: Some(BlockSpec {
                    allowed: vec!["node".to_owned()],
                    ..BlockSpec::default()
                }),
                symbol: None,
            },
            ConstructSpec {
                id: "node".to_owned(),
                implements: Vec::new(),
                custom_parser: None,
                syntax: vec!["node {name}".to_owned()],
                syntax_ids: Vec::new(),
                fields: vec![FieldSpec {
                    name: "name".to_owned(),
                    kind: FieldKind::Identifier,
                    required: true,
                    default: None,
                    constraints: FieldConstraints::default(),
                }],
                block: None,
                symbol: Some(SymbolSpec {
                    kind: "node".to_owned(),
                    name_field: "name".to_owned(),
                    scope: None,
                }),
            },
        ],
    }
}

#[test]
fn valid_language_definition_has_a_validated_state() {
    let result = valid_spec().validate();
    assert!(result.is_ok(), "{:#?}", result.err());
}

#[test]
fn validation_rejects_unsupported_language_model_schema_versions() {
    let mut spec = valid_spec();
    spec.schema_version = 6;

    let errors = spec
        .validate()
        .expect_err("unsupported language model schema is rejected");
    assert!(errors.iter().any(|error| {
        error.code == "LM1001" && error.message.contains("unsupported schema_version 6")
    }));
}

#[test]
fn symbol_policies_require_schema_five_and_a_declared_kind() {
    let policy = SymbolPolicySpec {
        kind: "node".to_owned(),
        declaration_order: Default::default(),
        duplicates: Default::default(),
        shadowing: Default::default(),
    };
    let mut spec = valid_spec();
    spec.symbol_policies.push(policy.clone());
    assert!(
        spec.validate()
            .unwrap_err()
            .iter()
            .any(|error| error.code == "LM1093")
    );

    let mut spec = valid_spec();
    spec.schema_version = 5;
    spec.symbol_policies.push(policy);
    assert!(spec.validate().is_ok());

    let mut spec = valid_spec();
    spec.schema_version = 5;
    spec.symbol_policies.push(SymbolPolicySpec {
        kind: "missing".to_owned(),
        declaration_order: Default::default(),
        duplicates: Default::default(),
        shadowing: Default::default(),
    });
    assert!(
        spec.validate()
            .unwrap_err()
            .iter()
            .any(|error| error.code == "LM1095")
    );
}

#[test]
fn schema_v4_accepts_and_validates_custom_comment_delimiters() {
    let mut spec = valid_spec();
    spec.schema_version = 4;
    spec.lexical = Some(
        LexicalSpec::new()
            .line_comment("//")
            .block_comment("/*", "*/"),
    );
    assert!(spec.clone().validate().is_ok());

    spec.schema_version = 3;
    assert!(
        spec.validate()
            .expect_err("custom comments require schema v4")
            .iter()
            .any(|error| error.code == "LM1091")
    );
}

#[test]
fn line_comments_can_share_a_prefix_with_a_shorter_operator() {
    let mut spec = valid_spec();
    spec.schema_version = 4;
    spec.lexical = Some(LexicalSpec::new().line_comment("//"));
    spec.expressions.push(
        ExpressionSpec::new("arithmetic")
            .atom(ExpressionAtomKind::Identifier)
            .operator(ExpressionOperatorSpec::infix(
                "/",
                10,
                ExpressionAssociativity::Left,
            )),
    );

    assert!(spec.validate().is_ok());
}

#[test]
fn comment_delimiters_reject_operators_that_they_would_hide() {
    let mut spec = valid_spec();
    spec.schema_version = 4;
    spec.lexical = Some(LexicalSpec::new().line_comment("//"));
    spec.expressions
        .push(
            ExpressionSpec::new("arithmetic").operator(ExpressionOperatorSpec::infix(
                "//=",
                10,
                ExpressionAssociativity::Left,
            )),
        );

    let errors = spec
        .validate()
        .expect_err("an operator beginning with the comment marker is unreachable");
    assert!(errors.iter().any(|error| {
        error.code == "LM1092"
            && error
                .message
                .contains("makes expression operator `//=` unreachable")
    }));
}

#[test]
fn lexical_comment_delimiters_must_be_unambiguous_punctuation() {
    for lexical in [
        LexicalSpec::new().line_comment(""),
        LexicalSpec::new().line_comment("line comment"),
        LexicalSpec::new().block_comment("/*", "/*"),
        LexicalSpec::new()
            .line_comment("//")
            .block_comment("/", "*/"),
    ] {
        let mut spec = valid_spec();
        spec.schema_version = 4;
        spec.lexical = Some(lexical);
        assert!(
            spec.validate()
                .expect_err("ambiguous comment policy is invalid")
                .iter()
                .any(|error| error.code == "LM1092")
        );
    }
}

#[test]
fn schema_v2_accepts_custom_block_delimiters_and_required_separators() {
    let mut spec = valid_spec();
    spec.schema_version = 2;
    spec.constructs[0].syntax = vec!["flow {name}".to_owned()];
    let block = spec.constructs[0]
        .block
        .as_mut()
        .expect("root block is declared");
    block.open = "begin".to_owned();
    block.close = "end".to_owned();
    block.separators = vec![BlockSeparator::literal(";")];
    block.separator_policy = BlockSeparatorPolicy::Required;

    assert!(spec.validate().is_ok());
}

#[test]
fn schema_v1_rejects_non_default_block_layout() {
    let mut spec = valid_spec();
    spec.constructs[0]
        .block
        .as_mut()
        .expect("root block is declared")
        .open = "begin".to_owned();

    let errors = spec
        .validate()
        .expect_err("schema v1 cannot carry non-default block configuration");
    assert!(
        errors.iter().any(|error| {
            error.code == "LM1071" && error.message.contains("schema_version to 2")
        })
    );
}

#[test]
fn schema_v2_block_conflicts_include_the_owned_opener() {
    let mut spec = LanguageBuilder::new("BlockOpeners")
        .version("0.1.0")
        .schema_version(2)
        .root("root")
        .construct(
            ConstructBuilder::new("root")
                .syntax("root")
                .block(["group_a", "group_b"]),
        )
        .construct(
            ConstructBuilder::new("group_a")
                .syntax("group")
                .block(["leaf"])
                .block_delimiters("begin", "end"),
        )
        .construct(
            ConstructBuilder::new("group_b")
                .syntax("group")
                .block(["leaf"])
                .block_delimiters("start", "finish"),
        )
        .construct(ConstructBuilder::new("leaf").syntax("leaf"))
        .into_spec();
    assert!(spec.clone().validate().is_ok());

    spec.constructs[2]
        .block
        .as_mut()
        .expect("group b has a block")
        .open = "begin".to_owned();
    let errors = spec
        .validate()
        .expect_err("matching block openers leave the child syntax ambiguous");
    assert!(
        errors
            .iter()
            .any(|error| { error.code == "LM1060" && error.message.contains("can both match") })
    );
}

#[test]
fn schema_v2_rejects_ambiguous_and_incomplete_block_layouts() {
    let mut spec = valid_spec();
    spec.schema_version = 2;
    let block = spec.constructs[0]
        .block
        .as_mut()
        .expect("root block is declared");
    block.open = "begin".to_owned();
    block.close = "begin".to_owned();
    block.separators = vec![
        BlockSeparator::literal("begin"),
        BlockSeparator::literal("begin"),
    ];
    block.separator_policy = BlockSeparatorPolicy::Required;
    let errors = spec
        .validate()
        .expect_err("equal delimiters and duplicate/conflicting separators are invalid");
    for code in ["LM1073", "LM1076", "LM1077"] {
        assert!(errors.iter().any(|error| error.code == code), "{errors:#?}");
    }

    let mut spec = valid_spec();
    spec.schema_version = 2;
    let block = spec.constructs[0]
        .block
        .as_mut()
        .expect("root block is declared");
    block.separator_policy = BlockSeparatorPolicy::Required;
    block.separators.clear();
    let errors = spec
        .validate()
        .expect_err("required policy needs at least one separator");
    assert!(errors.iter().any(|error| error.code == "LM1074"));
}

#[test]
fn construct_builder_accumulates_categories_across_composition_steps() {
    let language = LanguageBuilder::new("ComposableCategories")
        .version("0.1.0")
        .root("root")
        .category("statement")
        .category("expression")
        .construct(
            ConstructBuilder::new("root")
                .syntax("root {")
                .block(["statement"]),
        )
        .construct(
            ConstructBuilder::new("node")
                .syntax("node")
                .implement("statement")
                .implements(["expression"]),
        )
        .build()
        .expect("categories added in separate steps form one valid construct");

    assert_eq!(
        language.spec().constructs[1].implements,
        ["statement", "expression"]
    );
}

#[test]
fn language_builder_appends_construct_groups_in_order() {
    let language = LanguageBuilder::new("GroupedConstructs")
        .version("0.1.0")
        .root("root")
        .category("Item")
        .constructs([
            ConstructBuilder::new("root")
                .syntax("root {")
                .block(["Item"]),
            ConstructBuilder::new("statement")
                .syntax("statement")
                .implement("Item"),
        ])
        .build()
        .expect("a grouped builder input produces a valid language");

    assert_eq!(
        language
            .spec()
            .constructs
            .iter()
            .map(|construct| construct.id.as_str())
            .collect::<Vec<_>>(),
        ["root", "statement"]
    );
}

#[test]
fn language_builder_composes_declaration_groups_in_order() {
    let visibility = FieldKind::named_enumeration("Visibility", ["public", "private"]);
    let spec = LanguageBuilder::new("GroupedDeclarations")
        .version("0.1.0")
        .root("root")
        .source_extensions([".one", ".two"])
        .categories(["Statement", "Expression"])
        .rules([
            SyntaxRuleSpec {
                id: "identifier".to_owned(),
                parameters: Vec::new(),
                pattern: "{value}".to_owned(),
            },
            SyntaxRuleSpec {
                id: "named_identifier".to_owned(),
                parameters: vec!["prefix".to_owned()],
                pattern: "{prefix} {value}".to_owned(),
            },
        ])
        .named_enums([
            NamedEnum::new("Visibility", ["public", "private"]),
            NamedEnum::new("Mutability", ["mutable", "immutable"]),
        ])
        .record_types([
            NamedRecord::new("Position", [FieldSpec::required("line", FieldKind::Number)]),
            NamedRecord::new(
                "Range",
                [
                    FieldSpec::required("start", FieldKind::Number),
                    FieldSpec::required("end", FieldKind::Number),
                ],
            ),
        ])
        .behaviors([
            BehaviorSpec {
                id: "validate_file".to_owned(),
                stage: BehaviorStage::SemanticAnalysis,
            },
            BehaviorSpec {
                id: "resolve_symbols".to_owned(),
                stage: BehaviorStage::SymbolResolution,
            },
        ])
        .constructs([
            ConstructBuilder::new("root")
                .syntax("root {")
                .block(["Statement"]),
            ConstructBuilder::new("item")
                .syntax("item {visibility}")
                .field(FieldSpec::required("visibility", visibility))
                .implement("Statement"),
        ])
        .into_spec();
    spec.clone()
        .validate()
        .expect("each declaration family can be composed as an ordered group");

    assert_eq!(spec.source_extensions, [".one", ".two"]);
    assert_eq!(
        spec.categories
            .iter()
            .map(|category| category.id.as_str())
            .collect::<Vec<_>>(),
        ["Statement", "Expression"]
    );
    assert_eq!(
        spec.rules
            .iter()
            .map(|rule| rule.id.as_str())
            .collect::<Vec<_>>(),
        ["identifier", "named_identifier"]
    );
    assert_eq!(
        spec.enums
            .iter()
            .map(|enumeration| enumeration.id.as_str())
            .collect::<Vec<_>>(),
        ["Visibility", "Mutability"]
    );
    assert_eq!(
        spec.types
            .iter()
            .map(|record| record.id.as_str())
            .collect::<Vec<_>>(),
        ["Position", "Range"]
    );
    assert_eq!(
        spec.behaviors
            .iter()
            .map(|behavior| behavior.id.as_str())
            .collect::<Vec<_>>(),
        ["validate_file", "resolve_symbols"]
    );
}

#[test]
fn construct_builder_composes_syntax_and_field_groups_in_order() {
    let construct: ConstructSpec = ConstructBuilder::new("function")
        .syntax_variants([
            ("named", "fn {name}()"),
            ("typed", "fn {name}() -> {return_type}"),
        ])
        .fields([
            FieldSpec::required("name", FieldKind::Identifier),
            FieldSpec::optional("return_type", FieldKind::Identifier),
        ])
        .into();

    assert_eq!(construct.syntax_ids, ["named", "typed"]);
    assert_eq!(
        construct.syntax,
        ["fn {name}()", "fn {name}() -> {return_type}"]
    );
    assert_eq!(
        construct
            .fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        ["name", "return_type"]
    );

    let unnamed: ConstructSpec = ConstructBuilder::new("operator")
        .syntax_templates(["+ {value}", "- {value}"])
        .into();
    assert_eq!(unnamed.syntax, ["+ {value}", "- {value}"]);
    assert!(unnamed.syntax_ids.is_empty());
}

#[test]
fn syntax_pattern_composes_to_the_canonical_template_string() {
    let pattern = SyntaxPattern::literal("fn")
        .then(SyntaxPattern::capture("name"))
        .then(SyntaxPattern::literal("("))
        .then(SyntaxPattern::capture("parameters").comma_separated())
        .then(SyntaxPattern::literal(")"))
        .then(SyntaxPattern::choice([
            SyntaxPattern::literal("-> i32"),
            SyntaxPattern::literal("-> bool"),
        ]))
        .then(SyntaxPattern::literal("{"));
    let expected = r"fn {name} \( ({parameters}[,])* \) (-> i32|-> bool) \{";

    assert_eq!(pattern.as_str(), expected);
    assert_eq!(
        SyntaxPattern::capture("items").separated_by("; ").as_str(),
        "({items}[; ])*"
    );
    let unnamed: ConstructSpec = ConstructBuilder::new("unnamed")
        .syntax_pattern(pattern.clone())
        .into();
    assert_eq!(unnamed.syntax, [expected]);

    let language = LanguageBuilder::new("PatternBuilder")
        .version("0.1.0")
        .root("function")
        .construct(
            ConstructBuilder::new("function")
                .syntax_variant_pattern("signature", pattern)
                .field(FieldSpec::required("name", FieldKind::Identifier))
                .field(FieldSpec::optional(
                    "parameters",
                    FieldKind::List {
                        item: Box::new(FieldKind::Identifier),
                    },
                ))
                .block(std::iter::empty::<&str>()),
        )
        .build()
        .expect("composed pattern validates against declared fields");

    assert_eq!(
        language
            .construct("function")
            .expect("function exists")
            .syntax,
        [expected]
    );
    assert_eq!(
        language
            .construct("function")
            .expect("function exists")
            .syntax_ids,
        ["signature"]
    );
}

#[test]
fn optional_fields_can_have_defaults_without_becoming_required() {
    let mut spec = valid_spec();
    spec.constructs[1]
        .fields
        .push(FieldSpec::optional("label", FieldKind::Text).default_value("untitled"));

    let validated = spec
        .validate()
        .expect("an optional field can supply a fallback when omitted");
    let label = &validated.spec().constructs[1].fields[1];

    assert!(!label.required);
    assert_eq!(label.default.as_deref(), Some("untitled"));
}

#[test]
fn fluent_language_builder_exposes_the_schema_version() {
    let language = LanguageBuilder::new("SchemaBuilder")
        .schema_version(1)
        .version("0.1.0")
        .root("root")
        .construct(
            ConstructBuilder::new("root")
                .syntax("root {")
                .block(std::iter::empty::<&str>()),
        )
        .build()
        .expect("schema version 1 is supported");

    assert_eq!(language.spec().schema_version, 1);
}

#[test]
fn named_enum_json_roundtrip_preserves_type_identity_without_field_name_collision() {
    let visibility = FieldKind::named_enumeration("Visibility", ["public", "private"]);
    let spec = LanguageBuilder::new("NamedEnums")
        .version("0.1.0")
        .root("root")
        .named_enum("Visibility", ["public", "private"])
        .construct(
            ConstructBuilder::new("root")
                .syntax("root {visibility}[ roles {roles}] {")
                .field(FieldSpec::required("visibility", visibility.clone()))
                .field(FieldSpec::optional("roles", FieldKind::list(visibility)))
                .block(std::iter::empty::<&str>()),
        )
        .build()
        .expect("named enum declarations validate");

    let json = serde_json::to_value(spec.spec()).expect("language spec serializes");
    let fields = &json["constructs"][0]["fields"];
    assert_eq!(fields[0]["name"], "visibility");
    assert_eq!(fields[0]["kind"], "named_enum");
    assert_eq!(fields[0]["enum_name"], "Visibility");
    assert_eq!(fields[1]["item"]["enum_name"], "Visibility");
    let decoded: LanguageSpec = serde_json::from_value(json).expect("named enum JSON decodes");
    assert_eq!(&decoded, spec.spec());
}

#[test]
fn language_builder_can_return_a_spec_for_later_composition_and_validation() {
    let spec = LanguageBuilder::new("DeferredValidation")
        .version("0.1.0")
        .root("root")
        .construct(
            ConstructBuilder::new("root")
                .syntax("root {")
                .block(std::iter::empty::<&str>()),
        )
        .into_spec();

    assert_eq!(spec.name, "DeferredValidation");
    assert_eq!(
        spec.clone()
            .validate()
            .expect("the built spec is valid")
            .spec(),
        &spec
    );
}

#[test]
fn fluent_symbol_and_scope_builders_cover_scoped_resolution() {
    let symbol = SymbolSpec::new("variable", "name")
        .with_scope(ScopeSpec::new("function", "name").lookup(ScopeLookup::LexicalParents));

    assert_eq!(symbol.kind, "variable");
    assert_eq!(symbol.name_field, "name");
    assert_eq!(
        symbol.scope,
        Some(ScopeSpec {
            construct: "function".to_owned(),
            name_field: "name".to_owned(),
            lookup: ScopeLookup::LexicalParents,
        })
    );
    assert_eq!(
        ScopeSpec::new("module", "name").lookup(ScopeLookup::Exact),
        ScopeSpec::new("module", "name")
    );
}

#[test]
fn fluent_language_builder_normalizes_source_filename_suffixes() {
    let language = LanguageBuilder::new("FileTypes")
        .version("0.1.0")
        .root("root")
        .source_extension("calc")
        .source_extension(".d.ts")
        .construct(
            ConstructBuilder::new("root")
                .syntax("root {")
                .block(std::iter::empty::<&str>()),
        )
        .build()
        .expect("source filename suffixes are valid");

    assert_eq!(language.spec().source_extensions, [".calc", ".d.ts"]);
}

#[test]
fn source_filename_suffixes_must_be_unique_and_path_free() {
    let duplicate = LanguageBuilder::new("DuplicateSuffix")
        .version("0.1.0")
        .root("root")
        .source_extension("calc")
        .source_extension(".calc")
        .construct(
            ConstructBuilder::new("root")
                .syntax("root {")
                .block(std::iter::empty::<&str>()),
        )
        .build()
        .expect_err("normalized duplicates are rejected");
    assert!(duplicate.iter().any(|error| error.code == "LM1062"));

    let path = LanguageBuilder::new("PathSuffix")
        .version("0.1.0")
        .root("root")
        .source_extension("../calc")
        .construct(
            ConstructBuilder::new("root")
                .syntax("root {")
                .block(std::iter::empty::<&str>()),
        )
        .build()
        .expect_err("path traversal is not a filename suffix");
    assert!(path.iter().any(|error| error.code == "LM1061"));
}

#[test]
fn validation_rejects_colliding_syntax_realizations_between_allowed_siblings() {
    let mut direct = valid_spec();
    direct.constructs[1].syntax = vec!["node {name}[ at ({x}, {y})]".to_owned()];
    direct.constructs[1]
        .fields
        .push(FieldSpec::optional("x", FieldKind::Number));
    direct.constructs[1]
        .fields
        .push(FieldSpec::optional("y", FieldKind::Number));
    let mut service = direct.constructs[1].clone();
    service.id = "service".to_owned();
    service.syntax = vec!["node {name}".to_owned()];
    service.symbol = None;
    direct.constructs[0]
        .block
        .as_mut()
        .expect("root has a block")
        .allowed
        .push("service".to_owned());
    direct.constructs.push(service);

    let errors = direct
        .validate()
        .expect_err("identical allowed sibling syntax is ambiguous");
    assert!(
        errors.iter().any(|error| error.code == "LM1060"),
        "expected sibling-pattern conflict, got {errors:?}"
    );

    let mut categorized = valid_spec();
    categorized.categories.push(langmeld_core::CategorySpec {
        id: "Element".to_owned(),
    });
    categorized.constructs[0]
        .block
        .as_mut()
        .expect("root has a block")
        .allowed = vec!["Element".to_owned()];
    categorized.constructs[1].implements = vec!["Element".to_owned()];
    let mut extension = categorized.constructs[1].clone();
    extension.id = "service".to_owned();
    extension.implements = vec!["Element".to_owned()];
    extension.symbol = None;
    categorized.constructs.push(extension);

    let errors = categorized
        .validate()
        .expect_err("category-provided siblings must be checked too");
    assert!(
        errors.iter().any(|error| error.code == "LM1060"),
        "expected category sibling-pattern conflict, got {errors:?}"
    );

    let mut overlapping = valid_spec();
    overlapping.constructs[1].symbol = None;
    let mut service = overlapping.constructs[1].clone();
    service.id = "service".to_owned();
    service.syntax = vec!["node ({kind}|api {kind})".to_owned()];
    service.fields[0].name = "kind".to_owned();
    service.fields[0].kind = FieldKind::Enum {
        values: vec!["api".to_owned()],
    };
    overlapping.constructs[0]
        .block
        .as_mut()
        .expect("root has a block")
        .allowed
        .push("service".to_owned());
    overlapping.constructs.push(service);
    assert!(
        overlapping
            .validate()
            .is_err_and(|errors| errors.iter().any(|error| error.code == "LM1060")),
        "overlapping identifier and enum captures can create a parse-time tie"
    );

    let mut literal_overlap = valid_spec();
    let service = ConstructSpec {
        id: "service".to_owned(),
        implements: Vec::new(),
        custom_parser: None,
        syntax: vec!["node api".to_owned()],
        syntax_ids: Vec::new(),
        fields: Vec::new(),
        block: None,
        symbol: None,
    };
    literal_overlap.constructs[0]
        .block
        .as_mut()
        .expect("root has a block")
        .allowed
        .push("service".to_owned());
    literal_overlap.constructs.push(service);
    assert!(
        literal_overlap
            .validate()
            .is_err_and(|errors| errors.iter().any(|error| error.code == "LM1060")),
        "a literal token can overlap an identifier capture"
    );

    let mut disjoint = valid_spec();
    disjoint.constructs[1].symbol = None;
    disjoint.constructs[1].syntax = vec!["node {name}".to_owned()];
    disjoint.constructs[1].fields[0].kind = FieldKind::Enum {
        values: vec!["red".to_owned()],
    };
    let mut service = disjoint.constructs[1].clone();
    service.id = "service".to_owned();
    service.syntax = vec!["node {kind}".to_owned()];
    service.fields[0].name = "kind".to_owned();
    service.fields[0].kind = FieldKind::Enum {
        values: vec!["blue".to_owned()],
    };
    disjoint.constructs[0]
        .block
        .as_mut()
        .expect("root has a block")
        .allowed
        .push("service".to_owned());
    disjoint.constructs.push(service);
    assert!(
        disjoint.validate().is_ok(),
        "disjoint enum captures cannot create a parse-time tie"
    );
}

#[test]
fn validation_checks_repeated_pattern_overlaps_beyond_representative_lengths() {
    let mut repeated = valid_spec();
    repeated.constructs[1].symbol = None;
    repeated.constructs[1].syntax = vec!["many {first} {second} {third} {fourth}".to_owned()];
    repeated.constructs[1].fields = ["first", "second", "third", "fourth"]
        .into_iter()
        .map(|name| FieldSpec::required(name, FieldKind::Identifier))
        .collect();
    let mut service = repeated.constructs[1].clone();
    service.id = "service".to_owned();
    service.syntax = vec!["many ({aliases})*".to_owned()];
    service.fields = vec![FieldSpec::optional(
        "aliases",
        FieldKind::List {
            item: Box::new(FieldKind::Identifier),
        },
    )];
    service.symbol = None;
    repeated.constructs[0]
        .block
        .as_mut()
        .expect("root has a block")
        .allowed
        .push("service".to_owned());
    repeated.constructs.push(service);

    assert!(
        repeated
            .validate()
            .is_err_and(|errors| errors.iter().any(|error| error.code == "LM1060")),
        "four repeated identifiers can overlap four ordinary identifier captures"
    );

    let mut disjoint = valid_spec();
    disjoint.constructs[1].symbol = None;
    disjoint.constructs[1].syntax = vec!["node blue".to_owned()];
    disjoint.constructs[1].fields.clear();
    let mut service = disjoint.constructs[1].clone();
    service.id = "service".to_owned();
    service.syntax = vec!["node ({kind})*".to_owned()];
    service.fields = vec![FieldSpec::optional(
        "kind",
        FieldKind::List {
            item: Box::new(FieldKind::Enum {
                values: vec!["red".to_owned()],
            }),
        },
    )];
    disjoint.constructs[0]
        .block
        .as_mut()
        .expect("root has a block")
        .allowed
        .push("service".to_owned());
    disjoint.constructs.push(service);
    let result = disjoint.validate();
    assert!(
        result.is_ok(),
        "a repeated disjoint enum does not overlap the sibling literal: {result:?}"
    );
}

#[test]
fn validation_models_repeated_comma_separators_when_checking_ambiguity() {
    let mut spec = valid_spec();
    spec.constructs[1].symbol = None;
    spec.constructs[1].syntax = vec!["many {first} {second}".to_owned()];
    spec.constructs[1].fields = vec![
        FieldSpec::required("first", FieldKind::Identifier),
        FieldSpec::required("second", FieldKind::Identifier),
    ];
    let mut repeated = spec.constructs[1].clone();
    repeated.id = "repeated".to_owned();
    repeated.syntax = vec!["many ({items}[,])*".to_owned()];
    repeated.fields = vec![FieldSpec::optional(
        "items",
        FieldKind::List {
            item: Box::new(FieldKind::Identifier),
        },
    )];
    spec.constructs[0]
        .block
        .as_mut()
        .expect("root has a block")
        .allowed
        .push("repeated".to_owned());
    spec.constructs.push(repeated);

    assert!(
        spec.clone().validate().is_ok(),
        "a repeated list requiring commas cannot match adjacent captures without commas"
    );

    spec.constructs[1].syntax = vec!["many {first}, {second}".to_owned()];
    assert!(
        spec.clone()
            .validate()
            .is_err_and(|errors| errors.iter().any(|error| error.code == "LM1060")),
        "the same repeated pattern overlaps when the sibling also requires a comma"
    );

    spec.constructs[1].syntax = vec!["many {first} {second}".to_owned()];
    spec.constructs[2].syntax = vec!["many ({items}[;])*".to_owned()];
    assert!(
        spec.clone().validate().is_ok(),
        "the generic separator is honored while checking repeated patterns"
    );
    spec.constructs[1].syntax = vec!["many {first}; {second}".to_owned()];
    assert!(
        spec.validate()
            .is_err_and(|errors| errors.iter().any(|error| error.code == "LM1060")),
        "a sibling matching the repeated literal separator is detected as ambiguous"
    );
}

#[test]
fn custom_parser_can_own_layout_and_syntaxless_child_constructs() {
    let mut spec = valid_spec();
    spec.constructs[0].custom_parser = Some("layout".to_owned());
    spec.constructs[0].syntax = vec!["flow {name}:".to_owned()];
    spec.constructs[1].syntax.clear();

    assert!(spec.validate().is_ok());
}

#[test]
fn custom_parser_root_without_generic_children_does_not_require_a_block() {
    let mut spec = valid_spec();
    spec.constructs[0].custom_parser = Some("parse_file".to_owned());
    spec.constructs[0].block = None;

    assert!(spec.validate().is_ok());
}

#[test]
fn custom_parser_can_own_recursive_layout_blocks_without_brace_patterns() {
    let mut spec = valid_spec();
    spec.constructs[0].custom_parser = Some("layout".to_owned());
    spec.constructs[0].block = Some(BlockSpec {
        allowed: vec!["node".to_owned(), "if".to_owned()],
        ..BlockSpec::default()
    });
    spec.constructs.push(ConstructSpec {
        id: "if".to_owned(),
        implements: Vec::new(),
        custom_parser: None,
        syntax: vec!["if {condition}:".to_owned()],
        syntax_ids: Vec::new(),
        fields: vec![FieldSpec {
            name: "condition".to_owned(),
            kind: FieldKind::Text,
            required: true,
            default: None,
            constraints: FieldConstraints::default(),
        }],
        block: Some(BlockSpec {
            allowed: vec!["node".to_owned(), "if".to_owned()],
            ..BlockSpec::default()
        }),
        symbol: None,
    });

    assert!(spec.validate().is_ok());
}

#[test]
fn validation_rejects_unknown_captured_fields() {
    let mut spec = valid_spec();
    spec.constructs[1].syntax = vec!["node {missing}".to_owned()];

    let errors = spec.validate().expect_err("unknown field must be rejected");
    assert!(errors.iter().any(|error| error.code == "LM1008"));
}

#[test]
fn validation_rejects_duplicate_enum_values() {
    let mut spec = valid_spec();
    spec.constructs[1].fields.push(FieldSpec {
        name: "flavor".to_owned(),
        kind: FieldKind::Enum {
            values: vec!["light".to_owned(), "light".to_owned()],
        },
        required: false,
        default: None,
        constraints: FieldConstraints::default(),
    });
    spec.constructs[1]
        .syntax
        .push("node {name} {flavor}".to_owned());

    let errors = spec
        .validate()
        .expect_err("duplicate values must be rejected");
    assert!(errors.iter().any(|error| error.code == "LM1019"));
}

#[test]
fn validation_rejects_defaults_that_do_not_match_the_field_type() {
    let mut spec = valid_spec();
    spec.constructs[1].fields.push(FieldSpec {
        name: "flavor".to_owned(),
        kind: FieldKind::Enum {
            values: vec!["light".to_owned(), "dark".to_owned()],
        },
        required: true,
        default: Some("purple".to_owned()),
        constraints: FieldConstraints::default(),
    });

    let errors = spec
        .validate()
        .expect_err("enum default must be one of the allowed values");
    assert!(errors.iter().any(|error| error.code == "LM1025"));
}

#[test]
fn validation_rejects_defaults_for_symbol_names() {
    let mut spec = valid_spec();
    spec.constructs[1].fields[0].default = Some("fallback".to_owned());

    let errors = spec
        .validate()
        .expect_err("a default must not invent a declaration name");
    assert!(errors.iter().any(|error| error.code == "LM1026"));
}

#[test]
fn validation_accepts_reference_list_items() {
    let mut spec = valid_spec();
    spec.constructs[1].fields.push(FieldSpec {
        name: "links".to_owned(),
        kind: FieldKind::List {
            item: Box::new(FieldKind::Reference {
                target: "node".to_owned(),
            }),
        },
        required: false,
        default: None,
        constraints: FieldConstraints::default(),
    });

    assert!(spec.validate().is_ok());
}

#[test]
fn validation_accepts_reference_map_values() {
    let mut spec = valid_spec();
    spec.constructs[1].fields.push(FieldSpec {
        name: "links".to_owned(),
        kind: FieldKind::Map {
            key: langmeld_core::MapKeyKind::Identifier,
            value: Box::new(FieldKind::Reference {
                target: "node".to_owned(),
            }),
        },
        required: false,
        default: None,
        constraints: FieldConstraints::default(),
    });

    assert!(spec.validate().is_ok());
}

#[test]
fn validation_requires_repeated_captures_to_use_list_fields() {
    let mut spec = valid_spec();
    spec.constructs[1].syntax = vec!["node {name} ({x})+".to_owned()];
    spec.constructs[1]
        .fields
        .push(FieldSpec::required("x", FieldKind::Number));

    let errors = spec
        .validate()
        .expect_err("a repeated capture must collect into a list field");
    assert!(errors.iter().any(|error| error.code == "LM1050"));
}

#[test]
fn validation_rejects_required_list_fields_in_zero_or_more_groups() {
    let mut spec = valid_spec();
    spec.constructs[1].syntax = vec!["node {name} ({aliases})*".to_owned()];
    spec.constructs[1].fields.push(FieldSpec::required(
        "aliases",
        FieldKind::List {
            item: Box::new(FieldKind::Identifier),
        },
    ));

    let errors = spec
        .validate()
        .expect_err("zero-or-more syntax cannot guarantee a required list");
    assert!(errors.iter().any(|error| error.code == "LM1009"));
}

#[test]
fn validation_rejects_empty_or_inconsistent_choice_branches() {
    for pattern in ["node {name} (plain|)", "node {name} ({left}|{right})"] {
        let mut spec = valid_spec();
        spec.constructs[1].syntax = vec![pattern.to_owned()];
        let errors = spec
            .validate()
            .expect_err("choice groups must be nonempty and expose one field shape");
        assert!(
            errors.iter().any(|error| error.code == "LM1010"),
            "expected invalid pattern diagnostic for `{pattern}`, got {errors:?}"
        );
    }
}

#[test]
fn validation_rejects_unsupported_escapes_and_unmatchable_tokens() {
    for pattern in [r"node {name} \q", "node {name} \"", "node {name} #"] {
        let mut spec = valid_spec();
        spec.constructs[1].syntax = vec![pattern.to_owned()];
        let errors = spec
            .validate()
            .expect_err("only documented pattern escapes and source tokens are accepted");
        assert!(
            errors.iter().any(|error| error.code == "LM1010"),
            "expected invalid pattern diagnostic for `{pattern}`, got {errors:?}"
        );
    }
}

#[test]
fn validation_accepts_a_named_ancestor_scope() {
    let mut spec = valid_spec();
    spec.constructs[1]
        .symbol
        .as_mut()
        .expect("node declares symbols")
        .scope = Some(ScopeSpec {
        construct: "flow".to_owned(),
        name_field: "name".to_owned(),
        lookup: ScopeLookup::Exact,
    });

    assert!(spec.validate().is_ok());
}

#[test]
fn scope_lookup_roundtrips_with_legacy_exact_default() {
    let mut spec = valid_spec();
    spec.constructs[1]
        .symbol
        .as_mut()
        .expect("node declares symbols")
        .scope = Some(ScopeSpec {
        construct: "flow".to_owned(),
        name_field: "name".to_owned(),
        lookup: ScopeLookup::Exact,
    });
    let exact_json = serde_json::to_value(&spec).expect("spec serializes");
    assert!(exact_json["constructs"][1]["symbol"]["scope"]["lookup"].is_null());

    spec.constructs[1]
        .symbol
        .as_mut()
        .expect("node declares symbols")
        .scope
        .as_mut()
        .expect("node has a scope")
        .lookup = ScopeLookup::LexicalParents;
    let lexical_json = serde_json::to_value(&spec).expect("spec serializes");
    assert_eq!(
        lexical_json["constructs"][1]["symbol"]["scope"]["lookup"],
        "lexical_parents"
    );
    let restored: LanguageSpec = serde_json::from_value(lexical_json).expect("spec deserializes");
    assert_eq!(restored, spec);
}

#[test]
fn validation_accepts_categories_as_reusable_block_constraints() {
    let mut spec = valid_spec();
    spec.categories.push(langmeld_core::CategorySpec {
        id: "Element".to_owned(),
    });
    spec.constructs[0].block.as_mut().unwrap().allowed = vec!["Element".to_owned()];
    spec.constructs[1].implements = vec!["Element".to_owned()];

    assert!(spec.validate().is_ok());
}

#[test]
fn validation_rejects_unknown_implemented_categories_and_block_categories() {
    let mut spec = valid_spec();
    spec.constructs[0].block.as_mut().unwrap().allowed = vec!["Missing".to_owned()];
    spec.constructs[1].implements = vec!["Missing".to_owned()];

    let errors = spec
        .validate()
        .expect_err("unknown categories are rejected");
    assert!(errors.iter().any(|error| error.code == "LM1015"));
    assert!(errors.iter().any(|error| error.code == "LM1042"));
}

#[test]
fn validation_rejects_an_unknown_scope_construct() {
    let mut spec = valid_spec();
    spec.constructs[1]
        .symbol
        .as_mut()
        .expect("node declares symbols")
        .scope = Some(ScopeSpec {
        construct: "missing".to_owned(),
        name_field: "name".to_owned(),
        lookup: ScopeLookup::Exact,
    });

    let errors = spec
        .validate()
        .expect_err("scope construct must be declared");
    assert!(errors.iter().any(|error| error.code == "LM1027"));
}

#[test]
fn validation_rejects_required_captures_inside_optional_syntax() {
    let mut spec = valid_spec();
    spec.constructs[1].syntax = vec!["node {name}[ at {x}]".to_owned()];
    spec.constructs[1].fields.push(FieldSpec {
        name: "x".to_owned(),
        kind: FieldKind::Number,
        required: true,
        default: None,
        constraints: FieldConstraints::default(),
    });

    let errors = spec
        .validate()
        .expect_err("required field cannot only appear in an optional fragment");
    assert!(errors.iter().any(|error| error.code == "LM1009"));
}

#[test]
fn validation_rejects_malformed_optional_syntax_groups() {
    for syntax in ["node {name}[]", "node {name}[ at"] {
        let mut spec = valid_spec();
        spec.constructs[1].syntax = vec![syntax.to_owned()];

        let errors = spec
            .validate()
            .expect_err("optional groups must be nonempty and closed");
        assert!(errors.iter().any(|error| error.code == "LM1010"));
    }
}

#[test]
fn validation_expands_nested_named_syntax_rules() {
    let mut spec = valid_spec();
    spec.rules = vec![
        langmeld_core::SyntaxRuleSpec {
            id: "node_keyword".to_owned(),
            parameters: Vec::new(),
            pattern: "node".to_owned(),
        },
        langmeld_core::SyntaxRuleSpec {
            id: "node_declaration".to_owned(),
            parameters: Vec::new(),
            pattern: "@node_keyword {name}".to_owned(),
        },
    ];
    spec.constructs[1].syntax = vec!["@node_declaration".to_owned()];

    let validated = spec.validate().expect("nested rules validate");
    assert_eq!(
        validated.construct("node").unwrap().syntax,
        vec!["node {name}".to_owned()]
    );
}

#[test]
fn validation_expands_parameterized_syntax_rules_to_consumer_fields() {
    let mut spec = valid_spec();
    spec.rules = vec![langmeld_core::SyntaxRuleSpec {
        id: "coordinate".to_owned(),
        parameters: vec!["horizontal".to_owned(), "vertical".to_owned()],
        pattern: "({horizontal}, {vertical})".to_owned(),
    }];
    let node = &mut spec.constructs[1];
    node.syntax = vec!["point @coordinate(x, y)".to_owned()];
    node.fields = vec![
        FieldSpec {
            name: "x".to_owned(),
            kind: FieldKind::Number,
            required: true,
            default: None,
            constraints: FieldConstraints::default(),
        },
        FieldSpec {
            name: "y".to_owned(),
            kind: FieldKind::Number,
            required: true,
            default: None,
            constraints: FieldConstraints::default(),
        },
    ];
    node.symbol = None;

    let validated = spec.validate().expect("parameterized rules validate");
    assert_eq!(
        validated.construct("node").unwrap().syntax,
        vec!["point ({x}, {y})".to_owned()]
    );
}

#[test]
fn validation_reports_parameterized_rule_arity_errors() {
    let mut spec = valid_spec();
    spec.rules = vec![langmeld_core::SyntaxRuleSpec {
        id: "pair".to_owned(),
        parameters: vec!["left".to_owned(), "right".to_owned()],
        pattern: "{left}, {right}".to_owned(),
    }];
    spec.constructs[1].syntax = vec!["node @pair(name)".to_owned()];

    let errors = spec
        .validate()
        .expect_err("incorrect rule arity is rejected");
    assert!(errors.iter().any(|error| {
        error.code == "LM1051" && error.message.contains("expects 2 arguments but received 1")
    }));
}

#[test]
fn validation_rejects_unknown_and_cyclic_syntax_rules() {
    let mut unknown = valid_spec();
    unknown.constructs[1].syntax = vec!["node {name} @missing".to_owned()];
    let errors = unknown
        .validate()
        .expect_err("unknown syntax rules are rejected");
    assert!(errors.iter().any(|error| error.code == "LM1051"));

    let mut cyclic = valid_spec();
    cyclic.rules = vec![
        langmeld_core::SyntaxRuleSpec {
            id: "first".to_owned(),
            parameters: Vec::new(),
            pattern: "@second".to_owned(),
        },
        langmeld_core::SyntaxRuleSpec {
            id: "second".to_owned(),
            parameters: Vec::new(),
            pattern: "@first".to_owned(),
        },
    ];
    let errors = cyclic
        .validate()
        .expect_err("cyclic syntax rules are rejected");
    assert!(errors.iter().any(|error| error.code == "LM1051"));

    let mut malformed = valid_spec();
    malformed.rules.push(langmeld_core::SyntaxRuleSpec {
        id: "broken".to_owned(),
        parameters: Vec::new(),
        pattern: "[unfinished".to_owned(),
    });
    let errors = malformed
        .validate()
        .expect_err("malformed syntax rules are rejected even when unused");
    assert!(errors.iter().any(|error| error.code == "LM1052"));
}

#[test]
fn validation_checks_named_record_type_ids_and_fields() {
    let mut spec = valid_spec();
    spec.types = vec![langmeld_core::TypeSpec {
        id: "Artifact".to_owned(),
        fields: vec![FieldSpec {
            name: "checksum".to_owned(),
            kind: FieldKind::Text,
            required: false,
            default: None,
            constraints: FieldConstraints::default(),
        }],
    }];
    assert!(spec.clone().validate().is_ok());

    spec.types.push(spec.types[0].clone());
    let errors = spec
        .validate()
        .expect_err("duplicate named types are rejected");
    assert!(errors.iter().any(|error| error.code == "LM1054"));

    let mut empty = valid_spec();
    empty.types.push(langmeld_core::TypeSpec {
        id: "Empty".to_owned(),
        fields: Vec::new(),
    });
    let errors = empty
        .validate()
        .expect_err("empty named types are rejected");
    assert!(errors.iter().any(|error| error.code == "LM1055"));
}

#[test]
fn named_record_usages_must_match_declared_type_schemas() {
    let mut spec = valid_spec();
    let fields = vec![FieldSpec::required("checksum", FieldKind::Text)];
    spec.types.push(langmeld_core::TypeSpec {
        id: "Artifact".to_owned(),
        fields: fields.clone(),
    });
    spec.constructs[0].fields[0].kind = FieldKind::Properties {
        name: Some("Artifact".to_owned()),
        fields: fields.clone(),
    };
    assert!(spec.clone().validate().is_ok());

    spec.constructs[0].fields[0].kind = FieldKind::Properties {
        name: Some("Missing".to_owned()),
        fields: fields.clone(),
    };
    let errors = spec
        .clone()
        .validate()
        .expect_err("unknown named records are rejected");
    assert!(errors.iter().any(|error| error.code == "LM1058"));

    spec.constructs[0].fields[0].kind = FieldKind::Properties {
        name: Some("Artifact".to_owned()),
        fields: vec![FieldSpec::required("wrong", FieldKind::Text)],
    };
    let errors = spec
        .validate()
        .expect_err("named record shape mismatches are rejected");
    assert!(errors.iter().any(|error| error.code == "LM1057"));
}

#[test]
fn named_property_fields_roundtrip_without_colliding_with_field_names() {
    let field = FieldSpec::required(
        "position",
        FieldKind::named_record("Position", [FieldSpec::required("x", FieldKind::Number)]),
    );
    let json = serde_json::to_value(&field).expect("named property field serializes");

    assert_eq!(json["name"], "position");
    assert_eq!(json["record"], "Position");
    let decoded: FieldSpec =
        serde_json::from_value(json).expect("named property field deserializes");
    assert_eq!(decoded, field);
}

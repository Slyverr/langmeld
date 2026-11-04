use langmeld_core::{
    ConstructBuilder, ExpressionAssociativity, ExpressionAtomKind, ExpressionOperatorSpec,
    ExpressionSpec, FieldKind, FieldSpec, LanguageBuilder,
};

fn language() -> LanguageBuilder {
    LanguageBuilder::new("Calc")
        .schema_version(3)
        .version("0.1.0")
        .root("program")
        .expression(
            ExpressionSpec::new("arithmetic")
                .atom(ExpressionAtomKind::Number)
                .atom(ExpressionAtomKind::Identifier)
                .operator(ExpressionOperatorSpec::infix(
                    "+",
                    10,
                    ExpressionAssociativity::Left,
                )),
        )
        .construct(
            ConstructBuilder::new("program")
                .syntax("program")
                .block(["statement"]),
        )
        .construct(
            ConstructBuilder::new("statement")
                .syntax("eval {value};")
                .field(FieldSpec::required(
                    "value",
                    FieldKind::expression("arithmetic", ";"),
                )),
        )
}

#[test]
fn expression_declarations_round_trip_with_explicit_field_boundary() {
    let spec = language().build().expect("expression definition is valid");
    let json = serde_json::to_value(spec.spec()).expect("language definition serializes");

    assert_eq!(json["expressions"][0]["id"], "arithmetic");
    assert_eq!(json["expressions"][0]["atoms"][0], "number");
    assert_eq!(json["expressions"][0]["operators"][0]["token"], "+");
    assert_eq!(json["constructs"][1]["fields"][0]["kind"], "expression");
    assert_eq!(json["constructs"][1]["fields"][0]["grammar"], "arithmetic");
    assert_eq!(json["constructs"][1]["fields"][0]["terminator"], ";");

    let round_trip = serde_json::from_value::<langmeld_core::LanguageSpec>(json)
        .expect("language definition deserializes");
    assert_eq!(round_trip, spec.spec().clone());
}

#[test]
fn expression_fields_require_schema_three_and_a_declared_grammar() {
    let schema_error = language()
        .schema_version(2)
        .build()
        .expect_err("schema v2 cannot use expression declarations");
    assert!(schema_error.iter().any(|error| error.code == "LM1085"));

    let unknown_grammar = LanguageBuilder::new("Calc")
        .schema_version(3)
        .version("0.1.0")
        .root("program")
        .construct(
            ConstructBuilder::new("program")
                .syntax("program {value};")
                .field(FieldSpec::required(
                    "value",
                    FieldKind::expression("missing", ";"),
                )),
        )
        .build()
        .expect_err("unknown expression grammar must be rejected");
    assert!(unknown_grammar.iter().any(|error| error.code == "LM1088"));
}

#[test]
fn host_atom_parser_allows_a_grammar_without_built_in_atoms() {
    let language = LanguageBuilder::new("HostAtoms")
        .schema_version(3)
        .version("0.1.0")
        .root("program")
        .expression(ExpressionSpec::new("custom").host_atom_parser("host_atoms"))
        .construct(
            ConstructBuilder::new("program")
                .syntax("program")
                .block(std::iter::empty::<&str>()),
        )
        .build()
        .expect("the declared host parser owns this grammar's atom syntax");
    let json = serde_json::to_value(language.spec()).expect("host parser identity serializes");

    assert_eq!(json["expressions"][0]["host_atom_parser"], "host_atoms");
    let round_trip = serde_json::from_value::<langmeld_core::LanguageSpec>(json)
        .expect("host atom parser identity deserializes");
    assert_eq!(round_trip, language.spec().clone());
}

#[test]
fn expression_grammars_reject_duplicate_operators_and_terminator_collisions() {
    let duplicate = ExpressionOperatorSpec::infix("+", 10, ExpressionAssociativity::Left);
    let duplicate_operators = language()
        .expression(
            ExpressionSpec::new("other")
                .operator(duplicate.clone())
                .operator(duplicate),
        )
        .build()
        .expect_err("duplicate operator spelling must be rejected");
    assert!(
        duplicate_operators
            .iter()
            .any(|error| error.code == "LM1084")
    );

    let collision = LanguageBuilder::new("Calc")
        .schema_version(3)
        .version("0.1.0")
        .root("program")
        .expression(
            ExpressionSpec::new("arithmetic").operator(ExpressionOperatorSpec::infix(
                ";",
                10,
                ExpressionAssociativity::Left,
            )),
        )
        .construct(
            ConstructBuilder::new("program")
                .syntax("program {value};")
                .field(FieldSpec::required(
                    "value",
                    FieldKind::expression("arithmetic", ";"),
                )),
        )
        .build()
        .expect_err("boundary/operator collision must be rejected");
    assert!(collision.iter().any(|error| error.code == "LM1087"));
}

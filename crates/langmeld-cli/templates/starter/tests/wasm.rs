#![cfg(feature = "wasm")]

use langmeld::core::{
    ConstructBuilder, ExpressionAssociativity, ExpressionAtomKind, ExpressionOperatorSpec,
    ExpressionSpec, FieldKind, FieldSpec, LanguageBuilder,
};
use @CRATE@::LanguageHost;

#[test]
fn browser_parse_response_preserves_expression_structure_and_spans() {
    let language = LanguageBuilder::new("Calc")
        .version("0.1.0")
        .schema_version(3)
        .root("program")
        .expression(
            ExpressionSpec::new("arithmetic")
                .atom(ExpressionAtomKind::Number)
                .atom(ExpressionAtomKind::Identifier)
                .operator(ExpressionOperatorSpec::prefix("-", 30))
                .operator(ExpressionOperatorSpec::infix(
                    "+",
                    10,
                    ExpressionAssociativity::Left,
                ))
                .operator(ExpressionOperatorSpec::infix(
                    "*",
                    20,
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
                .syntax("evaluate {value};")
                .field(FieldSpec::required(
                    "value",
                    FieldKind::expression("arithmetic", ";"),
                )),
        )
        .build()
        .expect("expression language validates");
    let spec = serde_json::to_string(language.spec()).expect("language model serializes");
    let host = LanguageHost::from_spec_json(&spec).expect("browser host loads the model");
    let program = "program { evaluate -foo.bar(1)[2] + (2 * 3); }";
    let response = host
        .parse(program)
        .expect("browser host parses the expression");
    let response: serde_json::Value =
        serde_json::from_str(&response).expect("browser response is JSON");
    let expression = &response["program"]["nodes"][0]["children"][0]["fields"]["value"]["value"];

    assert_eq!(expression["kind"], "infix");
    assert_eq!(expression["operator"], "+");
    assert_eq!(expression["left"]["kind"], "prefix");
    assert_eq!(expression["left"]["operand"]["kind"], "index");
    assert_eq!(
        expression["left"]["operand"]["receiver"]["kind"],
        "call"
    );
    assert_eq!(
        expression["left"]["operand"]["receiver"]["callee"]["kind"],
        "member"
    );
    assert_eq!(expression["right"]["kind"], "group");
    assert_eq!(expression["right"]["expression"]["kind"], "infix");
    assert_eq!(expression["right"]["expression"]["operator"], "*");
    assert_eq!(
        expression["span"]["start"],
        program.find('-').expect("prefix operator starts")
    );
    assert_eq!(
        expression["operator_span"]["start"],
        program.find('+').expect("outer operator exists")
    );
}

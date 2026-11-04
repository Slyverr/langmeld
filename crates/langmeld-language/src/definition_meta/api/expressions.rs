use langmeld_core::{ConstructBuilder, FieldKind, FieldSpec};

pub(super) fn definitions() -> Vec<ConstructBuilder> {
    vec![
        ConstructBuilder::new("expression_decl")
            .syntax_variant("root", "expression {id} {")
            .field(FieldSpec::required("id", FieldKind::Identifier))
            .block([
                "expression_atom",
                "expression_operator",
                "expression_host_atom_parser",
            ]),
        ConstructBuilder::new("expression_host_atom_parser")
            .syntax_variant("default", "host_atom_parser: {id}")
            .field(FieldSpec::required("id", FieldKind::Identifier)),
        ConstructBuilder::new("expression_atom")
            .syntax_variant("default", "atom: {kind}")
            .field(FieldSpec::required(
                "kind",
                FieldKind::enumeration(["number", "boolean", "identifier"]),
            )),
        ConstructBuilder::new("expression_operator")
            .syntax_variant("prefix", "operator {token} prefix {prefix_precedence}")
            .syntax_variant("infix", "operator {token} infix {infix_precedence} {associativity}")
            .syntax_variant(
                "prefix_infix",
                "operator {token} prefix {prefix_precedence} infix {infix_precedence} {associativity}",
            )
            .field(FieldSpec::required("token", FieldKind::Text))
            .field(FieldSpec::optional("prefix_precedence", FieldKind::Number))
            .field(FieldSpec::optional("infix_precedence", FieldKind::Number))
            .field(FieldSpec::optional(
                "associativity",
                FieldKind::enumeration(["left", "right"]),
            )),
    ]
}

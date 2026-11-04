use std::collections::BTreeMap;

use langmeld::language::AstNode;

mod subtract;

pub(super) fn execute(
    node: &AstNode,
    variables: &mut BTreeMap<String, f64>,
) -> Result<(), String> {
    subtract::execute(node, variables)
}

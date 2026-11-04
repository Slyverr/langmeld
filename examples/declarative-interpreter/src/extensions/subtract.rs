use std::collections::BTreeMap;

use langmeld::language::AstNode;

use crate::{identifier, insert_variable, lookup_variable, reference};

pub(super) fn execute(
    node: &AstNode,
    variables: &mut BTreeMap<String, f64>,
) -> Result<(), String> {
    let right_name = reference(node, "right")?;
    let left_name = reference(node, "left")?;
    let name = identifier(node, "name")?.to_owned();
    let left = lookup_variable(variables, left_name)?;
    let right = lookup_variable(variables, right_name)?;
    let value = left - right;
    if !value.is_finite() {
        return Err(format!(
            "subtraction for `{name}` produced a non-finite value"
        ));
    }
    insert_variable(variables, name, value)
}

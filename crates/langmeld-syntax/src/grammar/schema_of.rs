//! Works out the node schema of a grammar from its labels.

use std::collections::HashSet;

use super::builder::{Definition, Operator};
use super::ir::Rule;
use super::pattern::Shape;
use super::{Cardinality, FieldSchema, NodeSchema, Pattern, Schema};

fn add(node: &mut NodeSchema, label: &str, name: &str, token: bool, cardinality: Cardinality) {
    let position = node.fields.iter().position(|field| field.name == label);
    let field = match position {
        Some(position) => {
            let field = &mut node.fields[position];
            // A second part under the same label.
            let new = !field.nodes.iter().chain(&field.tokens).any(|n| n == name);
            if !new {
                field.cardinality = Cardinality::Many;
            }
            field
        }
        None => {
            node.fields
                .push(FieldSchema::new(label, Vec::<String>::new(), cardinality));
            let last = node.fields.len() - 1;
            &mut node.fields[last]
        }
    };
    let list = if token {
        &mut field.tokens
    } else {
        &mut field.nodes
    };
    if !list.iter().any(|n| n == name) {
        list.push(name.to_owned());
    }
}

fn walk(
    pattern: &Pattern,
    cardinality: Cardinality,
    label: Option<&str>,
    definitions: &HashSet<&str>,
    node: &mut NodeSchema,
) {
    let inner = |pattern: &Pattern, cardinality, label, node: &mut NodeSchema| {
        walk(pattern, cardinality, label, definitions, node);
    };
    // Something optional inside a repetition still repeats.
    let optional = match cardinality {
        Cardinality::Many => Cardinality::Many,
        Cardinality::One | Cardinality::Optional => Cardinality::Optional,
    };
    match &pattern.0 {
        Shape::Reference(name) => {
            let is_node = definitions.contains(name.as_str());
            if is_node {
                node.children.insert(name.clone());
            }
            if let Some(label) = label {
                add(node, label, name, !is_node, cardinality);
            }
        }
        Shape::Literal(text) => {
            if let Some(label) = label {
                add(node, label, text, true, cardinality);
            }
        }
        Shape::Sequence(parts) => {
            for part in parts {
                inner(part, cardinality, label, node);
            }
        }
        Shape::Choice(parts) => {
            for part in parts {
                inner(part, optional, label, node);
            }
        }
        Shape::Optional(part) => inner(part, optional, label, node),
        Shape::Many(part) | Shape::Many1(part) => {
            inner(part, Cardinality::Many, label, node);
        }
        Shape::Separated { item, .. } => inner(item, Cardinality::Many, label, node),
        Shape::Field(name, part) => inner(part, cardinality, Some(name), node),
        Shape::Terminated(part) | Shape::Attempt(part) => {
            inner(part, cardinality, label, node);
        }
        Shape::Look(_) | Shape::Not(_) => {}
    }
}

pub(super) fn infer(definitions: &[&Definition], rules: &[Rule]) -> Schema {
    let names: HashSet<&str> = definitions.iter().map(|d| d.name()).collect();
    let node = |name: &str, body: &Pattern| {
        let mut node = NodeSchema::new(name);
        walk(body, Cardinality::One, None, &names, &mut node);
        node
    };
    let mut schema = Schema::default();
    let mut rules = rules.iter();
    for definition in definitions {
        match definition {
            Definition::Rule(name, body) => {
                let transparent = rules.next().is_some_and(|rule| rule.kind.is_none());
                match (&body.0, transparent) {
                    (Shape::Choice(alternatives), true) => {
                        let members = alternatives.iter().filter_map(|a| match &a.0 {
                            Shape::Reference(name) => Some(name.clone()),
                            _ => None,
                        });
                        schema = schema.with_union(name.clone(), members);
                    }
                    _ => schema = schema.with_node(node(name, body)),
                }
            }
            Definition::Expr(expr) => {
                let mut members = Vec::new();
                for (name, body) in &expr.atoms {
                    members.push(name.clone());
                    schema = schema.with_node(node(name, body));
                }
                for operator in &expr.operators {
                    let (name, made) = match operator {
                        Operator::Prefix { name, .. } | Operator::Infix { name, .. } => {
                            (name, NodeSchema::new(name.clone()))
                        }
                        Operator::Postfix { name, body, .. } => (name, node(name, body)),
                    };
                    members.push(name.clone());
                    schema = schema.with_node(made.with_child(expr.name.clone()));
                }
                schema = schema.with_union(expr.name.clone(), members);
            }
            Definition::Extern(rule) => {
                for name in &rule.produces {
                    schema = schema.with_node(NodeSchema::new(name.clone()).open());
                }
                schema = schema.with_union(rule.name.clone(), rule.produces.clone());
            }
        }
    }
    schema
}

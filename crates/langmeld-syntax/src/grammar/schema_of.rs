//! Works out the node schema of a grammar from its labels.

use std::collections::{BTreeSet, HashSet};

use super::builder::{Definition, Operator};
use super::ir::Rule;
use super::pattern::Shape;
use super::places::Items;
use super::{Cardinality, FieldSchema, NodeSchema, Pattern, Place, Schema};

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

fn operand(label: &str, expr: &str, skip: usize) -> FieldSchema {
    FieldSchema::new(label, [expr], Cardinality::One).at(Place::default().skip(skip))
}

fn token_field(label: &str, symbols: &[String]) -> FieldSchema {
    FieldSchema::token(label, symbols.iter().cloned(), Cardinality::One)
}

/// Fills in where each field of the nodes that rules make is.
fn place_fields(schema: &mut Schema, definitions: &[&Definition]) {
    let mut places = Vec::new();
    for definition in definitions {
        let bodies: Vec<(&str, Option<&str>, &Pattern)> = match definition {
            Definition::Rule(name, body) => vec![(name, None, body)],
            Definition::Expr(expr) => expr
                .atoms
                .iter()
                .map(|(name, body)| (name.as_str(), None, body))
                .chain(expr.operators.iter().filter_map(|operator| match operator {
                    Operator::Postfix { name, body, .. } => {
                        Some((name.as_str(), Some(expr.name.as_str()), body))
                    }
                    _ => None,
                }))
                .collect(),
            Definition::Extern(_) => Vec::new(),
        };
        for (name, operand, body) in bodies {
            let Some(node) = schema.node(name) else {
                continue;
            };
            let mut items = Items::default();
            if let Some(expr) = operand {
                items.leading(expr, "operand");
            }
            items.add(body, None);
            let stands_for = |name: &str| {
                let name = name.to_owned();
                let expanded = schema.expand([&name]);
                expanded
                    .into_iter()
                    .map(str::to_owned)
                    .collect::<BTreeSet<_>>()
            };
            for field in &node.fields {
                if operand.is_some() && field.name == "operand" {
                    continue;
                }
                let mut wanted = schema.expand(&field.nodes);
                wanted.extend(field.tokens.iter().map(String::as_str));
                let many = field.cardinality == Cardinality::Many;
                let place = items.place(&field.name, many, &wanted, &stands_for);
                places.push((name.to_owned(), field.name.clone(), place));
            }
        }
    }
    for (node, field, place) in places {
        schema.set_place(&node, &field, place);
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
                        Operator::Prefix { name, symbols, .. } => {
                            let made = NodeSchema::new(name.clone())
                                .with_field(token_field("operator", symbols))
                                .with_field(operand("operand", &expr.name, 0));
                            (name, made)
                        }
                        Operator::Infix { name, symbols, .. } => {
                            let made = NodeSchema::new(name.clone())
                                .with_field(operand("lhs", &expr.name, 0))
                                .with_field(token_field("operator", symbols))
                                .with_field(operand("rhs", &expr.name, 1));
                            (name, made)
                        }
                        Operator::Postfix { name, body, .. } => {
                            let mut made = node(name, body);
                            // A part of the author's own keeps its name.
                            if made.field("operand").is_none() {
                                made.fields.insert(0, operand("operand", &expr.name, 0));
                            }
                            (name, made)
                        }
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
    place_fields(&mut schema, definitions);
    schema
}

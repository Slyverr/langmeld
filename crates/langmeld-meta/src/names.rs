//! Checks that the lines of a `bind` block speak of nodes and parts the language has.

use langmeld_syntax::{Diagnostic, Language, Schema, closest};

use crate::declared::{BindRules, Binding, Path, Place};
use crate::editor::range;
use crate::problem::problem;

/// Pairs of words after `defines` that say opposite things, with why.
const CONFLICTS: [(&str, &str, &str); 3] = [
    (
        "after",
        "hoisted",
        "`after` shows a name only after its node, and `hoisted` in the whole scope",
    ),
    (
        "shadowing",
        "unique",
        "`shadowing` lets a name hide an earlier one, and `unique` makes a second one an error",
    ),
    (
        "global",
        "in parent",
        "`global` puts a name in no scope, and `in parent` puts it in the scope around",
    ),
];

struct Checker<'a> {
    schema: &'a Schema,
    known: Vec<&'a str>,
    found: Vec<Diagnostic>,
}

impl Checker<'_> {
    /// Keeps a problem once, however many parts of a line lead to it.
    fn add(&mut self, diagnostic: Diagnostic) {
        if !self.found.contains(&diagnostic) {
            self.found.push(diagnostic);
        }
    }

    fn unknown(&mut self, line: &str, node: &str, place: Place) {
        let (file, at) = range(place);
        let near = closest(node, &self.known)
            .map_or_else(String::new, |near| format!("; perhaps you meant `{near}`"));
        self.add(problem(
            "LM-F018",
            file,
            at,
            format!("`{line}` names `{node}`, which is not a rule or expression of the language"),
            format!("write the name of a rule, or of an expression, as declared in the file{near}"),
        ));
    }

    fn no_part(&mut self, node: &str, label: &str, place: Place) {
        let (file, at) = range(place);
        let labels: Vec<String> = self
            .schema
            .node(node)
            .map(|found| {
                found
                    .fields
                    .iter()
                    .map(|field| format!("`{}`", field.name))
                    .collect()
            })
            .unwrap_or_default();
        let parts = if labels.is_empty() {
            "it has no labelled parts".to_owned()
        } else {
            format!("its labelled parts are {}", labels.join(", "))
        };
        self.add(problem(
            "LM-F019",
            file,
            at,
            format!("`{node}` has no part labelled `{label}`"),
            format!(
                "label the part in the rule, as in `{label}:ident`, or name another part; {parts}"
            ),
        ));
    }

    /// Whether `node` is a rule or an expression of the language, or a group of them.
    fn is_node(&self, node: &str) -> bool {
        self.known.contains(&node)
    }

    /// Whether `label` is wrong for `node`; a group of nodes has no parts of its own,
    /// and custom code may give a node parts the grammar does not describe.
    fn lacks(&self, node: &str, label: &str) -> bool {
        self.schema
            .node(node)
            .is_some_and(|found| !found.open && found.field(label).is_none())
    }

    /// Checks a node, or a node and the label of one of its parts. Whether it is right.
    fn path(&mut self, line: &str, path: &Path) -> bool {
        let [node, rest @ ..] = path.parts.as_slice() else {
            return false;
        };
        if !self.is_node(node) {
            self.unknown(line, node, path.place);
            return false;
        }
        match rest {
            [] => true,
            [label] if self.lacks(node, label) => {
                self.no_part(node, label, path.place);
                false
            }
            [_] => true,
            [label, ..] => {
                let (file, at) = range(path.place);
                self.add(problem(
                    "LM-F019",
                    file,
                    at,
                    format!("`{}` is not a node, nor a part of one", path.parts.join(".")),
                    format!(
                        "write a node, or a node and the label of one of its parts, as in `{node}.{label}`"
                    ),
                ));
                false
            }
        }
    }

    /// Checks the part that `from` or `of` names, which is written as the label of a
    /// part of the line's own nodes, or as that node and label.
    fn other(&mut self, line: &str, other: &Path, targets: &[Path]) {
        let nodes: Vec<&str> = targets
            .iter()
            .filter_map(|target| target.parts.first().map(String::as_str))
            .collect();
        match other.parts.as_slice() {
            [] => {}
            [label] => {
                let real: Vec<&str> = nodes
                    .iter()
                    .copied()
                    .filter(|node| self.is_node(node))
                    .collect();
                for node in real {
                    if self.lacks(node, label) {
                        self.no_part(node, label, other.place);
                    }
                }
            }
            [node, ..] => {
                if self.path(line, other) && !nodes.contains(&node.as_str()) {
                    let (file, at) = range(other.place);
                    let written = other.parts.join(".");
                    let about = nodes.join("`, `");
                    self.add(problem(
                        "LM-F021",
                        file,
                        at,
                        format!("`{written}` is a part of `{node}`, but the line is about `{about}`"),
                        "name a part of the same node as the line's, or write only the label of the part",
                    ));
                }
            }
        }
    }
}

/// The problems of a `bind` block: lines that name nodes or parts the language does
/// not have, which would otherwise silently do nothing, and words that contradict
/// each other.
pub(crate) fn check(language: &Language, rules: &BindRules) -> Vec<Diagnostic> {
    let schema = language.schema();
    let known = schema
        .nodes()
        .map(|node| node.name.as_str())
        .chain(schema.unions().map(|(name, _)| name))
        .collect();
    let mut checker = Checker {
        schema,
        known,
        found: Vec::new(),
    };
    for rule in &rules.rules {
        match rule {
            Binding::Scope { nodes, place } => {
                for node in nodes {
                    if !checker.is_node(node) {
                        checker.unknown("scope", node, *place);
                    }
                }
            }
            Binding::Defines {
                targets,
                modifiers,
                place,
                ..
            } => {
                for target in targets {
                    checker.path("defines", target);
                }
                for (one, other, why) in CONFLICTS {
                    let has = |word: &str| modifiers.iter().any(|modifier| modifier == word);
                    if has(one) && has(other) {
                        let (file, at) = range(*place);
                        checker.add(problem(
                            "LM-F020",
                            file,
                            at,
                            format!("`{one}` and `{other}` contradict each other"),
                            format!("keep one of them: {why}"),
                        ));
                    }
                }
            }
            Binding::Refers { targets, .. } => {
                for target in targets {
                    checker.path("refers", target);
                }
            }
            Binding::Imports { targets, from, .. } => {
                for target in targets {
                    checker.path("imports", target);
                }
                checker.other("imports", from, targets);
            }
            Binding::Member { targets, of, .. } => {
                for target in targets {
                    checker.path("refers member", target);
                }
                checker.other("refers member", of, targets);
            }
            Binding::Includes { targets, of, .. } => {
                for target in targets {
                    checker.path("includes", target);
                }
                checker.other("includes", of, targets);
            }
        }
    }
    checker.found
}

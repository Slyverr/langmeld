//! What `bind { }` and `editor { }` blocks declare, kept as written for the chapters
//! that use them.

use serde::{Deserialize, Serialize};

/// Where something was written: a file's number in the load, and a byte range.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Place {
    /// The number of the file among those loaded, root first.
    pub file: u32,
    /// Start byte of the text.
    pub start: u32,
    /// End byte of the text.
    pub end: u32,
}

/// A name path such as `LetStmt.name`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Path {
    /// The parts, in order.
    pub parts: Vec<String>,
    /// Where it was written.
    pub place: Place,
}

/// One statement of a `bind { }` block.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Binding {
    /// `scope A, B`: nodes that open a scope.
    Scope {
        /// The node kinds.
        nodes: Vec<String>,
        /// Where it was written.
        place: Place,
    },
    /// `A.name defines value after`
    Defines {
        /// The names that are defined.
        targets: Vec<Path>,
        /// The namespace.
        namespace: String,
        /// The words after it: `after`, `hoisted`, `in parent`, `unique` and so on.
        modifiers: Vec<String>,
        /// Where it was written.
        place: Place,
    },
    /// `A refers value`
    Refers {
        /// The references.
        targets: Vec<Path>,
        /// The namespace.
        namespace: String,
        /// Where it was written.
        place: Place,
    },
    /// `A imports value from A.path`
    Imports {
        /// The imports.
        targets: Vec<Path>,
        /// The namespace.
        namespace: String,
        /// Where the file's path is written.
        from: Path,
        /// Where it was written.
        place: Place,
    },
    /// `A.member refers member of A.object`: uses of a member, which only the code of the
    /// language can look up, as in the `b` of `a.b`.
    Member {
        /// The uses.
        targets: Vec<Path>,
        /// The part whose member is meant.
        of: Path,
        /// Where it was written.
        place: Place,
    },
    /// `A.body includes members of A.target`
    Includes {
        /// The parts that see more.
        targets: Vec<Path>,
        /// The part whose members they see.
        of: Path,
        /// Where it was written.
        place: Place,
    },
}

/// One statement of an `editor { }` block.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum EditorRule {
    /// `symbol FnDecl kind function name name`
    Symbol {
        /// The node.
        node: String,
        /// What kind of symbol it is.
        kind: String,
        /// The field that names it.
        name: String,
        /// Where it was written.
        place: Place,
    },
    /// `fold Block`
    Fold {
        /// The nodes that can be folded.
        nodes: Vec<String>,
        /// Where it was written.
        place: Place,
    },
    /// `highlight NameRef by_binding`
    Highlight {
        /// The node.
        node: String,
        /// What decides the colour.
        by: String,
        /// Where it was written.
        place: Place,
    },
}

/// The `bind` statements of a language, in the order written.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindRules {
    /// The statements.
    pub rules: Vec<Binding>,
}

/// The `editor` statements of a language, in the order written.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Editor {
    /// The statements.
    pub rules: Vec<EditorRule>,
}

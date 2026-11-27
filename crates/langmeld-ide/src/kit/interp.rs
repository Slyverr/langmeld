//! What most tree-walking interpreters need: names and their values, a few common
//! values, and limits that end programs that run away.
//!
//! ```
//! use langmeld_ide::kit::interp::{Budget, Env, Value};
//!
//! let mut env: Env<Value> = Env::new();
//! let mut budget = Budget::new(1_000, 64);
//! env.define("answer", Value::Int(42));
//! budget.step()?;
//! assert_eq!(env.get("answer"), Some(&Value::Int(42)));
//! # Ok::<(), langmeld_ide::kit::interp::Exhausted>(())
//! ```

use std::collections::HashMap;
use std::convert::Infallible;
use std::error::Error;
use std::fmt;
use std::sync::Arc;

use langmeld_syntax::{Diagnostic, FileId, TextRange};

#[derive(Clone, Debug)]
struct Scope<V> {
    names: HashMap<String, V>,
    /// Whether names further out, except the outermost, are hidden.
    frame: bool,
}

/// The names a running program can see, in scopes nested one in another.
#[derive(Clone, Debug)]
pub struct Env<V> {
    /// The outermost first; there is always one.
    scopes: Vec<Scope<V>>,
}

impl<V> Default for Env<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V> Env<V> {
    /// An environment with one scope and no names.
    pub fn new() -> Self {
        Self {
            scopes: vec![Scope {
                names: HashMap::new(),
                frame: false,
            }],
        }
    }

    /// Starts a scope inside the current one, as a block does.
    pub fn push(&mut self) {
        self.scopes.push(Scope {
            names: HashMap::new(),
            frame: false,
        });
    }

    /// Starts the scope of a function call: it sees its own names and those of the
    /// outermost scope, not the caller's.
    pub fn push_frame(&mut self) {
        self.scopes.push(Scope {
            names: HashMap::new(),
            frame: true,
        });
    }

    /// Ends the current scope. The outermost one stays.
    pub fn pop(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    /// Names `value` in the current scope, giving back what the name stood for there.
    pub fn define(&mut self, name: impl Into<String>, value: V) -> Option<V> {
        let scope = self.scopes.last_mut()?;
        scope.names.insert(name.into(), value)
    }

    /// The scope that `name` is found in.
    fn find(&self, name: &str) -> Option<usize> {
        let mut index = self.scopes.len();
        while let Some(current) = index.checked_sub(1) {
            let scope = self.scopes.get(current)?;
            if scope.names.contains_key(name) {
                return Some(current);
            }
            index = if scope.frame { 1.min(current) } else { current };
        }
        None
    }

    /// What `name` stands for.
    pub fn get(&self, name: &str) -> Option<&V> {
        let index = self.find(name)?;
        self.scopes.get(index)?.names.get(name)
    }

    /// Changes what `name` stands for, where it is defined. Returns false if it is
    /// not.
    pub fn set(&mut self, name: &str, value: V) -> bool {
        let found = self
            .find(name)
            .and_then(|index| self.scopes.get_mut(index))
            .and_then(|scope| scope.names.get_mut(name));
        match found {
            Some(place) => {
                *place = value;
                true
            }
            None => false,
        }
    }
}

/// The values most languages have. A language adds its own, such as functions, as
/// `Other`.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub enum Value<Other = Infallible> {
    /// No value.
    #[default]
    Nothing,
    /// True or false.
    Bool(bool),
    /// A whole number.
    Int(i64),
    /// A number with a fraction.
    Float(f64),
    /// Text, cheap to copy.
    Text(Arc<str>),
    /// A value of the language's own.
    Other(Other),
}

impl<Other> Value<Other> {
    /// A text value.
    pub fn text(text: impl Into<Arc<str>>) -> Self {
        Self::Text(text.into())
    }

    /// How an error names the type of the value. `Other` values are "value".
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Nothing => "nothing",
            Self::Bool(_) => "boolean",
            Self::Int(_) => "whole number",
            Self::Float(_) => "number",
            Self::Text(_) => "text",
            Self::Other(_) => "value",
        }
    }
}

impl<Other: fmt::Display> fmt::Display for Value<Other> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nothing => f.write_str("nothing"),
            Self::Bool(value) => value.fmt(f),
            Self::Int(value) => value.fmt(f),
            Self::Float(value) => fmt::Debug::fmt(value, f),
            Self::Text(value) => value.fmt(f),
            Self::Other(value) => value.fmt(f),
        }
    }
}

/// Which limit of a [`Budget`] a program reached.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Exhausted {
    /// It ran for too many steps.
    Steps,
    /// Its calls went too deep.
    Depth,
}

impl fmt::Display for Exhausted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Steps => "the program ran for too many steps",
            Self::Depth => "calls go too deep",
        })
    }
}

impl Error for Exhausted {}

impl Exhausted {
    /// The limit reached as a problem with the code `code`, pointing at `range` of
    /// `file`, so that a program that runs away is reported like any other mistake
    /// instead of being left to hang or overflow the stack.
    ///
    /// ```
    /// use langmeld_ide::kit::interp::Budget;
    /// use langmeld_syntax::{FileId, TextRange, TextSize};
    ///
    /// let mut budget = Budget::new(0, 8);
    /// let reached = budget.step().expect_err("no steps are left");
    /// let range = TextRange::empty(TextSize::new(0));
    /// let problem = reached.diagnostic("CALC-010", FileId::new(0), range);
    /// assert_eq!(problem.code, "CALC-010");
    /// assert!(problem.message.contains("too many steps"));
    /// ```
    pub fn diagnostic(self, code: &'static str, file: FileId, range: TextRange) -> Diagnostic {
        let advice = match self {
            Self::Steps => "make the program do less, or raise the budget of steps",
            Self::Depth => "make the calls go less deep, or raise the budget of depth",
        };
        Diagnostic::error(code, file, range, self.to_string())
            .note(format!("To fix this, {advice}."))
    }
}

/// How long a program may run and how deep its calls may go. An interpreter counts a
/// step for each thing it does, so that no program runs for ever, and enters a level
/// for each call, so that none overflows the stack.
#[derive(Clone, Debug)]
pub struct Budget {
    steps: u64,
    depth: u32,
    max_depth: u32,
}

impl Budget {
    /// At most `steps` steps, and calls at most `max_depth` deep.
    pub fn new(steps: u64, max_depth: u32) -> Self {
        Self {
            steps,
            depth: 0,
            max_depth,
        }
    }

    /// Counts one step.
    pub fn step(&mut self) -> Result<(), Exhausted> {
        self.steps = self.steps.checked_sub(1).ok_or(Exhausted::Steps)?;
        Ok(())
    }

    /// Goes one call deeper.
    pub fn enter(&mut self) -> Result<(), Exhausted> {
        if self.depth >= self.max_depth {
            return Err(Exhausted::Depth);
        }
        self.depth += 1;
        Ok(())
    }

    /// Comes back from a call.
    pub fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }
}

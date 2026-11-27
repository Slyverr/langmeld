//! Runs Calc programs by walking their trees.
//!
//! A program is a list of `let` declarations, function declarations and expressions.
//! Whole numbers, numbers with a fraction, text and booleans are its values, and
//! functions can be called before they are declared. At the top of a file, each
//! expression that has a value shows it; `print` shows what it is given.
//!
//! Every mistake a program can make is a [`Diagnostic`] with a code that
//! [`EXPLANATIONS`] explains, and a program that runs too long or calls too deeply is
//! stopped by a [`Budget`] and reported the same way.

use std::fmt;

use langmeld::ast::AstNode;
use langmeld::bind::Project;
use langmeld::kit::interp::{Budget, Env, Exhausted, Value};
use langmeld::{Diagnostic, FileId, TextRange};

use crate::syntax::{Block, Call, Expr, File, FnDecl, If, Item, Literal, UseStmt};

/// How many steps a program may take before it is stopped.
pub const MOST_STEPS: u64 = 5_000_000;

/// How deep a program's calls may go before it is stopped.
pub const MOST_DEPTH: u32 = 128;

/// What `explain` says about each code of the language.
pub const EXPLANATIONS: [(&str, &str); 11] = [
    (
        "CALC001",
        "A name is used that nothing defines.\n\nDefine it with `let` before it is used, or \
         declare the function. Functions can be used before their declaration; values \
         cannot.",
    ),
    (
        "CALC002",
        "A number is divided by zero, which has no result.\n\nCheck the divisor before \
         dividing.",
    ),
    (
        "CALC003",
        "A number is too large to be kept, or cannot be read.\n\nWhole numbers hold up to \
         9223372036854775807. Use a number with a fraction, such as `1e30`, for larger \
         values.",
    ),
    (
        "CALC004",
        "An operation was given values of kinds it does not work on.\n\nNumbers can be \
         added, subtracted, multiplied and divided, and text can be joined with `+`. A \
         condition must be true or false.",
    ),
    (
        "CALC005",
        "A function was called with the wrong number of arguments.\n\nCall it with as many \
         arguments as it has parameters.",
    ),
    (
        "CALC006",
        "Something that is not a function was called.\n\nOnly functions can be followed by \
         `(...)`. The functions that exist without being declared are `print`, `sqrt` and \
         `abs`.",
    ),
    (
        "CALC007",
        "The program ran for too many steps.\n\nCalc stops programs after 5,000,000 steps, \
         so that one that never ends is reported instead of hanging. Make the program do \
         less.",
    ),
    (
        "CALC008",
        "Function calls went too deep.\n\nCalc stops calls nested more than 128 deep, so \
         that a function that calls itself for ever is reported instead of crashing. Make \
         the recursion end sooner.",
    ),
    (
        "CALC009",
        "A part of the program is missing.\n\nThe file has syntax problems, which `calc \
         check` shows. This appears only when a program that has them is run anyway.",
    ),
    (
        "CALC010",
        "A number is divided by a literal zero, so the division can only fail.\n\nThis is a \
         warning from `calc check`, found without running the program. Divide by a number \
         that is not zero.",
    ),
    (
        "CALC011",
        "A file named by `use` cannot be run.\n\nCheck the path: `use` names a file \
         relative to the folder of the file that uses it, written with `/`, and the file must \
         exist. A file that has syntax problems is not run either; `calc check` shows them.",
    ),
];

/// A function the program declared.
#[derive(Clone, Debug, PartialEq)]
pub struct Function {
    name: String,
    decl: FnDecl,
    /// The file that declares it, which problems in its body are in.
    file: FileId,
}

impl fmt::Display for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<function {}>", self.name)
    }
}

/// What Calc expressions give.
pub type Val = Value<Function>;

/// A problem found while running; there is at most one.
type Result<T> = std::result::Result<T, Box<Diagnostic>>;

struct Interpreter<'o> {
    /// The file whose statements are running.
    file: FileId,
    /// The files that `use` can name.
    project: Option<&'o Project<'o>>,
    /// The files being run, each of which used the next.
    running: Vec<FileId>,
    env: Env<Val>,
    budget: Budget,
    output: &'o mut dyn FnMut(&str),
}

/// The text of a node, without the space around it.
fn node_text(node: &langmeld::SyntaxNode) -> String {
    node.text().to_string().trim().to_owned()
}

impl Interpreter<'_> {
    fn fail(
        &self,
        code: &'static str,
        range: TextRange,
        message: impl Into<String>,
    ) -> Box<Diagnostic> {
        Box::new(Diagnostic::error(code, self.file, range, message))
    }

    fn exhausted(&self, reached: Exhausted, range: TextRange) -> Box<Diagnostic> {
        let code = match reached {
            Exhausted::Steps => "CALC007",
            _ => "CALC008",
        };
        Box::new(reached.diagnostic(code, self.file, range))
    }

    fn step(&mut self, range: TextRange) -> Result<()> {
        self.budget
            .step()
            .map_err(|reached| self.exhausted(reached, range))
    }

    /// A part the grammar requires. Trees with errors may lack it.
    fn part<T>(&self, part: Option<T>, range: TextRange) -> Result<T> {
        part.ok_or_else(|| self.fail("CALC009", range, "this part of the program is missing"))
    }

    /// Declares the functions of a list of items, so that they can be called from
    /// anywhere in it.
    fn declare(&mut self, items: &[Item]) {
        for item in items {
            if let Item::FnDecl(decl) = item
                && let Some(name) = decl.name()
            {
                let name = node_text(name.syntax());
                let function = Function {
                    name: name.clone(),
                    decl: decl.clone(),
                    file: self.file,
                };
                self.env.define(name, Value::Other(function));
            }
        }
    }

    /// Runs a list of items. The value is that of the last one, if it is an expression
    /// that is not ended by `;`.
    fn items(&mut self, items: Vec<Item>, show: bool) -> Result<Val> {
        self.declare(&items);
        let mut last = Value::Nothing;
        for item in items {
            self.step(item_range(&item))?;
            last = Value::Nothing;
            match item {
                Item::LetStmt(binding) => {
                    let name = self.part(binding.name(), binding.syntax().text_range())?;
                    let value = self.part(binding.value(), binding.syntax().text_range())?;
                    let value = self.value(&value)?;
                    self.env.define(node_text(name.syntax()), value);
                }
                Item::FnDecl(_) => {}
                Item::UseStmt(statement) => self.import(&statement)?,
                Item::ExprStmt(statement) => {
                    let expr = self.part(statement.expr(), statement.syntax().text_range())?;
                    let value = self.value(&expr)?;
                    let ended = statement
                        .syntax()
                        .children_with_tokens()
                        .any(|child| child.kind() == self.semicolon());
                    if show && value != Value::Nothing {
                        (self.output)(&value.to_string());
                    } else if !ended {
                        last = value;
                    }
                }
            }
        }
        Ok(last)
    }

    /// Runs the file a `use` names where it is written, unless it is running already.
    fn import(&mut self, statement: &UseStmt) -> Result<()> {
        let range = statement.syntax().text_range();
        let path = self.part(statement.path(), range)?;
        let project = self.project;
        let found = project.and_then(|project| {
            let id = project.import_at(self.file, path.syntax().text_range().start())?;
            Some((id, project.file(id)?))
        });
        let Some((id, imported)) = found else {
            let message = format!("cannot read {}", path.text());
            return Err(self.fail("CALC011", path.syntax().text_range(), message));
        };
        if self.running.contains(&id) {
            return Ok(());
        }
        // The problems of that file are shown in it.
        if let Some(problem) = imported.parse().diagnostics.first() {
            return Err(Box::new(problem.clone()));
        }
        let file = File::cast(imported.parse().syntax());
        let file = self.part(file, range)?;
        self.budget
            .enter()
            .map_err(|reached| self.exhausted(reached, range))?;
        self.running.push(id);
        let using = std::mem::replace(&mut self.file, id);
        let ran = self.items(file.items().collect(), false);
        self.file = using;
        self.running.pop();
        self.budget.leave();
        ran.map(drop)
    }

    fn semicolon(&self) -> langmeld::SyntaxKind {
        crate::syntax::language()
            .token_kind(";")
            .unwrap_or(langmeld::SyntaxKind::ERROR)
    }

    fn block(&mut self, block: &Block) -> Result<Val> {
        self.env.push();
        let value = self.items(block.stmts().collect(), false);
        self.env.pop();
        value
    }

    fn value(&mut self, expr: &Expr) -> Result<Val> {
        let range = expr.syntax().text_range();
        self.step(range)?;
        match expr {
            Expr::Literal(literal) => self.literal(literal),
            Expr::Paren(paren) => {
                let inner = self.part(paren.inner(), range)?;
                self.value(&inner)
            }
            Expr::Var(var) => {
                let name = node_text(var.syntax());
                self.env
                    .get(&name)
                    .cloned()
                    .ok_or_else(|| self.fail("CALC001", range, format!("`{name}` is not defined")))
            }
            Expr::If(branch) => self.branch(branch, range),
            Expr::Neg(negation) => {
                let operand = self.part(negation.operand(), range)?;
                match self.value(&operand)? {
                    Value::Int(whole) => whole
                        .checked_neg()
                        .map(Value::Int)
                        .ok_or_else(|| self.too_large(range)),
                    Value::Float(number) => Ok(Value::Float(-number)),
                    other => Err(self.fail(
                        "CALC004",
                        range,
                        format!("cannot negate {}", other.type_name()),
                    )),
                }
            }
            Expr::Mul(product) => {
                let (lhs, rhs) = (
                    self.part(product.lhs(), range)?,
                    self.part(product.rhs(), range)?,
                );
                let operator = self.part(product.operator(), range)?;
                let (lhs, rhs) = (self.value(&lhs)?, self.value(&rhs)?);
                self.arithmetic(operator.text(), lhs, rhs, range)
            }
            Expr::Add(sum) => {
                let (lhs, rhs) = (self.part(sum.lhs(), range)?, self.part(sum.rhs(), range)?);
                let operator = self.part(sum.operator(), range)?;
                let (lhs, rhs) = (self.value(&lhs)?, self.value(&rhs)?);
                self.arithmetic(operator.text(), lhs, rhs, range)
            }
            Expr::Cmp(comparison) => {
                let (lhs, rhs) = (
                    self.part(comparison.lhs(), range)?,
                    self.part(comparison.rhs(), range)?,
                );
                let operator = self.part(comparison.operator(), range)?;
                let (lhs, rhs) = (self.value(&lhs)?, self.value(&rhs)?);
                self.compare(operator.text(), &lhs, &rhs, range)
            }
            Expr::Call(call) => self.call(call, range),
        }
    }

    fn too_large(&self, range: TextRange) -> Box<Diagnostic> {
        self.fail("CALC003", range, "the number is too large")
    }

    fn literal(&self, literal: &Literal) -> Result<Val> {
        let range = literal.syntax().text_range();
        let written = node_text(literal.syntax());
        if written == "true" || written == "false" {
            return Ok(Value::Bool(written == "true"));
        }
        if let Some(quoted) = written.strip_prefix('"') {
            return Ok(Value::text(unescape(
                quoted.strip_suffix('"').unwrap_or(quoted),
            )));
        }
        let digits = written.replace('_', "");
        let hex = digits
            .strip_prefix("0x")
            .or_else(|| digits.strip_prefix("0X"));
        if let Some(hex) = hex {
            return i64::from_str_radix(hex, 16)
                .map(Value::Int)
                .map_err(|_| self.too_large(range));
        }
        if digits.contains(['.', 'e', 'E']) {
            return digits
                .parse::<f64>()
                .ok()
                .filter(|number| number.is_finite())
                .map(Value::Float)
                .ok_or_else(|| self.too_large(range));
        }
        digits
            .parse()
            .map(Value::Int)
            .map_err(|_| self.too_large(range))
    }

    fn branch(&mut self, branch: &If, range: TextRange) -> Result<Val> {
        let cond = self.part(branch.cond(), range)?;
        let then = self.part(branch.then(), range)?;
        match self.value(&cond)? {
            Value::Bool(true) => self.block(&then),
            Value::Bool(false) => branch
                .r#else()
                .map_or(Ok(Value::Nothing), |other| self.block(&other)),
            other => Err(self.fail(
                "CALC004",
                cond.syntax().text_range(),
                format!(
                    "the condition must be true or false, not {}",
                    other.type_name()
                ),
            )),
        }
    }

    fn arithmetic(&self, operator: &str, lhs: Val, rhs: Val, range: TextRange) -> Result<Val> {
        let word = match operator {
            "+" => "add",
            "-" => "subtract",
            "*" => "multiply",
            _ => "divide",
        };
        match (&lhs, &rhs) {
            (Value::Int(a), Value::Int(b)) => {
                let (a, b) = (*a, *b);
                let whole = match operator {
                    "+" => a.checked_add(b),
                    "-" => a.checked_sub(b),
                    "*" => a.checked_mul(b),
                    _ if b == 0 => return Err(self.divide_by_zero(range)),
                    _ if a.checked_rem(b) == Some(0) => a.checked_div(b),
                    // A division that leaves a remainder has a fraction.
                    _ => return Ok(Value::Float(a as f64 / b as f64)),
                };
                whole.map(Value::Int).ok_or_else(|| self.too_large(range))
            }
            (Value::Int(_) | Value::Float(_), Value::Int(_) | Value::Float(_)) => {
                let (a, b) = (number(&lhs), number(&rhs));
                let result = match operator {
                    "+" => a + b,
                    "-" => a - b,
                    "*" => a * b,
                    _ if b == 0.0 => return Err(self.divide_by_zero(range)),
                    _ => a / b,
                };
                // Numbers that overflow have no value to show.
                if result.is_finite() {
                    Ok(Value::Float(result))
                } else {
                    Err(self.too_large(range))
                }
            }
            (Value::Text(a), Value::Text(b)) if operator == "+" => {
                Ok(Value::text(format!("{a}{b}")))
            }
            _ => Err(self.fail(
                "CALC004",
                range,
                format!("cannot {word} {} and {}", lhs.type_name(), rhs.type_name()),
            )),
        }
    }

    fn divide_by_zero(&self, range: TextRange) -> Box<Diagnostic> {
        self.fail("CALC002", range, "division by zero")
    }

    fn compare(&self, operator: &str, lhs: &Val, rhs: &Val, range: TextRange) -> Result<Val> {
        let order = match (lhs, rhs) {
            (Value::Int(_) | Value::Float(_), Value::Int(_) | Value::Float(_)) => {
                number(lhs).partial_cmp(&number(rhs))
            }
            (Value::Text(a), Value::Text(b)) => Some(a.cmp(b)),
            _ => None,
        };
        if operator == "==" {
            return Ok(Value::Bool(match order {
                Some(order) => order.is_eq(),
                None => lhs == rhs,
            }));
        }
        match order {
            Some(order) => Ok(Value::Bool(order.is_lt())),
            None => Err(self.fail(
                "CALC004",
                range,
                format!("cannot compare {} and {}", lhs.type_name(), rhs.type_name()),
            )),
        }
    }

    fn call(&mut self, call: &Call, range: TextRange) -> Result<Val> {
        let callee = self.part(call.operand(), range)?;
        let arguments: Vec<Expr> = call.args().collect();
        let name = match &callee {
            Expr::Var(var) => Some(node_text(var.syntax())),
            _ => None,
        };
        let function = match name
            .as_deref()
            .map(|name| (name, self.env.get(name).cloned()))
        {
            Some((_, Some(Value::Other(function)))) => function,
            Some((name, None)) if matches!(name, "print" | "sqrt" | "abs") => {
                return self.builtin(name, &arguments, range);
            }
            Some((name, None)) => {
                return Err(self.fail("CALC001", range, format!("`{name}` is not defined")));
            }
            _ => {
                let what = self.value(&callee)?;
                return Err(self.fail(
                    "CALC006",
                    callee.syntax().text_range(),
                    format!("{} cannot be called", what.type_name()),
                ));
            }
        };
        let parameters: Vec<_> = function.decl.params().collect();
        if parameters.len() != arguments.len() {
            return Err(self.fail(
                "CALC005",
                range,
                format!(
                    "`{}` takes {} but was given {}",
                    function.name,
                    count(parameters.len()),
                    count(arguments.len())
                ),
            ));
        }
        let mut values = Vec::new();
        for argument in &arguments {
            values.push(self.value(argument)?);
        }
        let body = self.part(function.decl.body(), range)?;
        self.budget
            .enter()
            .map_err(|reached| self.exhausted(reached, range))?;
        self.env.push_frame();
        for (parameter, value) in parameters.iter().zip(values) {
            if let Some(name) = parameter.name() {
                self.env.define(node_text(name.syntax()), value);
            }
        }
        // Its problems are in the file that declares it.
        let caller = std::mem::replace(&mut self.file, function.file);
        let result = self.block(&body);
        self.file = caller;
        self.env.pop();
        self.budget.leave();
        result
    }

    fn builtin(&mut self, name: &str, arguments: &[Expr], range: TextRange) -> Result<Val> {
        let mut values = Vec::new();
        for argument in arguments {
            values.push(self.value(argument)?);
        }
        if name == "print" {
            let shown: Vec<_> = values.iter().map(ToString::to_string).collect();
            (self.output)(&shown.join(" "));
            return Ok(Value::Nothing);
        }
        let [value] = values.as_slice() else {
            return Err(self.fail(
                "CALC005",
                range,
                format!(
                    "`{name}` takes one argument but was given {}",
                    count(values.len())
                ),
            ));
        };
        match (name, value) {
            ("abs", Value::Int(whole)) => whole
                .checked_abs()
                .map(Value::Int)
                .ok_or_else(|| self.too_large(range)),
            ("abs", Value::Float(number)) => Ok(Value::Float(number.abs())),
            ("sqrt", Value::Int(_) | Value::Float(_)) => Ok(Value::Float(number(value).sqrt())),
            _ => Err(self.fail(
                "CALC004",
                range,
                format!("`{name}` does not work on {}", value.type_name()),
            )),
        }
    }
}

fn count(arguments: usize) -> String {
    match arguments {
        1 => "1 argument".to_owned(),
        n => format!("{n} arguments"),
    }
}

fn number(value: &Val) -> f64 {
    match value {
        Value::Int(whole) => *whole as f64,
        Value::Float(number) => *number,
        _ => f64::NAN,
    }
}

fn item_range(item: &Item) -> TextRange {
    match item {
        Item::LetStmt(it) => it.syntax().text_range(),
        Item::FnDecl(it) => it.syntax().text_range(),
        Item::UseStmt(it) => it.syntax().text_range(),
        Item::ExprStmt(it) => it.syntax().text_range(),
    }
}

/// The text of a string with its escapes read.
fn unescape(written: &str) -> String {
    let mut text = String::new();
    let mut chars = written.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            text.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => text.push('\n'),
            Some('t') => text.push('\t'),
            Some('r') => text.push('\r'),
            Some(other) => text.push(other),
            None => text.push('\\'),
        }
    }
    text
}

/// Runs a program that parsed without errors, giving what it shows to `output`, with
/// the most steps and calls Calc allows.
pub fn run(file: &File, id: FileId, output: &mut dyn FnMut(&str)) -> Result<()> {
    run_within(file, id, Budget::new(MOST_STEPS, MOST_DEPTH), output)
}

/// Like [`run`], with a budget of your own.
pub fn run_within(
    file: &File,
    id: FileId,
    budget: Budget,
    output: &mut dyn FnMut(&str),
) -> Result<()> {
    execute(file, id, None, budget, output)
}

/// Runs the file `id` of a project, in which `use` can name the other files, with the most
/// steps and calls Calc allows.
///
/// ```
/// use calc::interp::run_project;
/// use langmeld::bind::Project;
/// use langmeld::project::MemoryFiles;
///
/// let mut files = MemoryFiles::default();
/// files.insert("main.calc", "use \"lib.calc\"; print(twice(4));");
/// files.insert("lib.calc", "fn twice(n) { n * 2 }");
/// let mut project = Project::new(calc::syntax::loaded());
/// let main = project.open("main.calc", &files).expect("there");
/// let mut shown = Vec::new();
/// run_project(&project, main, &mut |line| shown.push(line.to_owned()))?;
/// assert_eq!(shown, ["8"]);
/// # Ok::<(), Box<langmeld::Diagnostic>>(())
/// ```
pub fn run_project<'p>(
    project: &'p Project<'p>,
    id: FileId,
    output: &'p mut dyn FnMut(&str),
) -> Result<()> {
    let found = project
        .file(id)
        .and_then(|file| File::cast(file.parse().syntax()));
    let Some(file) = found else {
        let message = "there is no such file in the project";
        return Err(Box::new(Diagnostic::error(
            "CALC011",
            id,
            TextRange::default(),
            message,
        )));
    };
    let budget = Budget::new(MOST_STEPS, MOST_DEPTH);
    execute(&file, id, Some(project), budget, output)
}

fn execute<'o>(
    file: &File,
    id: FileId,
    project: Option<&'o Project<'o>>,
    budget: Budget,
    output: &'o mut dyn FnMut(&str),
) -> Result<()> {
    let mut interpreter = Interpreter {
        file: id,
        project,
        running: vec![id],
        env: Env::new(),
        budget,
        output,
    };
    interpreter.items(file.items().collect(), true)?;
    Ok(())
}

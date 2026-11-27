//! Runs programs by walking their trees. Change this to give the language its own
//! meaning; the compiler shows what to update when the grammar changes.

use langmeld::ast::AstNode;
use langmeld::kit::interp::Env;
use langmeld::{Diagnostic, FileId, TextRange};

use crate::syntax::{Expr, File, Statement};

/// A problem found while running.
type Result<T> = std::result::Result<T, Box<Diagnostic>>;

struct Interpreter<'o> {
    file: FileId,
    names: Env<i64>,
    output: &'o mut dyn FnMut(&str),
}

impl Interpreter<'_> {
    fn fail(&self, code: &'static str, range: TextRange, message: String) -> Box<Diagnostic> {
        Box::new(Diagnostic::error(code, self.file, range, message))
    }

    /// A part the grammar requires. Trees with errors may lack it.
    fn part<T>(&self, part: Option<T>, of: &impl AstNode) -> Result<T> {
        part.ok_or_else(|| self.fail("{{CODE}}000", of.range(), "this is incomplete".into()))
    }

    fn statement(&mut self, statement: Statement) -> Result<()> {
        match statement {
            Statement::Let(binding) => {
                let name = self.part(binding.name(), &binding)?;
                let value = self.part(binding.value(), &binding)?;
                let value = self.value(&value)?;
                self.names.define(name.syntax().text().to_string(), value);
            }
            Statement::Print(print) => {
                let value = self.part(print.value(), &print)?;
                let value = self.value(&value)?;
                (self.output)(&value.to_string());
            }
        }
        Ok(())
    }

    fn value(&mut self, expr: &Expr) -> Result<i64> {
        let too_large = |this: &Self| {
            this.fail("{{CODE}}003", expr.range(), "the number is too large".into())
        };
        match expr {
            Expr::Number(number) => {
                let digits = number.syntax().text().to_string().replace('_', "");
                digits.trim().parse().map_err(|_| too_large(self))
            }
            Expr::Variable(variable) => {
                let name = variable.syntax().text().to_string();
                let name = name.trim();
                self.names.get(name).copied().ok_or_else(|| {
                    let message = format!("`{name}` is not defined");
                    self.fail("{{CODE}}001", variable.range(), message)
                })
            }
            Expr::Group(group) => {
                let inner = self.part(group.inner(), group)?;
                self.value(&inner)
            }
            Expr::Negation(negation) => {
                let operand = self.part(negation.operand(), negation)?;
                let value = self.value(&operand)?;
                value.checked_neg().ok_or_else(|| too_large(self))
            }
            Expr::Sum(sum) => {
                let (lhs, rhs) = (self.part(sum.lhs(), sum)?, self.part(sum.rhs(), sum)?);
                let (lhs, rhs) = (self.value(&lhs)?, self.value(&rhs)?);
                let operator = self.part(sum.operator(), sum)?;
                match operator.text() {
                    "+" => lhs.checked_add(rhs),
                    _ => lhs.checked_sub(rhs),
                }
                .ok_or_else(|| too_large(self))
            }
            Expr::Product(product) => {
                let lhs = self.part(product.lhs(), product)?;
                let rhs = self.part(product.rhs(), product)?;
                let (lhs, rhs) = (self.value(&lhs)?, self.value(&rhs)?);
                let operator = self.part(product.operator(), product)?;
                if operator.text() == "*" {
                    return lhs.checked_mul(rhs).ok_or_else(|| too_large(self));
                }
                if rhs == 0 {
                    let message = "division by zero".into();
                    return Err(self.fail("{{CODE}}002", product.range(), message));
                }
                lhs.checked_div(rhs).ok_or_else(|| too_large(self))
            }
        }
    }
}

/// Runs a program that parsed without errors, writing what it prints to `output`.
pub fn run(file: &File, id: FileId, output: &mut dyn FnMut(&str)) -> Result<()> {
    let mut interpreter = Interpreter {
        file: id,
        names: Env::new(),
        output,
    };
    file.statements()
        .try_for_each(|statement| interpreter.statement(statement))
}

/// What `explain` says about each code of the language.
pub const EXPLANATIONS: [(&str, &str); 4] = [
    ("{{CODE}}000", "A statement is incomplete."),
    ("{{CODE}}001", "A name is used that no `let` defines before it."),
    ("{{CODE}}002", "A number is divided by zero, which has no result."),
    ("{{CODE}}003", "A number does not fit in 64 bits."),
];

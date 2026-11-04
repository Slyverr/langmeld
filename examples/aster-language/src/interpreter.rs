use std::collections::BTreeMap;

use crate::{
    compiler::{Expr, Function, Instruction, Module},
    extensions,
};

const MAX_CALL_DEPTH: usize = 128;

pub(crate) fn interpret(module: &Module) -> Result<Vec<String>, String> {
    let mut interpreter = Interpreter {
        module,
        scopes: vec![BTreeMap::new()],
        output: Vec::new(),
        call_depth: 0,
    };
    if module.functions.contains_key("main") {
        interpreter.call("main", &[])?;
    } else if interpreter.execute(&module.main, false)?.is_some() {
        return Err("return is only valid inside a function".to_owned());
    }
    Ok(interpreter.output)
}

struct Interpreter<'a> {
    module: &'a Module,
    scopes: Vec<BTreeMap<String, f64>>,
    output: Vec<String>,
    call_depth: usize,
}

impl Interpreter<'_> {
    fn execute(
        &mut self,
        instructions: &[Instruction],
        in_function: bool,
    ) -> Result<Option<f64>, String> {
        for instruction in instructions {
            match instruction {
                Instruction::Let { name, value } => {
                    let value = self.evaluate(value)?;
                    self.insert(name, value)?;
                }
                Instruction::Return(value) => {
                    if !in_function {
                        return Err("return is only valid inside a function".to_owned());
                    }
                    return Ok(Some(self.evaluate(value)?));
                }
                Instruction::Call {
                    target,
                    arguments,
                    name,
                } => {
                    let value = self.call(target, arguments)?;
                    self.insert(name, value)?;
                }
                Instruction::Assert { target, expected } => {
                    let actual = self.lookup(target)?;
                    extensions::assertion::execute(target, actual, *expected)?;
                }
                Instruction::Print { target } => {
                    let value = self.lookup(target)?;
                    self.output.push(format!("{target} = {value}"));
                }
            }
        }
        Ok(None)
    }

    fn call(&mut self, name: &str, arguments: &[f64]) -> Result<f64, String> {
        if self.call_depth >= MAX_CALL_DEPTH {
            return Err(format!("function call depth exceeds {MAX_CALL_DEPTH}"));
        }
        let function = self
            .module
            .functions
            .get(name)
            .cloned()
            .ok_or_else(|| format!("function `{name}` is not available"))?;
        if arguments.len() != function.parameters.len() {
            return Err(format!(
                "function `{name}` expects {} argument(s), received {}",
                function.parameters.len(),
                arguments.len()
            ));
        }
        self.invoke(&function, arguments)
    }

    fn invoke(&mut self, function: &Function, arguments: &[f64]) -> Result<f64, String> {
        self.call_depth += 1;
        self.scopes.push(
            function
                .parameters
                .iter()
                .cloned()
                .zip(arguments.iter().copied())
                .collect(),
        );
        let result = self.execute(&function.body, true);
        self.scopes.pop();
        self.call_depth -= 1;
        result?.ok_or_else(|| "function finished without returning a value".to_owned())
    }

    fn evaluate(&self, expression: &Expr) -> Result<f64, String> {
        let value = match expression {
            Expr::Number(value) => *value,
            Expr::Variable(name) => self.lookup(name)?,
            Expr::Prefix { operator, operand } => {
                let operand = self.evaluate(operand)?;
                match operator.as_str() {
                    "-" => -operand,
                    other => return Err(format!("prefix operator `{other}` is unsupported")),
                }
            }
            Expr::Infix {
                left,
                operator,
                right,
            } => {
                let left = self.evaluate(left)?;
                let right = self.evaluate(right)?;
                match operator.as_str() {
                    "+" => left + right,
                    "-" => left - right,
                    "*" => left * right,
                    "/" if right != 0.0 => left / right,
                    "/" => return Err("division by zero".to_owned()),
                    other => return Err(format!("infix operator `{other}` is unsupported")),
                }
            }
        };
        if value.is_finite() {
            Ok(value)
        } else {
            Err("numeric operation produced a non-finite value".to_owned())
        }
    }

    fn lookup(&self, name: &str) -> Result<f64, String> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
            .ok_or_else(|| format!("variable `{name}` is not available"))
    }

    fn insert(&mut self, name: &str, value: f64) -> Result<(), String> {
        let scope = self
            .scopes
            .last_mut()
            .ok_or_else(|| "interpreter has no active scope".to_owned())?;
        if scope.contains_key(name) {
            return Err(format!(
                "variable `{name}` is already declared in this scope"
            ));
        }
        scope.insert(name.to_owned(), value);
        Ok(())
    }
}

use std::collections::BTreeMap;

use langmeld::{
    expression::{Atom, Expression},
    language::{PassDescriptor, PassPipeline, Program},
};

use crate::ast::{self, FunctionChild, ProgramChild};

#[derive(Clone, Debug)]
pub(crate) struct Module {
    pub functions: BTreeMap<String, Function>,
    pub main: Vec<Instruction>,
}

#[derive(Clone, Debug)]
pub(crate) struct Function {
    pub parameters: Vec<String>,
    pub body: Vec<Instruction>,
}

#[derive(Clone, Debug)]
pub(crate) enum Instruction {
    Let {
        name: String,
        value: Expr,
    },
    Return(Expr),
    Call {
        target: String,
        arguments: Vec<f64>,
        name: String,
    },
    Assert {
        target: String,
        expected: f64,
    },
    Print {
        target: String,
    },
}

#[derive(Clone, Debug)]
pub(crate) enum Expr {
    Number(f64),
    Variable(String),
    Prefix {
        operator: String,
        operand: Box<Expr>,
    },
    Infix {
        left: Box<Expr>,
        operator: String,
        right: Box<Expr>,
    },
}

struct CompileContext<'a> {
    programs: &'a [Program],
    module: Option<Module>,
}

pub(crate) fn compile(programs: &[Program]) -> Result<Module, String> {
    let mut context = CompileContext {
        programs,
        module: None,
    };
    let mut pipeline = PassPipeline::<CompileContext<'_>, String>::new(["programs"]);
    pipeline.register(
        PassDescriptor::new("lower").artifacts(["programs"], ["ir"]),
        |context| {
            context.module = Some(lower_programs(context.programs)?);
            Ok(vec!["ir".to_owned()])
        },
    );
    pipeline.register(
        PassDescriptor::new("constant-fold")
            .artifacts(["ir"], ["optimized-ir"])
            .after(["lower"]),
        |context| {
            let module = context
                .module
                .as_mut()
                .ok_or_else(|| "lowering did not produce IR".to_owned())?;
            optimize_module(module);
            Ok(vec!["optimized-ir".to_owned()])
        },
    );
    pipeline
        .run(&mut context)
        .map_err(|error| error.to_string())?;
    context
        .module
        .ok_or_else(|| "compiler pipeline completed without IR".to_owned())
}

fn lower_programs(programs: &[Program]) -> Result<Module, String> {
    let mut module = Module {
        functions: BTreeMap::new(),
        main: Vec::new(),
    };

    for program in programs {
        for root in &program.nodes {
            if root.construct != "program" {
                return Err(format!("expected program root, got `{}`", root.construct));
            }
            let program = ast::Program::try_from(root).map_err(|error| error.to_string())?;
            for child in program.children {
                match child {
                    ProgramChild::Function(function) => {
                        let name = function.name.value.clone();
                        let function = lower_function(function)?;
                        if module.functions.insert(name.clone(), function).is_some() {
                            return Err(format!("function `{name}` is declared more than once"));
                        }
                    }
                    child => module.main.push(lower_statement(child)?),
                }
            }
        }
    }
    Ok(module)
}

fn lower_function(function: ast::Function) -> Result<Function, String> {
    let mut parameters = Vec::new();
    let mut body = Vec::new();
    for child in function.children {
        match child {
            FunctionChild::Parameter(parameter) if body.is_empty() => {
                parameters.push(parameter.name.value);
            }
            FunctionChild::Parameter(_) => {
                return Err(format!(
                    "parameters in function `{}` must appear before its statements",
                    function.name.value
                ));
            }
            FunctionChild::Let(value) => body.push(lower_let(value)?),
            FunctionChild::Return(value) => body.push(lower_return(value)?),
            FunctionChild::Call(value) => body.push(lower_call(value)?),
            FunctionChild::Print(value) => body.push(lower_print(value)?),
            FunctionChild::Assert(value) => body.push(lower_assert(value)?),
        }
    }
    Ok(Function { parameters, body })
}

fn lower_statement(statement: ProgramChild) -> Result<Instruction, String> {
    match statement {
        ProgramChild::Let(value) => lower_let(value),
        ProgramChild::Return(value) => lower_return(value),
        ProgramChild::Call(value) => lower_call(value),
        ProgramChild::Print(value) => lower_print(value),
        ProgramChild::Assert(value) => lower_assert(value),
        ProgramChild::Function(_) => unreachable!("function declarations are handled by caller"),
    }
}

fn lower_let(value: ast::Let) -> Result<Instruction, String> {
    Ok(Instruction::Let {
        name: value.name.value,
        value: lower_expression(&value.value.value)?,
    })
}

fn lower_return(value: ast::Return) -> Result<Instruction, String> {
    Ok(Instruction::Return(lower_expression(&value.value.value)?))
}

fn lower_call(value: ast::Call) -> Result<Instruction, String> {
    Ok(Instruction::Call {
        target: value.target.value.as_str().to_owned(),
        arguments: value
            .arguments
            .value
            .into_iter()
            .map(|value| value.value)
            .collect(),
        name: value.name.value,
    })
}

fn lower_print(value: ast::Print) -> Result<Instruction, String> {
    Ok(Instruction::Print {
        target: value.target.value.as_str().to_owned(),
    })
}

fn lower_assert(value: ast::Assert) -> Result<Instruction, String> {
    Ok(Instruction::Assert {
        target: value.target.value.as_str().to_owned(),
        expected: value.expected.value,
    })
}

fn lower_expression(expression: &Expression) -> Result<Expr, String> {
    match expression {
        Expression::Atom(Atom::Number(value), _) => lower_number(value),
        Expression::Atom(Atom::Custom { kind, text }, _) if kind == "aster_number" => {
            let value = text
                .strip_prefix('@')
                .ok_or_else(|| format!("`{text}` is not an Aster numeric atom"))?;
            lower_number(value)
        }
        Expression::Atom(Atom::Name(name), _) => Ok(Expr::Variable(name.clone())),
        Expression::Atom(Atom::Boolean(_) | Atom::Custom { .. }, _) => {
            Err("Aster arithmetic accepts numeric expressions only".to_owned())
        }
        Expression::Group { expression, .. } => lower_expression(expression),
        Expression::Prefix {
            operator, operand, ..
        } => Ok(Expr::Prefix {
            operator: operator.clone(),
            operand: Box::new(lower_expression(operand)?),
        }),
        Expression::Infix {
            left,
            operator,
            right,
            ..
        } => Ok(Expr::Infix {
            left: Box::new(lower_expression(left)?),
            operator: operator.clone(),
            right: Box::new(lower_expression(right)?),
        }),
        Expression::Call { .. } | Expression::Member { .. } | Expression::Index { .. } => {
            Err("Aster arithmetic does not support call or member expressions".to_owned())
        }
    }
}

fn lower_number(value: &str) -> Result<Expr, String> {
    let value = value
        .parse::<f64>()
        .map_err(|_| format!("`{value}` is not a valid number"))?;
    if value.is_finite() {
        Ok(Expr::Number(value))
    } else {
        Err("numeric literal must be finite".to_owned())
    }
}

fn optimize_module(module: &mut Module) {
    for instruction in &mut module.main {
        optimize_instruction(instruction);
    }
    for function in module.functions.values_mut() {
        for instruction in &mut function.body {
            optimize_instruction(instruction);
        }
    }
}

fn optimize_instruction(instruction: &mut Instruction) {
    match instruction {
        Instruction::Let { value, .. } | Instruction::Return(value) => optimize_expression(value),
        Instruction::Call { .. } | Instruction::Assert { .. } | Instruction::Print { .. } => {}
    }
}

fn optimize_expression(expression: &mut Expr) {
    match expression {
        Expr::Prefix { operand, .. } => optimize_expression(operand),
        Expr::Infix {
            left,
            operator,
            right,
        } => {
            optimize_expression(left);
            optimize_expression(right);
            let folded = match (&**left, operator.as_str(), &**right) {
                (Expr::Number(left_value), "+", Expr::Number(right_value)) => {
                    Some(left_value + right_value)
                }
                (Expr::Number(left_value), "-", Expr::Number(right_value)) => {
                    Some(left_value - right_value)
                }
                (Expr::Number(left_value), "*", Expr::Number(right_value)) => {
                    Some(left_value * right_value)
                }
                (Expr::Number(left_value), "/", Expr::Number(right_value))
                    if *right_value != 0.0 =>
                {
                    Some(left_value / right_value)
                }
                _ => None,
            };
            if let Some(value) = folded.filter(|value| value.is_finite()) {
                *expression = Expr::Number(value);
            }
        }
        Expr::Number(_) | Expr::Variable(_) => {}
    }
}

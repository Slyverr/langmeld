use std::collections::HashMap;

use langmeld_core::{Diagnostic, SourceFile, Span};
use langmeld_expression::{Expression, ExpressionKind};
use langmeld_language::{
    AnalysisContext, AnalysisDocument, LanguageAnalyzer, LanguagePass, ParsedDocument,
};

use crate::{AsterParser, Function, Program, Statement, ValueType};

/// Index of a symbol in an [`Analysis`]'s symbol list.
pub type SymbolId = usize;

/// Index of a lexical scope in an [`Analysis`]'s scope list.
pub type ScopeId = usize;

/// The kind of declaration represented by a semantic symbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymbolKind {
    /// A function declaration.
    Function,
    /// A function parameter.
    Parameter,
    /// A local variable declaration.
    Variable,
}

/// Type information attached to an Aster declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SymbolType {
    /// Type of a local variable or parameter.
    Value(ValueType),
    /// Signature of a function.
    Function {
        /// Parameter types in declaration order.
        parameters: Vec<ValueType>,
        /// Function return type.
        returns: ValueType,
    },
}

/// One declaration discovered while analyzing a program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Symbol {
    /// Declared name.
    pub name: String,
    /// Kind of declaration.
    pub kind: SymbolKind,
    /// Lexical scope containing this declaration.
    pub scope: ScopeId,
    /// Inferred or declared type information.
    pub ty: SymbolType,
    /// Source range of the declared name.
    pub declaration: Span,
}

/// A name use and the declaration it resolves to, if any.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameReference {
    /// Referenced name.
    pub name: String,
    /// Source range of this use.
    pub span: Span,
    /// Index of the resolved declaration in [`Analysis::symbols`].
    pub target: Option<SymbolId>,
}

/// Aster-specific symbols and name references.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Analysis {
    /// Declarations in discovery order.
    pub symbols: Vec<Symbol>,
    /// Name uses in source traversal order.
    pub references: Vec<NameReference>,
}

/// Resolves Aster names and checks its small built-in type system.
#[must_use]
pub fn analyze(
    source: &SourceFile,
    parsed: &ParsedDocument<Program>,
) -> AnalysisDocument<Analysis> {
    let mut context = match AnalysisContext::new(source, parsed, Analyzer::new(source)) {
        Ok(context) => context,
        Err(diagnostic) => {
            return AnalysisDocument::new(source, Analysis::default(), vec![diagnostic]);
        }
    };
    context.run_pass(&DeclareFunctions);
    context.run_pass(&AnalyzeFunctionBodies);

    let (_, analyzer, diagnostics) = context.finish().into_parts();
    AnalysisDocument::new(source, analyzer.analysis, diagnostics)
}

/// Analyzer for Aster's scope and type rules.
#[derive(Clone, Copy, Debug, Default)]
pub struct AsterAnalyzer;

impl LanguageAnalyzer<AsterParser> for AsterAnalyzer {
    type Analysis = Analysis;

    fn analyze(
        &self,
        source: &SourceFile,
        parsed: &ParsedDocument<Program>,
    ) -> AnalysisDocument<Self::Analysis> {
        analyze(source, parsed)
    }
}

#[derive(Default)]
struct Scope {
    parent: Option<ScopeId>,
    declarations: HashMap<String, SymbolId>,
}

struct Analyzer<'a> {
    source: &'a SourceFile,
    analysis: Analysis,
    diagnostics: Vec<Diagnostic>,
    scopes: Vec<Scope>,
    functions: HashMap<String, SymbolId>,
}

impl<'a> Analyzer<'a> {
    fn new(source: &'a SourceFile) -> Self {
        Self {
            source,
            analysis: Analysis::default(),
            diagnostics: Vec::new(),
            scopes: vec![Scope::default()],
            functions: HashMap::new(),
        }
    }

    fn declare_functions(&mut self, program: &Program) {
        for function in &program.functions {
            let symbol = self.add_symbol(
                &function.name,
                SymbolKind::Function,
                0,
                function.name_span,
                SymbolType::Function {
                    parameters: function
                        .parameters
                        .iter()
                        .map(|parameter| parameter.value_type)
                        .collect(),
                    returns: function.return_type,
                },
            );
            if self.functions.contains_key(&function.name) {
                self.error(
                    "ASTER-SEM-001",
                    format!("function `{}` is already declared", function.name),
                    function.name_span,
                );
                continue;
            }
            self.scopes[0]
                .declarations
                .insert(function.name.clone(), symbol);
            self.functions.insert(function.name.clone(), symbol);
        }
    }

    fn analyze_function(&mut self, function: &Function) {
        let scope = self.new_scope(Some(0));
        for parameter in &function.parameters {
            self.declare_value(
                parameter.name.clone(),
                SymbolKind::Parameter,
                parameter.value_type,
                parameter.name_span,
                scope,
            );
        }
        self.analyze_statements(&function.body, scope, function.return_type);
        if !statements_always_return(&function.body) {
            self.error(
                "ASTER-SEM-012",
                format!("function `{}` may finish without returning", function.name),
                function.span,
            );
        }
    }

    fn analyze_statements(
        &mut self,
        statements: &[Statement],
        scope: ScopeId,
        return_type: ValueType,
    ) {
        for statement in statements {
            match statement {
                Statement::Let {
                    name,
                    name_span,
                    value,
                    ..
                } => {
                    let value_type = self.expression_type(value, scope);
                    self.declare_value(
                        name.clone(),
                        SymbolKind::Variable,
                        value_type,
                        *name_span,
                        scope,
                    );
                }
                Statement::Return { value, .. } => {
                    let actual = self.expression_type(value, scope);
                    self.expect_type(return_type, actual, value.span, "return value");
                }
                Statement::Block { statements, .. } => {
                    let child_scope = self.new_scope(Some(scope));
                    self.analyze_statements(statements, child_scope, return_type);
                }
            }
        }
    }

    fn expression_type(&mut self, expression: &Expression, scope: ScopeId) -> ValueType {
        match &expression.kind {
            ExpressionKind::Integer(_) => ValueType::I32,
            ExpressionKind::String(_) => ValueType::String,
            ExpressionKind::Name(name) => {
                let value_target = self.lookup_value(scope, name);
                let function_target = self.functions.get(name).copied();
                let target = value_target.or(function_target);
                self.analysis.references.push(NameReference {
                    name: name.clone(),
                    span: expression.span,
                    target,
                });
                if let Some(target) = value_target {
                    self.symbol_type(target)
                } else if function_target.is_some() {
                    self.error(
                        "ASTER-SEM-002",
                        format!("function `{name}` must be called"),
                        expression.span,
                    );
                    ValueType::Error
                } else {
                    self.error(
                        "ASTER-SEM-002",
                        format!("unknown name `{name}`"),
                        expression.span,
                    );
                    ValueType::Error
                }
            }
            ExpressionKind::Group(inner) => self.expression_type(inner, scope),
            ExpressionKind::Prefix { operator, operand } => {
                let operand_type = self.expression_type(operand, scope);
                if operator == "-" && operand_type == ValueType::I32 {
                    ValueType::I32
                } else {
                    self.expect_type(
                        ValueType::I32,
                        operand_type,
                        operand.span,
                        "unary arithmetic operand",
                    );
                    ValueType::Error
                }
            }
            ExpressionKind::Infix {
                operator,
                left,
                right,
            } => {
                let left_type = self.expression_type(left, scope);
                let right_type = self.expression_type(right, scope);
                match operator.as_str() {
                    "+" | "-" | "*" | "/" => {
                        self.expect_type(ValueType::I32, left_type, left.span, "left operand");
                        self.expect_type(ValueType::I32, right_type, right.span, "right operand");
                        if left_type == ValueType::I32 && right_type == ValueType::I32 {
                            ValueType::I32
                        } else {
                            ValueType::Error
                        }
                    }
                    "<" => {
                        self.expect_type(ValueType::I32, left_type, left.span, "left operand");
                        self.expect_type(ValueType::I32, right_type, right.span, "right operand");
                        if left_type == ValueType::I32 && right_type == ValueType::I32 {
                            ValueType::Bool
                        } else {
                            ValueType::Error
                        }
                    }
                    "==" => {
                        if left_type == ValueType::Error || right_type == ValueType::Error {
                            ValueType::Error
                        } else if left_type == right_type {
                            ValueType::Bool
                        } else {
                            self.error(
                                "ASTER-SEM-003",
                                "equality operands must have the same type",
                                expression.span,
                            );
                            ValueType::Error
                        }
                    }
                    _ => {
                        self.error(
                            "ASTER-SEM-004",
                            format!("operator `{operator}` is not supported by Aster"),
                            expression.span,
                        );
                        ValueType::Error
                    }
                }
            }
            ExpressionKind::Call { callee, arguments } => {
                self.call_type(callee, arguments, expression.span, scope)
            }
            ExpressionKind::Member { target, .. } => {
                let _ = self.expression_type(target, scope);
                self.error(
                    "ASTER-SEM-005",
                    "member access is not supported by Aster",
                    expression.span,
                );
                ValueType::Error
            }
            ExpressionKind::Index { target, index } => {
                let _ = self.expression_type(target, scope);
                let _ = self.expression_type(index, scope);
                self.error(
                    "ASTER-SEM-006",
                    "indexing is not supported by Aster",
                    expression.span,
                );
                ValueType::Error
            }
        }
    }

    fn call_type(
        &mut self,
        callee: &Expression,
        arguments: &[Expression],
        span: Span,
        scope: ScopeId,
    ) -> ValueType {
        let ExpressionKind::Name(name) = &callee.kind else {
            let _ = self.expression_type(callee, scope);
            self.error(
                "ASTER-SEM-007",
                "Aster calls must name a function directly",
                callee.span,
            );
            for argument in arguments {
                let _ = self.expression_type(argument, scope);
            }
            return ValueType::Error;
        };

        if let Some(local) = self.lookup_value(scope, name) {
            self.analysis.references.push(NameReference {
                name: name.clone(),
                span: callee.span,
                target: Some(local),
            });
            for argument in arguments {
                let _ = self.expression_type(argument, scope);
            }
            self.error(
                "ASTER-SEM-007",
                format!("value `{name}` is not callable"),
                callee.span,
            );
            return ValueType::Error;
        }

        let signature = self.functions.get(name).map(|symbol| {
            let ty = self.analysis.symbols[*symbol].ty.clone();
            (*symbol, ty)
        });
        self.analysis.references.push(NameReference {
            name: name.clone(),
            span: callee.span,
            target: signature.as_ref().map(|(symbol, _)| *symbol),
        });
        let argument_types = arguments
            .iter()
            .map(|argument| self.expression_type(argument, scope))
            .collect::<Vec<_>>();
        let Some((
            _,
            SymbolType::Function {
                parameters,
                returns,
            },
        )) = signature
        else {
            self.error(
                "ASTER-SEM-008",
                format!("unknown function `{name}`"),
                callee.span,
            );
            return ValueType::Error;
        };
        if arguments.len() != parameters.len() {
            self.error(
                "ASTER-SEM-009",
                format!(
                    "function `{name}` expects {} argument(s), received {}",
                    parameters.len(),
                    arguments.len()
                ),
                span,
            );
        }
        for (index, (expected, actual)) in parameters.iter().zip(argument_types.iter()).enumerate()
        {
            self.expect_type(
                *expected,
                *actual,
                arguments[index].span,
                &format!("argument {} to `{name}`", index + 1),
            );
        }
        returns
    }

    fn expect_type(&mut self, expected: ValueType, actual: ValueType, span: Span, context: &str) {
        if expected != ValueType::Error && actual != ValueType::Error && actual != expected {
            self.error(
                "ASTER-SEM-010",
                format!("{context} has type `{actual}`, expected `{expected}`"),
                span,
            );
        }
    }

    fn new_scope(&mut self, parent: Option<ScopeId>) -> ScopeId {
        let id = self.scopes.len();
        self.scopes.push(Scope {
            parent,
            declarations: HashMap::new(),
        });
        id
    }

    fn declare_value(
        &mut self,
        name: String,
        kind: SymbolKind,
        value_type: ValueType,
        declaration: Span,
        scope: ScopeId,
    ) {
        let symbol = self.add_symbol(
            &name,
            kind,
            scope,
            declaration,
            SymbolType::Value(value_type),
        );
        if self.scopes[scope].declarations.contains_key(&name) {
            self.error(
                "ASTER-SEM-011",
                format!("`{name}` is already declared in this scope"),
                declaration,
            );
            return;
        }
        self.scopes[scope].declarations.insert(name, symbol);
    }

    fn add_symbol(
        &mut self,
        name: &str,
        kind: SymbolKind,
        scope: ScopeId,
        declaration: Span,
        ty: SymbolType,
    ) -> SymbolId {
        let id = self.analysis.symbols.len();
        self.analysis.symbols.push(Symbol {
            name: name.to_owned(),
            kind,
            scope,
            ty,
            declaration,
        });
        id
    }

    fn lookup_value(&self, mut scope: ScopeId, name: &str) -> Option<SymbolId> {
        loop {
            if let Some(symbol) = self.scopes[scope].declarations.get(name)
                && self.analysis.symbols[*symbol].kind != SymbolKind::Function
            {
                return Some(*symbol);
            }
            scope = self.scopes[scope].parent?;
        }
    }

    fn symbol_type(&self, symbol: SymbolId) -> ValueType {
        self.analysis
            .symbols
            .get(symbol)
            .and_then(|symbol| match symbol.ty {
                SymbolType::Value(value_type) => Some(value_type),
                SymbolType::Function { .. } => None,
            })
            .unwrap_or(ValueType::Error)
    }

    fn error(&mut self, code: &str, message: impl Into<String>, span: Span) {
        self.diagnostics
            .push(Diagnostic::error(code, message, self.source, span));
    }
}

struct DeclareFunctions;

impl<'a> LanguagePass<Program, Analyzer<'a>> for DeclareFunctions {
    fn run(
        &self,
        _source: &SourceFile,
        parsed: &ParsedDocument<Program>,
        state: &mut Analyzer<'a>,
    ) -> Vec<Diagnostic> {
        state.declare_functions(parsed.tree());

        std::mem::take(&mut state.diagnostics)
    }
}

struct AnalyzeFunctionBodies;

impl<'a> LanguagePass<Program, Analyzer<'a>> for AnalyzeFunctionBodies {
    fn run(
        &self,
        _source: &SourceFile,
        parsed: &ParsedDocument<Program>,
        state: &mut Analyzer<'a>,
    ) -> Vec<Diagnostic> {
        for function in &parsed.tree().functions {
            state.analyze_function(function);
        }

        std::mem::take(&mut state.diagnostics)
    }
}

fn statements_always_return(statements: &[Statement]) -> bool {
    statements.iter().any(|statement| match statement {
        Statement::Return { .. } => true,
        Statement::Block { statements, .. } => statements_always_return(statements),
        Statement::Let { .. } => false,
    })
}

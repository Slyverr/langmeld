use std::collections::{BTreeMap, btree_map::Entry};

use langmeld_core::{Diagnostic, SourceFile};
use langmeld_language::{AnalysisContext, AnalysisDocument, LanguagePass, ParsedDocument};

use crate::ast::Program;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FunctionFacts {
    pub signatures: BTreeMap<String, FunctionSignature>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionSignature {
    pub parameter_types: Vec<String>,
    pub return_type: String,
}

pub fn analyze(
    source: &SourceFile,
    parsed: &ParsedDocument<Program>,
) -> AnalysisDocument<FunctionFacts> {
    let mut context = match AnalysisContext::new(source, parsed, FunctionFacts::default()) {
        Ok(context) => context,
        Err(diagnostic) => {
            return AnalysisDocument::new(source, FunctionFacts::default(), vec![diagnostic]);
        }
    };

    context.run_pass(&CollectSignatures);
    context.run_pass(&ResolveCalls);

    context.finish()
}

struct CollectSignatures;

impl LanguagePass<Program, FunctionFacts> for CollectSignatures {
    fn run(
        &self,
        source: &SourceFile,
        parsed: &ParsedDocument<Program>,
        facts: &mut FunctionFacts,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for function in &parsed.tree().functions {
            let signature = FunctionSignature {
                parameter_types: function
                    .parameters
                    .iter()
                    .map(|parameter| parameter.value_type.clone())
                    .collect(),
                return_type: function.return_type.clone(),
            };

            match facts.signatures.entry(function.name.clone()) {
                Entry::Vacant(entry) => {
                    entry.insert(signature);
                }
                Entry::Occupied(_) => diagnostics.push(Diagnostic::error(
                    "FUNCTION-004",
                    format!("function `{}` is declared more than once", function.name),
                    source,
                    function.name_span,
                )),
            }
        }

        diagnostics
    }
}

struct ResolveCalls;

impl LanguagePass<Program, FunctionFacts> for ResolveCalls {
    fn run(
        &self,
        source: &SourceFile,
        parsed: &ParsedDocument<Program>,
        facts: &mut FunctionFacts,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for function in &parsed.tree().functions {
            for call in &function.calls {
                let Some(signature) = facts.signatures.get(&call.name) else {
                    diagnostics.push(Diagnostic::error(
                        "FUNCTION-005",
                        format!("function `{}` is not declared", call.name),
                        source,
                        call.name_span,
                    ));
                    continue;
                };

                if signature.parameter_types.len() != call.argument_count {
                    diagnostics.push(Diagnostic::error(
                        "FUNCTION-006",
                        format!(
                            "function `{}` expects {} argument(s), got {}",
                            call.name,
                            signature.parameter_types.len(),
                            call.argument_count
                        ),
                        source,
                        call.name_span,
                    ));
                }
            }
        }

        diagnostics
    }
}

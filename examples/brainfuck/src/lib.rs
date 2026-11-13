mod ast;
mod lexer;
mod parser;
mod runtime;

use std::io::{Cursor, Read};

use langmeld_core::{SourceFile, format_diagnostics};
use langmeld_language::LanguageParser;

pub use ast::{Instruction, InstructionKind, Program};
pub use parser::BrainfuckParser;

/// Parses and executes one Brainfuck program with the default step limit.
///
/// # Errors
///
/// Returns formatted diagnostics when parsing fails or execution encounters a
/// runtime error, including an exhausted instruction limit.
pub fn run(source: &SourceFile, input: &[u8]) -> Result<Vec<u8>, String> {
    run_with_step_limit(source, input, runtime::DEFAULT_STEP_LIMIT)
}

/// Parses and executes one program with an explicit instruction budget.
///
/// # Errors
///
/// Returns formatted diagnostics when parsing fails or execution encounters a
/// runtime error, including an exhausted `max_steps` budget.
pub fn run_with_step_limit(
    source: &SourceFile,
    input: &[u8],
    max_steps: usize,
) -> Result<Vec<u8>, String> {
    let mut input = Cursor::new(input);
    run_with_reader_and_step_limit(source, &mut input, max_steps)
}

/// Parses and executes a program, reading each input byte when `,` runs.
///
/// # Errors
///
/// Returns formatted diagnostics when parsing fails, input reading fails, or
/// execution encounters a runtime error or exhausts its default step limit.
pub fn run_with_reader<R: Read>(source: &SourceFile, input: &mut R) -> Result<Vec<u8>, String> {
    run_with_reader_and_step_limit(source, input, runtime::DEFAULT_STEP_LIMIT)
}

fn run_with_reader_and_step_limit(
    source: &SourceFile,
    input: &mut dyn Read,
    max_steps: usize,
) -> Result<Vec<u8>, String> {
    let parsed = BrainfuckParser.parse(source);

    if !parsed.diagnostics().is_empty() {
        let formatted = format_diagnostics(std::slice::from_ref(source), parsed.diagnostics());

        if !parsed.is_valid() {
            return Err(formatted);
        }

        eprintln!("{formatted}");
    }

    runtime::execute(source, parsed.tree(), input, max_steps)
        .map_err(|diagnostic| format_diagnostics(std::slice::from_ref(source), &[diagnostic]))
}

// Diagnostics are the interpreter's source-aware failure type, so returning
// them directly avoids losing the error code and source span.
#![allow(clippy::result_large_err)]

use std::io::{self, Read};

use langmeld_core::{Diagnostic, SourceFile, Span};

use crate::{Instruction, InstructionKind, Program};

pub const TAPE_LENGTH: usize = 30_000;
pub const DEFAULT_STEP_LIMIT: usize = 10_000_000;

struct Runtime<'a> {
    tape: Vec<u8>,
    pointer: usize,
    input: &'a mut dyn Read,
    output: Vec<u8>,
    steps: usize,
    max_steps: usize,
}

pub fn execute(
    source: &SourceFile,
    program: &Program,
    input: &mut dyn Read,
    max_steps: usize,
) -> Result<Vec<u8>, Diagnostic> {
    let mut runtime = Runtime {
        tape: vec![0; TAPE_LENGTH],
        pointer: 0,
        input,
        output: Vec::new(),
        steps: 0,
        max_steps,
    };

    execute_instructions(source, &program.instructions, &mut runtime)?;

    Ok(runtime.output)
}

fn execute_instructions(
    source: &SourceFile,
    instructions: &[Instruction],
    runtime: &mut Runtime<'_>,
) -> Result<(), Diagnostic> {
    for instruction in instructions {
        match &instruction.kind {
            InstructionKind::Increment => {
                tick(source, instruction, runtime)?;
                runtime.tape[runtime.pointer] = runtime.tape[runtime.pointer].wrapping_add(1);
            }
            InstructionKind::Decrement => {
                tick(source, instruction, runtime)?;
                runtime.tape[runtime.pointer] = runtime.tape[runtime.pointer].wrapping_sub(1);
            }
            InstructionKind::MoveRight => {
                tick(source, instruction, runtime)?;
                if runtime.pointer + 1 == TAPE_LENGTH {
                    return Err(Diagnostic::error(
                        "BF-RUNTIME-001",
                        format!("data pointer moved beyond the {TAPE_LENGTH}-cell tape"),
                        source,
                        instruction.span,
                    ));
                }
                runtime.pointer += 1;
            }
            InstructionKind::MoveLeft => {
                tick(source, instruction, runtime)?;
                let Some(pointer) = runtime.pointer.checked_sub(1) else {
                    return Err(Diagnostic::error(
                        "BF-RUNTIME-002",
                        "data pointer moved before the start of the tape",
                        source,
                        instruction.span,
                    ));
                };
                runtime.pointer = pointer;
            }
            InstructionKind::Output => {
                tick(source, instruction, runtime)?;
                runtime.output.push(runtime.tape[runtime.pointer]);
            }
            InstructionKind::Input => {
                tick(source, instruction, runtime)?;
                runtime.tape[runtime.pointer] = read_input_byte(source, instruction, runtime)?;
            }
            InstructionKind::Loop(body) => {
                let opening_bracket = Span::new(instruction.span.start, instruction.span.start + 1);
                let closing_bracket = Span::new(instruction.span.end - 1, instruction.span.end);

                loop {
                    tick_at(source, opening_bracket, runtime)?;
                    if runtime.tape[runtime.pointer] == 0 {
                        break;
                    }

                    execute_instructions(source, body, runtime)?;

                    tick_at(source, closing_bracket, runtime)?;
                    if runtime.tape[runtime.pointer] == 0 {
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}

fn tick(
    source: &SourceFile,
    instruction: &Instruction,
    runtime: &mut Runtime<'_>,
) -> Result<(), Diagnostic> {
    tick_at(source, instruction.span, runtime)
}

fn tick_at(source: &SourceFile, span: Span, runtime: &mut Runtime<'_>) -> Result<(), Diagnostic> {
    if runtime.steps == runtime.max_steps {
        return Err(Diagnostic::error(
            "BF-RUNTIME-003",
            "execution exceeded the configured instruction limit",
            source,
            span,
        ));
    }

    runtime.steps += 1;

    Ok(())
}

fn read_input_byte(
    source: &SourceFile,
    instruction: &Instruction,
    runtime: &mut Runtime<'_>,
) -> Result<u8, Diagnostic> {
    let mut byte = [0];

    loop {
        match runtime.input.read(&mut byte) {
            Ok(0) => return Ok(0),
            Ok(_) => return Ok(byte[0]),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => {
                return Err(Diagnostic::error(
                    "BF-RUNTIME-004",
                    format!("cannot read program input: {error}"),
                    source,
                    instruction.span,
                ));
            }
        }
    }
}

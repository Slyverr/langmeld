use langmeld_core::{Diagnostic, SourceFile, Span};
use langmeld_language::{LanguageParser, LexOutput, Lexer, ParsedDocument};

use crate::{ast::OpenLoop, lexer::BrainfuckLexer};

const MAX_SOURCE_BYTES: usize = 4 * 1024 * 1024;
const MAX_COMMANDS: usize = 1_000_000;
const MAX_NESTING_DEPTH: usize = 128;

/// Brainfuck parser that returns a source-spanned instruction tree.
pub struct BrainfuckParser;

impl LanguageParser for BrainfuckParser {
    type Tree = crate::Program;

    fn parse(&self, source: &SourceFile) -> ParsedDocument<Self::Tree> {
        if source.text().len() > MAX_SOURCE_BYTES {
            return ParsedDocument::new(
                source,
                crate::Program::default(),
                vec![Diagnostic::error(
                    "BF-PARSE-001",
                    format!("source exceeds the {}-byte limit", MAX_SOURCE_BYTES),
                    source,
                    Span::new(0, source.text().len()),
                )],
            );
        }

        let LexOutput {
            tokens,
            mut diagnostics,
        } = BrainfuckLexer.lex_with_token_limit(source, MAX_COMMANDS);
        let mut blocks = vec![OpenLoop {
            start: 0,
            instructions: Vec::new(),
        }];
        let mut skipped_depth = 0usize;

        for token in tokens {
            let text = &source.text()[token.span.start..token.span.end];
            if skipped_depth > 0 {
                match text {
                    "[" => skipped_depth += 1,
                    "]" => skipped_depth -= 1,
                    _ => {}
                }
                continue;
            }

            match text {
                "[" if blocks.len() > MAX_NESTING_DEPTH => {
                    diagnostics.push(Diagnostic::error(
                        "BF-PARSE-002",
                        format!("loop nesting exceeds the limit of {MAX_NESTING_DEPTH}"),
                        source,
                        token.span,
                    ));
                    skipped_depth = 1;
                }
                "[" => blocks.push(OpenLoop {
                    start: token.span.start,
                    instructions: Vec::new(),
                }),
                "]" if blocks.len() == 1 => diagnostics.push(Diagnostic::error(
                    "BF-PARSE-003",
                    "closing loop bracket has no matching opening bracket",
                    source,
                    token.span,
                )),
                "]" => {
                    let block = blocks.pop().expect("a nested loop has an opening block");
                    blocks
                        .last_mut()
                        .expect("the root block remains on the parser stack")
                        .instructions
                        .push(crate::Instruction {
                            kind: crate::InstructionKind::Loop(block.instructions),
                            span: Span::new(block.start, token.span.end),
                        });
                }
                command => {
                    let Some(kind) = instruction_kind(command) else {
                        continue;
                    };
                    blocks
                        .last_mut()
                        .expect("the root block remains on the parser stack")
                        .instructions
                        .push(crate::Instruction {
                            kind,
                            span: token.span,
                        });
                }
            }
        }

        if skipped_depth > 0 {
            diagnostics.push(Diagnostic::error(
                "BF-PARSE-004",
                "loop nesting could not be recovered after exceeding its limit",
                source,
                Span::new(source.text().len(), source.text().len()),
            ));
        }

        while blocks.len() > 1 {
            let block = blocks.pop().expect("a nested loop has an opening block");
            diagnostics.push(Diagnostic::error(
                "BF-PARSE-005",
                "opening loop bracket has no matching closing bracket",
                source,
                Span::new(block.start, block.start + 1),
            ));
        }

        let root = blocks
            .pop()
            .expect("the parser always retains its root block");
        let program = crate::Program {
            instructions: root.instructions,
        };

        ParsedDocument::new(source, program, diagnostics)
    }
}

fn instruction_kind(command: &str) -> Option<crate::InstructionKind> {
    match command {
        "+" => Some(crate::InstructionKind::Increment),
        "-" => Some(crate::InstructionKind::Decrement),
        ">" => Some(crate::InstructionKind::MoveRight),
        "<" => Some(crate::InstructionKind::MoveLeft),
        "." => Some(crate::InstructionKind::Output),
        "," => Some(crate::InstructionKind::Input),
        _ => None,
    }
}

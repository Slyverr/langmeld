use std::io::Cursor;

use langmeld_brainfuck_example::{
    BrainfuckParser, InstructionKind, run, run_with_reader, run_with_step_limit,
};
use langmeld_core::SourceFile;
use langmeld_language::LanguageParser;

#[test]
fn parser_builds_nested_instructions_with_source_spans() {
    let source = SourceFile::new("nested.bf", "+[>+<-]");
    let parsed = BrainfuckParser.parse(&source);

    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(parsed.tree().instructions.len(), 2);
    let loop_instruction = &parsed.tree().instructions[1];
    assert_eq!(
        &source.text()[loop_instruction.span.start..loop_instruction.span.end],
        "[>+<-]"
    );

    let InstructionKind::Loop(body) = &loop_instruction.kind else {
        panic!("expected a loop instruction");
    };
    assert_eq!(body.len(), 4);
}

#[test]
fn runs_the_hello_world_program() {
    let source = SourceFile::new("hello.bf", include_str!("../hello.bf"));

    assert_eq!(
        run(&source, &[]).expect("program should execute"),
        b"Hello World!\n"
    );
}

#[test]
fn ignores_non_command_text_and_runs_nested_loops() {
    let source = SourceFile::new("comments.bf", "+++++ this is a comment [>+++++<-] >.");

    assert_eq!(run(&source, &[]).expect("program should execute"), b"\x19");
}

#[test]
fn reads_bytes_and_writes_zero_after_input_is_exhausted() {
    let source = SourceFile::new("input.bf", ",.,.");

    assert_eq!(run(&source, b"A").expect("program should execute"), b"A\0");
}

#[test]
fn reads_from_a_stream_only_when_the_program_requests_input() {
    let source = SourceFile::new("stream-input.bf", ",.");
    let mut input = Cursor::new(b"B".as_slice());

    assert_eq!(
        run_with_reader(&source, &mut input).expect("program should execute"),
        b"B"
    );
}

#[test]
fn wraps_cell_arithmetic_at_eight_bits() {
    let source = SourceFile::new("wrap.bf", "-.");

    assert_eq!(run(&source, &[]).expect("program should execute"), b"\xff");
}

#[test]
fn reports_unmatched_loop_delimiters_at_the_source() {
    let source = SourceFile::new("unclosed.bf", "++[>+");
    let error = run(&source, &[]).expect_err("unclosed loops are invalid");

    assert!(error.contains("unclosed.bf:1:"));

    let error = run(&SourceFile::new("extra-close.bf", "]"), &[])
        .expect_err("unmatched closing loops are invalid");
    assert!(error.contains("extra-close.bf:1:"));
}

#[test]
fn reports_tape_boundary_and_execution_limit_errors() {
    let pointer_error = run(&SourceFile::new("left.bf", "<"), &[])
        .expect_err("the pointer cannot move before the tape");
    let budget_error = run_with_step_limit(&SourceFile::new("loop.bf", "+[]"), &[], 4)
        .expect_err("an empty nonzero loop must respect the step limit");

    assert!(pointer_error.contains("moved before the start of the tape"));
    assert!(budget_error.contains("instruction limit"));
}

#[test]
fn counts_executed_loop_closing_brackets_in_the_instruction_budget() {
    let source = SourceFile::new("budget.bf", "+++[-]");
    let error = run_with_step_limit(&source, &[], 11)
        .expect_err("all executed Brainfuck commands must count toward the budget");

    assert!(error.contains("budget.bf:1:6"), "{error}");
    assert!(error.contains("instruction limit"), "{error}");
    assert!(run_with_step_limit(&source, &[], 12).is_ok());
}

use langmeld_core::Span;

/// A parsed Brainfuck program.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Program {
    /// Instructions in source order.
    pub instructions: Vec<Instruction>,
}

/// One instruction with its original source range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instruction {
    /// The instruction and any nested loop body.
    pub kind: InstructionKind,
    /// Byte range of the instruction in its source file.
    pub span: Span,
}

/// An executable Brainfuck command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstructionKind {
    /// Increment the current cell with 8-bit wrapping arithmetic.
    Increment,
    /// Decrement the current cell with 8-bit wrapping arithmetic.
    Decrement,
    /// Move one cell to the right.
    MoveRight,
    /// Move one cell to the left.
    MoveLeft,
    /// Write the current cell as one output byte.
    Output,
    /// Read one input byte into the current cell.
    Input,
    /// Repeat nested instructions while the current cell is not zero.
    Loop(Vec<Instruction>),
}

pub(crate) struct OpenLoop {
    pub(crate) start: usize,
    pub(crate) instructions: Vec<Instruction>,
}

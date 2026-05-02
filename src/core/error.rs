use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ForthErrorKind {
    StackUnderflow { exp: usize, found: usize },
    StackOverflow,
    InvalidOpcode(u8),
    UnknownToken(String),
    WordNotFound(String),
    DivideByZero,
    MemoryOOB { addr: usize, limit: usize },
    FileError { context: String, source: String },
    Abort(String),
    OptimizationFailed(String),
    ExecutionStateCorrupted(String),
    Exception(i64),
    // JIT Traps
    JitTrapOverflow,
    JitTrapUnderflow,
    JitTrapMemory,
    JitTrapMagic,
    JitTrapAlignment,
    JitTrapDivZero,
    JitTrapContextNull,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ForthPhase {
    Parsing,
    Compilation,
    Optimization,
    Execution,
    Initialization,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ForthError {
    pub kind: ForthErrorKind,
    pub phase: ForthPhase,
    pub message: String,
}

impl ForthError {
    pub fn new<S: Into<String>>(kind: ForthErrorKind, phase: ForthPhase, message: S) -> Self {
        Self { kind, phase, message: message.into() }
    }
}

pub type ForthResult<T> = Result<T, ForthError>;

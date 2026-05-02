use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ForthErrorKind {
    StackUnderflow { exp: usize, found: usize },
    StackOverflow,
    InvalidOpcode(u8),
    UnknownToken(String),
    WordNotFound(String),
    DivideByZero,
    MemoryOutOfBounds { addr: usize, limit: usize },
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
    pub fn new(kind: ForthErrorKind, phase: ForthPhase, message: &str) -> Self {
        Self { kind, phase, message: message.to_string() }
    }
}

pub type ForthResult<T> = Result<T, ForthError>;

use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ForthErrorKind {
    StackUnderflow,
    StackOverflow,
    InvalidOpcode,
    UnknownToken,
    WordNotFound,
    DivideByZero,
    MemoryOOB { addr: usize, limit: usize },
    FileError,
    Abort,
    InvalidWord,
    OptimizationFailed,
    ExecutionStateCorrupted,
    Exception,
    ReturnStackUnderflow,
    ReturnStackOverflow,
    // JIT Traps
    JitTrapOverflow,
    JitTrapUnderflow,
    JitTrapMemory { addr: u64, limit: u64 },
    JitTrapMagic,
    JitTrapAlignment,
    JitTrapDivZero,
    JitTrapContextNull,
    JitTrapJournalOverflow,
    JitTrapDifferentialFailure,
    JitTrapJumpOOB { target: u64 },
    JumpOutOfBounds { target: usize },
    LoopStackOverflow,
    AlignmentError { addr: usize, required: usize },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ForthPhase {
    Parsing,
    Compilation,
    Optimization,
    Execution,
    Initialization,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForthError {
    pub kind: ForthErrorKind,
    pub phase: ForthPhase,
}

impl ForthError {
    #[inline]
    pub const fn new(kind: ForthErrorKind, phase: ForthPhase) -> Self {
        Self { kind, phase }
    }
}

impl std::fmt::Display for ForthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind {
            ForthErrorKind::MemoryOOB { addr, limit } => 
                write!(f, "Memory OOB: addr {} >= limit {} during {:?}", addr, limit, self.phase),
            ForthErrorKind::JitTrapMemory { addr, limit } =>
                write!(f, "JIT Memory Trap: addr {} >= limit {} during {:?}", addr, limit, self.phase),
            ForthErrorKind::JitTrapJumpOOB { target } =>
                write!(f, "JIT Jump OOB: target {} during {:?}", target, self.phase),
            ForthErrorKind::AlignmentError { addr, required } =>
                write!(f, "Alignment Error: addr {} must be {}-byte aligned during {:?}", addr, required, self.phase),
            _ => write!(f, "{:?} during {:?}", self.kind, self.phase),
        }
    }
}

impl std::error::Error for ForthError {}

pub type ForthResult<T> = Result<T, ForthError>;


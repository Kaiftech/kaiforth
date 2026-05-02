use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExecutionStatus {
    Done,
    Yielded,
    Stop,
    Thrown(i64),
}

#[derive(Debug, Clone, PartialEq)]
pub enum TraceEvent {
    Op(Op, u64),
    EnterWord(usize),
    ExitWord,
    LoopStart,
    LoopEnd,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sym { Unknown, Literal(i64) }

#[repr(u8)] #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Op {
    Noop = 0, Push = 1, PushF = 2, Prim = 3, Call = 4, Ret = 5, Jump = 6, JZ = 7,
    Square = 8, Nip = 9, Tuck = 10, Inc = 11, Dec = 12, IsZero = 13, Drop2 = 14, Dup2 = 15,
    Fetch = 16, Store = 17, ToR = 18, FromR = 19, RFetch = 20, Add = 21, Sub = 22, Mul = 23, Div = 24,
    Do = 25, Loop = 26, PLoop = 27, I = 28, DoesPatch = 29, DoesJump = 30, Stop = 31,
    Dup = 32, Drop = 33, Swap = 34, Over = 35, Rot = 36, Dot = 37, Emit = 38, Cr = 39,
    Yield = 40, Catch = 41, Throw = 42, Eq = 43, Lt = 44, Gt = 45, Super = 49,
}

impl Op {
    pub fn from_u8(v: u8) -> Result<Self, String> {
        match v {
            0 => Ok(Op::Noop), 1 => Ok(Op::Push), 2 => Ok(Op::PushF), 3 => Ok(Op::Prim),
            4 => Ok(Op::Call), 5 => Ok(Op::Ret), 6 => Ok(Op::Jump), 7 => Ok(Op::JZ),
            8 => Ok(Op::Square), 9 => Ok(Op::Nip), 10 => Ok(Op::Tuck), 11 => Ok(Op::Inc),
            12 => Ok(Op::Dec), 13 => Ok(Op::IsZero), 14 => Ok(Op::Drop2), 15 => Ok(Op::Dup2),
            16 => Ok(Op::Fetch), 17 => Ok(Op::Store), 18 => Ok(Op::ToR), 19 => Ok(Op::FromR),
            20 => Ok(Op::RFetch), 21 => Ok(Op::Add), 22 => Ok(Op::Sub), 23 => Ok(Op::Mul),
            24 => Ok(Op::Div), 25 => Ok(Op::Do), 26 => Ok(Op::Loop), 27 => Ok(Op::PLoop),
            28 => Ok(Op::I), 29 => Ok(Op::DoesPatch), 30 => Ok(Op::DoesJump), 31 => Ok(Op::Stop),
            32 => Ok(Op::Dup), 33 => Ok(Op::Drop), 34 => Ok(Op::Swap), 35 => Ok(Op::Over),
            36 => Ok(Op::Rot), 37 => Ok(Op::Dot), 38 => Ok(Op::Emit), 39 => Ok(Op::Cr),
            40 => Ok(Op::Yield), 41 => Ok(Op::Catch), 42 => Ok(Op::Throw), 43 => Ok(Op::Eq),
            44 => Ok(Op::Lt), 45 => Ok(Op::Gt), 49 => Ok(Op::Super),
            _ => Err(format!("Invalid opcode 0x{:X}", v))
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct AddressRange { pub start: i64, pub end: i64 }

impl AddressRange {
    pub fn overlaps(&self, other: &Self) -> bool {
        self.start < other.end && other.start < self.end
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct MemoryAccess {
    pub read_set: Vec<AddressRange>,
    pub write_set: Vec<AddressRange>,
    pub unknown_read: bool,
    pub unknown_write: bool,
    pub alias_barrier: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct SemanticContract {
    pub pop_d: usize,
    pub push_d: usize,
    pub max_d: usize,
    pub pop_r: usize,
    pub push_r: usize,
    pub mem: MemoryAccess,
    pub pure: bool, 
    pub side_effects: bool,
}

impl MemoryAccess {
    pub fn canonicalize(&mut self) {
        self.read_set.sort_by_key(|r| r.start);
        self.write_set.sort_by_key(|r| r.start);
    }
    pub fn has_alias_risk(&self) -> bool {
        self.unknown_read || self.unknown_write || !self.write_set.is_empty()
    }
}

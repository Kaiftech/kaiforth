/*
 * Copyright (c) 2026 kaif(kaiftech)
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionStatus {
    Done,
    Yielded,
    Stop,
    Thrown(i64),
}

#[derive(Debug, Clone, PartialEq)]
pub enum TraceEvent {
    Op(usize, Op, u64),
    EnterWord(usize),
    ExitWord,
    LoopStart,
    LoopEnd,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sym { Unknown, Literal(i64) }

/// Supported Virtual Machine Instructions (Opcodes).
/// 
/// Instructions are 1-byte tags followed by optional 8-byte data in the `CodeBuf`.
#[repr(u8)] 
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Op {
    /// Do nothing.
    Noop = 0, 
    /// Push literal `i64` from data field to data stack.
    Push = 1, 
    /// Push literal `f64` from data field to float stack.
    PushF = 2, 
    /// Internal primitive operation marker.
    Prim = 3, 
    /// Call a word by its index.
    Call = 4, 
    /// Return from the current word.
    Ret = 5, 
    /// Unconditional relative jump.
    Jump = 6,
    /// Conditional relative jump (if TOS is zero).
    JZ = 7, 
    /// Remainder of division.
    Mod = 8, 
    /// Quotient and Remainder of division.
    DivMod = 9, 
    /// Execute an XT (word index) popped from the stack.
    Execute = 10, 
    /// Push the current data stack depth.
    Depth = 11,
    Square = 12, Nip = 13, Tuck = 14, Inc = 15, Dec = 16, IsZero = 17, Drop2 = 18, Dup2 = 19,
    Fetch = 20, Store = 21, FetchC = 22, StoreC = 23, ToR = 24, FromR = 25, RFetch = 26, 
    Add = 27, Sub = 28, Mul = 29, Div = 30, Do = 31, Loop = 32, PLoop = 33, I = 34, 
    DoesPatch = 35, DoesJump = 36, Stop = 37, Dup = 38, Drop = 39, Swap = 40, Over = 41, 
    Rot = 42, Dot = 43, Emit = 44, Cr = 45, Yield = 46, Catch = 47, Throw = 48, 
    Eq = 49, Lt = 50, Gt = 51, Pick = 52, Roll = 53, Max = 54, Min = 55, Abs = 56, Negate = 57,
    And = 58, Or = 59, Xor = 60, Invert = 61, LShift = 62, RShift = 63,
    Here = 64, Allot = 65, Comma = 66, CompileComma = 67, J = 68, Leave = 69, BL = 70, Super = 127,
}

impl Op {
    #[inline(always)]
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Op::Noop), 1 => Some(Op::Push), 2 => Some(Op::PushF), 3 => Some(Op::Prim),
            4 => Some(Op::Call), 5 => Some(Op::Ret), 6 => Some(Op::Jump), 7 => Some(Op::JZ),
            8 => Some(Op::Mod), 9 => Some(Op::DivMod), 10 => Some(Op::Execute), 11 => Some(Op::Depth),
            12 => Some(Op::Square), 13 => Some(Op::Nip), 14 => Some(Op::Tuck), 15 => Some(Op::Inc),
            16 => Some(Op::Dec), 17 => Some(Op::IsZero), 18 => Some(Op::Drop2), 19 => Some(Op::Dup2),
            20 => Some(Op::Fetch), 21 => Some(Op::Store), 22 => Some(Op::FetchC), 23 => Some(Op::StoreC),
            24 => Some(Op::ToR), 25 => Some(Op::FromR), 26 => Some(Op::RFetch), 27 => Some(Op::Add),
            28 => Some(Op::Sub), 29 => Some(Op::Mul), 30 => Some(Op::Div), 31 => Some(Op::Do),
            32 => Some(Op::Loop), 33 => Some(Op::PLoop), 34 => Some(Op::I), 35 => Some(Op::DoesPatch),
            36 => Some(Op::DoesJump), 37 => Some(Op::Stop), 38 => Some(Op::Dup), 39 => Some(Op::Drop),
            40 => Some(Op::Swap), 41 => Some(Op::Over), 42 => Some(Op::Rot), 43 => Some(Op::Dot),
            44 => Some(Op::Emit), 45 => Some(Op::Cr), 46 => Some(Op::Yield), 47 => Some(Op::Catch),
            48 => Some(Op::Throw), 49 => Some(Op::Eq), 50 => Some(Op::Lt), 51 => Some(Op::Gt),
            52 => Some(Op::Pick), 53 => Some(Op::Roll), 54 => Some(Op::Max), 55 => Some(Op::Min),
            56 => Some(Op::Abs), 57 => Some(Op::Negate), 58 => Some(Op::And), 59 => Some(Op::Or),
            60 => Some(Op::Xor), 61 => Some(Op::Invert), 62 => Some(Op::LShift), 63 => Some(Op::RShift),
            64 => Some(Op::Here), 65 => Some(Op::Allot), 66 => Some(Op::Comma), 67 => Some(Op::CompileComma),
            68 => Some(Op::J), 69 => Some(Op::Leave), 70 => Some(Op::BL),
            127 => Some(Op::Super),
            _ => None
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

impl MemoryAccess {
    pub fn canonicalize(&mut self) {
        self.read_set.sort_by_key(|r| r.start);
        self.write_set.sort_by_key(|r| r.start);
    }
    pub fn has_alias_risk(&self) -> bool {
        if self.unknown_read || self.unknown_write { return true; }
        for w in &self.write_set {
            for r in &self.read_set {
                if w.overlaps(r) { return true; }
            }
        }
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct SemanticContract {
    pub pop_d: usize,
    pub push_d: usize,
    pub max_d: usize,
    pub pop_r: usize,
    pub push_r: usize,
    pub max_r: usize,
    pub mem: MemoryAccess,
    pub pure: bool, 
    pub side_effects: bool,
}


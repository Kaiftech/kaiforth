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

use std::collections::HashMap;
use crate::core::error::ForthResult;
use crate::core::types::{Op, TraceEvent};
use crate::vm::memory::Memory;
use crate::optimizer::analysis::OptimizerState;
use crate::jit::runtime::JitEngine;

#[derive(Clone)]
pub enum WordKind {
    Primitive(usize),
    Defined(usize),
    Variable(usize),
    Created(usize),
}

#[derive(Clone)]
pub struct WordEntry {
    pub name: String,
    pub kind: WordKind,
    pub is_immediate: bool,
    pub is_hidden: bool,
}

#[derive(Clone)]
pub struct CodeBuf { pub ops: Vec<u8>, pub data: Vec<u64> }
impl CodeBuf {
    pub fn new() -> Self { Self { ops: Vec::new(), data: Vec::new() } }
    pub fn push(&mut self, op: Op, val: u64) { self.ops.push(op as u8); self.data.push(val); }
    pub fn patch_data(&mut self, addr: usize, val: u64) {
        if addr < self.data.len() {
            self.data[addr] = val;
        }
    }
    pub fn len(&self) -> usize { self.ops.len() }
    pub fn clear(&mut self) { self.ops.clear(); self.data.clear(); }
}

#[derive(Clone)]
pub struct WordList { pub name: String, pub words: HashMap<String, usize> }

#[derive(Clone)]
pub struct Dictionary {
    pub entries: Vec<WordEntry>,
    pub wordlists: Vec<WordList>,
    pub search_order: Vec<usize>,
    pub current: usize,
    pub latest_word: Option<usize>,
}

impl std::ops::Index<usize> for Dictionary {
    type Output = WordEntry;
    fn index(&self, index: usize) -> &Self::Output { &self.entries[index] }
}
impl Dictionary {
    pub fn new() -> Self {
        Self { entries: Vec::new(), wordlists: vec![WordList { name: "forth".into(), words: HashMap::new() }], search_order: vec![0], current: 0, latest_word: None }
    }
    pub fn insert(&mut self, name: String, kind: WordKind) -> usize { 
        let idx = self.entries.len();
        self.entries.push(WordEntry {
            name: name.clone(),
            kind,
            is_immediate: false,
            is_hidden: false,
        });
        self.wordlists[self.current].words.insert(name.to_lowercase(), idx); 
        self.latest_word = Some(idx);
        idx
    }
    
    pub fn set_immediate(&mut self, idx: usize) {
        if let Some(entry) = self.entries.get_mut(idx) {
            entry.is_immediate = true;
        }
    }

    pub fn set_hidden(&mut self, idx: usize, hidden: bool) {
        if let Some(entry) = self.entries.get_mut(idx) {
            entry.is_hidden = hidden;
        }
    }
    pub fn lookup(&self, name: &str) -> Option<usize> {
        let name = name.to_lowercase();
        for &wl_idx in self.search_order.iter().rev() {
            if let Some(&idx) = self.wordlists[wl_idx].words.get(&name) { return Some(idx); }
        }
        None
    }
}

pub struct System {
    pub compiling: bool,
    pub dict: Dictionary,
    pub memory: Memory,
    pub code: CodeBuf,
    pub tr_code: CodeBuf,
    pub files: HashMap<i64, std::fs::File>,
    pub next_file_id: i64,
    /// The JIT compilation and optimization engine.
    pub optimizer: OptimizerState,
    /// Current execution trace for the optimizer.
    pub runtime_trace: Vec<TraceEvent>,
    pub jit: JitEngine,
    pub base: i64,
    pub trace_enabled: bool,
    pub paranoid_mode: bool,
    pub jit_hits: u64,
    pub jit_poisoned: u64,
    pub jit_rollbacks: u64,
    pub virtual_time: u64,
    pub seed: u64,
    /// Compilation control stack for resolving jump targets (IF/ELSE/THEN).
    pub control_stack: Vec<usize>,
}

impl System {
    /// Creates a new System with the specified memory size.
    /// 
    /// # Parameters
    /// - `mem_size`: Size of the byte-addressable memory in bytes.
    pub fn new(mem_size: usize) -> ForthResult<Self> {
        Ok(Self {
            compiling: false,
            dict: Dictionary::new(),
            memory: Memory::try_new(mem_size)?,
            code: CodeBuf::new(),
            tr_code: CodeBuf::new(),
            files: HashMap::new(),
            next_file_id: 1,
            optimizer: OptimizerState::new(),
            runtime_trace: Vec::new(),
            jit: JitEngine::new(),
            base: 10,
            trace_enabled: false,
            paranoid_mode: false,
            jit_hits: 0,
            jit_poisoned: 0,
            jit_rollbacks: 0,
            virtual_time: 0,
            seed: 0xDEADBEEF,
            control_stack: Vec::new(),
        })
    }

    /// Creates an isolated clone of the system state (memory + dict + code) for differential audits.
    pub fn try_clone_isolated(&self) -> ForthResult<Self> {
        Ok(Self {
            compiling: self.compiling,
            dict: self.dict.clone(),
            memory: self.memory.try_clone()?,
            code: self.code.clone(),
            tr_code: self.tr_code.clone(),
            files: HashMap::new(), // Files are not shared with isolated clones
            next_file_id: self.next_file_id,
            optimizer: OptimizerState::new(), // Optimizer is irrelevant for audits
            runtime_trace: Vec::new(),
            jit: JitEngine::new(), // JIT is not used in Tier 1 audits
            base: self.base,
            trace_enabled: false,
            paranoid_mode: false,
            jit_hits: 0,
            jit_poisoned: 0,
            jit_rollbacks: 0,
            virtual_time: self.virtual_time,
            seed: self.seed,
            control_stack: Vec::new(),
        })
    }

    pub fn synchronize_jit(&mut self) -> ForthResult<()> {
        let pending: Vec<(usize, crate::optimizer::analysis::PatternContext)> = self.optimizer.pending_jit.drain(..).collect();
        for (ip, ctx) in pending {
            if let Some(contract) = ctx.contract.clone() {
                let ops = ctx.ops.clone();
                match self.jit.compile_super(ip, &ops, &contract, Some(ctx)) {
                    Ok(_) => { if self.paranoid_mode { println!("[DEBUG] JIT Compiled block at IP: {}", ip); } }
                    Err(e) => { if self.paranoid_mode { println!("[DEBUG] JIT Compile FAILED at IP: {}: {:?}", ip, e); } }
                }
            }
        }
        Ok(())
    }

    pub fn register_core(&mut self) {
        let ops = vec![
            ("dup", Op::Dup), ("drop", Op::Drop), ("swap", Op::Swap), ("over", Op::Over),
            ("rot", Op::Rot), ("nip", Op::Nip), ("tuck", Op::Tuck), ("2drop", Op::Drop2),
            ("2dup", Op::Dup2), ("+", Op::Add), ("-", Op::Sub), ("*", Op::Mul), ("/", Op::Div),
            ("mod", Op::Mod), ("/mod", Op::DivMod), ("1+", Op::Inc), ("1-", Op::Dec),
            ("sq", Op::Square), ("0=", Op::IsZero), ("depth", Op::Depth),
            ("@", Op::Fetch), ("!", Op::Store), ("c@", Op::FetchC), ("c!", Op::StoreC),
            ("=", Op::Eq), ("<", Op::Lt), (">", Op::Gt), ("execute", Op::Execute),
            ("pick", Op::Pick), ("roll", Op::Roll), ("max", Op::Max), ("min", Op::Min),
            ("abs", Op::Abs), ("negate", Op::Negate), ("and", Op::And), ("or", Op::Or),
            ("xor", Op::Xor), ("invert", Op::Invert), ("lshift", Op::LShift), ("rshift", Op::RShift),
            ("here", Op::Here), ("allot", Op::Allot), (",", Op::Comma), ("compile,", Op::CompileComma),
            ("i", Op::I), ("j", Op::J), ("bl", Op::BL),
            (".", Op::Dot), ("emit", Op::Emit), ("cr", Op::Cr), ("yield", Op::Yield),
            ("catch", Op::Catch), ("throw", Op::Throw), ("stop", Op::Stop),
        ];
        for (name, op) in ops {
            self.dict.insert(name.to_string(), WordKind::Primitive(op as usize));
        }
    }
}


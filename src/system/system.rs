use std::collections::HashMap;
use crate::core::error::{ForthResult, ForthError, ForthErrorKind, ForthPhase};
use crate::core::types::{Op, ExecutionStatus, TraceEvent};
use crate::vm::state::{Vm, WordEntry, WordKind};
use crate::vm::memory::Memory;
use crate::optimizer::analysis::OptimizerState;
use crate::jit::runtime::JitEngine;

pub struct CodeBuf { pub ops: Vec<u8>, pub data: Vec<u64> }
impl CodeBuf {
    pub fn new() -> Self { Self { ops: Vec::new(), data: Vec::new() } }
    pub fn push(&mut self, op: Op, val: u64) { self.ops.push(op as u8); self.data.push(val); }
    pub fn clear(&mut self) { self.ops.clear(); self.data.clear(); }
}

pub struct WordList { pub name: String, pub words: HashMap<String, usize> }
pub struct Dictionary {
    pub entries: Vec<WordEntry>,
    pub wordlists: Vec<WordList>,
    pub search_order: Vec<usize>,
    pub current: usize,
}

impl std::ops::Index<usize> for Dictionary {
    type Output = WordEntry;
    fn index(&self, index: usize) -> &Self::Output { &self.entries[index] }
}
impl Dictionary {
    pub fn new() -> Self {
        Self { entries: Vec::new(), wordlists: vec![WordList { name: "forth".into(), words: HashMap::new() }], search_order: vec![0], current: 0 }
    }
    pub fn insert(&mut self, name: String, idx: usize) { self.wordlists[self.current].words.insert(name.to_lowercase(), idx); }
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
    pub optimizer: OptimizerState,
    pub runtime_trace: Vec<TraceEvent>,
    pub jit: JitEngine,
    pub base: i64,
}

impl System {
    pub fn new() -> ForthResult<Self> {
        Ok(Self {
            compiling: false,
            dict: Dictionary::new(),
            memory: Memory::try_new(1024 * 1024)?,
            code: CodeBuf::new(),
            tr_code: CodeBuf::new(),
            files: HashMap::new(),
            next_file_id: 1,
            optimizer: OptimizerState::new(),
            runtime_trace: Vec::new(),
            jit: JitEngine::new(),
            base: 10,
        })
    }
}

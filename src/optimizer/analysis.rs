use std::collections::{HashMap, HashSet};
use serde::{Serialize, Deserialize};
use crate::core::types::{Op, SemanticContract, TraceEvent};
use crate::core::error::{ForthResult, ForthError, ForthErrorKind, ForthPhase};

use crate::optimizer::contract::validate_sequence_contract;
use crate::optimizer::segmentation::build_safe_segments;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq, Hash)]
pub struct PatternContext {
    pub word_id: usize,
    pub loop_depth: usize,
    pub original_ops: Vec<(Op, u64)>,
    pub ops: Vec<(Op, u64)>,
    pub contract: Option<SemanticContract>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct PatternStats {
    pub successes: u64,
    pub failures: u64,
    pub score: i64,
    pub instructions_saved: u64,
    pub jit_cycles: u64,
    pub fallback_cycles: u64,
    pub jit_executions: u64,
    pub fallback_executions: u64,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CodePatch {
    pub offset: usize,
    pub original_ops: Vec<u8>,
    pub original_data: Vec<u64>,
}

#[derive(Serialize, Deserialize)]
pub struct PersistenceContainer {
    pub magic: [u8; 8],
    pub version: u32,
    pub arch_id: u32, // 1: x86_64, 2: aarch64
    pub state: OptimizerState,
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct OptimizerState {
    pub super_instructions: Vec<PatternContext>,
    pub pending_jit: Vec<(usize, PatternContext)>,
    pub context_patterns: HashMap<PatternContext, PatternStats>,
    pub active_patches: HashMap<usize, CodePatch>,
    pub call_frequencies: HashMap<usize, u64>,
    pub dependencies: HashMap<usize, HashSet<usize>>,
    pub decay_counter: u64,
    pub eval_count: u64,
    pub next_opt_threshold: u64,
    pub score_threshold: i64,
    pub is_frozen: bool,
}

pub const MAX_PENDING_JIT: usize = 128;
pub const MAX_PATTERNS: usize = 1024;

impl OptimizerState {
    pub fn new() -> Self {
        Self {
            super_instructions: Vec::new(), pending_jit: Vec::new(), context_patterns: HashMap::new(),
            active_patches: HashMap::new(), call_frequencies: HashMap::new(),
            dependencies: HashMap::new(),
            decay_counter: 0, eval_count: 0, next_opt_threshold: 100, score_threshold: 100,
            is_frozen: false,
        }
    }

    pub fn freeze(&mut self) {
        self.is_frozen = true;
    }

    pub fn should_run_optimizer(&mut self) -> bool {
        if self.is_frozen { return false; }
        self.eval_count += 1;
        if self.eval_count >= self.next_opt_threshold {
            self.next_opt_threshold += (self.next_opt_threshold / 2).max(50);
            return true;
        }
        false
    }

    pub fn penalize_pattern(&mut self, ctx: &PatternContext) {
        if let Some(stats) = self.context_patterns.get_mut(ctx) {
            stats.failures += 1;
            stats.score -= 50;
        }
    }

    pub fn save_to_file(&self, path: &str) -> ForthResult<()> {
        let container = PersistenceContainer {
            magic: *b"KAIFORTH",
            version: 8,
            arch_id: if cfg!(target_arch = "x86_64") { 1 } else { 0 },
            state: self.clone(),
        };
        let json = serde_json::to_string(&container)
            .map_err(|_| ForthError::new(ForthErrorKind::ExecutionStateCorrupted, ForthPhase::Optimization))?;
        std::fs::write(path, json)
            .map_err(|_| ForthError::new(ForthErrorKind::ExecutionStateCorrupted, ForthPhase::Optimization))?;
        Ok(())
    }

    pub fn load_from_file(path: &str) -> ForthResult<Self> {
        let json = std::fs::read_to_string(path)
            .map_err(|_| ForthError::new(ForthErrorKind::ExecutionStateCorrupted, ForthPhase::Optimization))?;
        let container: PersistenceContainer = serde_json::from_str(&json)
            .map_err(|_| ForthError::new(ForthErrorKind::ExecutionStateCorrupted, ForthPhase::Optimization))?;
        
        if container.magic != *b"KAIFORTH" || container.version != 8 {
            return Err(ForthError::new(ForthErrorKind::Abort, ForthPhase::Optimization));
        }
        
        Ok(container.state)
    }

    pub fn observe_runtime_traces(&mut self, trace_buffer: &mut Vec<TraceEvent>) {
        if self.is_frozen || trace_buffer.len() < 2 { return; }
        let mut active_words = Vec::new();
        let mut loop_depth = 0;
        let mut current_seq = Vec::new();
        let mut current_ips = Vec::new();

        for ev in trace_buffer.iter() {
            match ev {
                TraceEvent::Op(ip, op, data) => {
                    current_seq.push((*op, *data));
                    current_ips.push(*ip);
                }
                TraceEvent::LoopStart => { 
                    loop_depth += 1; 
                    self.extract_patterns(&current_seq, &current_ips, active_words.last().copied().unwrap_or(usize::MAX), loop_depth); 
                    current_seq.clear(); current_ips.clear();
                }
                TraceEvent::LoopEnd => { 
                    self.extract_patterns(&current_seq, &current_ips, active_words.last().copied().unwrap_or(usize::MAX), loop_depth); 
                    if loop_depth > 0 { loop_depth -= 1; } 
                    current_seq.clear(); current_ips.clear();
                }
                TraceEvent::EnterWord(idx) => { 
                    if let Some(&parent) = active_words.last() { self.dependencies.entry(parent).or_insert_with(HashSet::new).insert(*idx); }
                    active_words.push(*idx);
                    *self.call_frequencies.entry(*idx).or_insert(0) += 1;
                    self.extract_patterns(&current_seq, &current_ips, *idx, loop_depth); 
                    current_seq.clear(); current_ips.clear();
                }
                TraceEvent::ExitWord => { 
                    self.extract_patterns(&current_seq, &current_ips, active_words.last().copied().unwrap_or(usize::MAX), loop_depth); 
                    active_words.pop();
                    current_seq.clear(); current_ips.clear();
                }
            }
        }
        self.extract_patterns(&current_seq, &current_ips, *active_words.last().unwrap_or(&usize::MAX), loop_depth);
        self.decay_counter += trace_buffer.len() as u64;
        trace_buffer.clear();

        if self.decay_counter > 200000 {
            for stats in self.context_patterns.values_mut() {
                let confidence = (stats.jit_executions as f64 / 1000.0).min(1.0);
                let decay_rate = 0.85 + (0.10 * confidence);
                stats.score = (stats.score as f64 * decay_rate) as i64;
            }
            self.context_patterns.retain(|_, stats| stats.score > 0 || stats.failures > 0);
            for count in self.call_frequencies.values_mut() { *count /= 2; }
            self.call_frequencies.retain(|_, c| *c > 5);
            self.decay_counter = 0;
        }
    }

    fn extract_patterns(&mut self, flat_ops: &[(Op, u64)], ips: &[usize], word_id: usize, loop_depth: usize) {
        if flat_ops.is_empty() { return; }
        let segments = build_safe_segments(flat_ops);
        let depth_multiplier = (1 + loop_depth) as i64;

        let mut current_offset = 0;
        for segment in segments {
            let seq = segment.ops;
            if current_offset >= ips.len() { break; }
            let start_ip = ips[current_offset];
            current_offset += seq.len();

            if let Some((contract, folded_opt)) = validate_sequence_contract(&seq) {
                let actual_seq = folded_opt.unwrap_or_else(|| seq.clone());
                let ctx = PatternContext { word_id, loop_depth, original_ops: seq, ops: actual_seq, contract: Some(contract) };
                let stats = self.context_patterns.entry(ctx.clone()).or_insert(PatternStats { 
                    successes: 0, failures: 0, score: 0, instructions_saved: 0,
                    jit_cycles: 0, fallback_cycles: 0, jit_executions: 0, fallback_executions: 0
                });
                if stats.failures > 5 { continue; } 
                stats.score += depth_multiplier;
                if stats.score >= self.score_threshold {
                    if !self.super_instructions.contains(&ctx) {
                        self.super_instructions.push(ctx.clone());
                    }
                    if self.pending_jit.len() < MAX_PENDING_JIT {
                        self.pending_jit.push((start_ip, ctx));
                    }
                }
            }
        }
    }
}

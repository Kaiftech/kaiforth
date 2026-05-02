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
    pub magic: String,
    pub version: u32,
    pub state: OptimizerState,
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct OptimizerState {
    pub super_instructions: Vec<PatternContext>,
    pub context_patterns: HashMap<PatternContext, PatternStats>,
    pub active_patches: HashMap<usize, CodePatch>,
    pub call_frequencies: HashMap<usize, u64>,
    pub dependencies: HashMap<usize, HashSet<usize>>,
    pub decay_counter: u64,
    pub eval_count: u64,
    pub next_opt_threshold: u64,
}

impl OptimizerState {
    pub fn new() -> Self {
        Self {
            super_instructions: Vec::new(), context_patterns: HashMap::new(),
            active_patches: HashMap::new(), call_frequencies: HashMap::new(),
            dependencies: HashMap::new(),
            decay_counter: 0, eval_count: 0, next_opt_threshold: 100,
        }
    }

    pub fn should_run_optimizer(&mut self) -> bool {
        self.eval_count += 1;
        if self.eval_count >= self.next_opt_threshold {
            self.next_opt_threshold += (self.next_opt_threshold / 2).max(50);
            return true;
        }
        false
    }

    pub fn save_to_file(&self, path: &str) -> ForthResult<()> {
        let container = PersistenceContainer {
            magic: "KAIFORTH".to_string(),
            version: 6,
            state: self.clone(),
        };
        let json = serde_json::to_string(&container)
            .map_err(|e| ForthError::new(ForthErrorKind::ExecutionStateCorrupted(e.to_string()), ForthPhase::Optimization, "Persistence Failure"))?;
        std::fs::write(path, json)
            .map_err(|e| ForthError::new(ForthErrorKind::ExecutionStateCorrupted(e.to_string()), ForthPhase::Optimization, "File Write Failure"))?;
        Ok(())
    }

    pub fn load_from_file(path: &str) -> ForthResult<Self> {
        let json = std::fs::read_to_string(path)
            .map_err(|e| ForthError::new(ForthErrorKind::ExecutionStateCorrupted(e.to_string()), ForthPhase::Optimization, "File Read Failure"))?;
        let container: PersistenceContainer = serde_json::from_str(&json)
            .map_err(|e| ForthError::new(ForthErrorKind::ExecutionStateCorrupted(e.to_string()), ForthPhase::Optimization, "Deserialization Failure"))?;
        
        if container.magic != "KAIFORTH" || container.version != 6 {
            return Err(ForthError::new(ForthErrorKind::Abort("Incompatible Persistence File".into()), ForthPhase::Optimization, "Version Mismatch"));
        }
        
        Ok(container.state)
    }

    pub fn observe_runtime_traces(&mut self, trace_buffer: &mut Vec<TraceEvent>) {
        if trace_buffer.len() < 2 { return; }
        let mut active_words = Vec::new();
        let mut loop_depth = 0;
        let mut current_seq = Vec::new();

        for ev in trace_buffer.iter() {
            match ev {
                TraceEvent::Op(op, data) => current_seq.push((*op, *data)),
                TraceEvent::LoopStart => { 
                    loop_depth += 1; 
                    self.extract_patterns(&current_seq, active_words.last().copied().unwrap_or(usize::MAX), loop_depth); 
                    current_seq.clear(); 
                }
                TraceEvent::LoopEnd => { 
                    self.extract_patterns(&current_seq, active_words.last().copied().unwrap_or(usize::MAX), loop_depth); 
                    if loop_depth > 0 { loop_depth -= 1; } 
                    current_seq.clear(); 
                }
                TraceEvent::EnterWord(idx) => { 
                    if let Some(&parent) = active_words.last() { self.dependencies.entry(parent).or_insert_with(HashSet::new).insert(*idx); }
                    active_words.push(*idx);
                    *self.call_frequencies.entry(*idx).or_insert(0) += 1;
                    self.extract_patterns(&current_seq, *idx, loop_depth); 
                    current_seq.clear(); 
                }
                TraceEvent::ExitWord => { 
                    self.extract_patterns(&current_seq, active_words.last().copied().unwrap_or(usize::MAX), loop_depth); 
                    active_words.pop();
                    current_seq.clear(); 
                }
            }
        }
        self.extract_patterns(&current_seq, *active_words.last().unwrap_or(&usize::MAX), loop_depth);
        self.decay_counter += trace_buffer.len() as u64;
        trace_buffer.clear();

        if self.decay_counter > 200000 {
            for stats in self.context_patterns.values_mut() {
                let confidence = (stats.jit_executions as f64 / 1000.0).min(1.0);
                let decay_rate = 0.85 + (0.10 * confidence);
                stats.score = (stats.score as f64 * decay_rate) as i64;
                stats.jit_cycles = (stats.jit_cycles as f64 * 0.9) as u64;
                stats.fallback_cycles = (stats.fallback_cycles as f64 * 0.9) as u64;
            }
            self.context_patterns.retain(|_, stats| stats.score > 0 || stats.failures > 0);
            for count in self.call_frequencies.values_mut() { *count /= 2; }
            self.call_frequencies.retain(|_, c| *c > 5);
            self.decay_counter = 0;
        }
    }

    fn extract_patterns(&mut self, flat_ops: &[(Op, u64)], word_id: usize, loop_depth: usize) {
        if flat_ops.is_empty() { return; }
        let segments = build_safe_segments(flat_ops);
        let depth_multiplier = (1 + loop_depth) as i64;

        for segment in segments {
            let seq = segment.ops;
            if let Some((contract, folded_opt)) = validate_sequence_contract(&seq) {
                let actual_seq = folded_opt.unwrap_or_else(|| seq.clone());
                let ctx = PatternContext { word_id, loop_depth, original_ops: seq, ops: actual_seq, contract: Some(contract) };
                let stats = self.context_patterns.entry(ctx.clone()).or_insert(PatternStats { 
                    successes: 0, failures: 0, score: 0, instructions_saved: 0,
                    jit_cycles: 0, fallback_cycles: 0, jit_executions: 0, fallback_executions: 0
                });
                if stats.failures > 5 { continue; } 
                stats.score += depth_multiplier;
                if stats.score >= 100 && !self.super_instructions.contains(&ctx) {
                    self.super_instructions.push(ctx);
                }
            }
        }
    }
}

use crate::core::types::{Op, SemanticContract};

pub struct SafeSegment {
    pub ops: Vec<(Op, u64)>,
    pub contract: SemanticContract,
}

pub fn build_safe_segments(seq: &[(Op, u64)]) -> Vec<SafeSegment> {
    let mut segments = Vec::new();
    let mut current_ops = Vec::new();
    let mut current_depth = 0i64;
    let mut min_depth = 0i64;
    let mut max_depth = 0i64;
    
    for (op, data) in seq {
        let contract = match op.inline_contract() {
            Some(c) => c,
            None => {
                // Cannot inline/optimize across this op, flush current segment
                if !current_ops.is_empty() {
                    segments.push(finalize_segment(current_ops, min_depth, max_depth, current_depth));
                    current_ops = Vec::new();
                    current_depth = 0; min_depth = 0; max_depth = 0;
                }
                continue;
            }
        };

        // SAFETY RULE: A sequence is only JIT-safe if every intermediate state is safe.
        // If an operation would cause an intermediate underflow relative to the segment start, SPLIT.
        if current_depth - (contract.pop_d as i64) < 0 {
            if !current_ops.is_empty() {
                segments.push(finalize_segment(current_ops, min_depth, max_depth, current_depth));
                current_ops = Vec::new();
                current_depth = 0; min_depth = 0; max_depth = 0;
            }
        }

        // Splitting on side effects or barriers to ensure atomicity of optimized blocks
        if contract.side_effects || contract.mem.alias_barrier {
            if !current_ops.is_empty() {
                segments.push(finalize_segment(current_ops, min_depth, max_depth, current_depth));
                current_ops = Vec::new();
                current_depth = 0; min_depth = 0; max_depth = 0;
            }
        }

        current_ops.push((*op, *data));
        current_depth -= contract.pop_d as i64;
        if current_depth < min_depth { min_depth = current_depth; }
        current_depth += contract.push_d as i64;
        if current_depth > max_depth { max_depth = current_depth; }
    }

    if !current_ops.is_empty() {
        segments.push(finalize_segment(current_ops, min_depth, max_depth, current_depth));
    }

    segments
}

fn finalize_segment(ops: Vec<(Op, u64)>, _min_d: i64, _max_d: i64, _final_d: i64) -> SafeSegment {
    // Re-calculate the ACTUAL contract for this segment
    if let Some((contract, _)) = crate::optimizer::contract::validate_sequence_contract(&ops) {
        SafeSegment { ops, contract }
    } else {
        // Fallback to safe but conservative contract if validation fails
        SafeSegment { 
            ops, 
            contract: SemanticContract {
                pop_d: 0, push_d: 0, max_d: 1024, pop_r: 0, push_r: 0,
                mem: crate::core::types::MemoryAccess {
                    read_set: Vec::new(), write_set: Vec::new(),
                    unknown_read: true, unknown_write: true, alias_barrier: true
                },
                pure: false, side_effects: true,
            }
        }
    }
}

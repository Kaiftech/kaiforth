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
                if !current_ops.is_empty() {
                    if let Some(s) = finalize_segment(current_ops, min_depth, max_depth, current_depth) {
                        segments.push(s);
                    }
                    current_ops = Vec::new();
                    current_depth = 0; min_depth = 0; max_depth = 0;
                }
                continue;
            }
        };

        if current_depth - (contract.pop_d as i64) < 0 {
            if !current_ops.is_empty() {
                if let Some(s) = finalize_segment(current_ops, min_depth, max_depth, current_depth) {
                    segments.push(s);
                }
                current_ops = Vec::new();
                current_depth = 0; min_depth = 0; max_depth = 0;
            }
        }

        if *op == Op::Jump || *op == Op::JZ {
            let target_idx_i = current_ops.len() as i64 + 1 + *data as i64;
            if target_idx_i < 0 || target_idx_i >= (current_ops.len() + 32) as i64 {
                 if !current_ops.is_empty() {
                    if let Some(s) = finalize_segment(current_ops, min_depth, max_depth, current_depth) {
                        segments.push(s);
                    }
                    current_ops = Vec::new();
                    current_depth = 0; min_depth = 0; max_depth = 0;
                }
            }
        }

        current_ops.push((*op, *data));
        current_depth -= contract.pop_d as i64;
        if current_depth < min_depth { min_depth = current_depth; }
        current_depth += contract.push_d as i64;
        if current_depth > max_depth { max_depth = current_depth; }
    }

    if !current_ops.is_empty() {
        if let Some(s) = finalize_segment(current_ops, min_depth, max_depth, current_depth) {
            segments.push(s);
        }
    }

    segments
}

fn finalize_segment(ops: Vec<(Op, u64)>, _min_d: i64, _max_d: i64, _final_d: i64) -> Option<SafeSegment> {
    if let Some((contract, _)) = crate::optimizer::contract::validate_sequence_contract(&ops) {
        Some(SafeSegment { ops, contract })
    } else {
        None
    }
}

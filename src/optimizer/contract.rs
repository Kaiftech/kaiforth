use crate::core::types::{Op, SemanticContract, MemoryAccess, AddressRange, Sym};
use std::collections::HashSet;

impl Op {
    pub fn inline_contract(&self) -> Option<SemanticContract> {
        let sc = |pop_d, push_d, max_d, pop_r, push_r, max_r, reads, writes, alias, pure, side| Some(SemanticContract {
            pop_d, push_d, max_d, pop_r, push_r, max_r,
            mem: MemoryAccess { 
                read_set: Vec::new(), write_set: Vec::new(), 
                unknown_read: reads, unknown_write: writes, 
                alias_barrier: alias 
            }, pure, side_effects: side
        });
        
        match self {
            Op::Noop | Op::Stop => sc(0, 0, 0, 0, 0, 0, false, false, false, true, false),
            Op::Push | Op::PushF => sc(0, 1, 1, 0, 0, 0, false, false, false, true, false),
            Op::Drop => sc(1, 0, 0, 0, 0, 0, false, false, false, true, false),
            Op::Dup => sc(1, 2, 1, 0, 0, 0, false, false, false, true, false), 
            Op::Square | Op::Inc | Op::Dec | Op::IsZero => sc(1, 1, 0, 0, 0, 0, false, false, false, true, false), 
            Op::Swap | Op::Over => sc(2, 2, 0, 0, 0, 0, false, false, false, true, false),
            Op::Nip => sc(2, 1, 0, 0, 0, 0, false, false, false, true, false),
            Op::Drop2 => sc(2, 0, 0, 0, 0, 0, false, false, false, true, false),
            Op::Dup2 => sc(2, 4, 2, 0, 0, 0, false, false, false, true, false),
            Op::Tuck => sc(2, 3, 1, 0, 0, 0, false, false, false, true, false),
            Op::Add | Op::Sub | Op::Mul | Op::Div => sc(2, 1, 0, 0, 0, 0, false, false, false, true, false),
            Op::Rot => sc(3, 3, 0, 0, 0, 0, false, false, false, true, false),
            Op::Fetch => sc(1, 1, 0, 0, 0, 0, true, false, false, false, false),
            Op::Store => sc(2, 0, 0, 0, 0, 0, false, true, false, false, true), 
            Op::ToR => sc(1, 0, 0, 0, 1, 1, false, false, false, false, false),
            Op::FromR | Op::RFetch => sc(0, 1, 1, 1, 0, 0, false, false, false, false, false),
            Op::Dot | Op::Emit => sc(1, 0, 0, 0, 0, 0, false, false, true, false, true), 
            Op::Cr => sc(0, 0, 0, 0, 0, 0, false, false, true, false, true),
            Op::Throw => sc(1, 0, 0, 0, 0, 0, false, false, true, false, true),
            Op::Eq | Op::Lt | Op::Gt => sc(2, 1, 2, 0, 0, 0, false, false, false, true, false),
            Op::Jump => sc(0, 0, 0, 0, 0, 0, false, false, false, false, false),
            Op::JZ => sc(1, 0, 0, 0, 0, 0, false, false, false, false, false),
            Op::Do => sc(2, 0, 0, 0, 2, 2, false, false, false, false, true),
            Op::Loop | Op::PLoop => sc(0, 0, 0, 2, 0, 0, false, false, false, false, true),
            Op::I => sc(0, 1, 1, 0, 0, 0, false, false, false, false, false),
            Op::DoesJump => sc(0, 0, 0, 0, 0, 0, false, false, true, false, true),
            Op::DoesPatch => sc(0, 0, 0, 0, 0, 0, false, false, true, false, true),
            _ => None,
        }
    }
}

pub fn validate_sequence_contract(seq: &[(Op, u64)]) -> Option<(SemanticContract, Option<Vec<(Op, u64)>>)> {
    let mut stack_depths = vec![None; seq.len() + 1];
    let mut r_stack_depths = vec![None; seq.len() + 1];
    stack_depths[0] = Some(0i64);
    r_stack_depths[0] = Some(0i64);
    
    let mut min_d = 0i64;
    let mut max_d = 0i64;
    let mut min_r = 0i64;
    let mut max_r = 0i64;
    let mut unknown_read = false;
    let mut unknown_write = false;
    let mut pure = true;
    let mut side_effects = false;

    for i in 0..seq.len() {
        let d_depth = stack_depths[i]?;
        let r_depth = r_stack_depths[i]?;
        
        let (op, data) = seq[i];
        let contract = op.inline_contract()?;
        
        let next_d = d_depth - contract.pop_d as i64;
        if next_d < min_d { min_d = next_d; }
        let final_d = next_d + contract.push_d as i64;
        if final_d > max_d { max_d = final_d; }

        let next_r = r_depth - contract.pop_r as i64;
        if next_r < min_r { min_r = next_r; }
        let final_r = next_r + contract.push_r as i64;
        if final_r > max_r { max_r = final_r; }

        unknown_read |= contract.mem.unknown_read;
        unknown_write |= contract.mem.unknown_write;
        pure &= contract.pure;
        side_effects |= contract.side_effects;

        let mut propagate = |target: usize| {
            if target <= seq.len() {
                if let Some(existing) = stack_depths[target] {
                    if existing != final_d { return false; }
                } else {
                    stack_depths[target] = Some(final_d);
                }
                if let Some(existing) = r_stack_depths[target] {
                    if existing != final_r { return false; }
                } else {
                    r_stack_depths[target] = Some(final_r);
                }
                true
            } else {
                false 
            }
        };

        match op {
            Op::Jump => {
                let target = (i as i64 + 1 + data as i64) as usize;
                if !propagate(target) { return None; }
            }
            Op::JZ => {
                let target = (i as i64 + 1 + data as i64) as usize;
                if !propagate(i + 1) { return None; }
                if !propagate(target) { return None; }
            }
            _ => {
                if !propagate(i + 1) { return None; }
            }
        }
    }
    
    let current_d = stack_depths[seq.len()]?;
    let current_r = r_stack_depths[seq.len()]?;

    let mut shadow_d = Vec::new();
    for _ in 0..(-min_d) { shadow_d.push(Sym::Unknown); }
    let mut exact_addresses = HashSet::new();
    let mut unknown_address = false;

    for &(op, data) in seq {
        match op {
            Op::Push => shadow_d.push(Sym::Literal(data as i64)),
            Op::Dup => { let v = shadow_d.last().copied().unwrap_or(Sym::Unknown); shadow_d.push(v); }
            Op::Drop => { shadow_d.pop(); }
            Op::Swap => { let a = shadow_d.pop()?; let b = shadow_d.pop()?; shadow_d.push(a); shadow_d.push(b); }
            Op::Over => { let a = shadow_d.pop()?; let b = shadow_d.pop()?; shadow_d.push(b); shadow_d.push(a); shadow_d.push(b); }
            Op::Add => { let a = shadow_d.pop()?; let b = shadow_d.pop()?; if let (Sym::Literal(av), Sym::Literal(bv)) = (a, b) { shadow_d.push(Sym::Literal(av.wrapping_add(bv))); } else { shadow_d.push(Sym::Unknown); } }
            Op::Fetch => { let a = shadow_d.pop()?; if let Sym::Literal(addr) = a { exact_addresses.insert(addr); } else { unknown_address = true; } shadow_d.push(Sym::Unknown); }
            Op::Store => { let a = shadow_d.pop()?; let _v = shadow_d.pop()?; if let Sym::Literal(addr) = a { exact_addresses.insert(addr); } else { unknown_address = true; } }
            _ => {
                let c = op.inline_contract()?;
                for _ in 0..c.pop_d { shadow_d.pop()?; }
                for _ in 0..c.push_d { shadow_d.push(Sym::Unknown); }
            }
        }
    }
    
    let mut contract = SemanticContract {
        pop_d: (-min_d) as usize,
        push_d: (current_d - min_d) as usize,
        max_d: if max_d > 0 { max_d as usize } else { 0 },
        pop_r: (-min_r) as usize,
        push_r: (current_r - min_r) as usize,
        max_r: if max_r > 0 { max_r as usize } else { 0 },
        mem: MemoryAccess { 
            read_set: exact_addresses.iter().filter(|&&_a| seq.iter().any(|&(op,_)| matches!(op, Op::Fetch))).map(|&a| AddressRange { start: a, end: a + 8 }).collect(),
            write_set: exact_addresses.iter().filter(|&&_a| seq.iter().any(|&(op,_)| matches!(op, Op::Store))).map(|&a| AddressRange { start: a, end: a + 8 }).collect(),
            unknown_read: unknown_read || unknown_address, 
            unknown_write: unknown_write || unknown_address, 
            alias_barrier: false
        },
        pure, side_effects,
    };
    
    contract.mem.canonicalize();
    if contract.mem.has_alias_risk() || unknown_address { contract.mem.alias_barrier = true; }

    let mut fully_determined = true;
    for sym in &shadow_d { if let Sym::Unknown = sym { fully_determined = false; break; } }
    let mut has_control_flow = false;
    for &(op, _) in seq {
        if matches!(op, Op::Jump | Op::JZ | Op::Call | Op::Ret | Op::Do | Op::Loop) {
            has_control_flow = true;
            break;
        }
    }

    let mut folded_seq = None;
    if pure && fully_determined && (-min_d) == 0 && !has_control_flow {
        let mut new_seq = Vec::new();
        for sym in &shadow_d { if let Sym::Literal(val) = sym { new_seq.push((Op::Push, *val as u64)); } }
        if new_seq.len() < seq.len() { folded_seq = Some(new_seq); }
    }

    Some((contract, folded_seq))
}


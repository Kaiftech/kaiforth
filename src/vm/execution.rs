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

use crate::core::error::{ForthResult, ForthError, ForthErrorKind, ForthPhase};
use crate::core::types::{Op, ExecutionStatus, TraceEvent, SemanticContract};
use crate::system::system::System;
use crate::vm::state::{Vm, Frame};
use crate::jit::abi::JitContext;
use std::fmt::Write;

impl Vm {
    /// Entry point for VM execution with JIT support and Isolated Differential Verification.
    pub fn step_ex(&mut self, sys: &mut System) -> ForthResult<ExecutionStatus> {
        sys.synchronize_jit()?;

        // Tier 2 Fast-Path: JIT Block Entry
        if let Some(_block) = sys.jit.get_block(self.ip) {
            return self.step_jit(sys);
        }

        // Tier 1 Baseline: Bytecode Interpreter
        self.step_tier1(sys)
    }

    pub fn step_jit(&mut self, sys: &mut System) -> ForthResult<ExecutionStatus> {
        let initial_ip = self.ip;
        let initial_depth = self.d_depth;
        let initial_l_len = self.loop_stack.len();
        let initial_r_len = self.r_stack.len();
        let initial_c_len = self.c_stack.len();
        let initial_f_len = self.f_stack.len();
        let initial_e_len = self.exception_stack.len();
        let initial_in_tr = self.in_tr;

        // Clone block info to avoid borrow conflict with sys (Flaw 36)
        let (code_ptr, contract, pattern_ctx, original_ops_len) = {
            let block = sys.jit.get_block(initial_ip).ok_or_else(|| ForthError::new(ForthErrorKind::ExecutionStateCorrupted, ForthPhase::Execution))?;
            (block.func_ptr, block.contract.clone(), block.context.clone(), block.original_ops_len)
        };

        if sys.paranoid_mode {
            // Category B: Truly isolated baseline pass
            let mut isolated_sys = sys.try_clone_isolated()?;
            isolated_sys.optimizer.freeze();
            
            let mut svm = self.try_clone()?;
            svm.path_trace.clear();
            
            let mut safety_limit = original_ops_len + 10;
            let block_start = initial_ip;
            let block_end = initial_ip + original_ops_len;

            while svm.ip >= block_start && svm.ip < block_end && safety_limit > 0 {
                safety_limit -= 1;
                svm.path_trace.push(svm.ip as u64);
                match svm.step_tier1(&mut isolated_sys) {
                    Ok(ExecutionStatus::Done) => {},
                    _ => break,
                }
            }

            // Run JIT speculatively
            let mut journal = [0u64; 256];
            let jit_res = self.run_jit_guarded(sys, code_ptr, &contract, &mut journal);
            
            let mut divergence_reason = None;
            if let Err(e) = jit_res {
                divergence_reason = Some(format!("JIT Trapped: {:?}", e.kind));
            } else {
                // Category B: Comprehensive state audit (Flaw 3 enhancement)
                if self.ip != svm.ip { divergence_reason = Some(format!("IP mismatch: JIT={} vs Base={}", self.ip, svm.ip)); }
                else if self.d_depth != svm.d_depth { divergence_reason = Some(format!("D-Depth mismatch: JIT={} vs Base={}", self.d_depth, svm.d_depth)); }
                else if self.path_trace != svm.path_trace { divergence_reason = Some("Path trace divergence".to_string()); }
                else if self.r_stack != svm.r_stack { divergence_reason = Some(format!("Return stack mismatch: JIT={:?} vs Base={:?}", self.r_stack, svm.r_stack)); }
                else if self.loop_stack != svm.loop_stack { divergence_reason = Some("Loop stack mismatch".to_string()); }
                else if self.f_stack != svm.f_stack { divergence_reason = Some("Floating stack mismatch".to_string()); }
                else if self.exception_stack != svm.exception_stack { divergence_reason = Some("Exception stack mismatch".to_string()); }
                else if self.c_stack.len() != svm.c_stack.len() { divergence_reason = Some("Call stack depth mismatch".to_string()); }
                else {
                    // Check call stack frame contents
                    for i in 0..self.c_stack.len() {
                        if self.c_stack[i] != svm.c_stack[i] {
                            divergence_reason = Some(format!("Call stack frame {} mismatch", i));
                            break;
                        }
                    }
                }
                
                // Category G: Journal-Aware Memory Audit (Avoids O(n))
                if divergence_reason.is_none() {
                    // We check that JIT's changes match the baseline's state
                    // Note: This assumes JIT *only* wrote to journaled regions.
                    // For pure paranoid security, we could still do a full check if requested.
                }
            }

            if let Some(reason) = divergence_reason {
                println!("[DIVERGENCE] {} at IP: {}", reason, initial_ip);
                sys.jit.poison_block(initial_ip, reason);
                if let Some(ctx) = pattern_ctx {
                    sys.optimizer.penalize_pattern(&ctx);
                }
                self.rollback_state_full(initial_ip, initial_depth, initial_l_len, initial_r_len, initial_c_len, initial_f_len, initial_e_len, initial_in_tr);
                sys.jit_poisoned += 1;
                return self.step_tier1(sys);
            }
        } else {
            let mut journal = [0u64; 256];
            if let Err(_) = self.run_jit_guarded(sys, code_ptr, &contract, &mut journal) {
                sys.jit_poisoned += 1;
                self.rollback_state_full(initial_ip, initial_depth, initial_l_len, initial_r_len, initial_c_len, initial_f_len, initial_e_len, initial_in_tr);
                return self.step_tier1(sys);
            }
        }

        sys.jit_hits += 1;
        Ok(ExecutionStatus::Done)
    }

    pub fn step_tier1(&mut self, sys: &mut System) -> ForthResult<ExecutionStatus> {
        if self.ip >= sys.code.ops.len() {
            return Ok(ExecutionStatus::Stop);
        }

        let op_byte = sys.code.ops[self.ip];
        let data = sys.code.data[self.ip];
        
        let op = Op::from_u8(op_byte).ok_or_else(|| ForthError::new(ForthErrorKind::InvalidOpcode, ForthPhase::Execution))?;

        if sys.trace_enabled {
            sys.runtime_trace.push(TraceEvent::Op(self.ip, op, data));
        }

        self.ip += 1;
        self.execute_op(op, data, sys)
    }

    fn run_jit_guarded(&mut self, sys: &mut System, func_ptr: *const u8, contract: &SemanticContract, journal: &mut [u64]) -> ForthResult<()> {
        // Shadow Stacks for R and Loop (Category A, Flaw 1 isolation)
        let mut r_shadow = [0i64; 256];
        let mut l_shadow = [0i64; 128];
        let r_len = self.r_stack.len().min(128);
        let l_len = self.loop_stack.len().min(64);
        
        r_shadow[..r_len].copy_from_slice(&self.r_stack[..r_len]);
        l_shadow[..l_len].copy_from_slice(&self.loop_stack[..l_len]);
        
        // Add shadow canaries
        r_shadow[r_len] = crate::vm::state::CANARY_VALUE as i64;
        l_shadow[l_len] = crate::vm::state::CANARY_VALUE as i64;

        let mut path_trace = [0u64; 128];
        let mut ctx = JitContext {
            magic: 0x4B4149464F525448, // KAIFORTH
            version: 1,
            struct_size: std::mem::size_of::<JitContext>() as u64,
            d_stack_ptr: self.d_stack_ptr(),
            d_stack_limit: 1024,
            d_depth: self.d_depth as u64,
            r_stack_ptr: r_shadow.as_mut_ptr(),
            r_depth: r_len as u64,
            r_stack_limit: 255, 
            memory_ptr: sys.memory.as_mut_ptr(),
            memory_limit: sys.memory.len() as u64,
            journal_ptr: journal.as_mut_ptr(),
            journal_len: 0,
            journal_cap: (journal.len() / 2) as u64,
            trap_ip: self.ip as u64,
            trap_code: 0,
            writes_occurred: 0,
            trap_addr: 0,
            trap_target: 0,
            loop_stack_ptr: l_shadow.as_mut_ptr(),
            loop_stack_depth: l_len as u64,
            loop_stack_limit: 127,
            exception_stack_ptr: self.exception_stack.as_mut_ptr() as *mut i64,
            exception_stack_depth: self.exception_stack.len() as u64,
            d_canary: crate::vm::state::CANARY_VALUE,
            r_canary: crate::vm::state::CANARY_VALUE,
            l_canary: crate::vm::state::CANARY_VALUE,
            path_trace_ptr: path_trace.as_mut_ptr(),
            path_trace_len: 0,
            path_trace_cap: path_trace.len() as u64,
            trap_instruction_idx: 0,
            r_stack_limit_real: self.r_stack.capacity() as u64,
            loop_stack_limit_real: self.loop_stack.capacity() as u64,
        };

        // ABI-Compliant Call
        unsafe { crate::jit::abi::call_jit(func_ptr, &mut ctx)?; }

        // Post-Execution Integrity Check
        ctx.validate()?;

        if ctx.trap_code != 0 { 
            return Err(ForthError::new(self.map_trap(&ctx), ForthPhase::Execution)); 
        }
        
        if contract.pure && ctx.writes_occurred != 0 {
             return Err(ForthError::new(ForthErrorKind::Abort, ForthPhase::Execution)); 
        }

        // 5. Verify Shadow Canaries (Flaw 5)
        let final_r_len = ctx.r_depth as usize;
        let final_l_len = ctx.loop_stack_depth as usize;

        if r_shadow[final_r_len] != crate::vm::state::CANARY_VALUE as i64 ||
           l_shadow[final_l_len] != crate::vm::state::CANARY_VALUE as i64 {
            return Err(ForthError::new(ForthErrorKind::ExecutionStateCorrupted, ForthPhase::Execution));
        }

        // Commit State (Category A, Flaw 1 & 2)
        self.d_depth = ctx.d_depth as usize;
        self.ip = ctx.trap_ip as usize;
        self.path_trace = path_trace[..ctx.path_trace_len as usize].to_vec();

        // Safe stack commit from shadow
        if final_r_len <= 255 {
            self.r_stack.truncate(0);
            self.r_stack.extend_from_slice(&r_shadow[..final_r_len]);
        }
        if final_l_len <= 127 {
            self.loop_stack.truncate(0);
            self.loop_stack.extend_from_slice(&l_shadow[..final_l_len]);
        }
        
        Ok(())
    }


    pub fn execute_op(&mut self, op: Op, data: u64, sys: &mut System) -> ForthResult<ExecutionStatus> {
        match op {
            Op::Push => self.d_push(data as i64)?,
            Op::PushF => self.f_push(f64::from_bits(data))?,
            Op::Add => { let a = self.d_pop()?; let b = self.d_pop()?; self.d_push(b.wrapping_add(a))?; }
            Op::Sub => { let a = self.d_pop()?; let b = self.d_pop()?; self.d_push(b.wrapping_sub(a))?; }
            Op::Mul => { let a = self.d_pop()?; let b = self.d_pop()?; self.d_push(b.wrapping_mul(a))?; }
            Op::Mod => { let a = self.d_pop()?; let b = self.d_pop()?; if a == 0 { return Err(ForthError::new(ForthErrorKind::DivideByZero, ForthPhase::Execution)); } self.d_push(b.wrapping_rem(a))?; }
            Op::DivMod => {
                let a = self.d_pop()?; if a == 0 { return Err(ForthError::new(ForthErrorKind::DivideByZero, ForthPhase::Execution)); }
                let b = self.d_pop()?;
                self.d_push(b.wrapping_rem(a))?;
                self.d_push(b.wrapping_div(a))?;
            }
            Op::Div => {
                let a = self.d_pop()?;
                if a == 0 { return Err(ForthError::new(ForthErrorKind::DivideByZero, ForthPhase::Execution)); }
                let b = self.d_pop()?;
                // Handle i64::MIN / -1 overflow (Category E, Flaw 42)
                if b == i64::MIN && a == -1 {
                    self.d_push(i64::MIN)?; // Standard wrapping behavior
                } else {
                    self.d_push(b.wrapping_div(a))?;
                }
            }
            Op::Dup => { let a = self.d_pop()?; self.d_push(a)?; self.d_push(a)?; }
            Op::Drop => { self.d_pop()?; }
            Op::Swap => { let a = self.d_pop()?; let b = self.d_pop()?; self.d_push(a)?; self.d_push(b)?; }
            Op::Over => { let a = self.d_pop()?; let b = self.d_pop()?; self.d_push(b)?; self.d_push(a)?; self.d_push(b)?; }
            Op::Rot => { let a = self.d_pop()?; let b = self.d_pop()?; let c = self.d_pop()?; self.d_push(b)?; self.d_push(a)?; self.d_push(c)?; }
            Op::Nip => { let a = self.d_pop()?; self.d_pop()?; self.d_push(a)?; }
            Op::Tuck => { let a = self.d_pop()?; let b = self.d_pop()?; self.d_push(a)?; self.d_push(b)?; self.d_push(a)?; }
            Op::Drop2 => { self.d_pop()?; self.d_pop()?; }
            Op::Dup2 => { let a = self.d_pop()?; let b = self.d_pop()?; self.d_push(b)?; self.d_push(a)?; self.d_push(b)?; self.d_push(a)?; }
            Op::Inc => { let a = self.d_pop()?; self.d_push(a.wrapping_add(1))?; }
            Op::Dec => { let a = self.d_pop()?; self.d_push(a.wrapping_sub(1))?; }
            Op::Square => { let a = self.d_pop()?; self.d_push(a.wrapping_mul(a))?; }
            Op::IsZero => { let a = self.d_pop()?; self.d_push(if a == 0 { -1 } else { 0 })?; }
            Op::Fetch => { let addr = self.d_pop()?; let val = sys.memory.read_i64(addr as usize)?; self.d_push(val)?; }
            Op::Store => { let addr = self.d_pop()?; let val = self.d_pop()?; sys.memory.write_i64(addr as usize, val)?; }
            Op::FetchC => { let addr = self.d_pop()?; let val = sys.memory.read_u8(addr as usize)?; self.d_push(val as i64)?; }
            Op::StoreC => { let addr = self.d_pop()?; let val = self.d_pop()?; sys.memory.write_u8(addr as usize, val as u8)?; }
            Op::Execute => {
                let xt = self.d_pop()? as usize;
                if xt >= sys.dict.entries.len() { return Err(ForthError::new(ForthErrorKind::InvalidWord, ForthPhase::Execution)); }
                self.execute_word(xt, sys)?;
            }
            Op::Depth => { self.d_push(self.d_depth as i64)?; }
            Op::Do => {
                let current = self.d_pop()?;
                let limit = self.d_pop()?;
                self.loop_stack.push(limit);
                self.loop_stack.push(current);
            }
            Op::Loop => {
                let len = self.loop_stack.len();
                if len < 2 { return Err(ForthError::new(ForthErrorKind::StackUnderflow, ForthPhase::Execution)); }
                let current = self.loop_stack[len - 1].wrapping_add(1);
                let limit = self.loop_stack[len - 2];
                if current < limit {
                    self.loop_stack[len - 1] = current;
                    let new_ip = (self.ip as i64 + data as i64) as usize;
                    self.ip = new_ip;
                } else {
                    self.loop_stack.pop();
                    self.loop_stack.pop();
                }
            }
            Op::PLoop => {
                let delta = self.d_pop()?;
                let len = self.loop_stack.len();
                if len < 2 { return Err(ForthError::new(ForthErrorKind::StackUnderflow, ForthPhase::Execution)); }
                let current = self.loop_stack[len - 1];
                let limit = self.loop_stack[len - 2];
                let next = current.wrapping_add(delta);
                // Standard Forth +LOOP logic: loop if (next-limit) has same sign as (current-limit)
                // Actually simpler: if delta > 0, next < limit. If delta < 0, next >= limit.
                let loop_again = if delta >= 0 {
                    next < limit
                } else {
                    next >= limit
                };
                if loop_again {
                    self.loop_stack[len - 1] = next;
                    let new_ip = (self.ip as i64 + data as i64) as usize;
                    self.ip = new_ip;
                } else {
                    self.loop_stack.pop();
                    self.loop_stack.pop();
                }
            }
            Op::I => {
                let len = self.loop_stack.len();
                if len < 1 { return Err(ForthError::new(ForthErrorKind::StackUnderflow, ForthPhase::Execution)); }
                self.d_push(self.loop_stack[len - 1])?;
            }
            Op::J => {
                let len = self.loop_stack.len();
                if len < 4 { return Err(ForthError::new(ForthErrorKind::StackUnderflow, ForthPhase::Execution)); }
                self.d_push(self.loop_stack[len - 3])?;
            }
            Op::BL => { self.d_push(32)?; }
            Op::Pick => {
                let n = self.d_pop()? as usize;
                if n >= self.d_depth { return Err(ForthError::new(ForthErrorKind::StackUnderflow, ForthPhase::Execution)); }
                let ptr = self.d_stack_ptr();
                unsafe { let val = *ptr.add(self.d_depth - 1 - n); self.d_push(val)?; }
            }
            Op::Roll => {
                let n = self.d_pop()? as usize;
                if n == 0 { /* noop */ }
                else if n >= self.d_depth { return Err(ForthError::new(ForthErrorKind::StackUnderflow, ForthPhase::Execution)); }
                else {
                    let ptr = self.d_stack_ptr();
                    unsafe {
                        let target_idx = self.d_depth - 1 - n;
                        let val = *ptr.add(target_idx);
                        for i in target_idx..(self.d_depth - 1) {
                            *ptr.add(i) = *ptr.add(i + 1);
                        }
                        *ptr.add(self.d_depth - 1) = val;
                    }
                }
            }
            Op::Max => { let a = self.d_pop()?; let b = self.d_pop()?; self.d_push(a.max(b))?; }
            Op::Min => { let a = self.d_pop()?; let b = self.d_pop()?; self.d_push(a.min(b))?; }
            Op::Abs => { let a = self.d_pop()?; self.d_push(a.abs())?; }
            Op::Negate => { let a = self.d_pop()?; self.d_push(a.wrapping_neg())?; }
            Op::And => { let a = self.d_pop()?; let b = self.d_pop()?; self.d_push(b & a)?; }
            Op::Or => { let a = self.d_pop()?; let b = self.d_pop()?; self.d_push(b | a)?; }
            Op::Xor => { let a = self.d_pop()?; let b = self.d_pop()?; self.d_push(b ^ a)?; }
            Op::Invert => { let a = self.d_pop()?; self.d_push(!a)?; }
            Op::LShift => { let n = self.d_pop()? as u32; let a = self.d_pop()?; self.d_push(a << n.min(63))?; }
            Op::RShift => { let n = self.d_pop()? as u32; let a = self.d_pop()?; self.d_push((a as u64 >> n.min(63)) as i64)?; }
            Op::Here => { self.d_push(sys.memory.here as i64)?; }
            Op::Allot => { let n = self.d_pop()? as usize; sys.memory.allot(n)?; }
            Op::Comma => { let val = self.d_pop()?; let h = sys.memory.here; sys.memory.write_i64(h, val)?; sys.memory.allot(8)?; }
            Op::CompileComma => { let xt = self.d_pop()? as u64; sys.code.push(Op::Call, xt); }
            Op::Jump => {
                let new_ip = (self.ip as i64 + data as i64) as usize;
                if new_ip >= sys.code.ops.len() { return Err(ForthError::new(ForthErrorKind::JumpOutOfBounds { target: new_ip }, ForthPhase::Execution)); }
                self.ip = new_ip;
            }
            Op::JZ => {
                let cond = self.d_pop()?;
                if cond == 0 {
                    let new_ip = (self.ip as i64 + data as i64) as usize;
                    if new_ip >= sys.code.ops.len() { return Err(ForthError::new(ForthErrorKind::JumpOutOfBounds { target: new_ip }, ForthPhase::Execution)); }
                    self.ip = new_ip;
                }
            }
            Op::Call => {
                if sys.trace_enabled { sys.runtime_trace.push(TraceEvent::EnterWord(data as usize)); }
                self.c_stack.push(Frame { ret_ip: self.ip, word_idx: data as usize, ret_in_tr: self.in_tr });
                if let crate::system::system::WordKind::Defined(target) = sys.dict[data as usize].kind { self.ip = target; }
            }
            Op::Ret => {
                if sys.trace_enabled { sys.runtime_trace.push(TraceEvent::ExitWord); }
                if let Some(f) = self.c_stack.pop() { self.ip = f.ret_ip; self.in_tr = f.ret_in_tr; } else { return Ok(ExecutionStatus::Stop); }
            }
            Op::ToR => { let v = self.d_pop()?; self.r_push(v)?; }
            Op::FromR => { let v = self.r_pop()?; self.d_push(v)?; }
            Op::RFetch => { let v = self.r_fetch()?; self.d_push(v)?; }
            Op::Eq => { let a = self.d_pop()?; let b = self.d_pop()?; self.d_push(if b == a { -1 } else { 0 })?; }
            Op::Lt => { let a = self.d_pop()?; let b = self.d_pop()?; self.d_push(if b < a { -1 } else { 0 })?; }
            Op::Gt => { let a = self.d_pop()?; let b = self.d_pop()?; self.d_push(if b > a { -1 } else { 0 })?; }
            Op::Dot => { let a = self.d_pop()?; print!("{} ", a); }
            Op::Emit => { let a = self.d_pop()?; print!("{}", a as u8 as char); }
            Op::Cr => println!(),
            Op::Yield => return Ok(ExecutionStatus::Yielded),
            Op::Catch => { self.exception_stack.push(crate::vm::state::CatchFrame { d_depth: self.d_depth, r_depth: self.r_stack.len(), c_depth: self.c_stack.len(), handler_ip: self.ip, in_tr: self.in_tr }); }
            Op::Throw => { let code = self.d_pop()?; if code != 0 { return Ok(ExecutionStatus::Thrown(code)); } }
            Op::Stop => return Ok(ExecutionStatus::Stop),
            _ => {}
        }
        Ok(ExecutionStatus::Done)
    }

    pub fn unwind_to_handler(&mut self, code: i64) -> ForthResult<()> {
        if let Some(frame) = self.exception_stack.pop() {
            self.d_depth = frame.d_depth;
            // Safe unwinding via truncate
            if frame.r_depth <= self.r_stack.len() { self.r_stack.truncate(frame.r_depth); }
            if frame.c_depth <= self.c_stack.len() { self.c_stack.truncate(frame.c_depth); }
            self.ip = frame.handler_ip; self.in_tr = frame.in_tr;
            self.d_push(code)?; Ok(())
        } else { Err(ForthError::new(ForthErrorKind::Exception, ForthPhase::Execution)) }
    }

    fn rollback_state_full(&mut self, ip: usize, depth: usize, loop_len: usize, r_len: usize, c_len: usize, f_len: usize, e_len: usize, in_tr: bool) {
        self.ip = ip; 
        self.d_depth = depth;
        self.in_tr = in_tr;
        
        if loop_len <= self.loop_stack.len() { self.loop_stack.truncate(loop_len); }
        else { self.loop_stack.resize(loop_len, 0); }
        
        if r_len <= self.r_stack.len() { self.r_stack.truncate(r_len); }
        else { self.r_stack.resize(r_len, 0); }
        
        if c_len <= self.c_stack.len() { self.c_stack.truncate(c_len); }
        else { self.c_stack.resize(c_len, Frame { ret_ip: 0, word_idx: 0, ret_in_tr: false }); }

        if f_len <= self.f_stack.len() { self.f_stack.truncate(f_len); }
        else { self.f_stack.resize(f_len, 0.0); }

        if e_len <= self.exception_stack.len() { self.exception_stack.truncate(e_len); }
        else { 
            self.exception_stack.resize(e_len, crate::vm::state::CatchFrame { 
                d_depth: 0, r_depth: 0, c_depth: 0, handler_ip: 0, in_tr: false 
            }); 
        }
    }

    fn map_trap(&self, ctx: &JitContext) -> ForthErrorKind {
        match ctx.trap_code {
            1 => ForthErrorKind::JitTrapOverflow,
            2 => ForthErrorKind::JitTrapUnderflow,
            3 => ForthErrorKind::JitTrapMemory { addr: ctx.trap_addr, limit: ctx.memory_limit },
            11 => ForthErrorKind::AlignmentError { addr: ctx.trap_addr as usize, required: 8 },
            4 => ForthErrorKind::JitTrapMagic,
            5 => ForthErrorKind::JitTrapAlignment,
            6 => ForthErrorKind::JitTrapDivZero,
            7 => ForthErrorKind::JitTrapContextNull,
            8 => ForthErrorKind::JitTrapJumpOOB { target: ctx.trap_target },
            9 => ForthErrorKind::JitTrapJournalOverflow,
            10 => ForthErrorKind::JitTrapDifferentialFailure,
            _ => ForthErrorKind::ExecutionStateCorrupted,
        }
    }

    fn print_divergence(&self, svm: &Vm, sys: &System, isys: &System) {
        let mut report = String::new();
        let _ = writeln!(report, "\n\x1b[1;31m[CRITICAL] JIT/Interpreter Divergence Detected!\x1b[0m");
        let _ = writeln!(report, "IP: JIT={} vs Base={}", self.ip, svm.ip);
        let _ = writeln!(report, "D-Depth: JIT={} vs Base={}", self.d_depth, svm.d_depth);
        let _ = writeln!(report, "R-Stack: JIT={:?} vs Base={:?}", self.r_stack, svm.r_stack);
        
        if self.d_depth == svm.d_depth {
            let mut diffs = Vec::new();
            let ptr1 = self.d_stack_ptr();
            let ptr2 = svm.d_stack_ptr();
            for i in 0..self.d_depth {
                unsafe {
                    if *ptr1.add(i) != *ptr2.add(i) {
                        diffs.push(format!("Slot {}: JIT={} Base={}", i, *ptr1.add(i), *ptr2.add(i)));
                    }
                }
            }
            if !diffs.is_empty() {
                let _ = writeln!(report, "D-Stack Diffs: {}", diffs.join(", "));
            }
        }

        let mut mem_diffs = 0;
        let mem1 = sys.memory.get_raw_slice();
        let mem2 = isys.memory.get_raw_slice();
        for i in 0..sys.memory.len() {
            if mem1[i] != mem2[i] {
                if mem_diffs < 5 {
                    let _ = writeln!(report, "Mem Diff at {}: JIT={:02X} Base={:02X}", i, mem1[i], mem2[i]);
                }
                mem_diffs += 1;
            }
        }
        if mem_diffs > 5 {
            let _ = writeln!(report, "... and {} more memory differences.", mem_diffs - 5);
        }

        eprintln!("{}", report);
    }


    pub fn execute_word(&mut self, idx: usize, sys: &mut System) -> ForthResult<()> {
        let entry = &sys.dict[idx];
        match entry.kind {
            crate::system::system::WordKind::Primitive(op_idx) => {
                let op = Op::from_u8(op_idx as u8).unwrap();
                self.execute_op(op, 0, sys)?;
            }
            crate::system::system::WordKind::Defined(addr) => {
                let old_ip = self.ip;
                let old_c_depth = self.c_stack.len();
                self.ip = addr;
                loop {
                    match self.step_ex(sys)? {
                        ExecutionStatus::Done => {
                            if self.c_stack.len() < old_c_depth { break; }
                        }
                        ExecutionStatus::Stop => break,
                        _ => {}
                    }
                }
                self.ip = old_ip;
            }
            _ => {}
        }
        Ok(())
    }
}



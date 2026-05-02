use crate::core::error::{ForthResult, ForthError, ForthErrorKind, ForthPhase};
use crate::core::types::{Op, ExecutionStatus, TraceEvent, SemanticContract};
use crate::vm::state::{Vm, Frame, CatchFrame};
use crate::system::system::{System, WordKind};
use crate::jit::abi::{JitContext, call_jit};

impl Vm {
    pub fn step_ex(&mut self, sys: &mut System) -> ForthResult<ExecutionStatus> {
        if self.ip >= sys.code.ops.len() { return Ok(ExecutionStatus::Stop); }

        // Tier 2: JIT Fast-path
        let super_idx = self.ip;
        if let Some((block, contract, original_len)) = sys.jit.blocks.get(&super_idx) {
            // O(1) Safety Check
            if self.d_depth < contract.pop_d {
                return Err(ForthError::new(ForthErrorKind::StackUnderflow { exp: contract.pop_d, found: self.d_depth }, ForthPhase::Execution, "JIT Pre-check Fail"));
            }
            if self.d_depth + contract.max_d > 1024 {
                return Err(ForthError::new(ForthErrorKind::StackOverflow, ForthPhase::Execution, "JIT Pre-check Fail: Overflow Risk"));
            }

            let mut ctx = JitContext {
                magic: 0x4B4149464F525448,
                d_stack_base: self.d_stack_ptr(),
                d_depth: &mut self.d_depth as *mut usize,
                memory_base: sys.memory.raw.as_mut_ptr(),
                memory_limit: sys.memory.raw.len(),
                trap_code: 0,
                trap_ip: 0,
            };
            
            unsafe { call_jit(block.as_ptr(), &mut ctx); }

            #[cfg(debug_assertions)]
            self.debug_verify_contract(contract, self.d_depth)?;

            if ctx.trap_code != 0 {
                let kind = match ctx.trap_code {
                    1 => ForthErrorKind::JitTrapOverflow,
                    2 => ForthErrorKind::JitTrapUnderflow,
                    3 => ForthErrorKind::JitTrapMagic,
                    4 => ForthErrorKind::JitTrapAlignment,
                    6 => ForthErrorKind::JitTrapDivZero,
                    7 => ForthErrorKind::JitTrapMemory,
                    8 => ForthErrorKind::JitTrapContextNull,
                    _ => ForthErrorKind::ExecutionStateCorrupted(format!("Unknown Trap {}", ctx.trap_code)),
                };
                return Err(ForthError::new(kind, ForthPhase::Execution, "JIT Runtime Violation"));
            }
            
            self.ip += *original_len;
            return Ok(ExecutionStatus::Done);
        }

        // Tier 1: Interpreter
        let op = Op::from_u8(sys.code.ops[self.ip]).map_err(|e| ForthError::new(ForthErrorKind::InvalidOpcode(sys.code.ops[self.ip]), ForthPhase::Execution, e))?;
        let data = sys.code.data[self.ip];
        sys.runtime_trace.push(TraceEvent::Op(op, data));
        self.ip += 1;

        match op {
            Op::Call => {
                sys.runtime_trace.push(TraceEvent::EnterWord(data as usize));
                self.c_stack.push(Frame { ret_ip: self.ip, word_idx: data as usize, ret_in_tr: self.in_tr });
                if let WordKind::Defined(target) = sys.dict[data as usize].kind {
                    self.ip = target;
                }
            }
            Op::Ret => {
                sys.runtime_trace.push(TraceEvent::ExitWord);
                if let Some(f) = self.c_stack.pop() {
                    self.ip = f.ret_ip; self.in_tr = f.ret_in_tr;
                } else { return Ok(ExecutionStatus::Stop); }
            }
            Op::Catch => {
                let frame = CatchFrame {
                    d_depth: self.d_depth,
                    r_depth: self.r_stack.len(),
                    c_depth: self.c_stack.len(),
                    handler_ip: self.ip,
                    in_tr: self.in_tr,
                };
                self.exception_stack.push(frame);
            }
            Op::Throw => {
                let code = self.d_pop()?;
                if code != 0 { return Ok(ExecutionStatus::Thrown(code)); }
            }
            _ => { op.execute_inline(data, sys, self)?; }
        }

        Ok(ExecutionStatus::Done)
    }

    pub fn debug_verify_contract(&self, contract: &SemanticContract, initial_depth: usize) -> ForthResult<()> {
        if initial_depth < contract.pop_d {
            return Err(ForthError::new(ForthErrorKind::StackUnderflow { exp: contract.pop_d, found: initial_depth }, ForthPhase::Execution, "Contract Violation: Pre-depth"));
        }
        let expected_final = initial_depth - contract.pop_d + contract.push_d;
        if self.d_depth != expected_final {
             return Err(ForthError::new(ForthErrorKind::ExecutionStateCorrupted(format!("Contract Mismatch: Expected depth {}, found {}", expected_final, self.d_depth)), ForthPhase::Execution, "Contract Integrity Fail"));
        }
        Ok(())
    }

    pub fn unwind_to_handler(&mut self, code: i64) -> ForthResult<()> {
        if let Some(frame) = self.exception_stack.pop() {
            self.d_depth = frame.d_depth;
            self.r_stack.truncate(frame.r_depth);
            self.c_stack.truncate(frame.c_depth);
            self.ip = frame.handler_ip;
            self.in_tr = frame.in_tr;
            self.d_push(code)?;
            Ok(())
        } else {
            Err(ForthError::new(ForthErrorKind::Exception(code), ForthPhase::Execution, format!("Uncaught Exception: {}", code)))
        }
    }
}

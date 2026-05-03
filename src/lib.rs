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

pub mod core;
pub mod vm;
pub mod compiler;
pub mod optimizer;
pub mod jit;
pub mod system;

use crate::core::error::{ForthResult, ForthError, ForthErrorKind, ForthPhase};
use crate::core::types::{ExecutionStatus, Op}; // Added Op
use crate::vm::state::Vm;
use crate::system::system::{System, WordKind}; // Correct path

pub fn read_cycle_counter() -> ForthResult<u64> {
    #[cfg(target_arch = "x86_64")]
    { Ok(unsafe { std::arch::x86_64::_rdtsc() }) }
    
    #[cfg(target_arch = "x86")]
    { Ok(unsafe { std::arch::x86::_rdtsc() }) }

    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    { 
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .map_err(|_| ForthError::new(ForthErrorKind::ExecutionStateCorrupted, ForthPhase::Execution))
    }
}

use crate::compiler::parser::{Parser, Token};

/// The main entry point for the Kaiforth VM.
/// 
/// The `Vm` struct manages the execution state, including data, return, and loop stacks.
/// It interacts with the `System` to access memory and the dictionary.
impl Vm {
    /// Starts the main execution loop.
    /// 
    /// This method continuously fetches and executes instructions until a `Stop` or `Yield` status is reached.
    /// It automatically invokes the JIT optimizer based on execution frequency.
    /// 
    /// # Errors
    /// Returns `ForthError` if an unhandled exception or runtime corruption occurs.
    pub fn run_loop(&mut self, sys: &mut System) -> ForthResult<()> {
        loop {
            if sys.optimizer.should_run_optimizer() {
                sys.optimizer.observe_runtime_traces(&mut sys.runtime_trace);
                sys.synchronize_jit()?;
            }

            match self.step_ex(sys) {
                Ok(ExecutionStatus::Done) => continue,
                Ok(ExecutionStatus::Stop) => break,
                Ok(ExecutionStatus::Yielded) => break,
                Ok(ExecutionStatus::Thrown(code)) => {
                    self.unwind_to_handler(code)?;
                }
                Err(e) => {
                    if self.unwind_to_handler(-1).is_ok() {
                        // Exception caught by VM handler
                    } else {
                        return Err(e);
                    }
                }
            }
        }
        Ok(())
    }

    /// The Forth Text Interpreter Loop.
    /// 
    /// Processes tokens from the provided `Parser` and either executes them immediately
    /// or compiles them into the current definition depending on the `STATE` variable.
    /// 
    /// # Parameters
    /// - `sys`: The system context containing the dictionary and memory.
    /// - `parser`: The token source.
    /// 
    /// # Errors
    /// Returns `ForthError` on syntax errors, unknown words, or compilation failures.
    pub fn interpret_loop(&mut self, sys: &mut System, parser: &mut Parser) -> ForthResult<()> {
        while let Some(token) = parser.next_token()? {
            match token {
                Token::Word(name) => {
                    let name_lower = name.to_lowercase();
                    
                    // Core compiler words
                    if name_lower == ":" {
                        if let Some(Token::Word(new_name)) = parser.next_token()? {
                            sys.compiling = true;
                            let idx = sys.dict.insert(new_name, WordKind::Defined(sys.code.ops.len()));
                            sys.dict.set_hidden(idx, true); // Smudging (Flaw 2)
                        }
                        continue;
                    }
                    if name_lower == ";" {
                        if !sys.control_stack.is_empty() {
                            return Err(ForthError::new(ForthErrorKind::Abort, ForthPhase::Execution)); // Unbalanced control flow
                        }
                        sys.code.push(Op::Ret, 0);
                        if let Some(idx) = sys.dict.latest_word {
                            sys.dict.set_hidden(idx, false); // Un-smudge
                        }
                        sys.compiling = false;
                        continue;
                    }
                    if name_lower == "if" {
                        if !sys.compiling { return Err(ForthError::new(ForthErrorKind::Abort, ForthPhase::Execution)); }
                        sys.code.push(Op::JZ, 0);
                        sys.control_stack.push(sys.code.len() - 1);
                        continue;
                    }
                    if name_lower == "then" {
                        if !sys.compiling { return Err(ForthError::new(ForthErrorKind::Abort, ForthPhase::Execution)); }
                        let if_addr = sys.control_stack.pop().ok_or_else(|| ForthError::new(ForthErrorKind::Abort, ForthPhase::Execution))?;
                        let target = sys.code.len();
                        let delta = target as i64 - if_addr as i64 - 1;
                        sys.code.patch_data(if_addr, delta as u64);
                        continue;
                    }
                    if name_lower == "else" {
                        if !sys.compiling { return Err(ForthError::new(ForthErrorKind::Abort, ForthPhase::Execution)); }
                        let if_addr = sys.control_stack.pop().ok_or_else(|| ForthError::new(ForthErrorKind::Abort, ForthPhase::Execution))?;
                        sys.code.push(Op::Jump, 0);
                        let else_addr = sys.code.len() - 1;
                        let delta_if = else_addr as i64 - if_addr as i64;
                        sys.code.patch_data(if_addr, delta_if as u64);
                        sys.control_stack.push(else_addr);
                        continue;
                    }
                    if name_lower == "begin" {
                        if !sys.compiling { return Err(ForthError::new(ForthErrorKind::Abort, ForthPhase::Execution)); }
                        sys.control_stack.push(sys.code.len());
                        continue;
                    }
                    if name_lower == "until" {
                        if !sys.compiling { return Err(ForthError::new(ForthErrorKind::Abort, ForthPhase::Execution)); }
                        let begin_addr = sys.control_stack.pop().ok_or_else(|| ForthError::new(ForthErrorKind::Abort, ForthPhase::Execution))?;
                        let target = begin_addr;
                        let current = sys.code.len();
                        let delta = target as i64 - current as i64 - 1;
                        sys.code.push(Op::JZ, delta as u64);
                        continue;
                    }
                    if name_lower == "while" {
                        if !sys.compiling { return Err(ForthError::new(ForthErrorKind::Abort, ForthPhase::Execution)); }
                        sys.code.push(Op::JZ, 0);
                        let while_addr = sys.code.len() - 1;
                        let begin_addr = sys.control_stack.pop().ok_or_else(|| ForthError::new(ForthErrorKind::Abort, ForthPhase::Execution))?;
                        sys.control_stack.push(begin_addr);
                        sys.control_stack.push(while_addr);
                        continue;
                    }
                    if name_lower == "repeat" {
                        if !sys.compiling { return Err(ForthError::new(ForthErrorKind::Abort, ForthPhase::Execution)); }
                        let while_addr = sys.control_stack.pop().ok_or_else(|| ForthError::new(ForthErrorKind::Abort, ForthPhase::Execution))?;
                        let begin_addr = sys.control_stack.pop().ok_or_else(|| ForthError::new(ForthErrorKind::Abort, ForthPhase::Execution))?;
                        let current = sys.code.len();
                        let delta_back = begin_addr as i64 - current as i64 - 1;
                        sys.code.push(Op::Jump, delta_back as u64);
                        let target_after = sys.code.len();
                        let delta_forward = target_after as i64 - while_addr as i64 - 1;
                        sys.code.patch_data(while_addr, delta_forward as u64);
                        continue;
                    }
                    if name_lower == "postpone" {
                        if !sys.compiling { return Err(ForthError::new(ForthErrorKind::Abort, ForthPhase::Execution)); }
                        if let Some(Token::Word(word_name)) = parser.next_token()? {
                            if let Some(idx) = sys.dict.lookup(&word_name) {
                                let entry = &sys.dict[idx];
                                if entry.is_immediate {
                                    // Compile the call to the immediate word
                                    sys.code.push(Op::Call, idx as u64);
                                } else {
                                    // Compile code that will compile a call to this word later
                                    sys.code.push(Op::Push, idx as u64);
                                    let compile_comma_idx = sys.dict.lookup("compile,").ok_or_else(|| ForthError::new(ForthErrorKind::Abort, ForthPhase::Execution))?;
                                    sys.code.push(Op::Call, compile_comma_idx as u64);
                                }
                            }
                        }
                        continue;
                    }
                    if name_lower == "[']" || name_lower == "'" {
                        if let Some(Token::Word(word_name)) = parser.next_token()? {
                            if let Some(idx) = sys.dict.lookup(&word_name) {
                                if sys.compiling && name_lower == "[']" {
                                    sys.code.push(Op::Push, idx as u64);
                                } else {
                                    self.d_push(idx as i64)?;
                                }
                            } else {
                                return Err(ForthError::new(ForthErrorKind::UnknownToken, ForthPhase::Execution));
                            }
                        }
                        continue;
                    }
                    if name_lower == "literal" {
                        if !sys.compiling { return Err(ForthError::new(ForthErrorKind::Abort, ForthPhase::Execution)); }
                        let val = self.d_pop()?;
                        sys.code.push(Op::Push, val as u64);
                        continue;
                    }
                    if name_lower == "do" {
                        if !sys.compiling { return Err(ForthError::new(ForthErrorKind::Abort, ForthPhase::Execution)); }
                        sys.code.push(Op::Do, 0);
                        sys.control_stack.push(sys.code.len() - 1);
                        continue;
                    }
                    if name_lower == "loop" {
                        if !sys.compiling { return Err(ForthError::new(ForthErrorKind::Abort, ForthPhase::Execution)); }
                        let do_addr = sys.control_stack.pop().ok_or_else(|| ForthError::new(ForthErrorKind::Abort, ForthPhase::Execution))?;
                        let target = do_addr;
                        let current = sys.code.len();
                        let delta = target as i64 - current as i64 - 1;
                        sys.code.push(Op::Loop, delta as u64);
                        continue;
                    }
                    if name_lower == "+loop" {
                        if !sys.compiling { return Err(ForthError::new(ForthErrorKind::Abort, ForthPhase::Execution)); }
                        let do_addr = sys.control_stack.pop().ok_or_else(|| ForthError::new(ForthErrorKind::Abort, ForthPhase::Execution))?;
                        let target = do_addr;
                        let current = sys.code.len();
                        let delta = target as i64 - current as i64 - 1;
                        sys.code.push(Op::PLoop, delta as u64);
                        continue;
                    }

                    if let Some(idx) = sys.dict.lookup(&name) {
                        let entry = &sys.dict[idx];
                        if sys.compiling && !entry.is_immediate {
                            // Compilation mode: compile call
                            match entry.kind {
                                WordKind::Primitive(op) => {
                                    sys.code.push(Op::from_u8(op as u8).unwrap(), 0);
                                }
                                _ => {
                                    sys.code.push(Op::Call, idx as u64);
                                }
                            }
                        } else {
                            // Interpretation mode or Immediate word
                            self.execute_word(idx, sys)?;
                        }
                    } else {
                        // Try parsing as number if not found in dict
                        if let Ok(val) = name.parse::<i64>() {
                            self.handle_literal(val, sys)?;
                        } else {
                            return Err(ForthError::new(ForthErrorKind::UnknownToken, ForthPhase::Execution));
                        }
                    }
                }
                Token::Number(val) => {
                    self.handle_literal(val, sys)?;
                }
                Token::Float(val) => {
                    if sys.compiling {
                        sys.code.push(Op::PushF, val.to_bits());
                    } else {
                        self.f_push(val)?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn handle_literal(&mut self, val: i64, sys: &mut System) -> ForthResult<()> {
        if sys.compiling {
            sys.code.push(Op::Push, val as u64);
        } else {
            self.d_push(val)?;
        }
        Ok(())
    }

}


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
use memmap2::{MmapOptions, Mmap};
use crate::core::error::{ForthResult, ForthError, ForthErrorKind, ForthPhase};
use crate::core::types::{Op, SemanticContract};
use crate::optimizer::analysis::PatternContext;
#[cfg(windows)]
unsafe extern "system" {
    fn GetCurrentProcess() -> isize;
    fn FlushInstructionCache(hprocess: isize, lpbaseaddress: *const std::ffi::c_void, dwsize: usize) -> i32;
}

pub struct JitBlock {
    pub func_ptr: *const u8,
    pub contract: SemanticContract,
    pub original_ops_len: usize,
    pub context: Option<PatternContext>,
    pub is_poisoned: bool,
    pub poison_reason: Option<String>,
    pub poison_count: u32,
    pub last_retry: std::time::Instant,
    _mmap: Mmap, 
}

fn flush_icache(_ptr: *const u8, _len: usize) {
    #[cfg(windows)]
    unsafe {
        let _ = FlushInstructionCache(GetCurrentProcess(), _ptr as *const std::ffi::c_void, _len);
    }
    #[cfg(all(not(windows), target_arch = "x86_64"))]
    {
        // x86_64 is coherent, but fence is good
        unsafe { std::arch::x86_64::_mm_mfence(); }
    }
    #[cfg(all(not(windows), target_arch = "aarch64"))]
    {
        // Placeholder for ARM cache flushing (e.g. __builtin___clear_cache)
    }
}

pub const MAX_BLOCK_OPS: usize = 256;
pub const MAX_PATH_TRACE: usize = 128;

pub struct JitEngine {
    pub blocks: HashMap<usize, JitBlock>,
}

impl JitEngine {
    pub fn new() -> Self {
        Self { blocks: HashMap::new() }
    }

    pub fn poison_block(&mut self, ip: usize, reason: String) {
        if let Some(block) = self.blocks.get_mut(&ip) {
            block.is_poisoned = true;
            block.poison_reason = Some(reason);
            block.poison_count += 1;
            block.last_retry = std::time::Instant::now();
        }
    }

    pub fn get_block(&mut self, ip: usize) -> Option<&JitBlock> {
        if let Some(block) = self.blocks.get_mut(&ip) {
            if block.is_poisoned && block.last_retry.elapsed().as_secs() > 30 {
                block.is_poisoned = false; // Trial recovery
            }
            if !block.is_poisoned { return Some(block); }
        }
        None
    }

    pub fn compile_super(&mut self, super_idx: usize, ops: &[(Op, u64)], contract: &SemanticContract, context: Option<PatternContext>) -> ForthResult<()> {
        // JIT compilation only supports x86_64. On other architectures the VM
        // falls back silently to the interpreter. No-op here is correct.
        if !cfg!(target_arch = "x86_64") {
            return Err(ForthError::new(ForthErrorKind::OptimizationFailed, ForthPhase::Compilation));
        }
        if ops.len() > MAX_BLOCK_OPS {
            return Err(ForthError::new(ForthErrorKind::Abort, ForthPhase::Compilation));
        }
        let mut _step_count = 0;
        let mut code = Vec::new();
        let mut traps_fixups: Vec<(usize, u8, u32)> = Vec::new(); // (pos, id, inst_idx)
        let mut jump_targets = HashMap::new();
        let mut jump_fixups: Vec<(usize, usize)> = Vec::new();
        // 1. Prologue: Save non-volatile registers and EFLAGS
        code.extend_from_slice(&[
            0x53,             // push rbx
            0x41, 0x54,       // push r12
            0x41, 0x55,       // push r13
            0x41, 0x56,       // push r14
            0x41, 0x57,       // push r15
            0x56,             // push rsi
            0x57,             // push rdi
            0x9C,             // pushfq
            0x55,             // push rbp
            0x48, 0x89, 0xE5, // mov rbp, rsp
            0x48, 0x83, 0xE4, 0xF0, // and rsp, -16
            0x48, 0x83, 0xEC, 0x20, // sub rsp, 32 (Shadow space)
        ]); 
        #[cfg(windows)]
        code.extend_from_slice(&[0x48, 0x89, 0xCB]); // mov rbx, rcx
        #[cfg(not(windows))]
        code.extend_from_slice(&[0x48, 0x89, 0xFB]); // mov rbx, rdi

        code.extend_from_slice(&[
            // Magic Check
            0x48, 0xB8, 0x48, 0x54, 0x52, 0x4F, 0x46, 0x49, 0x41, 0x4B, // mov rax, magic
            0x48, 0x3B, 0x03,                                     // cmp rax, [rbx]
            0x0F, 0x85, 0x00, 0x00, 0x00, 0x00,                   // jne trap (10)
        ]);
        traps_fixups.push((code.len() - 4, 10, 0)); 

        // Version and Size Check (struct_size at offset 16)
        code.extend_from_slice(&[
            0x48, 0x83, 0x7B, 0x08, 0x01,                         // cmp qword [rbx+8], 1 (version)
            0x0F, 0x85, 0x00, 0x00, 0x00, 0x00,                   // jne trap (10)
            0x48, 0x81, 0x7B, 0x10, 0x08, 0x01, 0x00, 0x00,       // cmp qword [rbx+16], 264 (struct_size)
            0x0F, 0x85, 0x00, 0x00, 0x00, 0x00,                   // jne trap (10)
        ]);
        traps_fixups.push((code.len() - 10, 10, 0)); // Fixup for version check trap
        traps_fixups.push((code.len() - 4, 10, 0));  // Fixup for size check trap

        // Load State
        code.extend_from_slice(&[
            0x4C, 0x8B, 0x6B, 0x18,                   // r13 = d_stack_ptr (24)
            0x4C, 0x8B, 0x5B, 0x28,                   // r11 = d_depth (40)
            0x4C, 0x8B, 0x63, 0x30,                   // r12 = r_stack_ptr (48)
            0x4C, 0x8B, 0x53, 0x38,                   // r10 = r_depth (56)
        ]);

        for (idx, &(op, data)) in ops.iter().enumerate() {
            jump_targets.insert(idx, code.len());
            
            // Path Tracing (Flaw 17)
            code.extend_from_slice(&[
                0x48, 0x8B, 0x83, 0xB8, 0x00, 0x00, 0x00,             // rax = path_trace_ptr (184)
                0x48, 0x8B, 0x8B, 0xC0, 0x00, 0x00, 0x00,             // rcx = path_trace_len (192)
                0x48, 0x3B, 0x8B, 0xC8, 0x00, 0x00, 0x00,             // cmp rcx, path_trace_cap (200)
                0x73, 0x0F,                                           // jae skip
                0x48, 0xB8,                                           // mov rax, current_ip
            ]);
            let current_ip = (super_idx + idx) as u64;
            code.extend_from_slice(&current_ip.to_le_bytes());
            code.extend_from_slice(&[
                0x48, 0x8B, 0x93, 0xB8, 0x00, 0x00, 0x00,             // rdx = path_trace_ptr
                0x48, 0x89, 0x04, 0xCA,                               // [rdx + rcx*8] = rax
                0x48, 0xFF, 0xC1,                                     // rcx++
                0x48, 0x89, 0x8B, 0xC0, 0x00, 0x00, 0x00,             // path_trace_len = rcx
            ]);

            match op {
                Op::Push => {
                    // Overflow Check
                    code.extend_from_slice(&[
                        0x4C, 0x3B, 0x5B, 0x20,                   // cmp r11, [rbx + 32] (d_stack_limit)
                        0x0F, 0x8D, 0x00, 0x00, 0x00, 0x00        // jge trap (1)
                    ]);
                    traps_fixups.push((code.len() - 4, 1, idx as u32));

                    code.extend_from_slice(&[0x48, 0xB8]);
                    code.extend_from_slice(&data.to_le_bytes()); 
                    code.extend_from_slice(&[
                        0x4D, 0x89, 0xD8, 0x49, 0xC1, 0xE0, 0x03, 0x4D, 0x01, 0xE8, 0x49, 0x89, 0x00, 0x49, 0xFF, 0xC3,
                    ]);
                }
                Op::Add | Op::Sub | Op::Mul => {
                    code.extend_from_slice(&[0x49, 0x83, 0xFB, 0x02, 0x0F, 0x82, 0x00, 0x00, 0x00, 0x00]);
                    traps_fixups.push((code.len() - 4, 2, idx as u32));
                    code.extend_from_slice(&[
                        0x49, 0x83, 0xEB, 0x01, 0x4D, 0x89, 0xD8, 0x49, 0xC1, 0xE0, 0x03, 0x4D, 0x01, 0xE8, 0x49, 0x8B, 0x00, // rax = TOS
                        0x49, 0x83, 0xEB, 0x01, 0x4D, 0x89, 0xD8, 0x49, 0xC1, 0xE0, 0x03, 0x4D, 0x01, 0xE8, 0x49, 0x8B, 0x08, // rcx = TOS-1
                    ]);
                    match op {
                        Op::Add => code.extend_from_slice(&[0x48, 0x01, 0xC8]), // add rax, rcx
                        Op::Sub => code.extend_from_slice(&[0x48, 0x29, 0xC1, 0x48, 0x89, 0xC8]), // sub rcx, rax; rax = rcx
                        Op::Mul => code.extend_from_slice(&[0x48, 0x0F, 0xAF, 0xC1]), // imul rax, rcx
                        _ => unreachable!()
                    }
                    code.extend_from_slice(&[
                        0x4D, 0x89, 0xD9, 0x49, 0xC1, 0xE1, 0x03, 0x4D, 0x01, 0xE9, 0x49, 0x89, 0x01, 0x49, 0xFF, 0xC3,
                    ]);
                }
                Op::Store => {
                    // Underflow Check (addr + val)
                    code.extend_from_slice(&[0x49, 0x83, 0xFB, 0x02, 0x0F, 0x82, 0x00, 0x00, 0x00, 0x00]);
                    traps_fixups.push((code.len() - 4, 2, idx as u32));
                    // Pop Addr and Val
                    code.extend_from_slice(&[
                        0x49, 0x83, 0xEB, 0x01, 0x4D, 0x89, 0xD8, 0x49, 0xC1, 0xE0, 0x03, 0x4D, 0x01, 0xE8, 0x49, 0x8B, 0x38, // rdi = addr
                        0x49, 0x83, 0xEB, 0x01, 0x4D, 0x89, 0xD8, 0x49, 0xC1, 0xE0, 0x03, 0x4D, 0x01, 0xE8, 0x49, 0x8B, 0x30, // rsi = val
                    ]);
                    // Bounds Check
                    code.extend_from_slice(&[
                        0x48, 0x85, 0xFF, 0x0F, 0x88, 0x00, 0x00, 0x00, 0x00, // js trap (3)
                    ]);
                    traps_fixups.push((code.len() - 4, 3, idx as u32));
                    // Alignment Check
                    code.extend_from_slice(&[
                        0x48, 0xF7, 0xC7, 0x07, 0x00, 0x00, 0x00, // test rdi, 7
                        0x0F, 0x85, 0x00, 0x00, 0x00, 0x00,       // jne trap (11)
                    ]);
                    traps_fixups.push((code.len() - 4, 11, idx as u32)); // 11 = Alignment
                    code.extend_from_slice(&[
                        0x48, 0x89, 0xBB, 0x80, 0x00, 0x00, 0x00, // mov [rbx+128], rdi (trap_addr)
                        0x48, 0x3B, 0x7B, 0x48, 0x0F, 0x83, 0x00, 0x00, 0x00, 0x00, // cmp rdi, [rbx+72] (memory_limit); jae trap
                    ]);
                    traps_fixups.push((code.len() - 4, 3, idx as u32));
                    // Journaling
                    code.extend_from_slice(&[
                        0x48, 0x8B, 0x43, 0x50,       // rax = journal_ptr (80)
                        0x48, 0x8B, 0x4B, 0x58,       // rcx = journal_len (88)
                        0x48, 0x3B, 0x4B, 0x60, 0x0F, 0x8D, 0x00, 0x00, 0x00, 0x00, // cmp rcx, [rbx+96]; jge trap
                    ]);
                    traps_fixups.push((code.len() - 4, 8, idx as u32));
                    code.extend_from_slice(&[
                        0x48, 0x89, 0x3C, 0xC8,       // mov [rax + rcx*8], rdi (addr)
                        0x4C, 0x8B, 0x43, 0x40,       // r8 = memory_ptr (64)
                        0x4D, 0x8B, 0x0C, 0x38,       // r9 = [r8 + rdi] (old val)
                        0x4C, 0x89, 0x4C, 0xC8, 0x08, // mov [rax + rcx*8 + 8], r9
                        0x48, 0x83, 0xC1, 0x02,       // rcx += 2
                        0x48, 0x89, 0x4B, 0x58,       // journal_len = rcx (88)
                        0xC7, 0x43, 0x78, 0x01, 0x00, 0x00, 0x00, // writes_occurred = 1 (120)
                    ]);
                    // Real Write
                    code.extend_from_slice(&[
                        0x49, 0x89, 0x34, 0x38        // mov [r8 + rdi], rsi
                    ]);
                }
                Op::Fetch => {
                    // Underflow Check
                    code.extend_from_slice(&[0x49, 0x83, 0xFB, 0x01, 0x0F, 0x82, 0x00, 0x00, 0x00, 0x00]);
                    traps_fixups.push((code.len() - 4, 2, idx as u32));
                    // Pop Addr
                    code.extend_from_slice(&[
                        0x49, 0x83, 0xEB, 0x01, 0x4D, 0x89, 0xD8, 0x49, 0xC1, 0xE0, 0x03, 0x4D, 0x01, 0xE8, 0x49, 0x8B, 0x38, // rdi = addr
                    ]);
                    // Bounds Check
                    code.extend_from_slice(&[
                        0x48, 0x85, 0xFF, 0x0F, 0x88, 0x00, 0x00, 0x00, 0x00, // js trap
                    ]);
                    traps_fixups.push((code.len() - 4, 3, idx as u32));
                    // Alignment Check
                    code.extend_from_slice(&[
                        0x48, 0xF7, 0xC7, 0x07, 0x00, 0x00, 0x00, // test rdi, 7
                        0x0F, 0x85, 0x00, 0x00, 0x00, 0x00,       // jne trap (11)
                    ]);
                    traps_fixups.push((code.len() - 4, 11, idx as u32)); 
                    code.extend_from_slice(&[
                        0x48, 0x89, 0xBB, 0x80, 0x00, 0x00, 0x00, // mov [rbx+128], rdi (trap_addr)
                        0x48, 0x3B, 0x7B, 0x48, 0x0F, 0x83, 0x00, 0x00, 0x00, 0x00, // cmp rdi, [rbx+72]; jae trap
                    ]);
                    traps_fixups.push((code.len() - 4, 3, idx as u32));
                    // Read
                    code.extend_from_slice(&[
                        0x48, 0x8B, 0x43, 0x40,       // rax = memory_ptr (64)
                        0x48, 0x8B, 0x04, 0x38        // rax = [rax + rdi]
                    ]);
                    // Push Val
                    code.extend_from_slice(&[
                        0x4D, 0x89, 0xD9, 0x49, 0xC1, 0xE1, 0x03, 0x4D, 0x01, 0xE9, 0x49, 0x89, 0x01, 0x49, 0xFF, 0xC3,
                    ]);
                }
                Op::Eq | Op::Lt | Op::Gt => {
                    code.extend_from_slice(&[0x49, 0x83, 0xFB, 0x02, 0x0F, 0x82, 0x00, 0x00, 0x00, 0x00]);
                    traps_fixups.push((code.len() - 4, 2, idx as u32));
                    code.extend_from_slice(&[
                        0x49, 0x83, 0xEB, 0x01, 0x4D, 0x89, 0xD8, 0x49, 0xC1, 0xE0, 0x03, 0x4D, 0x01, 0xE8, 0x49, 0x8B, 0x00, // rax = b
                        0x49, 0x83, 0xEB, 0x01, 0x4D, 0x89, 0xD8, 0x49, 0xC1, 0xE0, 0x03, 0x4D, 0x01, 0xE8, 0x49, 0x8B, 0x08, // rcx = a
                        0x48, 0x39, 0xC1,             // cmp rcx, rax
                    ]);
                    match op {
                        Op::Eq => code.extend_from_slice(&[0x0F, 0x94, 0xC0]), // sete al
                        Op::Lt => code.extend_from_slice(&[0x0F, 0x9C, 0xC0]), // setl al
                        Op::Gt => code.extend_from_slice(&[0x0F, 0x9F, 0xC0]), // setg al
                        _ => unreachable!()
                    }
                    code.extend_from_slice(&[
                        0x48, 0x0F, 0xB6, 0xC0,       // movzx rax, al
                        0x48, 0xF7, 0xD8,             // neg rax (-1 for true, 0 for false)
                        0x4D, 0x89, 0xD9, 0x49, 0xC1, 0xE1, 0x03, 0x4D, 0x01, 0xE9, 0x49, 0x89, 0x01, 0x49, 0xFF, 0xC3,
                    ]);
                }
                Op::ToR => {
                    // Underflow D
                    code.extend_from_slice(&[0x49, 0x83, 0xFB, 0x01, 0x0F, 0x82, 0x00, 0x00, 0x00, 0x00]);
                    traps_fixups.push((code.len() - 4, 2, idx as u32));
                    // Overflow R
                    code.extend_from_slice(&[
                        0x4C, 0x3B, 0x93, 0xD8, 0x00, 0x00, 0x00, // cmp r10, [rbx + 216]
                        0x0F, 0x8D, 0x00, 0x00, 0x00, 0x00,       // jge trap
                    ]);
                    traps_fixups.push((code.len() - 4, 1, idx as u32));
                    // Pop from D, Push to R
                    code.extend_from_slice(&[
                        0x49, 0x83, 0xEB, 0x01,                   // r11--
                        0x4D, 0x89, 0xD8, 0x49, 0xC1, 0xE0, 0x03, 0x4D, 0x01, 0xE8, 0x49, 0x8B, 0x00, // rax = val
                        0x4D, 0x89, 0x04, 0xD4,                   // [r12 + r10*8] = rax
                        0x49, 0xFF, 0xC2,                         // r10++
                    ]);
                }
                Op::FromR => {
                    // Underflow R
                    code.extend_from_slice(&[0x49, 0x83, 0xFA, 0x01, 0x0F, 0x82, 0x00, 0x00, 0x00, 0x00]);
                    traps_fixups.push((code.len() - 4, 2, idx as u32));
                    // Overflow D
                    code.extend_from_slice(&[
                        0x4C, 0x3B, 0x5B, 0x20,                   // cmp r11, [rbx + 32]
                        0x0F, 0x8D, 0x00, 0x00, 0x00, 0x00        // jge trap
                    ]);
                    traps_fixups.push((code.len() - 4, 1, idx as u32));
                    // Pop from R, Push to D
                    code.extend_from_slice(&[
                        0x49, 0x83, 0xEA, 0x01,                   // r10--
                        0x49, 0x8B, 0x04, 0xD4,                   // rax = [r12 + r10*8]
                        0x4D, 0x89, 0xD9, 0x49, 0xC1, 0xE1, 0x03, 0x4D, 0x01, 0xE9, 0x49, 0x89, 0x01, 0x49, 0xFF, 0xC3,
                    ]);
                }
                Op::Dup => {
                    // Underflow
                    code.extend_from_slice(&[0x49, 0x83, 0xFB, 0x01, 0x0F, 0x82, 0x00, 0x00, 0x00, 0x00]);
                    traps_fixups.push((code.len() - 4, 2, idx as u32));
                    code.extend_from_slice(&[
                        0x4C, 0x3B, 0x5B, 0x20,                   // cmp r11, [rbx + 32]
                        0x0F, 0x8D, 0x00, 0x00, 0x00, 0x00        // jge trap
                    ]);
                    traps_fixups.push((code.len() - 4, 1, idx as u32));

                    code.extend_from_slice(&[
                        0x4F, 0x8B, 0x44, 0xDD, 0xF8, // r8 = [r13 + r11*8 - 8]
                        0x4F, 0x89, 0x44, 0xDD, 0x00, // [r13 + r11*8] = r8
                        0x49, 0xFF, 0xC3,             // r11++
                    ]);
                }
                Op::Drop => {
                    code.extend_from_slice(&[0x49, 0x83, 0xFB, 0x01, 0x0F, 0x82, 0x00, 0x00, 0x00, 0x00]);
                    traps_fixups.push((code.len() - 4, 2, idx as u32));
                    code.extend_from_slice(&[0x49, 0xFF, 0xCB]); // r11--
                }
                Op::Swap => {
                    code.extend_from_slice(&[0x49, 0x83, 0xFB, 0x02, 0x0F, 0x82, 0x00, 0x00, 0x00, 0x00]);
                    traps_fixups.push((code.len() - 4, 2, idx as u32));
                    code.extend_from_slice(&[
                        0x4F, 0x8B, 0x44, 0xDD, 0xF8, // r8 = [r13 + r11*8 - 8]
                        0x4F, 0x8B, 0x4C, 0xDD, 0xF0, // r9 = [r13 + r11*8 - 16]
                        0x4F, 0x89, 0x4C, 0xDD, 0xF8, // [r13 + r11*8 - 8] = r9
                        0x4F, 0x89, 0x44, 0xDD, 0xF0, // [r13 + r11*8 - 16] = r8
                    ]);
                }
                Op::Over => {
                    // Underflow
                    code.extend_from_slice(&[0x49, 0x83, 0xFB, 0x02, 0x0F, 0x82, 0x00, 0x00, 0x00, 0x00]);
                    traps_fixups.push((code.len() - 4, 2, idx as u32));
                    // Overflow
                    code.extend_from_slice(&[
                        0x4C, 0x3B, 0x5B, 0x20,                   // cmp r11, [rbx + 32]
                        0x0F, 0x8D, 0x00, 0x00, 0x00, 0x00        // jge trap
                    ]);
                    traps_fixups.push((code.len() - 4, 1, idx as u32));

                    code.extend_from_slice(&[
                        0x4F, 0x8B, 0x44, 0xDD, 0xF0, // r8 = [r13 + r11*8 - 16]
                        0x4F, 0x89, 0x44, 0xDD, 0x00, // [r13 + r11*8] = r8
                        0x49, 0xFF, 0xC3,             // r11++
                    ]);
                }
                Op::Inc | Op::Dec => {
                    code.extend_from_slice(&[0x49, 0x83, 0xFB, 0x01, 0x0F, 0x82, 0x00, 0x00, 0x00, 0x00]);
                    traps_fixups.push((code.len() - 4, 2, idx as u32));
                    code.extend_from_slice(&[
                        0x4F, 0x8B, 0x44, 0xDD, 0xF8, // r8 = [r13 + r11*8 - 8]
                    ]);
                    if op == Op::Inc { code.extend_from_slice(&[0x49, 0xFF, 0xC0]); } // inc r8
                    else { code.extend_from_slice(&[0x49, 0xFF, 0xC8]); }             // dec r8
                    code.extend_from_slice(&[
                        0x4D, 0x89, 0x44, 0xDF, 0xF8, // [r13 + r11*8 - 8] = r8
                    ]);
                }
                Op::Jump | Op::JZ => {
                    if op == Op::JZ {
                        code.extend_from_slice(&[0x49, 0x83, 0xFB, 0x01, 0x0F, 0x82, 0x00, 0x00, 0x00, 0x00]);
                        traps_fixups.push((code.len() - 4, 2, idx as u32));
                        code.extend_from_slice(&[
                            0x49, 0xFF, 0xCB, 0x4D, 0x89, 0xD8, 0x49, 0xC1, 0xE0, 0x03, 0x4D, 0x01, 0xE8, 0x49, 0x8B, 0x00,
                            0x48, 0x85, 0xC0, 0x0F, 0x84, 0x00, 0x00, 0x00, 0x00, 
                        ]);
                    } else {
                        code.extend_from_slice(&[0xE9, 0x00, 0x00, 0x00, 0x00]);
                    }
                    let target_idx_i64 = idx as i64 + 1 + data as i64;
                    if target_idx_i64 < 0 || target_idx_i64 > ops.len() as i64 {
                        return Err(ForthError::new(ForthErrorKind::JumpOutOfBounds { target: target_idx_i64 as usize }, ForthPhase::Compilation));
                    }
                    let target_idx = target_idx_i64 as usize;
                    jump_fixups.push((code.len() - 4, target_idx));
                }
                Op::Do => {
                    // Underflow Check (pop 2)
                    code.extend_from_slice(&[0x49, 0x83, 0xFB, 0x02, 0x0F, 0x82, 0x00, 0x00, 0x00, 0x00]);
                    traps_fixups.push((code.len() - 4, 2, idx as u32));
                    // Pop start and limit
                    code.extend_from_slice(&[
                        0x49, 0x83, 0xEB, 0x01, 0x4D, 0x89, 0xD8, 0x49, 0xC1, 0xE0, 0x03, 0x4D, 0x01, 0xE8, 0x49, 0x8B, 0x30, // rsi = start
                        0x49, 0x83, 0xEB, 0x01, 0x4D, 0x89, 0xD8, 0x49, 0xC1, 0xE0, 0x03, 0x4D, 0x01, 0xE8, 0x49, 0x8B, 0x38, // rdi = limit
                    ]);
                    // Push to loop_stack
                    code.extend_from_slice(&[
                        0x4C, 0x8B, 0x8B, 0x90, 0x00, 0x00, 0x00, // r9 = loop_stack_ptr (144)
                        0x48, 0x8B, 0x93, 0x98, 0x00, 0x00, 0x00, // rdx = loop_stack_depth (152)
                        // Overflow check: cap at loop_stack_limit
                        0x48, 0x3B, 0x93, 0xE0, 0x00, 0x00, 0x00, // cmp rdx, [rbx + 224]
                        0x0F, 0x8D, 0x00, 0x00, 0x00, 0x00,       // jge trap (1)
                    ]);
                    traps_fixups.push((code.len() - 4, 1, idx as u32));
                    code.extend_from_slice(&[
                        0x49, 0x89, 0x3C, 0xD1,                   // [r9 + rdx*8] = rdi (limit)
                        0x48, 0xFF, 0xC2,
                        0x49, 0x89, 0x34, 0xD1,                   // [r9 + rdx*8] = rsi (start)
                        0x48, 0xFF, 0xC2,
                        0x48, 0x89, 0x93, 0x98, 0x00, 0x00, 0x00, // loop_stack_depth = rdx
                    ]);
                }
                Op::Loop => {
                    // Fetch from loop_stack
                    code.extend_from_slice(&[
                        0x4C, 0x8B, 0x8B, 0x90, 0x00, 0x00, 0x00, // r9 = loop_stack_ptr
                        0x48, 0x8B, 0x93, 0x98, 0x00, 0x00, 0x00, // rdx = loop_stack_depth
                        0x49, 0x83, 0xFA, 0x02, 0x0F, 0x82, 0x00, 0x00, 0x00, 0x00, // jb trap (9)
                    ]);
                    traps_fixups.push((code.len() - 4, 9, idx as u32));
                    
                    code.extend_from_slice(&[
                        0x49, 0x8B, 0x74, 0xD1, 0xF8,             // rsi = start [r9 + rdx*8 - 8]
                        0x49, 0x8B, 0x7C, 0xD1, 0xF0,             // rdi = limit [r9 + rdx*8 - 16]
                        0x48, 0xFF, 0xC6,                         // inc rsi
                        0x48, 0x39, 0xFE,                         // cmp rsi, rdi
                        0x0F, 0x8D, 0x00, 0x00, 0x00, 0x00,       // jge loop_end
                    ]);
                    let jge_fixup = code.len() - 4;
                    
                    // Loop continues: write back start and jump
                    code.extend_from_slice(&[
                        0x49, 0x89, 0x74, 0xD1, 0xF8,             // [r9 + rdx*8 - 8] = rsi (start)
                        0xE9, 0x00, 0x00, 0x00, 0x00,             // jmp target
                    ]);
                    let rel = data as i32;
                    jump_fixups.push((code.len() - 4, (idx as i64 + 1 + rel as i64) as usize));
                    
                    // Loop ends: update depth
                    let end_pos = code.len();
                    let jge_rel = (end_pos - (jge_fixup + 4)) as i32;
                    code[jge_fixup..jge_fixup+4].copy_from_slice(&jge_rel.to_le_bytes());
                    
                    code.extend_from_slice(&[
                        0x49, 0x83, 0xEA, 0x02,                   // rdx -= 2
                        0x48, 0x89, 0x93, 0x98, 0x00, 0x00, 0x00, // loop_stack_depth = rdx
                    ]);
                }
                Op::I => {
                    // Fetch current index from loop_stack
                    code.extend_from_slice(&[
                        0x4C, 0x8B, 0x8B, 0x90, 0x00, 0x00, 0x00, // r9 = loop_stack_ptr
                        0x48, 0x8B, 0x93, 0x98, 0x00, 0x00, 0x00, // rdx = loop_stack_depth
                        0x49, 0x83, 0xFA, 0x01, 0x0F, 0x82, 0x00, 0x00, 0x00, 0x00, // jb trap
                    ]);
                    traps_fixups.push((code.len() - 4, 9, idx as u32));
                    code.extend_from_slice(&[
                        0x4B, 0x8B, 0x44, 0xD1, 0xF8,             // mov rax, [r9 + rdx*8 - 8]
                        0x4D, 0x89, 0x04, 0xDD,                   // mov [r13 + r11*8], rax
                        0x49, 0xFF, 0xC3,                         // inc r11
                    ]);
                }
                _ => { 
                    #[cfg(debug_assertions)]
                    panic!("Compiler bug: Opcode {:?} reached JIT emitter but was not filtered.", op);
                    #[cfg(not(debug_assertions))]
                    return Err(ForthError::new(ForthErrorKind::InvalidOpcode, ForthPhase::Compilation));
                }
            }
        }

        // Epilogue
        jump_targets.insert(ops.len(), code.len());
        let epilogue_pos = code.len();
        code.extend_from_slice(&[
            0x4C, 0x89, 0x5B, 0x28,                   // d_depth = r11 (40)
            0x4C, 0x89, 0x53, 0x38,                   // r_depth = r10 (56)
            0x48, 0xB8
        ]);
        let end_ip = super_idx + ops.len();
        code.extend_from_slice(&end_ip.to_le_bytes());
        code.extend_from_slice(&[
            0x48, 0x89, 0x43, 0x68, // trap_ip = rax (104)
            0x48, 0x89, 0xEC,       // mov rsp, rbp
            0x5D,                   // pop rbp
            0x9D,                   // popfq
            0x5F,             // pop rdi
            0x5E,             // pop rsi
            0x41, 0x5F,       // pop r15
            0x41, 0x5E,       // pop r14
            0x41, 0x5D,       // pop r13
            0x41, 0x5C,       // pop r12
            0x5B,             // pop rbx
            0xC3              // ret
        ]);

        // Trap Pad: Handle all unique trap sites
        let mut trap_targets = HashMap::new();
        for (i, &(_pos, id, inst_idx)) in traps_fixups.iter().enumerate() {
            trap_targets.insert(i, code.len());
            code.extend_from_slice(&[
                0x48, 0xC7, 0x43, 0x70, id, 0x00, 0x00, 0x00, // trap_code = id (112)
                0x48, 0xC7, 0x83, 0xD0, 0x00, 0x00, 0x00,           // mov [rbx+208], inst_idx
            ]);
            code.extend_from_slice(&inst_idx.to_le_bytes());
            code.extend_from_slice(&[
                0x4C, 0x89, 0x5B, 0x28,                   // d_depth = r11 (40)
                0x4C, 0x89, 0x53, 0x38,                   // r_depth = r10 (56)
                0x48, 0x89, 0xEC,       // mov rsp, rbp
                0x5D,                   // pop rbp
                0x9D,                   // popfq
                0x5F,             // pop rdi
                0x5E,             // pop rsi
                0x41, 0x5F,       // pop r15
                0x41, 0x5E,       // pop r14
                0x41, 0x5D,       // pop r13
                0x41, 0x5C,       // pop r12
                0x5B,             // pop rbx
                0xC3              // ret
            ]);
            while code.len() % 16 != 0 { code.push(0x90); }
        }

        // Fixups
        for (pos, target_idx) in jump_fixups {
            if let Some(&target_pos) = jump_targets.get(&target_idx) {
                let offset = (target_pos as i32 - (pos as i32 + 4)) as i32;
                code[pos..pos+4].copy_from_slice(&offset.to_le_bytes());
            } else {
                let offset = (epilogue_pos as i32 - (pos as i32 + 4)) as i32;
                code[pos..pos+4].copy_from_slice(&offset.to_le_bytes());
            }
        }

        for (i, &(pos, _id, _inst_idx)) in traps_fixups.iter().enumerate() {
            if let Some(&target) = trap_targets.get(&i) {
                _step_count += 1;
                let off = (target as i32 - (pos as i32 + 4)) as i32;
                code[pos..pos+4].copy_from_slice(&off.to_le_bytes());
            }
        }

        let mut mmap = MmapOptions::new().len(code.len().max(4096)).map_anon().map_err(|_| ForthError::new(ForthErrorKind::ExecutionStateCorrupted, ForthPhase::Compilation))?;
        mmap[..code.len()].copy_from_slice(&code);
        let mmap = mmap.make_exec().map_err(|_| ForthError::new(ForthErrorKind::ExecutionStateCorrupted, ForthPhase::Compilation))?;
        flush_icache(mmap.as_ptr(), code.len());

        self.blocks.insert(super_idx, JitBlock {
            func_ptr: mmap.as_ptr(), 
            contract: contract.clone(), 
            original_ops_len: ops.len(), 
            context, 
            is_poisoned: false, 
            poison_reason: None,
            poison_count: 0,
            last_retry: std::time::Instant::now(),
            _mmap: mmap,
        });
        Ok(())
    }
}


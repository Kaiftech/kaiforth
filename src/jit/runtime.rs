use std::collections::HashMap;
use memmap2::{MmapMut, MmapOptions};
use crate::jit::abi::JitContext;
use crate::core::types::{Op, SemanticContract};

pub struct JitEngine {
    pub blocks: HashMap<usize, (MmapMut, SemanticContract, usize)>, // (code, contract, original_len)
}

impl JitEngine {
    pub fn new() -> Self { Self { blocks: HashMap::new() } }
    
    pub fn compile_super(&mut self, super_idx: usize, ops: &[(Op, u64)], contract: &SemanticContract) {
        let mut code = Vec::new();
        let mut traps = Vec::new();

        // 1. Prologue: Hardened Context Validation
        code.extend_from_slice(&[
            0x48, 0x85, 0xFF,                         // test rdi, rdi
            0x0F, 0x84, 0x00, 0x00, 0x00, 0x00,       // jz trap_context_null (8)
        ]);
        traps.push((code.len() - 4, 8));

        // Alignment check
        code.extend_from_slice(&[
            0x48, 0x89, 0xF8,                         // mov rax, rdi
            0x48, 0x83, 0xE0, 0x07,                   // and rax, 7
            0x0F, 0x85, 0x00, 0x00, 0x00, 0x00,       // jnz trap_align (4)
        ]);
        traps.push((code.len() - 4, 4));

        // Magic check
        code.extend_from_slice(&[
            0x48, 0xB8, 0x48, 0x54, 0x52, 0x4F, 0x46, 0x49, 0x41, 0x4B, // mov rax, 0x4B4149464F525448
            0x48, 0x39, 0x07,                         // cmp [rdi], rax
            0x0F, 0x85, 0x00, 0x00, 0x00, 0x00,       // jnz trap_magic (3)
        ]);
        traps.push((code.len() - 4, 3));

        // Memory Base Sanitization
        code.extend_from_slice(&[
            0x48, 0x8B, 0x47, 0x18,                   // mov rax, [rdi+24] (mem_base)
            0x48, 0x85, 0xC0,                         // test rax, rax
            0x0F, 0x84, 0x00, 0x00, 0x00, 0x00,       // jz trap_memory (7)
        ]);
        traps.push((code.len() - 4, 7));

        // 2. Body Setup
        code.extend_from_slice(&[
            0x48, 0x8B, 0x4F, 0x08,                   // mov rcx, [rdi+8] (stack base)
            0x48, 0x8B, 0x57, 0x10,                   // mov rdx, [rdi+16] (depth ptr)
            0x48, 0x8B, 0x1A,                         // mov r11, [rdx] (current depth)
        ]);

        for &(op, data) in ops {
            match op {
                Op::Push => {
                    code.extend_from_slice(&[
                        0x49, 0x81, 0xFB, 0x00, 0x04, 0x00, 0x00, // cmp r11, 1024
                        0x0F, 0x83, 0x00, 0x00, 0x00, 0x00,       // jae trap_overflow (1)
                        0x48, 0xB8]);                             
                    traps.push((code.len() - 4, 1));
                    code.extend_from_slice(&data.to_le_bytes());
                    code.extend_from_slice(&[
                        0x48, 0x89, 0x04, 0xD9,                   // mov [rcx+r11*8], rax
                        0x49, 0xFF, 0xC3,                         // inc r11
                    ]);
                }
                Op::Add | Op::Sub | Op::Mul => {
                    code.extend_from_slice(&[
                        0x49, 0x83, 0xFB, 0x02,                   // cmp r11, 2
                        0x0F, 0x82, 0x00, 0x00, 0x00, 0x00,       // jb trap_underflow (2)
                        0x49, 0xFF, 0xCB,                         // dec r11
                        0x48, 0x8B, 0x04, 0xD9,                   // mov rax, [rcx+r11*8]
                        0x49, 0xFF, 0xCB,                         // dec r11
                    ]);
                    traps.push((code.len() - 13, 2));
                    match op {
                        Op::Add => code.extend_from_slice(&[0x48, 0x03, 0x04, 0xD9]),
                        Op::Sub => code.extend_from_slice(&[0x48, 0x2B, 0x04, 0xD9]),
                        Op::Mul => code.extend_from_slice(&[0x48, 0x0F, 0xAF, 0x04, 0xD9]),
                        _ => unreachable!()
                    }
                    code.extend_from_slice(&[
                        0x48, 0x89, 0x04, 0xD9,                   // mov [rcx+r11*8], rax
                        0x49, 0xFF, 0xC3,                         // inc r11
                    ]);
                }
                Op::Div => {
                    code.extend_from_slice(&[
                        0x49, 0x83, 0xFB, 0x02,                   // cmp r11, 2
                        0x0F, 0x82, 0x00, 0x00, 0x00, 0x00,       // jb trap_underflow
                        0x49, 0xFF, 0xCB,                         // dec r11
                        0x48, 0x8B, 0x0C, 0xD9,                   // mov rcx_tmp, [rcx+r11*8] (divisor)
                        0x48, 0x85, 0xC9,                         // test rcx_tmp, rcx_tmp
                        0x0F, 0x84, 0x00, 0x00, 0x00, 0x00,       // jz trap_divzero (6)
                        0x49, 0xFF, 0xCB,                         // dec r11
                        0x48, 0x8B, 0x04, 0xD9,                   // mov rax, [rcx+r11*8]
                        0x48, 0x99,                               // cdq
                        0x48, 0xF7, 0xF9,                         // idiv rcx_tmp
                        0x48, 0x8B, 0x4F, 0x08,                   // Restore rcx
                        0x48, 0x8B, 0x57, 0x10,                   // Restore rdx
                        0x48, 0x89, 0x04, 0xD9,                   // mov [rcx+r11*8], rax
                        0x49, 0xFF, 0xC3,                         // inc r11
                    ]);
                    traps.push((code.len() - 17, 2));
                    traps.push((code.len() - 11, 6));
                }
                Op::Fetch => {
                    code.extend_from_slice(&[
                        0x49, 0x83, 0xFB, 0x01,                   // cmp r11, 1
                        0x0F, 0x82, 0x00, 0x00, 0x00, 0x00,       // jb trap_underflow
                        0x49, 0xFF, 0xCB,                         // dec r11
                        0x48, 0x8B, 0x04, 0xD9,                   // mov rax, [rcx+r11*8] (addr)
                        0x48, 0x8B, 0x77, 0x18,                   // mov rsi, [rdi+24] (mem_base)
                        0x48, 0x3B, 0x47, 0x20,                   // cmp rax, [rdi+32] (mem_limit)
                        0x0F, 0x83, 0x00, 0x00, 0x00, 0x00,       // jae trap_memory (7)
                        0x48, 0x8B, 0x04, 0x06,                   // mov rax, [rsi+rax]
                        0x48, 0x89, 0x04, 0xD9,                   // mov [rcx+r11*8], rax
                        0x49, 0xFF, 0xC3,                         // inc r11
                    ]);
                    traps.push((code.len() - 21, 2));
                    traps.push((code.len() - 12, 7));
                }
                _ => {}
            }
        }

        // Epilogue
        code.extend_from_slice(&[
            0x48, 0x89, 0x1A,                         // mov [rdx], r11
            0xC3                                      // ret
        ]);

        // Finalize Trasp
        for (pos, id) in traps {
            let target = code.len() + (id as usize - 1) * 9;
            let offset = (target as i32 - (pos as i32 + 4)) as i32;
            code[pos..pos+4].copy_from_slice(&offset.to_le_bytes());
        }

        // Add trap landing pads
        for id in 1..=8 {
            code.extend_from_slice(&[
                0x48, 0xC7, 0x47, 0x28, id as u8, 0x00, 0x00, 0x00, // mov dword ptr [rdi+40], id
                0xC3                                                // ret
            ]);
        }

        let mut mmap = MmapOptions::new().len(code.len().max(4096)).map_anon().unwrap().make_mut().unwrap();
        mmap[..code.len()].copy_from_slice(&code);
        self.blocks.insert(super_idx, (mmap, contract.clone(), ops.len()));
    }
}

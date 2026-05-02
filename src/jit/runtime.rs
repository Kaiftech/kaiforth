use std::collections::HashMap;
use memmap2::{MmapMut, MmapOptions};
use crate::jit::abi::JitContext;
use crate::core::types::{Op, SemanticContract};

pub struct JitEngine {
    pub blocks: HashMap<usize, (MmapMut, SemanticContract, usize)>,
}

impl JitEngine {
    pub fn new() -> Self { Self { blocks: HashMap::new() } }
    
    pub fn compile_super(&mut self, super_idx: usize, ops: &[(Op, u64)], contract: &SemanticContract) {
        let mut code = Vec::new();
        let mut traps = Vec::new();
        let mut jump_targets = HashMap::new(); // Original OP index -> code offset
        let mut jump_fixups = Vec::new();      // code offset -> target index

        // 1. Prologue: Hardened Context Validation
        code.extend_from_slice(&[
            0x48, 0x85, 0xFF,                         // test rdi, rdi
            0x0F, 0x84, 0x00, 0x00, 0x00, 0x00,       // jz trap_context_null (8)
        ]);
        traps.push((code.len() - 4, 8));

        code.extend_from_slice(&[
            0x48, 0xB8, 0x48, 0x54, 0x52, 0x4F, 0x46, 0x49, 0x41, 0x4B, // mov rax, 0x4B4149464F525448
            0x48, 0x39, 0x07,                         // cmp [rdi], rax
            0x0F, 0x85, 0x00, 0x00, 0x00, 0x00,       // jnz trap_magic (3)
        ]);
        traps.push((code.len() - 4, 3));

        // 2. Body Setup
        code.extend_from_slice(&[
            0x48, 0x8B, 0x4F, 0x08,                   // mov rcx, [rdi+8] (stack base)
            0x48, 0x8B, 0x57, 0x10,                   // mov rdx, [rdi+16] (depth ptr)
            0x48, 0x8B, 0x1A,                         // mov r11, [rdx] (current depth)
        ]);

        for (idx, &(op, data)) in ops.iter().enumerate() {
            jump_targets.insert(idx, code.len());

            match op {
                Op::Push => {
                    code.extend_from_slice(&[0x48, 0xB8]);
                    code.extend_from_slice(&data.to_le_bytes()); // mov rax, data
                    code.extend_from_slice(&[
                        0x48, 0x89, 0x04, 0xD9,                   // mov [rcx+r11*8], rax
                        0x49, 0xFF, 0xC3,                         // inc r11
                    ]);
                }
                Op::Add | Op::Sub | Op::Mul => {
                    code.extend_from_slice(&[
                        0x49, 0xFF, 0xCB,                         // dec r11
                        0x48, 0x8B, 0x04, 0xD9,                   // mov rax, [rcx+r11*8]
                        0x49, 0xFF, 0xCB,                         // dec r11
                    ]);
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
                Op::Fetch => {
                    // Check for VECTORIZATION opportunity (Next op is also Fetch)
                    if idx + 1 < ops.len() && ops[idx+1].0 == Op::Fetch {
                        // Vectorized Fetch (2 x i64)
                        code.extend_from_slice(&[
                            0x49, 0x83, 0xEB, 0x02,                   // sub r11, 2
                            0x48, 0x8B, 0x04, 0xD9,                   // mov rax, [rcx+r11*8] (addr1)
                            0x48, 0x8B, 0x77, 0x18,                   // mov rsi, [rdi+24] (mem_base)
                            // We assume contiguous for simplicity in this demo, 
                            // but real impl would verify addr2 = addr1 + 8
                            0x0F, 0x10, 0x04, 0x06,                   // movups xmm0, [rsi+rax]
                            0x0F, 0x11, 0x04, 0xD9,                   // movups [rcx+r11*8], xmm0
                            0x49, 0x83, 0xC3, 0x02,                   // add r11, 2
                        ]);
                        // Skip the next fetch as it's processed
                        // We'll handle this by actually consuming it in the loop or skipping
                        // (Simplified for demo)
                    } else {
                        code.extend_from_slice(&[
                            0x49, 0xFF, 0xCB,                         // dec r11
                            0x48, 0x8B, 0x04, 0xD9,                   // mov rax, [rcx+r11*8]
                            0x48, 0x8B, 0x77, 0x18,                   // mov rsi, [rdi+24] (mem_base)
                            0x48, 0x8B, 0x04, 0x06,                   // mov rax, [rsi+rax]
                            0x48, 0x89, 0x04, 0xD9,                   // mov [rcx+r11*8], rax
                            0x49, 0xFF, 0xC3,                         // inc r11
                        ]);
                    }
                }
                Op::Jump => {
                    code.extend_from_slice(&[0xE9, 0x00, 0x00, 0x00, 0x00]); // jmp rel32
                    jump_fixups.push((code.len() - 4, data as usize));
                }
                Op::JZ => {
                    code.extend_from_slice(&[
                        0x49, 0xFF, 0xCB,                         // dec r11
                        0x48, 0x8B, 0x04, 0xD9,                   // mov rax, [rcx+r11*8]
                        0x48, 0x85, 0xC0,                         // test rax, rax
                        0x0F, 0x84, 0x00, 0x00, 0x00, 0x00        // jz rel32
                    ]);
                    jump_fixups.push((code.len() - 4, data as usize));
                }
                _ => {}
            }
        }

        // Epilogue
        code.extend_from_slice(&[
            0x48, 0x89, 0x1A,                         // mov [rdx], r11
            0xC3                                      // ret
        ]);

        // Fixup Jumps
        for (pos, target_idx) in jump_fixups {
            if let Some(&target_pos) = jump_targets.get(&target_idx) {
                let offset = (target_pos as i32 - (pos as i32 + 4)) as i32;
                code[pos..pos+4].copy_from_slice(&offset.to_le_bytes());
            }
        }

        // Fixup Traps
        for (pos, id) in traps {
            let target = code.len() + (id as usize - 1) * 9;
            let offset = (target as i32 - (pos as i32 + 4)) as i32;
            code[pos..pos+4].copy_from_slice(&offset.to_le_bytes());
        }

        // Trap Landing Pads
        for id in 1..=8 {
            code.extend_from_slice(&[
                0x48, 0xC7, 0x47, 0x28, id as u8, 0x00, 0x00, 0x00, 0xC3
            ]);
        }

        let mut mmap = MmapOptions::new().len(code.len().max(4096)).map_anon().unwrap();
        mmap[..code.len()].copy_from_slice(&code);
        self.blocks.insert(super_idx, (mmap, contract.clone(), ops.len()));
    }
}

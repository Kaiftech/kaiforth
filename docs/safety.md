# Safety Model (Hardware-Enforced)

Kaiforth uses hardware-level memory protection to guarantee execution safety with zero software overhead in the critical path.

## 1. Dual-Guard Hardware Stack
The data stack is allocated as a 4-page memory region with the following layout:
- **Page 0**: Leading Guard Page (`PROT_NONE` / `NOACCESS`)
- **Page 1-2**: Data Region (1024 cells of 8-byte `i64`)
- **Page 3**: Trailing Guard Page (`PROT_NONE` / `NOACCESS`)

### Behavior:
- **Underflow**: Any attempt to pop from an empty stack (writing/reading before Page 1) triggers an immediate hardware `Segmentation Fault` or `Access Violation`.
- **Overflow**: Any attempt to push beyond cell 1024 (writing into Page 3) triggers a hardware fault.
- **JIT Optimization**: Because these boundaries are hardware-enforced, the JIT-ed machine code has **zero software bounds checks**, enabling native-speed stack operations.

## 2. Vectorized Memory Safety
The JIT-ed `Fetch` and `Store` operations are optimized using **SIMD (SSE/AVX)** when contiguous access is detected.
- **Alignment**: The system ensures that memory-mapped segments are aligned to at least 16-byte boundaries to support `MOVAPS`/`MOVUPS` vectorized instructions.

## 3. Zero-Trust Machine Code
- **Trap 8 (Context NULL)**: Validates that the VM context is properly initialized.
- **Trap 3 (Magic Signature)**: Confirms the presence of the `0x4B4149464F525448` header.
- **Control Flow Integrity**: Jumps and branches within JIT blocks are re-calculated and patched during compilation to ensure they never escape the authorized machine code segment.

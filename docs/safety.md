# Safety Model (Zero-Trust & Transactional)

Kaiforth uses a multi-layered security architecture that combines hardware-level protection with **Shadow-Signal Transactional Auditing** to guarantee execution safety for untrusted code.

## 1. Hardware-Assisted Stack Protection
The data stack is protected by both software and hardware guards:
- **Leading/Trailing Guard Pages**: Memory-mapped pages with `PROT_NONE` surround the data region, catching illegal stack-pointer drift at the CPU level.
- **Software Bounds Checks**: The JIT compiler emits proactive `r11` (depth register) validation before every `Push` operation, ensuring that overflows are caught gracefully (Trap 1) before hitting the guard page.

## 2. Transactional Memory & Shadow Journaling
To achieve atomic execution safety, Kaiforth treats every JIT-optimized block as a **Transaction**.
- **The Journal**: Before any `Op::Store` is committed to main memory, the JIT records the target offset and the **original value** in a stack-allocated journal.
- **Shadow Detection**: The VM auditor (in Rust) uses the journal length as a "Shadow Signal" to detect memory writes. This makes it impossible for compromised JIT code to hide side effects or violate "Pure" block contracts.
- **Atomic Rollback**: If a JIT block encounters a hardware trap, contract violation, or out-of-bounds jump, the VM performs a **reverse-replay** of the journal, restoring all mutated memory to its exact pre-execution state.

## 3. W^X (Write XOR Execute) Protection
- **Separation of Concerns**: JIT memory is never simultaneously writable and executable.
- **Transition**: Code is generated in a `RW` buffer and then strictly transitioned to `RX` via hardware-enforced protection before the code is called.

## 4. ABI Hardening
- **Alignment Verification**: The JIT entry point enforces a mandatory **16-byte stack alignment** and 8-byte memory alignment check. This prevents Undefined Behavior (UB) caused by misaligned SIMD or pointer operations.
- **Null Safety**: All code pointers are validated for nullability before execution.

## 5. Formal Trap Hierarchy
JIT machine code communicates faults via an explicit trap system:
- **Trap 1/2**: Stack Overflow/Underflow (Software Guard).
- **Trap 7**: Absolute Memory Boundary Violation.
- **Trap 9**: Control-Flow Boundary Violation (Jump OOB).
- **Trap 10**: Transaction Journal Overflow (Capacity Exhaustion).
- **Shadow Audit**: Contract Verification ensures semantic alignment (D-Stack and Loop-Stack depth) after every successful block exit.

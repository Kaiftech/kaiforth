# Tiered Execution Pipeline

Kaiforth uses a three-tier execution strategy to balance cold-start latency and peak runtime performance.

## Tier 1: Bytecode Interpreter
- **Mechanism**: A standard dispatch loop.
- **Safety**: 100% safe Rust-level bounds checks.
- **Telemetry**: Streams `TraceEvent` objects into a runtime buffer for the optimizer.

## Tier 2: Profiling & Adaptive Optimizer
- **Mechanism**: The Segmentation engine identifies "Safe Basic Blocks" from the runtime trace.
- **Contract Generation**: Every sequence is assigned a `SemanticContract` describing its stack and memory impact.
- **Verification**: In debug mode, `debug_verify_contract` asserts that actual execution matches the contract's predictions.

## Tier 3: JIT Super-Instructions
- **Mechanism**: Optimized segments are compiled into raw x64 machine code with hardware-level protection.
- **Transactional Safety**: Every JIT execution follows a **Commit or Rollback** lifecycle.
    - **Step 1: Preparation**: Journal is initialized; Stack alignment and memory bounds are verified.
    - **Step 2: Execution**: Native machine code runs; Writes are recorded in the shadow journal.
    - **Step 3: Audit**: Rust post-handler verifies the `SemanticContract` using shadow signals from the journal.
    - **Step 4: Recovery**: If any trap (1-10) or contract mismatch occurs, the VM performs an **Atomic Rollback** of all memory mutation using the journal and falls back to Tier 1.
- **Desync Prevention**: The dispatcher uses the stored `original_len` to skip the bytecode segment after a successful JIT commit.

## Trap Mapping (Hardware-to-VM)
Precise fault mapping allows for safe recovery and debugging:
- **1**: Stack Overflow (Software Guard)
- **2**: Stack Underflow (Software Guard)
- **3**: Magic Signature Mismatch
- **4**: ABI Stack Alignment Fault
- **6**: Divide by Zero
- **7**: Memory OOB (Absolute Range Check)
- **8**: Context NULL
- **9**: Control-Flow OOB (Out-of-block Jump)
- **10**: Transaction Journal Overflow (mid-flight exhaustion)

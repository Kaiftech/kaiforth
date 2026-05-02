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
- **Fault Tolerance**: Explicit JIT Traps (1-8) map directly back to `ForthErrorKind` variants for precise debugging.
- **Desync Prevention**: The dispatcher uses the stored `original_len` of the bytecode segment to skip the correct number of instructions after a JIT run.

## Trap Mapping
- **1**: Stack Overflow
- **2**: Stack Underflow
- **3**: Magic Mismatch
- **4**: Alignment Fault
- **6**: Divide by Zero
- **7**: Memory OOB
- **8**: Context NULL

# Kaiforth VM

Kaiforth is a high-performance, production-hardened Forth Virtual Machine written in Safe Rust. It features a zero-panic design, hardware-enforced memory safety, and a persistent, adaptive Just-In-Time (JIT) optimizer. 

The architecture strictly adheres to a **Zero-Trust Model**, ensuring that speculative optimizations never compromise the integrity of the execution state.

---

## 🚀 Quickstart

Get up and running with Kaiforth in seconds.

### Prerequisites
- Rust (Edition 2024, version 1.85+)
- Cargo

### Build & Run
```bash
# Clone the repository
git clone https://github.com/yourusername/kaiforth.git
cd kaiforth

# Build for release (Optimized JIT requires release mode for peak performance)
cargo build --release

# Run the REPL
cargo run --release

# Run a specific Forth script
cargo run --release -- script.fth
```

---

## 📜 The History & The Glory

Kaiforth was born out of a desire to see how fast, safe, and intelligent a modern Forth engine could be when built with extreme architectural discipline.

- **Phase 1: The Zero-Panic Interpreter.** We started by eliminating all `unwrap()`, `expect()`, and unchecked memory access. Every failure became a strictly typed `ForthResult`. The foundation was unshakeable, but purely interpreted.
- **Phase 2: The Adaptive Profiler.** We didn't want a static, predictable engine. We built a trace-based telemetry system that watches chronological execution, detects loops, and scores repetitive mathematical sequences based on CPU cycle-reduction yield.
- **Phase 3: The JIT Emitter.** Safe Basic Blocks were converted into native x64 machine code. We bypassed the interpreter dispatch overhead and achieved near-native arithmetic speeds.
- **Phase 4: Extreme Hardening (The Glory).** We stopped trusting our own software. We implemented **Hardware-Enforced Stack Pages** (`[Guard][Data][Guard]`), O(1) Pre-Execution Contracts, and JIT Trap Sanitization. Finally, we added **SIMD Vectorized Memory Dispatch** and **Versioned Persistence** to instantly warm up the VM across sessions.

Kaiforth is no longer just a toy interpreter; it is a **hardware-backed, context-aware, adaptive execution engine.**

---

## 🏗️ Architecture & Tiered Execution

Kaiforth utilizes a 3-tier execution strategy to balance cold-start latency with peak runtime performance:

1. **Tier 1: Bytecode Interpreter (Cold Path)**
   - Executes standard bytecode safely with full Rust-level bounds checks.
   - Actively streams `TraceEvent` telemetry (opcodes, call depths, loops) into a runtime buffer.

2. **Tier 2: Adaptive Optimizer (Warm Path)**
   - **Segmentation Engine**: Analyzes execution traces to identify hot, repetitive mathematical subsets. It breaks sequences into atomic "Safe Basic Blocks," splitting paths whenever stack stability or memory aliasing becomes uncertain.
   - **Contract Synthesis**: Each extracted block is assigned a rigid `SemanticContract` detailing its stack and memory impact.

3. **Tier 3: JIT Super-Instructions (Hot Path)**
   - Hot sequences are compiled into raw x64 machine code.
   - The VM uses the instruction pointer (`ip`) to perform an O(1) lookup.
   - **Pre-Call Verification**: Before jumping into machine code, the VM performs a constant-time check to guarantee the current stack depth satisfies the segment's `SemanticContract`.

---

## 🛡️ Hardware-Enforced Zero-Trust Safety

Kaiforth relies on hardware limits and strict machine-code validation rather than trusting software state or optimization predictions.

### 1. Dual-Guard Hardware Stack
The Data Stack is allocated as a segmented memory-mapped region with explicit guard pages:
- **Layout**: `[PROT_NONE Guard] [Data Pages: 1024 cells] [PROT_NONE Guard]`
- **Effect**: Any attempt to overflow or underflow the stack triggers an immediate hardware `Segmentation Fault` (or `Access Violation` on Windows), halting the process instantly.
- **Performance**: This allows the JIT-ed machine code to operate with **zero software bounds checks** on the stack, enabling native-speed arithmetic.

### 2. JIT Trap Sanitization
The generated machine code exhaustively sanitizes its environment before executing:
- **Context Integrity**: Validates the `JitContext` for NULL pointers (Trap 8), 8-byte alignment (Trap 4), and a strict magic signature (Trap 3).
- **Memory Safety**: Verifies the `memory_base` pointer is sane (Trap 7).
- **Arithmetic Safety**: Pre-validates divisors before executing `idiv` to trap division by zero cleanly (Trap 6) instead of suffering uncatchable CPU exceptions.

### 3. ANS-Compliant Unwinding
If a trap or exception occurs, Kaiforth's `THROW/CATCH` system guarantees clean recovery:
- `CATCH` frames anchor the call stack, return stack, and data depth.
- `THROW` (or a caught JIT Trap) aborts execution and cleanly truncates all state back to the nearest anchor, preventing partial state leaks.

---

## 🧠 Adaptive Intelligence & Persistence

Kaiforth acts as a learning engine that adapts to workloads over time without relying on AI or heuristic guessing.

### 1. Sequence Scoring & Feedback
- **Promotion**: Sequences are scored based on cost-reduction yield (e.g., fusing memory operations scores exponentially higher than fusing simple math).
- **Decay**: The system garbage-collects unused super-instructions by halving scores every ~200,000 instructions.
- **Negative Feedback**: If a JIT sequence structurally traps, it receives a failure strike. Repeated failures permanently ban the sequence from future optimization.

### 2. Versioned Persistence
The `OptimizerState` can be serialized to disk (`opt.json`), allowing the VM to evolve across sessions:
- **Instant Warm-Up**: A restarted VM reloads known hot patterns and compiles them instantly, bypassing the Tier 1 profiling phase.
- **Integrity Validation**: Snapshots are wrapped in a `PersistenceContainer` with magic headers and version bytes. Incompatible or corrupted snapshots are gracefully rejected to maintain stability.

---

## ⚡ Advanced Optimizations

- **Vectorized SIMD Dispatch**: When the optimizer detects contiguous memory operations, the JIT emits SSE/AVX instructions (e.g., `MOVUPS`) to load/store 128-bit blocks (2 x 64-bit cells) simultaneously, significantly reducing latency for high-density data words.
- **Native Control Flow JIT**: The emitter fully supports `Jump` and `JZ` structures with relative patch resolution, eliminating the interpreter dispatch overhead entirely inside tight operational loops.

---

## 💻 Cross-Platform Guarantee

Kaiforth normalizes ABI inconsistencies (Win64 vs. SysV) entirely within its isolated `jit/abi.rs` layer. The GitHub Actions CI pipeline ensures that context pointers, memory protection, and trap logic execute deterministically across **Windows**, **Linux**, and **macOS**.

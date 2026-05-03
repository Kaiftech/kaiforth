# Optimizer Architecture

Kaiforth uses a segmented trace-based optimizer that transforms frequently executed bytecode paths into native machine code super-instructions.

## 1. Segmentation Engine
Unlike simple pattern matching, the Kaiforth optimizer uses a **Segmentation Engine** to break execution traces into atomic, safe basic blocks.
- **Safety Boundaries**: Splits occur at any point where stack stability is uncertain or memory aliasing might occur.
- **Contract Synthesis**: Each segment is analyzed to produce a `SemanticContract` which is then used by the JIT to generate safety traps.

## 2. Adaptive Learning
Patterns are tracked in a `HashMap` of `PatternStats`.
- **Promotion**: A pattern is promoted to `super_instructions` once its success score exceeds the threshold.
- **Contract Verification**: Every promoted sequence undergoes a **branch-aware** static analysis that proves the stack effect is identical across all possible jump/branch permutations within the segment.
- **Decay**: Unused or unstable patterns are slowly purged from the state.

## 3. Tiered JIT Dispatch
The VM uses the `ip` to look up optimized blocks in O(1) time.
- **Desync Prevention**: The stored `original_len` of the bytecode segment ensures the VM skips exactly the right number of instructions after a JIT run, even if the JIT internal logic performs constant folding or instruction reordering.
- **Fault Recovery**: JIT Traps are intercepted and converted into standard `ForthError` types, allowing the system to continue in Tier 1 (Interpreter) mode after a minor runtime violation.

# Learning Model

This document explains what "learning" practically means within the Kaiforth Runtime, how telemetry is processed, and how the system structurally adapts over time.

## 1. What "Learning" Means
The system employs **no AI, no heuristics-guessing, and no external black-box models**. 
Learning is defined strictly as: *Tracking the chronologically executed opcodes, identifying repetitive mathematical subsets within those traces, and structurally prioritizing sequences that save the most execution clock cycles.*

## 2. Tracked Topography Data
The system tracks operations contextually via `TraceContext`. Rather than creating a blind, flat vector of `[Op1, Op2, Op3]`, the tracker injects hierarchy constraints:
- `context_depth`: Incremented when entering a Call, decremented on Exit.
- `loop_depth`: Incremented inside `Do..Loop` structures.

The sequence miner (`extract_patterns`) uses these metrics to apply a `depth_multiplier`. 
A `Dup Add` running inside a nested loop is mathematically weighted exponentially higher than a `Dup Add` running once in the global scope.

## 3. Sequence Scoring & Selection Strategy
Super-instructions are **not** promoted based on instruction count alone.
The system calculates a pipeline cost:
```rust
Cost Reduction = (Sum of Individual Op Costs) + (Sequence Length * 2)
```
- A simple ALU op (e.g., `Add`) reduces cost by 1 unit.
- A `Fetch` or `Store` reduces cost by 3 units (as fusing memory instructions bypasses significant CPU branch misprediction penalties).

The system actively sorts and applies the super-instructions with the highest cost-reduction yield.

## 4. Memory Decay & Negative Feedback
The system forgets old behavior to adapt to new workloads.
- **Decay**: Every ~200,000 recorded instructions, all tracked sequence scores are halved (`count /= 2`). Patterns falling below the activity threshold are garbage collected.
- **Negative Feedback**: If a pattern structurally traps during live execution and causes a rollback, it receives a failure strike. Sequences with `>3` failure strikes are permanently ignored. The system does not waste cycles re-optimizing fundamentally unstable execution paths.

# Safety Model (Ultra-Hardened)

This document outlines the **Hardware-Enforced Zero-Trust** model of the Kaiforth VM.

## 1. Hardware-Level Stack Protection
The Data Stack is allocated as a memory-mapped region with a trailing **Guard Page**.
- **Segmented Allocation**: 1024 cells (8KB) backed by physical RAM.
- **PROT_NONE Guard**: The page following the stack is marked as `NOACCESS` (Windows) or `PROT_NONE` (Unix).
- **Result**: Any software bug or JIT bypass attempting to write past cell 1024 triggers a hardware page fault, halting the process instantly.

## 2. JIT Trap Classification (Sanitization)
The JIT-ed machine code no longer trusts any pointer or state. It performs exhaustive validation in its prologue:
- **Trap 8 (Context Null)**: Validates the `JitContext` pointer.
- **Trap 4 (Alignment)**: Ensures 8-byte alignment for the context.
- **Trap 3 (Magic Signature)**: Verifies the `0x4B414946...` magic header.
- **Trap 7 (Memory Sanitization)**: Validates that the `memory_base` pointer is non-null and sane before any `Fetch/Store`.
- **Trap 6 (DivZero)**: Pre-validates every divisor before `idiv` execution.

## 3. Contract ↔ Runtime Verification
Kaiforth implements a dual-layer verification system to ensure the optimizer never "lies":
- **Pre-Execution**: O(1) check of `SemanticContract` ensures the current stack depth satisfies the segment's requirements.
- **Segment Length Integrity**: The JIT dispatcher uses explicit segment lengths stored in the block map, eliminating desync risks when skipping bytecode.

## 4. Unwinding & Exception Safety
- **THROW/CATCH**: Implements recursive unwinding. If a `THROW` occurs inside a JIT block, the JIT aborts cleanly, and the VM restores state to the nearest `CATCH` anchor.
- **No Partial State**: The system ensures no partial JIT-ed state corruption survives an exception or trap.

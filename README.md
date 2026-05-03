# Kaiforth: Production-Grade JIT-Optimized Forth VM

Kaiforth is a high-performance, hardware-hardened, and ANS Forth compliant Virtual Machine written in Rust. It features a state-of-the-art JIT compiler with differential execution verification, designed for safety-critical and performance-sensitive applications.

## 🚀 Key Features

- **JIT Acceleration**: Transparently compiles hot Forth words to native machine code (x86_64).
- **Hardened Execution**: Uses `mmap` with guard pages, stack canaries, and shadow-stack verification to prevent memory corruption.
- **Differential Verification**: A unique "Paranoid Mode" that executes JIT code and Interpreted code in parallel, verifying state consistency on every step.
- **ANS Core Compliant**: Implements the standard Forth Core word set, including control flow (`IF/ELSE/THEN`, `DO/LOOP`), execution tokens (`EXECUTE`), and memory manipulation.
- **Zero-Dependency Core**: Minimal external dependencies for maximum stability and security auditability.

---

## 🛠️ Getting Started

### Prerequisites
- **Rust**: [Install Rust](https://rustup.rs/) (2021 Edition or later).
- **Platform**: Supports Windows, Linux, and macOS. Native JIT acceleration is currently optimized for x86_64; other architectures (like AArch64) automatically fallback to the high-performance interpreter core.

### Build and Install
```bash
git clone https://github.com/kaiftech/kaiforth.git
cd kaiforth
cargo build --release
```

### Run the REPL
Start the interactive interpreter:
```bash
./target/release/kaiforth
```

### Run a Script
```bash
./target/release/kaiforth script.fs
```

---

## 📖 Forth Primer

Kaiforth uses standard Forth postfix notation. Data is manipulated on a global Data Stack.

### Basic Math
```forth
2 3 + .      \ Pushes 2, pushes 3, adds them, prints 5
10 5 / .     \ Prints 2
```

### Word Definitions
Define new functions using `:` and `;`.
```forth
: square ( n -- n*n )
  dup * ;

5 square .   \ Prints 25
```

### Control Flow
Kaiforth supports standard ANS Forth conditionals and loops.
```forth
: is-it-even? ( n -- )
  2 mod 0= if
    ." Even"
  else
    ." Odd"
  then ;

4 is-it-even? cr  \ Prints "Even"
```

### Loops
```forth
: countdown ( n -- )
  0 do
    i . cr
  -1 +loop ;

10 countdown
```

---

## 🛡️ Architecture & Safety

### Component Diagram
```text
[ Source ] -> [ Parser ] -> [ Interpreter ] <-> [ JIT Engine ]
                                 |                  |
                                 v                  v
                          [ Hardened Memory ] [ Hardware Stacks ]
                                 |                  |
                                 \------------------/
                                          |
                                [ Differential Verifier ]
```

### Security Mechanisms
1. **Guard Pages**: Data stack is surrounded by `PROT_NONE` memory to trap overflows instantly at the hardware level.
2. **Stack Canaries**: `0xDEADBEEFCAFEBABE` values are checked after every JIT block execution to detect "off-by-one" stack corruption.
3. **Shadow Transactions**: JIT execution is performed in a transactional buffer. If divergence is detected, the state is rolled back to the last known-good interpreted state.

---

## ⚙️ Configuration

Tune the VM behavior via environment variables or CLI flags:

- `KAIFORTH_JIT_ENABLED`: `1` or `0` (Default: 1)
- `KAIFORTH_PARANOID`: Enable differential verification (Default: 0)
- `KAIFORTH_TRACE`: Print execution traces for debugging.

---

## 📜 License
Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.

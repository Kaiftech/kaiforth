# Kaiforth: Production-Grade JIT-Optimized Forth VM

[![CI](https://github.com/kaiftech/kaiforth/actions/workflows/ci.yml/badge.svg)](https://github.com/kaiftech/kaiforth/actions/workflows/ci.yml)
[![Release](https://github.com/kaiftech/kaiforth/actions/workflows/release.yml/badge.svg)](https://github.com/kaiftech/kaiforth/releases/latest)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)

Kaiforth is a high-performance, hardware-hardened, and ANS Forth compliant Virtual Machine written in Rust. It features a state-of-the-art JIT compiler with differential execution verification, designed for safety-critical and performance-sensitive applications.

## 🚀 Key Features

- **JIT Acceleration**: Transparently compiles hot Forth words to native machine code (x86_64).
- **Hardened Execution**: Uses `mmap` with guard pages, stack canaries, and shadow-stack verification to prevent memory corruption.
- **Differential Verification**: A unique "Paranoid Mode" that executes JIT code and Interpreted code in parallel, verifying state consistency on every step.
- **ANS Core Compliant**: Implements the standard Forth Core word set, including control flow (`IF/ELSE/THEN`, `DO/LOOP`), execution tokens (`EXECUTE`), and memory manipulation.
- **Zero-Dependency Core**: Minimal external dependencies for maximum stability and security auditability.

---

## ⬇️ Download Pre-Built Binaries

No Rust installation required. Grab the latest release for your platform directly from [GitHub Releases](https://github.com/kaiftech/kaiforth/releases/latest):

| Platform | Architecture | Download |
|---|---|---|
| Linux | x86_64 | [`kaiforth-linux-x86_64`](https://github.com/kaiftech/kaiforth/releases/latest/download/kaiforth-linux-x86_64) |
| Windows | x86_64 | [`kaiforth-windows-x86_64.exe`](https://github.com/kaiftech/kaiforth/releases/latest/download/kaiforth-windows-x86_64.exe) |
| macOS | Intel (x86_64) | [`kaiforth-macos-x86_64`](https://github.com/kaiftech/kaiforth/releases/latest/download/kaiforth-macos-x86_64) |
| macOS | Apple Silicon (ARM64) | [`kaiforth-macos-arm64`](https://github.com/kaiftech/kaiforth/releases/latest/download/kaiforth-macos-arm64) |

**Linux/macOS** — make it executable and run:
```bash
chmod +x kaiforth-linux-x86_64
./kaiforth-linux-x86_64
```

**Windows** — just double-click or run from PowerShell:
```powershell
.\kaiforth-windows-x86_64.exe
```

---

## 🛠️ Build From Source

### Prerequisites
- **Rust**: [Install Rust](https://rustup.rs/) (2024 Edition).
- **Platform**: Windows, Linux, macOS. JIT acceleration is x86_64 only; AArch64 automatically uses the interpreter core.

```bash
git clone https://github.com/kaiftech/kaiforth.git
cd kaiforth
cargo build --release
```

Output binary:
```
target/release/kaiforth        # Linux / macOS
target/release/kaiforth.exe    # Windows
```

### Run the REPL
```bash
./target/release/kaiforth
```

### Run a Forth Script
```bash
./target/release/kaiforth script.fs
```

### Run Tests
```bash
cargo test
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

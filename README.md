# kaiforth

A high-performance Forth virtual machine written in safe Rust.  
Packed-opcode core · recursive constant-folding optimizer · zero panics.

---

## Build & Run

**Requirements:** Rust toolchain — install from [rustup.rs](https://rustup.rs)

```bash
# Clone
git clone https://github.com/<your-username>/kaiforth.git
cd kaiforth

# Interactive REPL
cargo run --release

# Run a .fs file
cargo run --release -- examples/fib.fs

# Run benchmark
cargo run --release -- benchmarks/bench.fs
```

The compiled binary is at `target/release/kaiforth.exe` (Windows) or `target/release/kaiforth` (Linux/macOS).  
You can run it directly after building:

```bash
cargo build --release
./target/release/kaiforth           # REPL
./target/release/kaiforth file.fs   # script
```

---

## The Forth Language — Quick Reference

Forth is a **stack-based** language. You push values onto the stack, then call words (functions) that consume and produce values.

### How the stack works

```forth
3 4 +       \ push 3, push 4, add → stack: [7]
7 .         \ print top of stack  → prints: 7
```

Reading stack notation: `( before -- after )`  
Example: `+` has signature `( a b -- a+b )`

---

### Arithmetic

```forth
2 3 +   .    \ 5
10 4 -  .    \ 6
3 4 *   .    \ 12
10 2 /  .    \ 5
7 3 mod .    \ 1
5 negate .   \ -5
-3 abs  .    \ 3
```

---

### Stack Operations

| Word   | Effect              |
|--------|---------------------|
| `dup`  | `( a -- a a )`      |
| `drop` | `( a -- )`          |
| `swap` | `( a b -- b a )`    |
| `over` | `( a b -- a b a )`  |
| `2dup` | `( a b -- a b a b )` |
| `2drop`| `( a b -- )`        |

```forth
5 dup * .        \ 25  (5 squared)
1 2 swap . .     \ 1 2
```

---

### Printing

```forth
42 .        \ print integer
.s          \ show entire stack (non-destructive)
65 emit     \ print ASCII character → A
cr          \ newline
```

---

### Defining Words

```forth
: square ( n -- n^2 )  dup * ;
: cube   ( n -- n^3 )  dup square * ;

5 square .   \ 25
3 cube .     \ 27
```

Words are compiled and optimized — `dup *` is fused into a single `Square` opcode automatically.

---

### Variables & Constants

```forth
variable counter       \ declare a variable
10 counter !           \ store 10 into counter
counter @ .            \ fetch and print → 10

42 constant answer     \ declare a constant
answer .               \ → 42
```

---

### Conditionals

```forth
\ if ... then
: positive? ( n -- )
  0 > if ." yes" then ;

\ if ... else ... then
: sign ( n -- )
  0 < if ." negative" else ." non-negative" then ;
```

---

### Loops

```forth
\ begin ... until  (repeat until top is nonzero)
: countdown ( n -- )
  begin
    dup .
    1 -
    dup 0 =
  until
  drop ;

5 countdown    \ prints: 5 4 3 2 1

\ begin ... while ... repeat
: sum-to ( n -- sum )
  0 swap
  begin
    dup 0 >
  while
    over + swap 1 - swap
  repeat
  drop ;

10 sum-to .    \ 55
```

---

### Recursion

```forth
: fib ( n -- fib[n] )
  dup 2 < if exit then
  dup 1 - recurse
  swap 2 - recurse
  + ;

10 fib .    \ 55
```

---

### Float Arithmetic

```forth
1.5 2.5 f+ f.    \ 4.0
3.0 2.0 f* f.    \ 6.0
10.0 4.0 f/ f.   \ 2.5
```

---

### Comparisons

```forth
3 5 <  .    \ -1  (true in Forth)
5 3 <  .    \ 0   (false)
4 4 =  .    \ -1
0 0=   .    \ -1  (0= checks if zero)
```

Forth uses `-1` for true and `0` for false.

---

### Including Files

```forth
include examples/fib.fs
```

---

### Diagnostic Tools

```forth
see square      \ decompile and show optimized bytecode of 'square'
trace           \ toggle instruction-level tracing ON/OFF
bench           \ toggle execution timing ON/OFF
.s              \ show current stack
bye             \ exit
```

**Example `see` output:**
```
Definition of 'square':
   0 | Square          ← dup * was fused into one opcode
   1 | Exit
```

---

## Examples

Run the included examples:

```bash
cargo run --release -- examples/fib.fs
cargo run --release -- benchmarks/bench.fs
```

---

## Architecture

| Layer | Detail |
|---|---|
| **Instruction set** | 16 opcodes packed as `u8` (cache-friendly) |
| **Optimizer** | Recursive fix-point: constant folding + superinstruction fusion |
| **Safety** | Zero `unsafe`, zero `unwrap`, all errors return `ForthResult` |
| **Diagnostics** | `RichError` with call stack, word context, data/float snapshots |
| **Dispatch** | `match` on safe `Op::from_u8` — validated at every step |

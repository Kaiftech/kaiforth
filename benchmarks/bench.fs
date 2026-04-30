\ ─────────────────────────────────────────────
\ kaiforth — benchmarks/bench.fs
\ Performance stress test
\ Run with: kaiforth benchmarks/bench.fs
\ ─────────────────────────────────────────────

bench   \ toggle benchmarking ON

\ --- Superinstruction fusion: dup * -> Square ---
: square dup * ;

\ --- Constant-fold chain: 2 3 + 4 * -> Push(20) ---
2 3 + 4 * .    ( should print 20 )

\ --- Repeated squares (hot loop) ---
: sq10 ( n -- n^10 )
  square square square square square square square square square square ;

2 sq10 .    ( 2^10 = 1024 )

\ --- Recursive fib (stress) ---
: fib ( n -- fib[n] )
  dup 2 < if exit then
  dup 1 - recurse
  swap 2 - recurse + ;

10 fib .    ( 55 )

bench   \ toggle benchmarking OFF

\ ─────────────────────────────────────────────
\ kaiforth — examples/fib.fs
\ Recursive Fibonacci with optimizer showcase
\ ─────────────────────────────────────────────

\ --- Optimizer demo: 'dup *' fuses to Square ---
: square ( n -- n^2 )  dup * ;
: cube   ( n -- n^3 )  dup square * ;

\ --- Recursive Fibonacci ---
: fib ( n -- fib[n] )
  dup 2 < if exit then
  dup 1 - recurse
  swap 2 - recurse
  + ;

\ --- Inspect optimized bytecode ---
see square
see fib

\ --- Run ---
5 square .     ( 25 )
3 cube .       ( 27 )
7 fib .        ( 13 )

\ --- Float arithmetic ---
2.5 3.5 f+ 2.0 f* f. cr   ( 12.0 )

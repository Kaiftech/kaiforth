\ test_all.fs

: type ( addr len -- )
  0 do dup i + c@ emit loop drop ;

\ 1. Arithmetic & Stack
: test-arith
  10 20 + 30 = if ." Arithmetic OK" cr else ." Arithmetic FAIL" cr then ;
test-arith
1 2 3 rot . . . cr

\ 2. Loops
: count-test ( n -- )
  0 do i . loop ;
5 count-test cr

\ 3. Create Does
: my-var create , does> @ ;
123 my-var foo
foo . cr

\ 4. Exception Handling
: test-throw 1 throw ;
: test-catch ['] test-throw catch ;
test-catch . cr

\ 5. String System
S" Hello World" type cr

\ 7. Base
hex 10 decimal . cr
bye

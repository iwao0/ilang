// `console.log(some(<value>))` prints the inner value, not the box
// pointer. Pre-fix the JIT side passed the raw box pointer to
// `emit_print_value`, so primitive Optional values printed as
// `some(<address>)` while the interpreter happily printed
// `some(<value>)` — a parity mismatch the harness would have caught
// once tests were written.
//
// expect: some(42)
// expect: some(3.14)
// expect: some(hi)
// expect: some(true)
// expect: some([1, 2])
// expect: none

console.log(some(42))
console.log(some(3.14))
console.log(some("hi"))
console.log(some(true))
console.log(some([1, 2]))
console.log(none)

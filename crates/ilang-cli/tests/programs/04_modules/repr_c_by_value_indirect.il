// `by_value` for **structs > 16 B**: the JIT marks the param with
// Cranelift's `StructArgument(size)` purpose, which delegates to
// the per-target C ABI (AArch64 AAPCS64 hands the callee a hidden
// pointer; x86_64 SysV copies onto the stack). The ilang side just
// hands over the user pointer; Cranelift does the rest.
//
// interp: skip
use test

@repr(C) class big32 {
    a: i64
    b: i64
    c: i64
    d: i64
}

@extern(by_value) fn big32_sum(big: big32): i64

let g = new big32()
g.a = 10
g.b = 20
g.c = 30
g.d = 40
test.expect(big32_sum(g), 100)

// Original is untouched (caller still owns it post-call).
g.a = 1000
g.b = 2000
g.c = 3000
g.d = 4000
test.expect(big32_sum(g), 10000)

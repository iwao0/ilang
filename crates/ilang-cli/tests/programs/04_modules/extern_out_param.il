// `out<T>` parameter marshalling: the C side declares `*mut T` and
// writes through it; the JIT auto-allocates a stack slot at the
// call site, hands its address over as the actual arg, and after
// the call surfaces the written value as part of the result. The
// caller never sees a raw pointer — the inner T is folded into the
// return as an extra tuple element (or replaces the return entirely
// when the C side returns void with one out).
//
// JIT-only — the interpreter has no stack-slot model for this.
// interp: skip
use test

@extern fn parse_int_out(s: string, out_val: out<i32>): i32

// Single out + non-Unit return → ilang sees `(i32, i32)`. First
// element is the C status (1 = ok, 0 = parse fail), second is the
// parsed value loaded from the slot.
let r1 = parse_int_out("42")
test.expect(r1[0] as i64, 1)
test.expect(r1[1] as i64, 42)

let r2 = parse_int_out("-7")
test.expect(r2[0] as i64, 1)
test.expect(r2[1] as i64, -7)

// Failure path: parse fails, out value is never written. The
// caller still gets a tuple (the slot keeps whatever Cranelift's
// stack-init placed there, which is implementation-defined — we
// only assert the status element).
let r3 = parse_int_out("not a number")
test.expect(r3[0] as i64, 0)

// Multiple outs → tuple grows to (status, q, r).
@extern fn div_mod_out(a: i32, b: i32, out_q: out<i32>, out_r: out<i32>): i32

let r4 = div_mod_out(17 as i32, 5 as i32)
test.expect(r4[0] as i64, 0)
test.expect(r4[1] as i64, 3)
test.expect(r4[2] as i64, 2)

let r5 = div_mod_out(1 as i32, 0 as i32)
test.expect(r5[0] as i64, -1)

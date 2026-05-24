// By-value extended to **struct returns** (≤ 16 B integer-only).
// The C side returns the struct in 1–2 GPRs (X0 / X0:X1 on
// AArch64; RAX / RAX:RDX on x86_64 SysV); the JIT receives the
// chunks, allocs a fresh ilang instance, and stores the bits
// back into its user area so the caller sees a normal heap
// Object with rc=1.
//
// jit: skip
use std.test

@extern(C) {
    struct point2 {
        x: i32
        y: i32
    }

    struct range64 {
        lo: i64
        hi: i64
    }

    fn make_point2(x: i32, y: i32): point2
    fn make_range64(lo: i64, hi: i64): range64
}

let p = make_point2(11 as i32, 22 as i32)
test.expect(p.x as i64, 11)
test.expect(p.y as i64, 22)

let r = make_range64(100, 250)
test.expect(r.lo, 100)
test.expect(r.hi, 250)

// Multiple calls, independent results.
let p2 = make_point2(-3 as i32, 4 as i32)
test.expect(p2.x as i64, -3)
test.expect(p2.y as i64, 4)
test.expect(p.x as i64, 11)

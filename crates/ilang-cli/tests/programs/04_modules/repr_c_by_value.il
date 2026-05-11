// Pass `@extern(C) struct`s **by value** to a host C-ABI fn. The JIT
// expands each struct param into 1–2 i64 chunks per the AArch64 /
// SysV "integer-only ≤ 16 B composite" rule. The Rust-side host
// helpers (`test_sum_point2`, etc.) are declared with `extern "C"`
// + `#[repr(C)]` so they read the chunks back as natural fields.
//
// Inside `@extern(C)`, struct args without `*` pass by value
// implicitly — no explicit flag needed.
//
// Interpreter has no native-extern marshalling, skip there.
// jit: skip
use test

@extern(C) {
    struct point2 {
        x: i32
        y: i32
    }

    struct range64 {
        lo: i64
        hi: i64
    }

    struct tagged {
        tag: i32
        payload: i64
    }

    fn sum_point2(p: point2): i64
    fn range64_width(r: range64): i64
    fn tagged_payload_if(t: tagged, expected_tag: i32): i64
}

// 8 B struct → 1 i64 chunk.
let p = new point2()
p.x = 10 as i32
p.y = 32 as i32
test.expect(sum_point2(p), 42)

// 16 B struct → 2 i64 chunks.
let r = new range64()
r.lo = 100
r.hi = 250
test.expect(range64_width(r), 150)

// 16 B struct (i32 + 4 B padding + i64) → 2 chunks. Tag matches.
let t = new tagged()
t.tag = 7 as i32
t.payload = 999
test.expect(tagged_payload_if(t, 7 as i32), 999)
test.expect(tagged_payload_if(t, 8 as i32), -1)

// Mutate after a call: by-value means the C side saw a copy, the
// original struct is untouched and re-passable.
p.x = 1 as i32
p.y = 2 as i32
test.expect(sum_point2(p), 3)

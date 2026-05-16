// True by-value passing for CRepr struct / union across ilang →
// ilang calls. Previously these were pointer-passed, which both
// gave aliasing semantics that conflicted with the documented
// "copied on assignment / argument passing" rule AND defeated
// stack promotion (the callee's pointer use leaked the value
// out of the caller's frame, forcing heap fallback).
//
// Now params are split per the C ABI: ≤8 B → 1 i64 chunk,
// ≤16 B → 2 i64 chunks, all-float ≤4 fields → HFA float regs,
// >16 B → indirect sret. The callee receives chunks and
// reassembles them into a fresh stack buffer that lives in its
// own frame, so the caller's stack-promoted struct survives the
// call intact.
use test

// ≤8 B → single i64 chunk path.
struct Small {
    x: i32
    y: i32
}

// ≤16 B → 2 i64 chunk path.
struct Mid {
    a: i64
    b: i64
}

// >16 B → indirect sret return path; param path uses 4 chunks
// (each i64). Cranelift falls back to a hidden pointer for
// param too on most ABIs when the struct exceeds the chunk
// budget; either way the test only cares about the values.
struct Big {
    a: i64
    b: i64
    c: i64
    d: i64
}

// All-float ≤4 fields → HFA float register path.
struct Vec3 {
    x: f32
    y: f32
    z: f32
}

fn sum_small(p: Small): i32 { p.x + p.y }
fn sum_mid(m: Mid): i64 { m.a + m.b }
fn sum_big(b: Big): i64 { b.a + b.b + b.c + b.d }
fn sum_vec3(v: Vec3): f32 { v.x + v.y + v.z }

fn make_small(): Small { Small { x: 3, y: 4 } }
fn make_mid(): Mid { Mid { a: 100, b: 200 } }
fn make_big(): Big { Big { a: 1, b: 2, c: 3, d: 4 } }
fn make_vec3(): Vec3 { Vec3 { x: 1.0 as f32, y: 2.0 as f32, z: 3.0 as f32 } }

// Pass through a call twice — proves the caller's local survives
// the first call (with the old pointer-passing the second call
// would still work, but stack promotion would have leaked the
// pointer on the first call; here we confirm both behaviour and
// that the same value flows through).
test.expect(sum_small(make_small()) as i64, 7)
test.expect(sum_mid(make_mid()), 300)
test.expect(sum_big(make_big()), 10)
test.expectF64((sum_vec3(make_vec3()) as f64) - 6.0, 0.0)

// Mutating the callee's copy doesn't touch the caller's value —
// the whole point of value semantics. Cover the small (chunked)
// and big (caller-side memcpy) paths separately because they go
// through different ABI code: chunks bypass the caller's pointer
// entirely (the bytes ride in registers), while >16 B sends a
// pointer to a scratch StackSlot the caller copied into.
fn mutate_small(p: Small) {
    p.x = 999
    p.y = 999
}
fn mutate_big(b: Big) {
    b.a = 999
    b.b = 999
    b.c = 999
    b.d = 999
}

let orig_small = Small { x: 11, y: 22 }
mutate_small(orig_small)
test.expect(orig_small.x as i64, 11)
test.expect(orig_small.y as i64, 22)

let orig_big = Big { a: 11, b: 22, c: 33, d: 44 }
mutate_big(orig_big)
test.expect(orig_big.a, 11)
test.expect(orig_big.b, 22)
test.expect(orig_big.c, 33)
test.expect(orig_big.d, 44)

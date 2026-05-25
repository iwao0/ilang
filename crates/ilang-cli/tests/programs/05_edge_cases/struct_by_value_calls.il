// True by-value passing for CRepr struct / union across
// ilang → ilang calls. Previously these were pointer-passed,
// which both gave aliasing semantics that conflicted with the
// documented "copied on assignment / argument passing" rule
// AND defeated stack promotion (the callee's pointer use leaked
// the value out of the caller's frame, forcing heap fallback).
//
// The ilang ABI now splits args per the by-value rules with a
// generous chunk cap (`IL_BYVAL_CHUNK_MAX` in
// `crates/ilang-mir-codegen/src/compile/abi.rs`, currently
// 64 B). Within the cap the struct rides in 1..N i64 GPR chunks
// (N = ceil(c_size / 8)). All-float ≤4 fields go through HFA
// float regs. Over the cap, the call site memcpys the source
// struct into a scratch StackSlot in the caller's frame and
// hands the slot's pointer to the callee. Either way the callee
// receives its own copy.
//
// (The C ABI keeps the platform-fixed 16 B threshold; this
// fixture only exercises the ilang side.)
use std.test as test
// 8 B → single i64 chunk.
struct Small {
    x: i32
    y: i32
}

// 16 B → 2 i64 chunks.
struct Mid {
    a: i64
    b: i64
}

// 32 B → 4 i64 chunks. Under the previous 16 B cap this used
// to fall through to the memcpy-scratch path; with the new
// 64 B cap it rides in registers like the smaller cases.
struct Big {
    a: i64
    b: i64
    c: i64
    d: i64
}

// 96 B → over the 64 B cap, takes the caller-side memcpy path.
struct Huge {
    a: i64
    b: i64
    c: i64
    d: i64
    e: i64
    f: i64
    g: i64
    h: i64
    i: i64
    j: i64
    k: i64
    l: i64
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
fn sum_huge(h: Huge): i64 {
    h.a + h.b + h.c + h.d + h.e + h.f + h.g + h.h + h.i + h.j + h.k + h.l
}
fn sum_vec3(v: Vec3): f32 { v.x + v.y + v.z }

fn make_small(): Small { Small { x: 3, y: 4 } }
fn make_mid(): Mid { Mid { a: 100, b: 200 } }
fn make_big(): Big { Big { a: 1, b: 2, c: 3, d: 4 } }
fn make_huge(): Huge {
    Huge { a: 1, b: 2, c: 3, d: 4, e: 5, f: 6, g: 7, h: 8, i: 9, j: 10, k: 11, l: 12 }
}
fn make_vec3(): Vec3 { Vec3 { x: 1.0 as f32, y: 2.0 as f32, z: 3.0 as f32 } }

test.expect(sum_small(make_small()) as i64, 7)
test.expect(sum_mid(make_mid()), 300)
test.expect(sum_big(make_big()), 10)
test.expect(sum_huge(make_huge()), 78)
test.expectF64((sum_vec3(make_vec3()) as f64) - 6.0, 0.0)

// Mutating the callee's copy doesn't touch the caller's value —
// the whole point of value semantics. Cover the chunked path
// (≤ cap) and the memcpy-scratch path (> cap) separately so a
// regression in either ABI shape is caught here.
fn mutate_small(p: Small) { p.x = 999; p.y = 999 }
fn mutate_big(b: Big) { b.a = 999; b.b = 999; b.c = 999; b.d = 999 }
fn mutate_huge(h: Huge) {
    h.a = 999
    h.l = 999
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

let orig_huge = Huge { a: 11, b: 22, c: 33, d: 44, e: 55, f: 66, g: 77, h: 88, i: 99, j: 100, k: 111, l: 122 }
mutate_huge(orig_huge)
test.expect(orig_huge.a, 11)
test.expect(orig_huge.l, 122)

// Set<T> across every primitive element type. `set_basics.il`
// already covers the full method matrix on `Set<i64>` and
// `Set<string>`; this file repeats every method (add / has /
// delete / size / clear / values / forEach + union /
// intersection / difference + isSubsetOf / isSupersetOf /
// isDisjointFrom) against each remaining width — signed i8/i16/i32,
// unsigned u8/u16/u32/u64, bool, and floats — so a regression that
// only shows up on a narrower or differently-tagged element type
// gets caught here. Mirrors `map_keys_int_widths.il`'s concern
// (width-collapse on values()) and extends it to every method.
use std.test as test

// ---- i8 ----
let i8s = new Set<i8>()
i8s.add(-128i8)
i8s.add(127i8)
i8s.add(-128i8)             // dup ignored
test.expect(i8s.size(), 2)
test.expectBool(i8s.has(127i8), true)
test.expectBool(i8s.has(0i8), false)
test.expectBool(i8s.delete(127i8), true)
test.expectBool(i8s.delete(127i8), false)
test.expect(i8s.size(), 1)
i8s.add(127i8)
// values() must preserve i8 width — i8 + i8 wraps within the type.
let i8_vs = i8s.values()
let i8_sum: i8 = i8_vs[0] + i8_vs[1]
test.expect(i8_sum as i64, -1)   // -128 + 127
// forEach visits every remaining element exactly once.
let i8_seen = 0
i8s.forEach(fn(v: i8) { i8_seen = i8_seen + (v as i64) })
test.expect(i8_seen, -1)
i8s.clear()
test.expect(i8s.size(), 0)

// i8 set algebra.
let i8a = new Set<i8>()
i8a.add(1i8)
i8a.add(2i8)
i8a.add(3i8)
let i8b = new Set<i8>()
i8b.add(2i8)
i8b.add(3i8)
i8b.add(4i8)
test.expect(i8a.union(i8b).size(), 4)
test.expect(i8a.intersection(i8b).size(), 2)
test.expect(i8a.difference(i8b).size(), 1)
test.expectBool(i8a.isSubsetOf(i8a.union(i8b)), true)
test.expectBool(i8a.isSupersetOf(i8b), false)
test.expectBool(i8a.isDisjointFrom(i8b), false)
let i8c = new Set<i8>()
i8c.add(10i8)
test.expectBool(i8a.isDisjointFrom(i8c), true)

// ---- i16 ----
let i16s = new Set<i16>()
i16s.add(-32768i16)
i16s.add(32767i16)
i16s.add(32767i16)
test.expect(i16s.size(), 2)
test.expectBool(i16s.has(-32768i16), true)
test.expectBool(i16s.delete(-32768i16), true)
test.expect(i16s.size(), 1)
let i16_total: i16 = 0i16
i16s.forEach(fn(v: i16) { i16_total = i16_total + v })
test.expect(i16_total as i64, 32767)
i16s.clear()
test.expect(i16s.size(), 0)

let i16a = new Set<i16>()
i16a.add(100i16)
i16a.add(200i16)
let i16b = new Set<i16>()
i16b.add(200i16)
i16b.add(300i16)
test.expect(i16a.intersection(i16b).size(), 1)
test.expectBool(i16a.isDisjointFrom(i16b), false)

// ---- i32 ----
let i32s = new Set<i32>()
i32s.add(-2147483648i32)
i32s.add(2147483647i32)
test.expect(i32s.size(), 2)
let i32_vs = i32s.values()
let i32_sum: i32 = i32_vs[0] + i32_vs[1]
test.expect(i32_sum as i64, -1)
test.expectBool(i32s.delete(2147483647i32), true)
test.expect(i32s.size(), 1)
i32s.clear()
test.expect(i32s.size(), 0)

let i32a = new Set<i32>()
i32a.add(1i32)
i32a.add(2i32)
let i32b = new Set<i32>()
i32b.add(2i32)
i32b.add(3i32)
test.expect(i32a.union(i32b).size(), 3)
test.expect(i32a.difference(i32b).size(), 1)

// ---- u8 ----
let u8s = new Set<u8>()
u8s.add(0u8)
u8s.add(255u8)
u8s.add(255u8)
test.expect(u8s.size(), 2)
test.expectBool(u8s.has(0u8), true)
test.expectBool(u8s.has(128u8), false)
// values() must preserve u8 — u8 arithmetic wraps at 256.
let u8_vs = u8s.values()
let u8_wrap: u8 = u8_vs[0] + 1u8 + u8_vs[1]   // 0 + 1 + 255 → 0
test.expect(u8_wrap as i64, 0)
test.expectBool(u8s.delete(0u8), true)
test.expect(u8s.size(), 1)
let u8_total: u8 = 0u8
u8s.forEach(fn(v: u8) { u8_total = u8_total + v })
test.expect(u8_total as i64, 255)
u8s.clear()
test.expect(u8s.size(), 0)

let u8a = new Set<u8>()
u8a.add(1u8)
u8a.add(2u8)
let u8b = new Set<u8>()
u8b.add(2u8)
u8b.add(3u8)
test.expect(u8a.intersection(u8b).size(), 1)
test.expectBool(u8a.isSubsetOf(u8a.union(u8b)), true)

// ---- u16 ----
let u16s = new Set<u16>()
u16s.add(0u16)
u16s.add(65535u16)
test.expect(u16s.size(), 2)
test.expectBool(u16s.has(65535u16), true)
test.expectBool(u16s.delete(0u16), true)
test.expect(u16s.size(), 1)
u16s.clear()
test.expect(u16s.size(), 0)

let u16a = new Set<u16>()
u16a.add(10u16)
u16a.add(20u16)
let u16b = new Set<u16>()
u16b.add(20u16)
test.expect(u16a.union(u16b).size(), 2)
test.expectBool(u16a.isSupersetOf(u16b), true)

// ---- u32 ----
let u32s = new Set<u32>()
u32s.add(0u32)
u32s.add(4294967295u32)
test.expect(u32s.size(), 2)
test.expectBool(u32s.has(4294967295u32), true)
test.expectBool(u32s.delete(0u32), true)
test.expect(u32s.size(), 1)
u32s.clear()
test.expect(u32s.size(), 0)

let u32a = new Set<u32>()
u32a.add(7u32)
let u32b = new Set<u32>()
u32b.add(8u32)
test.expectBool(u32a.isDisjointFrom(u32b), true)
test.expect(u32a.difference(u32b).size(), 1)

// ---- u64 ----
let u64s = new Set<u64>()
u64s.add(0u64)
u64s.add(18446744073709551615u64)
u64s.add(18446744073709551615u64)
test.expect(u64s.size(), 2)
test.expectBool(u64s.has(18446744073709551615u64), true)
test.expectBool(u64s.delete(18446744073709551615u64), true)
test.expect(u64s.size(), 1)
let u64_total: u64 = 0u64
u64s.forEach(fn(v: u64) { u64_total = u64_total + v })
test.expect(u64_total as i64, 0)
u64s.clear()
test.expect(u64s.size(), 0)

let u64a = new Set<u64>()
u64a.add(1u64)
u64a.add(2u64)
let u64b = new Set<u64>()
u64b.add(2u64)
u64b.add(3u64)
test.expect(u64a.intersection(u64b).size(), 1)
test.expect(u64a.union(u64b).size(), 3)

// ---- bool ----
let bs = new Set<bool>()
bs.add(true)
bs.add(false)
bs.add(true)                // dup ignored
test.expect(bs.size(), 2)
test.expectBool(bs.has(true), true)
test.expectBool(bs.has(false), true)
test.expectBool(bs.delete(false), true)
test.expectBool(bs.delete(false), false)
test.expect(bs.size(), 1)
let b_seen = 0
bs.forEach(fn(v: bool) { if v { b_seen = b_seen + 1 } })
test.expect(b_seen, 1)
bs.clear()
test.expect(bs.size(), 0)

let ba = new Set<bool>()
ba.add(true)
let bb = new Set<bool>()
bb.add(false)
test.expect(ba.union(bb).size(), 2)
test.expect(ba.intersection(bb).size(), 0)
test.expectBool(ba.isDisjointFrom(bb), true)

// ---- f32 ----
let f32s = new Set<f32>()
f32s.add(1.5f32)
f32s.add(-0.25f32)
f32s.add(1.5f32)            // dup ignored (bitwise-equal)
test.expect(f32s.size(), 2)
test.expectBool(f32s.has(1.5f32), true)
test.expectBool(f32s.has(0.0f32), false)
test.expectBool(f32s.delete(-0.25f32), true)
test.expect(f32s.size(), 1)
let f32_total: f32 = 0.0f32
f32s.forEach(fn(v: f32) { f32_total = f32_total + v })
test.expectF64(f32_total as f64, 1.5)
f32s.clear()
test.expect(f32s.size(), 0)

let f32a = new Set<f32>()
f32a.add(1.0f32)
f32a.add(2.0f32)
let f32b = new Set<f32>()
f32b.add(2.0f32)
f32b.add(3.0f32)
test.expect(f32a.union(f32b).size(), 3)
test.expect(f32a.intersection(f32b).size(), 1)
test.expectBool(f32a.isSubsetOf(f32a.union(f32b)), true)

// ---- f64 ----
let f64s = new Set<f64>()
f64s.add(1.5)
f64s.add(-0.25)
f64s.add(1.5)
test.expect(f64s.size(), 2)
test.expectBool(f64s.has(-0.25), true)
test.expectBool(f64s.delete(1.5), true)
test.expect(f64s.size(), 1)
let f64_total: f64 = 0.0
f64s.forEach(fn(v: f64) { f64_total = f64_total + v })
test.expectF64(f64_total, -0.25)
f64s.clear()
test.expect(f64s.size(), 0)

let f64a = new Set<f64>()
f64a.add(1.0)
f64a.add(2.0)
let f64b = new Set<f64>()
f64b.add(2.0)
test.expect(f64a.difference(f64b).size(), 1)
test.expectBool(f64a.isSupersetOf(f64b), true)

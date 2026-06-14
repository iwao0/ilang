// `@bits(N)` on a `u64` field. Consecutive u64 bitfields pack into a
// single 8-byte storage unit, and the JIT must read/write that unit at
// its FULL 64-bit width. The codegen used to clamp the storage type to
// `I32` (its `elem_clif_type` helper returns `None` for u64, falling
// back to a 32-bit unit), so any field wider than 32 bits — or a
// `@bits(64)` holding the top bits — truncated to its low 32 bits:
//   `@bits(64) b` set to all-ones read back as `0xFFFFFFFF` not `-1`,
//   `@bits(33) lo` masked to 32 bits, the high half of a u64 pattern
//   silently dropped. The read-modify-write store also only touched the
//   low 4 bytes. Both paths now select the storage unit from the field's
//   declared width, so u64 units use `I64`.
//
// expect: ok
use std.test as test
@extern(C) {
    struct Full {
        @bits(64) b: u64
    }
    struct Pack {
        @bits(33) lo: u64
        @bits(31) hi: u64
    }
    struct Trunc {
        @bits(40) big: u64
        @bits(20) small: u64
    }
}

// Full 64-bit field round-trips every bit (all-ones reads as -1 when
// reinterpreted as i64; an arbitrary pattern keeps its high half).
let f = new Full()
f.b = 0xFFFFFFFFFFFFFFFF as u64
test.expectTrue((f.b as i64) == (-1 as i64))
f.b = 0x123456789ABCDEF0 as u64
test.expect(f.b as i64, 1311768467463790320)

// Two u64 bitfields share one 8-byte unit; writing one must not
// disturb the other (read-modify-write hits all 8 bytes).
let p = new Pack()
p.lo = 0x1FFFFFFFF as u64   // 33 bits all set
p.hi = 0x7FFFFFFF as u64    // 31 bits all set
test.expect(p.lo as i64, 8589934591)
test.expect(p.hi as i64, 2147483647)
p.hi = 1 as u64
test.expect(p.hi as i64, 1)
test.expect(p.lo as i64, 8589934591)   // lo survives
p.lo = 0 as u64
test.expect(p.lo as i64, 0)
test.expect(p.hi as i64, 1)            // hi survives

// Values wider than the declared width truncate to the field's bits.
let t = new Trunc()
t.big = 0x1FFFFFFFFFF as u64    // 41 bits → low 40 bits = 0xFFFFFFFFFF
test.expect(t.big as i64, 1099511627775)
t.small = 0xFFFFFFF as u64     // 28 bits → low 20 bits = 0xFFFFF
test.expect(t.small as i64, 1048575)
test.expect(t.big as i64, 1099511627775)   // big preserved

console.log("ok")

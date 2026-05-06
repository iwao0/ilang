// `@bits(N)` bitfields on `@extern(C) struct` fields. Consecutive
// bitfields of the same underlying integer width pack into a single
// shared storage unit (GCC-style packing); the JIT emits a
// shift+mask read and a read-modify-write store.
use test

@extern(C) {
    struct flags {
        @bits(1) active: u32
        @bits(3) mode: u32
        @bits(4) reserved: u32
        // Non-bitfield breaks the run; this `tag` lands in its own
        // 4-byte slot at offset 4.
        tag: u32
        // Another run starts here.
        @bits(2) phase: u8
        @bits(6) hop: u8
    }
}

let f = new flags()
// Defaults: every bitfield reads as 0.
test.expect(f.active as i64, 0)
test.expect(f.mode as i64, 0)
test.expect(f.reserved as i64, 0)
test.expect(f.tag as i64, 0)
test.expect(f.phase as i64, 0)
test.expect(f.hop as i64, 0)

// Single-bit field — toggles cleanly.
f.active = 1 as u32
test.expect(f.active as i64, 1)
f.active = 0 as u32
test.expect(f.active as i64, 0)

// Multi-bit field — fills its width.
f.mode = 5 as u32   // 0b101
test.expect(f.mode as i64, 5)
f.reserved = 15 as u32  // 0b1111
test.expect(f.reserved as i64, 15)

// Independence: setting `mode` doesn't disturb its neighbours.
f.active = 1 as u32
f.mode = 3 as u32
f.reserved = 12 as u32
test.expect(f.active as i64, 1)
test.expect(f.mode as i64, 3)
test.expect(f.reserved as i64, 12)
test.expect(f.tag as i64, 0)

// Overflow: writing a value wider than the declared bits truncates.
// 0xFF into a 4-bit slot → 0xF.
f.reserved = 255 as u32
test.expect(f.reserved as i64, 15)

// Tag (full-width field) is still a regular u32.
f.tag = 0xCAFE as u32
test.expect(f.tag as i64, 51966)
test.expect(f.active as i64, 1)
test.expect(f.mode as i64, 3)

// Second bitfield run (u8 unit) is independent of the first.
f.phase = 2 as u8     // 0b10 in 2 bits
f.hop = 42 as u8      // 0b101010 in 6 bits — fits
test.expect(f.phase as i64, 2)
test.expect(f.hop as i64, 42)

// Confirm phase/hop don't affect the u32 run.
test.expect(f.tag as i64, 51966)
test.expect(f.reserved as i64, 15)

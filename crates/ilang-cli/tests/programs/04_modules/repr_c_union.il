// `union` inside `@extern(C)` lays every field at offset 0 —
// writing one overwrites the others. Useful for C union interop
// (`siginfo_t`, `union sigval`, etc.) and for type punning between
// integer and float bit patterns.
//
// JIT-only — the interpreter has no shared-storage representation
// for fields (it uses a HashMap<name, Value>).
// interp: skip
use test

@extern(C) {
    union word_view {
        as_u32: u32
        as_i32: i32
        as_f32: f32
        bytes: u8[4]
    }

    union mixed {
        small: u8
        big: u64
    }
}

let w = new word_view()

// Write through one field, read back through another. The bit
// pattern 0x42400000 is the IEEE-754 single-precision encoding of
// 48.0 — flipping between u32 and f32 reinterprets the bits.
w.as_u32 = 0x42400000 as u32
test.expectF64(w.as_f32 as f64, 48.0)

// Write through f32, read back through u32 to confirm the round-
// trip works in the opposite direction.
w.as_f32 = -1.0 as f32   // IEEE-754 -1.0 = 0xBF800000
test.expect(w.as_u32 as i64, 0xBF800000)

// i32 view (signed) — same bits, different interpretation.
w.as_i32 = -1 as i32
test.expect(w.as_u32 as i64, 0xFFFFFFFF)

// byte-level access via the embedded u8[4]. Little-endian on the
// targets we care about, so 0xDEADBEEF lays down as EF BE AD DE.
w.as_u32 = 0xDEADBEEF as u32
test.expect(w.bytes[0] as i64, 0xEF)
test.expect(w.bytes[1] as i64, 0xBE)
test.expect(w.bytes[2] as i64, 0xAD)
test.expect(w.bytes[3] as i64, 0xDE)

// Writing one byte at a time leaves the others alone, so the u32
// view sees the byte-level edit.
w.bytes[0] = 0x01 as u8
w.bytes[1] = 0x00 as u8
w.bytes[2] = 0x00 as u8
w.bytes[3] = 0x00 as u8
test.expect(w.as_u32 as i64, 0x00000001)

// Mixed-width union — verifies the size = max(field sizes) rule.
let m = new mixed()
m.big = 0xCAFEBABE_DEADBEEF as u64
// `small` reads the low byte of the u64 (LE: 0xEF).
test.expect(m.small as i64, 0xEF)
m.small = 0x99 as u8
// Writing `small` patches just the low byte; the upper 7 bytes
// remain the original 0xCAFEBABE_DEADBE prefix.
test.expect(m.big as i64, 0xCAFEBABE_DEADBE99)

// `@packed` drops natural alignment so every field sits at offset =
// sum of prior sizes (no padding) and the struct's overall alignment
// is 1. Mirrors C's `__attribute__((packed))`.
//
// Reads / writes still produce the same logical values; the
// difference is the in-memory layout, observable through:
//   - struct size (fewer bytes than the aligned form)
//   - byte offsets of each field (verified via `pkt_byte_at` /
//     `alg_byte_at` host externs that read raw bytes)
//
// jit: skip
use test

@extern(C) {
    @packed struct packed_header {
        magic: u8
        length: u32
        flags: u8
        code: u16
    }

    struct aligned_header {
        magic: u8
        length: u32
        flags: u8
        code: u16
    }

    @packed struct odd_pack {
        a: u8
        b: u64
        c: u8
    }

    fn pkt_byte_at(p: *packed_header, off: i64): i32
    fn alg_byte_at(p: *aligned_header, off: i64): i32

    // The byte-level layout checks dereference `&p` / `&a`, which
    // the type checker only allows inside an `@extern(C)` body.
    // Wrap the whole test driver here so the `&local` chain stays
    // FFI-scoped.
    fn run_packed_tests() {
        // Read / write through every field works identically —
        // packing only changes byte placement, not values.
        let p = new packed_header()
        p.magic = 0xAB as u8
        p.length = 0x12345678 as u32
        p.flags = 0x55 as u8
        p.code = 0xBEEF as u16
        test.expect(p.magic as i64, 0xAB)
        test.expect(p.length as i64, 0x12345678)
        test.expect(p.flags as i64, 0x55)
        test.expect(p.code as i64, 0xBEEF)

        let a = new aligned_header()
        a.magic = 0xAB as u8
        a.length = 0x12345678 as u32
        a.flags = 0x55 as u8
        a.code = 0xBEEF as u16
        test.expect(a.magic as i64, 0xAB)
        test.expect(a.length as i64, 0x12345678)
        test.expect(a.flags as i64, 0x55)
        test.expect(a.code as i64, 0xBEEF)

        // Multiple packed fields with different widths in a
        // different order — verifies offsets follow declared
        // order with no padding.
        let o = new odd_pack()
        o.a = 1 as u8
        o.b = 0x0807060504030201 as u64
        o.c = 99 as u8
        test.expect(o.a as i64, 1)
        test.expect(o.b as i64, 0x0807060504030201)
        test.expect(o.c as i64, 99)

        // Byte-level layout check via `byteAt(ptr, offset)`.
        // Confirms packed puts `length` at offset 1 (right after
        // the u8 magic) instead of the aligned-form offset 4.
        p.magic = 0x11 as u8
        p.length = 0xAABBCCDD as u32
        p.flags = 0x99 as u8
        p.code = 0x7788 as u16

        // Packed: length lives at offset 1 (LE: DD CC BB AA), no padding.
        test.expect(pkt_byte_at(&p, 0), 0x11)
        test.expect(pkt_byte_at(&p, 1), 0xDD)
        test.expect(pkt_byte_at(&p, 2), 0xCC)
        test.expect(pkt_byte_at(&p, 3), 0xBB)
        test.expect(pkt_byte_at(&p, 4), 0xAA)
        test.expect(pkt_byte_at(&p, 5), 0x99)
        test.expect(pkt_byte_at(&p, 6), 0x88)
        test.expect(pkt_byte_at(&p, 7), 0x77)

        a.magic = 0x11 as u8
        a.length = 0xAABBCCDD as u32
        a.flags = 0x99 as u8
        a.code = 0x7788 as u16

        // Aligned: length is at offset 4 (3 bytes of padding before it).
        test.expect(alg_byte_at(&a, 0), 0x11)
        test.expect(alg_byte_at(&a, 4), 0xDD)
        test.expect(alg_byte_at(&a, 5), 0xCC)
        test.expect(alg_byte_at(&a, 6), 0xBB)
        test.expect(alg_byte_at(&a, 7), 0xAA)
        test.expect(alg_byte_at(&a, 8), 0x99)
    }
}

run_packed_tests()

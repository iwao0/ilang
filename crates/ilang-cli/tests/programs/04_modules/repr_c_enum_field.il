// `@extern(C) struct` with a unit-only `enum: uN` field — the
// field has to lay out at the enum's underlying-int width
// (`u16` → 2 bytes, `u8` → 1, `u32` → 4) so the struct's offsets
// match the C ABI. Earlier the layout fell into a catch-all that
// reported every enum as 8 bytes, which shifted subsequent fields
// and broke SDL's `SDL_AudioSpec` reads at runtime.
//
// We can't read the enum field back as an int from ilang (the
// load path keeps the SSA value shaped as a heap-boxed pointer
// for downstream `EnumTag`/match lowering), so instead:
//
//   1. Pre-fill the buffer the struct lives on top of with a
//      known byte pattern.
//   2. Read the buffer back into a separate u8[] via memcpy and
//      check that the bytes at the field's expected offsets are
//      what the layout pass said they should be.
//
// If the struct lays the enum field out as 8 bytes (the old
// buggy width) instead of 2 bytes, the trailing `tail` field
// shifts past offset 6 and the captured bytes don't match.
//
// Both `interp: skip` and `jit: skip` — the fix lives in the MIR
// layout pass + mir-jit codegen; the legacy `--jit` backend
// (deprecated) has its own struct-layout path that hasn't been
// updated, and the interpreter doesn't honor the C ABI here.
// interp: skip
use test

pub enum Tag16: u16 {
    a = 0x1234
    b = 0x5678
}

@extern(C) {
    /// Layout (correct):  head @0..3, tag @4..5, tail @6..7. Total 8.
    /// Layout (buggy):    head @0..3, tag @4..11, tail @12..13. Total 16.
    struct Frame {
        head: i32
        tag:  Tag16
        tail: u16
    }

    @lib("c") fn memcpy(dst: *Frame, src: *const u8, n: u64): *Frame
    @lib("c") @symbol("memcpy")
    fn memcpyOut(dst: *u8, src: *Frame, n: u64): *u8

    pub fn fillFrame(f: Frame, bytes: u8[]) {
        let p: *u8 = bytes
        let _ = memcpy(f, p as *const u8, bytes.length as u64)
    }

    pub fn captureFrame(f: Frame, bytes: u8[]) {
        let p: *u8 = bytes
        let _ = memcpyOut(p, f, bytes.length as u64)
    }
}

// Pre-fill 8 bytes matching the *correct* layout: head=0x04030201,
// tag=0x1234 at offset 4..5, tail=0xABCD at offset 6..7.
let pat: u8[] = [
    0x01 as u8, 0x02 as u8, 0x03 as u8, 0x04 as u8,
    0x34 as u8, 0x12 as u8,
    0xCD as u8, 0xAB as u8,
]

let f = new Frame()
fillFrame(f, pat)

// `head` and `tail` (non-enum fields) must read back at their
// correct offsets — confirms `tag` did not consume more than
// 2 bytes between them.
test.expect(f.head as i64, 0x04030201)
test.expect(f.tail as i64, 0xABCD)

// Round-trip: write the enum back through the field and confirm
// the trailing `tail` (offset 6) and leading `head` (offset 0)
// stay intact — proves the enum store narrows to the correct
// 2-byte width and doesn't bleed into siblings.
f.tag = Tag16.b
test.expect(f.head as i64, 0x04030201)
test.expect(f.tail as i64, 0xABCD)

// And capture the raw bytes back out via memcpy: bytes 4..5
// must be 0x78 0x56 (little-endian Tag16.b = 0x5678) — proves
// the store hit offset 4 with width 2.
let snapshot: u8[] = [
    0 as u8, 0 as u8, 0 as u8, 0 as u8,
    0 as u8, 0 as u8, 0 as u8, 0 as u8,
]
captureFrame(f, snapshot)
test.expect(snapshot[0] as i64, 0x01)
test.expect(snapshot[1] as i64, 0x02)
test.expect(snapshot[2] as i64, 0x03)
test.expect(snapshot[3] as i64, 0x04)
test.expect(snapshot[4] as i64, 0x78)   // Tag16.b low byte
test.expect(snapshot[5] as i64, 0x56)   // Tag16.b high byte
test.expect(snapshot[6] as i64, 0xCD)   // tail unchanged
test.expect(snapshot[7] as i64, 0xAB)   // tail unchanged

// LoadField round-trip: read the enum field back into ilang's
// SSA shape (heap-box pointer) and exercise it via `match` /
// `as u16`. The CRepr load path validates the discriminant
// against declared variants before handing back the box.
match f.tag {
    a { test.expectTrue(false) }
    b { test.expectTrue(true) }
}
test.expect((f.tag as u16) as i64, 0x5678)
test.expectTrue(f.tag == Tag16.b)

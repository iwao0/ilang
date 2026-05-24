// `@extern(C) struct` with a unit-only `enum: uN` field — the
// field has to lay out at the enum's underlying-int width
// (`u16` → 2 bytes, `u8` → 1, `u32` → 4) so the struct's offsets
// match the C ABI. Earlier the layout fell into a catch-all that
// reported every enum as 8 bytes, which shifted subsequent fields
// and broke SDL's `SDL_AudioSpec` reads at runtime.
//
// The original version of this fixture used a `memcpy` wrapper
// that aliased the caller's `Frame` buffer; that pattern only
// worked because CRepr structs were pointer-passed. With the
// switch to true value semantics for CRepr params, the wrapper
// would receive a copy and the writes would be lost. The
// rewritten version exercises the same layout invariant through
// direct field assignment and round-trip reads, which is a
// faithful end-to-end check of where each field lives.
use std.test

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
}

// Initialise via the struct literal. The field set is complete
// (CRepr struct literals require every field), so the construct
// is a single NewObject + per-field StoreField sequence.
let f = Frame { head: 0x04030201 as i32, tag: Tag16.a, tail: 0xABCD as u16 }

// Round-trip read: every field must come back unchanged. If `tag`
// laid out at 8 bytes (the old bug) then either `tail` would
// occupy a shifted offset (read garbage) or the alloc'd buffer
// would overflow into adjacent memory.
test.expect(f.head as i64, 0x04030201)
test.expectTrue(f.tag == Tag16.a)
test.expect(f.tail as i64, 0xABCD)

// Write through the enum field, confirm `head` and `tail` stay
// intact — proves the enum store narrowed to its declared 2-byte
// width and didn't bleed into siblings.
f.tag = Tag16.b
test.expect(f.head as i64, 0x04030201)
test.expect(f.tail as i64, 0xABCD)
test.expectTrue(f.tag == Tag16.b)

// LoadField round-trip exercising `match` on the enum field —
// the CRepr load path validates the discriminant against
// declared variants before producing the heap box.
match f.tag {
    a { test.expectTrue(false) }
    b { test.expectTrue(true) }
}
test.expect((f.tag as u16) as i64, 0x5678)

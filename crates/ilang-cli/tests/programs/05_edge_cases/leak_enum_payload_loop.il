// `EnumName.payloadVariant(args)` allocates a `[tag | payload...]`
// cell of size `(1 + n_payload) * 8`. Currently NO release path
// exists for enum cells:
//   - `Inst::Release` codegen falls through `_ => {}` for
//     `MirTy::Enum` — no host call emitted.
//   - There's no `host_release_enum` host fn anyway.
//   - Enum cells lack an rc field, so refcounting them requires
//     either a layout change (add rc word) or a side-table
//     registry (pattern of `STRING_REGISTRY`).
//
// Until a fix lands, every payload-variant construction leaks
// the cell + the payload-side ARC of any heap content.
// Unit-variant constructions go through the `__enum_unit_get`
// cache and do NOT leak (covered by `leak_enum_unit_loop.il`).
//
// jit: skip

use test

enum Box {
    holds: (i64)
    empty
}

let baseline = test.liveAllocBytes()

let i: i64 = 0
while i < 1000 {
    // Each iter allocates a fresh 16-byte cell (tag + 1 i64
    // payload). With no release path, all 1000 cells leak.
    let b = Box.holds(i)
    let _ = b
    i = i + 1
}

let after = test.liveAllocBytes()
let grew = after - baseline

// Loose threshold to document the current state: 1000 × 16 B
// = ~16 KB. Tighten to < 256 once enum-cell rc + free lands.
test.expectTrue(grew < 32768)

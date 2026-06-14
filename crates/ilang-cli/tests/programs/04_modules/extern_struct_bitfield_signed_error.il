// `@bits(N)` is unsigned-only (docs: "unsigned integers only —
// u8/u16/u32/u64"). The repr-C CLASS path enforced this, but an
// `@extern(C) { struct ... }` skipped the bitfield validation entirely:
// a signed `@bits(4) x: i32` compiled and read back without
// sign-extension (`-1` stored as `0b1111` read as `15`). The struct
// body check now runs the same `validate_bitfield` the class path uses.

// expect-error: bitfields are only supported on unsigned integer types

@extern(C) {
    struct S {
        @bits(4) x: i32
    }
}

let s = new S()
console.log("unreachable")

// A `@bits(N)` width must fit the underlying type (`N ≤ 32` for `u32`).
// The parser only bounds `1..=64` globally; the per-type check lived in
// the repr-C class path and was skipped for `@extern(C) { struct ... }`,
// so `@bits(40) x: u32` (40 > 32) compiled. The struct body check now
// runs the shared `validate_bitfield`.

// expect-error: width must be in 1..=32 for u32

@extern(C) {
    struct S {
        @bits(40) x: u32
    }
}

let s = new S()
console.log("unreachable")

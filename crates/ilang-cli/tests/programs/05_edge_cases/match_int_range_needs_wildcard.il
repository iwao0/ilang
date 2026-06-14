// Exhaustiveness on an integer scrutinee is value-range-UNAWARE: even
// when open-ended range arms (`..0`, `1..`) plus the literal `0` between
// them mathematically cover every i64, the checker still demands a `_`
// wildcard. Only bool has a value space the checker enumerates; for
// numbers a literal/range match is always "open" and needs `_`. This
// pins the conservative rule so it can't silently regress into
// accepting a match that drops the wildcard.
//
// expect-error: non-exhaustive match on `i64`
fn sign(n: i64): string {
    match n {
        ..0 { "neg" }
        0   { "zero" }
        1.. { "pos" }
    }
}
console.log(sign(0))

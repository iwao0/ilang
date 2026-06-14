// Structural `==` (round 127) covers DYNAMIC arrays (`T[]`); fixed-length
// arrays (`T[n]`) have a different storage profile and are deliberately
// left out, so `==` on them stays a clean type error rather than
// silently comparing by pointer.
//
// expect-error: cannot apply binary op between i64[2] and i64[2]
let a: i64[2] = [1, 2]
let b: i64[2] = [1, 2]
let _ = a == b

// Tuple element access (`t[N]`) requires a compile-time non-negative
// integer literal — a runtime variable index would defeat the
// per-slot type tracking the checker relies on. Per syntax.md §3
// "Variables — Destructuring let / Tuple element access".
//
// expect-error: tuple index must be a non-negative integer literal

let t = (1, "a", true)
let i = 0
let x = t[i]

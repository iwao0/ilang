// Reading past the end of an array aborts at runtime. (Both interp
// and JIT report this; the harness only sees a non-zero exit + stderr.)
// expect-error: out of bounds
let xs: i64[] = [1, 2, 3]
let _bad = xs[99]

// `[[a, b], [c]]` infers an outer `i64[][]` even though the inner
// element types differ in static length (`i64[2]` vs `i64[1]`).
// Pre-fix the type checker pinned the outer element type to the
// first inner array's exact `i64[2]` and then rejected `[c]` as
// `i64[1]` — making the obvious "list of variable-length rows"
// shape illegal at type-check time. The fix erases the inner
// `fixed` length when unifying outer-array element types,
// matching how the JIT and interp actually represent these as
// dynamic arrays at runtime.
//
// expect: 2
// expect: 1
// expect: [[1, 2], [3]]

let xss: i64[][] = [[1, 2], [3]]
console.log(xss[0].length)
console.log(xss[1].length)
console.log(xss)

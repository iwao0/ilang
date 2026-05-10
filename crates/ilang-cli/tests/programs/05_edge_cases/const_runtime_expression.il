// `const` accepts any expression for its initializer (not just
// compile-time literals). When the RHS isn't constant-foldable
// the loader demotes it to a runtime one-shot init that runs
// before any user code.
// expect: 42
fn answer(): i64 { 42 }
const X: i64 = answer()
console.log(X)

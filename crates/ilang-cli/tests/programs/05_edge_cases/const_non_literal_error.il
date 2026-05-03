// const RHS can't reference runtime values (function calls etc.).
// expect-error: not allowed in `const`
fn answer(): i64 { 42 }
const X: i64 = answer()

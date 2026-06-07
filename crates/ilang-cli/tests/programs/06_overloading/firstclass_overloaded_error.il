// Referring to an overloaded name as a first-class value
// (`let f = name`) is rejected — overload resolution needs argument
// types to disambiguate. Per syntax.md §6 "First-class references".
// expect-error: bare references to overloaded fns are ambiguous
fn pick(n: i64): i64 { n }
fn pick(s: string): i64 { 0 }

let f = pick
let _ = f(1)

// No viable overload — typechecker rejects.
// expect-error: no matching overload
fn handle(s: string): i64 { s.length }
fn handle(n: i64): i64 { n * 2 }

let _ = handle(true)   // bool matches neither

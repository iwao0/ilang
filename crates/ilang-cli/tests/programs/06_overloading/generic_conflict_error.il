// Generic + non-generic same name is rejected — keeps overload
// resolution simple (generic resolution is its own special path).
// expect-error: generic
fn id<T>(x: T): T { x }
fn id(x: i64): i64 { x * 2 }
let _ = id(5)

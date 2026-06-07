// Two overloads with the same parameter types but different return
// types are rejected — overload resolution looks at the call-site
// argument types only, so a return-type-only duplicate would be
// unreachable. Per syntax.md §6 "Disallowed combinations".
//
// expect-error: duplicate overload

fn f(x: i64): string { "s" }
fn f(x: i64): i32 { 0 }

// `todo()` is a diverging built-in (Rust's `todo!()`): it type-checks
// as any expected type and panics if reached. A match arm or fn body
// left as `todo()` must compile cleanly, and code paths that never
// reach it must run normally.
//
// expect: 5
// expect: ok

enum Shape {
    circle: (i64)
    square: (i64, i64)
}

fn area(s: Shape): i64 {
    match s {
        circle(r) { r }
        square(_, _) { todo() }   // not implemented yet, but never reached here
    }
}

fn stub(): string {
    // A fn whose body is a stub still type-checks against its return type.
    if true { "ok" } else { todo() }
}

console.log(area(Shape.circle(5)).toString())
console.log(stub())

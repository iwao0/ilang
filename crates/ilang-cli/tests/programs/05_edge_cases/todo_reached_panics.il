// Reaching `todo()` at runtime aborts with a "not yet implemented"
// message and a non-zero exit, exactly like Rust's `todo!()`.
//
// expect-error: not yet implemented

enum Shape {
    circle: (i64)
    square: (i64, i64)
}

fn area(s: Shape): i64 {
    match s {
        circle(r) { r }
        square(_, _) { todo() }
    }
}

// This time the square arm IS reached, so the program must abort.
console.log(area(Shape.square(2, 3)).toString())

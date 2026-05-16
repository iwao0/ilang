// CRepr struct stack-promotion: a `struct` literal whose pointer
// never leaves the function frame is allocated as a Cranelift
// StackSlot instead of going through `__mir_alloc`. Escape
// analysis falls back to heap when the value is returned, passed
// to a call, or otherwise leaks. The user-visible behaviour
// (field reads, value semantics) is identical in either case —
// this fixture exists to make sure both paths produce the same
// answers, not to assert which one was taken.
use test

struct Point {
    x: i32
    y: i32
}

// Purely local: literal stays inside the function. Stack alloc.
fn local_sum(): i32 {
    let p = Point { x: 3, y: 4 }
    p.x + p.y
}

// Returned: literal must outlive the call. Heap alloc.
fn make_point(): Point {
    Point { x: 10, y: 20 }
}

// Passed to a call: Call args leak the candidate. Heap alloc.
fn add_pt(p: Point): i32 {
    p.x + p.y
}

fn pass_to_call(): i32 {
    let p = Point { x: 100, y: 200 }
    add_pt(p)
}

test.expect(local_sum() as i64, 7)
test.expect(make_point().x as i64, 10)
test.expect(make_point().y as i64, 20)
test.expect(pass_to_call() as i64, 300)

// `if` / `match` / array-literal joins of two classes that share no
// parent class but implement a common INTERFACE now join to that
// interface, mirroring how subclasses of a common parent already join.
//
// Before, the branch/literal join only consulted `common_ancestor`
// (the class hierarchy), so `if c { new Circle() } else { new Square() }`
// — both `Shape` — was rejected ("expected Circle, got Square") even
// with a `Shape` target. `common_object_join` now falls back to a
// unique common interface (an ambiguous multiple-interface join still
// errors — see branch_join_common_interface_ambiguous_error.il).
//
// Dispatch goes through the interface; ARC across the heap-returning
// interface method is exact.

use std.test as test

let deinits: i64[] = [0]

class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

interface Shape {
    area(): i64
    boxed(): Box
}

class Circle: Shape {
    r: i64
    init(x: i64) { this.r = x }
    area(): i64 { this.r * this.r }
    boxed(): Box { new Box(this.r) }
}

class Square: Shape {
    s: i64
    init(x: i64) { this.s = x }
    area(): i64 { this.s * this.s }
    boxed(): Box { new Box(this.s) }
}

// if/else join -> Shape (return type).
fn pickIf(k: i64): Shape {
    if k % 2 == 0 { new Circle(k) } else { new Square(k) }
}

// match join -> Shape.
fn pickMatch(k: i64): Shape {
    match k {
        0 { new Circle(2) }
        _ { new Square(3) }
    }
}

// 1 Box/round (boxed() on the chosen shape).
fn useIf(): i64 {
    pickIf(4).boxed().n            // Circle(4).boxed().n = 4
}
// 1 Box/round.
fn useMatch(): i64 {
    pickMatch(1).boxed().n         // Square(3).boxed().n = 3
}
// array literal [Circle, Square] -> Shape[]; 2 Box/round.
fn useArray(): i64 {
    let shapes: Shape[] = [new Circle(2), new Square(3)]
    let total = 0
    for s in shapes {
        total = total + s.boxed().n
    }
    total                          // 2 + 3 = 5
}

let acc: i64[] = [0]

test.expect(useIf(), 4)
test.expect(useMatch(), 3)
test.expect(useArray(), 5)

// Per round: useIf 1 + useMatch 1 + useArray 2 = 4 deinits.
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 {
    acc[0] = acc[0] + useIf() + useMatch() + useArray()
    i = i + 1
}
test.expect(deinits[0] - bd, 400)
test.expect(test.liveAllocBytes() - d0, 0)

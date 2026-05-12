// match arms returning multiple sibling subclasses join at the
// nearest common ancestor (92a2185).
use test

class Shape {
    init() {}
    name(): string { "shape" }
}
class Circle: Shape {
    init() { super() }
    override name(): string { "circle" }
}
class Square: Shape {
    init() { super() }
    override name(): string { "square" }
}

fn pick(n: i64): Shape {
    match n {
        0 { new Circle() }
        1 { new Square() }
        _ { new Shape() }
    }
}

test.expectStr(pick(0).name(), "circle")
test.expectStr(pick(1).name(), "square")
test.expectStr(pick(99).name(), "shape")

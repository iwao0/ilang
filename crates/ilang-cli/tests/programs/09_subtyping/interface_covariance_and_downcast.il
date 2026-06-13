// Confirmation pin (第47弾): an interface implementor flows into every
// covariant store position, and `as?` downcasts back to the concrete
// class — the subclass machinery (wrap, covariance, downcast) all
// generalizes to interfaces. ARC exact across each.
//
//   * Optional wrap     let s: Shape? = some(new Circle())
//   * enum payload      Holder.shape(new Circle())   variant shape: (Shape)
//   * Map value         {"c": new Circle()}  : Map<string, Shape>
//   * tuple element     (new Circle(), 5)    : (Shape, i64)
//   * tuple slot wrap   arr[0] = (new Circle(), 7) : (Shape?, i64)[]
//   * generic arg       new Holder<Shape>(new Circle())
//   * as? downcast      sh as? Circle  -> Circle?  (interface -> concrete)

use std.test as test

let deinits: i64[] = [0]

interface Shape {
    area(): i64
}

class Circle: Shape {
    r: i64
    init(x: i64) { this.r = x }
    deinit() { deinits[0] = deinits[0] + 1 }
    area(): i64 { this.r * this.r }
}

class Square: Shape {
    s: i64
    init(x: i64) { this.s = x }
    deinit() { deinits[0] = deinits[0] + 1 }
    area(): i64 { this.s * this.s }
}

enum Holder {
    empty
    shape: (Shape)
}

class Generic<T> {
    item: T
    init(x: T) { this.item = x }
    get(): T { this.item }
}

// Optional wrap. 1 Circle/round.
fn opt(): i64 {
    let s: Shape? = some(new Circle(3))
    match s {
        some(sh) { sh.area() }
        none { -1 }
    }
}

// enum payload. 1 Circle/round.
fn enumPayload(): i64 {
    let h: Holder = Holder.shape(new Circle(4))
    match h {
        shape(sh) { sh.area() }
        empty { -1 }
    }
}

// Map value covariance. 2 shapes/round.
fn mapValue(): i64 {
    let m: Map<string, Shape> = {"c": new Circle(2), "s": new Square(3)}
    let a = match m.get("c") {
        some(sh) { sh.area() }
        none { -1 }
    }
    let b = match m.get("s") {
        some(sh) { sh.area() }
        none { -1 }
    }
    a + b
}

// tuple element + tuple slot wrap via index store. 2 Circles/round.
fn tupleWrap(): i64 {
    let plain: (Shape, i64) = (new Circle(3), 5)
    let arr: (Shape?, i64)[] = [(none, 0)]
    arr[0] = (new Circle(4), 7)
    let wrapped = match arr[0][0] {
        some(sh) { sh.area() + arr[0][1] }
        none { arr[0][1] }
    }
    plain[0].area() + plain[1] + wrapped
}

// generic arg + as? downcast. 1 Circle/round.
fn genericDowncast(): i64 {
    let g = new Generic<Shape>(new Circle(5))
    let sh = g.get()
    let c: Circle? = sh as? Circle
    match c {
        some(circle) { circle.r }     // downcast back to concrete
        none { sh.area() }
    }
}

let acc: i64[] = [0]

test.expect(opt(), 9)
test.expect(enumPayload(), 16)
test.expect(mapValue(), 13)
test.expect(tupleWrap(), 9 + 5 + 16 + 7)   // 37
test.expect(genericDowncast(), 5)

// Per round: opt 1 + enumPayload 1 + mapValue 2 + tupleWrap 2 +
// genericDowncast 1 = 7 deinits.
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 {
    acc[0] = acc[0] + opt() + enumPayload() + mapValue() + tupleWrap()
        + genericDowncast()
    i = i + 1
}
test.expect(deinits[0] - bd, 700)
test.expect(test.liveAllocBytes() - d0, 0)

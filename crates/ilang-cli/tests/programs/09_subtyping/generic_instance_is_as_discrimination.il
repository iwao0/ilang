// Clean-round pin (follows round 124's is/as? fix): `is` / `as?` on a
// GENERIC class instance recognises the interface it implements and
// discriminates by type argument. Each `Box<T>` instantiation is a
// distinct monomorphized class with its own class-id, so `Box<i64>` must
// NOT answer true for `as? Box<string>`. Round 116 pinned the success
// case (`s as? Box<i64>`); the wrong-type-arg failure and the
// instance->interface direction were untested.
//
// expect: ok
use std.test as test
let deinits: i64[] = [0]
class Cell {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

interface Sized { size(): i64 }
class Box<T>: Sized {
    v: T
    init(v: T) { this.v = v }
    size(): i64 { 1 }
}
class Empty: Sized {
    size(): i64 { 0 }
}

// instance -> interface, through an interface-typed binding
let s: Sized = new Box<i64>(7)
test.expectTrue(s is Sized)
test.expectTrue(s is Box<i64>)
test.expectTrue(!(s is Box<string>))      // distinct instantiation
test.expectTrue(!(s is Empty))

// as? discriminates by type argument
test.expect(match s as? Box<i64> {
    some(b) { b.size() }
    none { -1 }
}, 1)
test.expect(match s as? Box<string> {     // wrong type arg -> none
    some(_) { 1 }
    none { 0 }
}, 0)

// mixed interface array: count only the Box<i64> elements
let xs: Sized[] = [new Box<i64>(1), new Empty(), new Box<i64>(2)]
let i = 0
let boxes = 0
while i < 3 {
    let mb = xs[i] as? Box<i64>
    match mb {
        some(_) { boxes = boxes + 1 }
        none {}
    }
    i = i + 1
}
test.expect(boxes, 2)

// ARC: as? hit (boxes the value) and miss, under churn, leak-free
class HBox<T>: Sized {
    c: Cell
    init() { this.c = new Cell(9) }
    size(): i64 { 1 }
}
class HOther: Sized {
    size(): i64 { 0 }
}
fn castChurn(hit: bool): i64 {
    let v: Sized = if hit { new HBox<i64>() } else { new HOther() }
    match v as? HBox<i64> {
        some(g) { g.size() }
        none { -1 }
    }
}
let acc: i64[] = [0]
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let j = 0
while j < 100 { acc[0] = acc[0] + castChurn(j % 2 == 0); j = j + 1 }
test.expect(deinits[0] - bd, 50)           // 50 HBox each hold one Cell
test.expect(test.liveAllocBytes() - d0, 0)

console.log("ok")

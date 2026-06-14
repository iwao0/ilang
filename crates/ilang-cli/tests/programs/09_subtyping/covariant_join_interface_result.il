// Clean-round pin for the join covariance (第79-81弾): the earlier
// fixtures cover classes / arrays / maps / tuples, but not two cases that
// also rely on it —
//
//   * an `if`/`match` over two DIFFERENT interface implementors joins to
//     the interface (`Circle`/`Square` -> `Shape`), since they share no
//     class ancestor but both satisfy `Shape`;
//   * `Result.ok(new Dog())` ⊔ `Result.ok(new Cat())` joins the `ok`
//     payload covariantly to `Result<Animal, string>` (and the same in
//     the `err` position), composing the Any-hole merge with the
//     subclass join.
//
// No new behaviour — confirms these forms type-check, dispatch
// polymorphically, and stay ARC-exact (each payload drops once per round,
// flat churn delta).

// expect: shape 9 16
// expect: res dog cat
// expect: err dog
// expect: churn ok

use std.test as test

let deinits: i64[] = [0]

interface Shape {
    area(): i64
}
class Circle: Shape {
    pub r: i64
    init(x: i64) { this.r = x }
    deinit() { deinits[0] = deinits[0] + 1 }
    area(): i64 { this.r * this.r }
}
class Square: Shape {
    pub s: i64
    init(x: i64) { this.s = x }
    deinit() { deinits[0] = deinits[0] + 1 }
    area(): i64 { this.s * this.s }
}

class Animal {
    pub name: string
    init(n: string) { this.name = n }
    deinit() { deinits[0] = deinits[0] + 1 }
}
class Dog: Animal { init() { super("dog") } }
class Cat: Animal { init() { super("cat") } }

// two interface implementors join to the interface, dispatched virtually.
fn shapeOf(b: bool): Shape {
    if b { new Circle(3) } else { new Square(4) }
}
console.log("shape " + shapeOf(true).area().toString()
    + " " + shapeOf(false).area().toString())

// Result ok-payload covaries (Dog/Cat -> Animal) through the join.
fn okAnimal(b: bool): Result<Animal, string> {
    if b { Result.ok(new Dog()) } else { Result.ok(new Cat()) }
}
fn okName(r: Result<Animal, string>): string {
    match r {
        ok(a) { a.name }
        err(e) { e }
    }
}
console.log("res " + okName(okAnimal(true)) + " " + okName(okAnimal(false)))

// the err position covaries too.
fn errAnimal(b: bool): Result<i64, Animal> {
    if b { Result.err(new Dog()) } else { Result.err(new Cat()) }
}
let en = match errAnimal(true) {
    ok(_) { "?" }
    err(a) { a.name }
}
console.log("err " + en)

// ARC: every Circle/Square/Dog/Cat drops exactly once per round.
let acc: i64[] = [0]
let d0 = test.liveAllocBytes()
let bd = deinits[0]
let i = 0
while i < 100 {
    acc[0] = acc[0] + shapeOf(i % 2 == 0).area()
    acc[0] = acc[0] + okName(okAnimal(true)).length
    let _ = errAnimal(false)
    i = i + 1
}
test.expect(test.liveAllocBytes() - d0, 0)
test.expect(deinits[0] - bd, 300)   // shape + okAnimal + errAnimal = 3/round
console.log("churn ok")

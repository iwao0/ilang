// Two more transient-temporary release sites that the
// interpreter previously missed:
//
//   1. `calc().add(2, 3)` — fresh receiver under a method call.
//      The receiver leaves no surviving binding; the JIT releases
//      it after the dispatch returns.
//
//   2. `make_arr()[0]` — fresh array under an index expression.
//      Element 0 is extracted (cloned, rc bumped), the rest have
//      no other owner; releasing the array fires `deinit` on the
//      unselected elements while the extracted one stays alive
//      via the new binding's ref.
//
// expect: deinit Calc
// expect: 5
// expect: after method
// expect: deinit 20
// expect: after index
// expect: 10
// expect: deinit 10

class Calc {
    init() {}
    add(a: i64, b: i64): i64 { a + b }
    deinit() { console.log("deinit Calc") }
}
fn calc(): Calc { new Calc() }

let r1 = calc().add(2, 3)
console.log(r1)
console.log("after method")

class T {
    n: i64
    init(n: i64) { this.n = n }
    deinit() { console.log("deinit " + this.n.toString()) }
}
fn make_arr(): T[] { [new T(10), new T(20)] }

let first = make_arr()[0]
console.log("after index")
console.log(first.n)

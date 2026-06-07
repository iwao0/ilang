// A closure built inside a method body captures `this` and calls
// an overloaded method on it. Overload resolution happens at the
// call site (closure-construction time) using the parameter type
// the closure receives, not at definition time. Per syntax.md §6
// (closure + `this` capture) and §7 (method overloading).
use std.test as test

class Calc {
    scale: i64
    init(s: i64) { this.scale = s }
    op(x: i64): i64 { x * this.scale }
    op(x: f64): i64 { (x * (this.scale as f64)) as i64 }

    // Closure captures `this` so the i64 overload of `op` resolves
    // through the captured receiver — different `Calc` instances
    // produce different closures.
    makeI64Op(): fn(i64): i64 {
        fn(y: i64): i64 { this.op(y) }
    }
    makeF64Op(): fn(f64): i64 {
        fn(y: f64): i64 { this.op(y) }
    }
}

let c3 = new Calc(3)
let f_i = c3.makeI64Op()
let f_f = c3.makeF64Op()
test.expect(f_i(5), 15)
test.expect(f_i(10), 30)
test.expect(f_f(2.5), 7)

// A different scale → different captured `this` → different
// closure behaviour, but each closure still walks the same
// overloaded `op` table.
let c10 = new Calc(10)
test.expect(c10.makeI64Op()(5), 50)
test.expect(c10.makeF64Op()(2.5), 25)

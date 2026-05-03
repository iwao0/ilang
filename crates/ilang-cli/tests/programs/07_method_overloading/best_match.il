// Best-match scoring picks the exact overload over the widening one.
use test

class M {
    init() {}
    f(x: i64): string { "i64" }
    f(x: f64): string { "f64" }
}

let m = new M()
test.expectStr(m.f(1), "i64")        // exact i64
test.expectStr(m.f(1.5), "f64")      // exact f64
let n: i32 = 7
test.expectStr(m.f(n), "i64")        // i32 widens to i64 (cost 1) over f64 (cost 2)

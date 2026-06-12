// Regression guard for the on-demand `this` capture (b12fe580): a
// NESTED closure that reads a field by bare name must still capture
// `this`. The first cut of the fix only resolved `this` via
// `lookup_var` (a local), so an inner closure — whose `this` is itself
// a capture forwarded from the enclosing closure, not a local — paniced
// with "class method closure must capture `this` for implicit field
// ref". Routing the on-demand `this` through the normal free-var
// resolution (which also forwards from `captures_in_scope`) fixes both
// the direct and the nested case.
//
// Each chain escapes (returned, never stored back into the receiver) so
// there's no `this` cycle; values must be right and ARC exact.

use std.test as test

let deinits: i64[] = [0]

class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

class C {
    v: Box
    init(x: i64) { this.v = new Box(x) }
    // Two-level nesting; the innermost reads field `v` by bare name.
    twoDeep(): fn(): fn(): i64 {
        fn(): fn(): i64 {
            fn(): i64 { v.n }
        }
    }
    // One level; explicit `this.` from inside the closure.
    oneDeep(): fn(): i64 {
        fn(): i64 { this.v.n }
    }
}

// Innermost closure reached through the full chain. 1 Box/round.
fn nested(): i64 {
    let c = new C(5)
    let f1 = c.twoDeep()
    let f2 = f1()
    f2()                                  // 5
}
// Single closure, explicit this. 1 Box/round.
fn direct(): i64 {
    let c = new C(6)
    let g = c.oneDeep()
    g()                                   // 6
}

let acc: i64[] = [0]

test.expect(nested(), 5)
test.expect(direct(), 6)

// Per round: nested 1 + direct 1 = 2 deinits.
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 {
    acc[0] = acc[0] + nested() + direct()
    i = i + 1
}
test.expect(deinits[0] - bd, 200)
test.expect(test.liveAllocBytes() - d0, 0)

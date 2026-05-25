// `obj.f = new_closure` replaces a closure-typed field. The old
// closure value held by the field has to be released BEFORE the
// new one is stored, otherwise the field-write path leaks the
// previous closure's cell every iteration. `strategy_pattern_via
// _fn_field.il` exercises the functional behaviour; this fixture
// pins down the memory reclaim.
//

use std.test as test
class Holder {
    f: fn(i64): i64
    init(g: fn(i64): i64) { this.f = g }
}

let baseline = test.liveAllocBytes()

let i: i64 = 0
while i < 200 {
    let h = new Holder(fn(x: i64): i64 { x * 2 })
    h.f = fn(x: i64): i64 { x + 100 }
    let _ = h
    i = i + 1
}

let after = test.liveAllocBytes()
// Per-iter cells: Holder object + 2 closure cells (the init's
// initial + the replacement). Holder cell leak baseline applies
// (~24 bytes/iter); closure cells must drop.
test.expectTrue(after - baseline < 65536)

// `new C(fresh_heap_arg)` for every heap arg type whose init's
// field assignment retains. `lower_new`'s post-call release set
// must match the field-assign retain set; this fixture pins
// each pair so a future re-narrowing in either direction trips
// the leak counter.
//
// mir-jit only — `liveAllocCount` isn't wired into AOT.
// aot: skip
use std.test as test

class Inner {
    n: i64
    init(n: i64) { this.n = n }
}

class HoldObj { v: Inner; init(v: Inner) { this.v = v } }
class HoldFn  { v: fn(i64): i64; init(v: fn(i64): i64) { this.v = v } }
class HoldArr { v: i64[]; init(v: i64[]) { this.v = v } }
class HoldTup { v: (i64, string); init(v: (i64, string)) { this.v = v } }
class HoldMap { v: Map<string, i64>; init(v: Map<string, i64>) { this.v = v } }
class HoldOpt { v: i64?; init(v: i64?) { this.v = v } }
class HoldStr { v: string; init(v: string) { this.v = v } }

fn one_round() {
    let _o = new HoldObj(new Inner(7))
    let _f = new HoldFn(fn(n: i64): i64 { n + 1 })
    let _a = new HoldArr([1, 2, 3])
    let _t = new HoldTup((9, "hi"))
    let _m = new HoldMap({"k": 1})
    let _opt = new HoldOpt(some(5))
    // Heap-typed string built by str_concat (fresh) — literal
    // strings go through a separate intern path and don't exercise
    // the ARC release.
    let _s = new HoldStr("hi " + 42.toString())
    let _ = 0
}

one_round()
let baseline = test.liveAllocCount()
for _ in 0..50 {
    one_round()
}
let diff = test.liveAllocCount() - baseline
test.expect(diff, 0)

// Run every higher-order method that takes a closure (array.map /
// .filter / .forEach / .find / .findIndex / .every / .some / .sort
// and map.forEach) many times in a loop, then assert the live-
// alloc count returns to the steady baseline. Locks in the
// refcount fix that consumes the caller's +1 on the closure arg.
//
// mir-jit only — the test intrinsic isn't wired into AOT.
// aot: skip
use std.test as test

fn dbl(n: i64): i64 { n * 2 }
fn isOdd(n: i64): bool { n % 2 == 1 }
fn cmp(a: i64, b: i64): i64 { a - b }

fn one_round() {
    let xs: i64[] = [1, 2, 3]
    let _ys = xs.map(dbl)
    let _zs = xs.filter(isOdd)
    xs.forEach(fn(_n: i64) {})
    let _f = xs.find(isOdd)
    let _i = xs.findIndex(fn(n: i64): bool { n > 1 })
    let _e = xs.every(fn(n: i64): bool { n > 0 })
    let _s = xs.some(isOdd)
    let _so = xs.sort(cmp)
    let m: Map<string, i64> = {"a": 1, "b": 2}
    m.forEach(fn(_k: string, _v: i64) {})
    let _ = 0
}

// Warm-up so OnceLock-backed tables init before baseline.
one_round()

let baseline = test.liveAllocCount()
for _ in 0..50 {
    one_round()
}
let diff = test.liveAllocCount() - baseline
test.expect(diff, 0)

// Free-fn calls and method calls with fresh heap args follow the
// same caller-releases-fresh convention as `lower_new`. Without
// the symmetric post-call release in `lower_call` /
// `lower_object_method`, every heap-arg `foo(fresh)` and
// `obj.method(fresh)` leaks the transient's +1.
//
// mir-jit only — `liveAllocCount` isn't wired into AOT.
// aot: skip
use std.test as test

class Box {
    xs: i64[]
    init(xs: i64[]) { this.xs = xs }
    swap(new_xs: i64[]) { this.xs = new_xs }
}

fn take(xs: i64[]): Box { new Box(xs) }
fn use_map(_m: Map<string, i64>) {}
fn use_str(_s: string) {}

fn one_round() {
    // free fn with fresh array arg
    let _b = take([1, 2, 3])
    // free fn with fresh map arg
    use_map({"k": 1})
    // free fn with fresh string-concat arg
    use_str("hi " + 42.toString())
    // method call with fresh array arg
    let b = new Box([4, 5, 6])
    b.swap([7, 8, 9])
    let _ = 0
}

one_round()
let baseline = test.liveAllocCount()
for _ in 0..50 {
    one_round()
}
let diff = test.liveAllocCount() - baseline
test.expect(diff, 0)

// `(new Foo()).method()` and similar transient-receiver patterns
// don't leak (03bb407 + beee631 + 46ae19a). Bound-loop check —
// running this 1k times stays steady on RC.
use std.test as test
class Foo {
    arr: i64[]
    init() { this.arr = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] }
    first(): i64 { this.arr[0] }
}
fn make(): Foo { new Foo() }

let total: i64 = 0
for _ in 0..1000 {
    total = total + (new Foo()).first()
    total = total + make().first()
    total = total + make().arr.length
}
test.expect(total, 1000 * (1 + 1 + 10))

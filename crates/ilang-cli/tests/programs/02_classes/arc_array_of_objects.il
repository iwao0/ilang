// Array of objects: when the array's last reference dies, every
// element is released — cascading deinits if the elements held heap
// resources.
use std.test

class Counter {
    n: i64
    init() { this.n = 0 }
    bump() { this.n = this.n + 1 }
    get(): i64 { this.n }
}

class Tracked {
    c: Counter
    init(c: Counter) { this.c = c }
    deinit() { this.c.bump() }
}

let total = new Counter()

{
    let xs: Tracked[] = [
        new Tracked(total),
        new Tracked(total),
        new Tracked(total),
    ]
    test.expect(xs.length, 3)
    test.expect(total.get(), 0)
}
// Array dies — all 3 elements released.
test.expect(total.get(), 3)

// pop() transfers ownership: array no longer drops the popped element.
{
    let xs: Tracked[] = [new Tracked(total), new Tracked(total)]
    let popped = xs.pop()
    test.expectBool(popped.isSome, true)
    // popped holds 1 elem; xs still holds 1 elem.
    // xs dies first (popped goes after), so 1 deinit happens here.
}
// Then `popped` goes out of scope — final element released.
test.expect(total.get(), 5)        // 3 from first block + 2 from this one

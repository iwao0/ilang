// `let y = x` creates a new binding that retains `x`'s allocation.
// The object only dies when ALL strong refs are gone.
use test

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

// Two bindings to the same allocation. deinit must NOT fire when the
// inner alias goes out of scope.
{
    let outer = new Tracked(total)
    {
        let inner = outer       // alias — both retain
        // inner falls out of scope here; deinit must NOT fire yet
    }
    test.expect(total.get(), 0)
    // outer still alive
    test.expect(total.get(), 0)
}
// Now outer is gone too — deinit fires once total.
test.expect(total.get(), 1)

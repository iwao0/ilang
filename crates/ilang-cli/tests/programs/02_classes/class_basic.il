// Class fundamentals: fields, init, methods, implicit and explicit `this`.
use test

class Counter {
    count: i64
    init(start: i64) { this.count = start }
    bump(): i64 {
        // Implicit `this` for fields and methods.
        count += 1
        count
    }
    add(k: i64): i64 {
        this.count = this.count + k     // explicit also OK
        this.count
    }
    peek(): i64 { this.count }
}

let c = new Counter(10)
test.expect(c.peek(), 10)
test.expect(c.bump(), 11)            // returns the new value
test.expect(c.bump(), 12)
test.expect(c.add(8), 20)
test.expect(c.count, 20)             // direct field read

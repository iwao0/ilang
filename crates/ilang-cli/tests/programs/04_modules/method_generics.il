// Generic methods on classes — both instance and static. Type
// arguments are inferred from the call-site argument types; the
// monomorphizer specializes each (class, method, args) tuple into a
// dedicated concrete method.
use std.test as test

pub class Bag {
    pub instCount<T>(xs: T[]): i64 {
        xs.length
    }
    pub static of<T>(xs: T[]): i64 {
        xs.length
    }
}

let b = new Bag()
test.expect(b.instCount([1, 2, 3]), 3)
test.expect(b.instCount(["a", "b"]), 2)
test.expect(Bag.of([1.0, 2.0, 3.0, 4.0]), 4)

// Multiple type params on a static generic.
pub class Pair {
    pub static make<A, B>(a: A, b: B): i64 {
        // dummy — just prove it parses and instantiates twice
        1
    }
}
test.expect(Pair.make(7, "x"), 1)
test.expect(Pair.make(1.5, true), 1)

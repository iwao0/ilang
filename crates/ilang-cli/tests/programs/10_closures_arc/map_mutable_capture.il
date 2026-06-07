// A closure capturing a Map and mutating it across calls — the
// captured Map lives in the closure's heap-allocated env, ARC
// keeps it alive while the closure is alive, and writes persist
// between invocations. Per syntax.md §6 "Every capture type is
// supported (... map ...)".
use std.test as test

fn makeMemoDoubler(): fn(i64): i64 {
    let cache: Map<i64, i64> = new Map<i64, i64>()
    fn(x: i64): i64 {
        if cache.has(x) {
            cache[x]
        } else {
            let v = x * 2
            cache[x] = v
            v
        }
    }
}

let m = makeMemoDoubler()
test.expect(m(5), 10)
test.expect(m(7), 14)
test.expect(m(5), 10)        // hits the cache from the first call
test.expect(m(0), 0)
test.expect(m(7), 14)        // hits the cache again

// Independent closure instances see independent caches.
let n = makeMemoDoubler()
test.expect(n(3), 6)
test.expect(n(5), 10)

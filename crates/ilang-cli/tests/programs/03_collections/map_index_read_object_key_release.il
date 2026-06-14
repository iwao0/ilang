// Indexing a `Map` with a FRESH object key (`m[new K(1)]`) leaked the
// lookup key. `__map_get` only borrows the key (hash + equals) and
// never takes ownership, but the index-read lowering — unlike the set /
// has / delete paths — never released the transient +1 on a fresh heap
// key, so one key object leaked per read. (String keys built fresh,
// e.g. `m[a + b]`, leaked the same way.) The read path now releases a
// fresh heap key after the borrowed lookup.
use std.test as test

class Tally { n: i64; init() { this.n = 0 } }
let tk = new Tally()

class K {
    pub id: i64
    init(id: i64) { this.id = id }
    equals(o: K): bool { this.id == o.id }
    hashCode(): i64 { this.id }
    deinit() { tk.n = tk.n + 1 }
}

fn run() {
    let m = new Map<K, i64>()
    m[new K(1)] = 10               // stored key (#1)
    m[new K(2)] = 20               // stored key (#2)
    // Index reads with fresh object keys: each lookup key is transient
    // and must be released after the borrowed read.
    test.expect(m[new K(1)], 10)   // transient (#3)
    test.expect(m[new K(2)], 20)   // transient (#4)
    test.expect(m[new K(1)], 10)   // transient (#5)
    m.clear()                      // releases the two stored keys
}
run()

// All five K instances (2 stored + 3 transient lookups) deinit once.
test.expect(tk.n, 5)

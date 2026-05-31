// `Set<MyClass>` must retain on `add` and release on `delete` /
// `clear` / set-drop. Mirror `leak_map_delete_loop.il` but with a
// class-typed element: insert + delete in a tight loop should
// fire the value's `deinit` exactly once per iteration (no leaked
// shares left in the set when the loop body exits), and the
// set's own drop at scope exit must clean up any remaining
// elements.
use std.test as test

class Counter {
    n: i64
    init() { this.n = 0 }
    bump() { this.n = this.n + 1 }
    get(): i64 { this.n }
}

class Tracked {
    pub key: i64
    pub parent: Counter
    init(key: i64, p: Counter) {
        this.key = key
        this.parent = p
    }
    deinit() { this.parent.bump() }
    pub equals(other: Tracked): bool { this.key == other.key }
    pub hashCode(): i64 { this.key }
}

// ---- add + delete churn ----
let deinits = new Counter()
let s = new Set<Tracked>()
let i: i64 = 0
while i < 1000 {
    let t = new Tracked(i, deinits)
    s.add(t)
    let _ = s.delete(t)
    i = i + 1
}
// Each iteration: t gets retained by `add`, released by `delete`,
// then its scope-exit release drops the local share. Net deinit
// per iteration = 1.
test.expect(deinits.get(), 1000)
test.expect(s.size(), 0)

// ---- set drops leftover elements ----
let drops = new Counter()
{
    let s2 = new Set<Tracked>()
    let j: i64 = 0
    while j < 100 {
        s2.add(new Tracked(j, drops))
        j = j + 1
    }
    test.expect(s2.size(), 100)
    // Block exit drops `s2`, which must release every retained
    // element. No `delete` calls in this arm — the leak check is
    // on the set's own teardown path.
}
test.expect(drops.get(), 100)

// ---- clear() drops every element ----
let cleared = new Counter()
let s3 = new Set<Tracked>()
let k: i64 = 0
while k < 50 {
    s3.add(new Tracked(k, cleared))
    k = k + 1
}
test.expect(s3.size(), 50)
s3.clear()
test.expect(s3.size(), 0)
test.expect(cleared.get(), 50)

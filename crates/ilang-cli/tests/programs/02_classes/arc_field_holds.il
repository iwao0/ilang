// An Object-typed field keeps its target alive until the holder dies.
// When the holder dies, the field is released, possibly cascading.
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

class Holder {
    inner: Tracked
    init(t: Tracked) { this.inner = t }
}

let total = new Counter()

// Holder owns inner via a field. Even when the local binding to the
// inner Tracked goes out of scope, Holder keeps it alive.
let holder: Holder = {
    let t = new Tracked(total)
    new Holder(t)
}
// `t` is gone here, but Holder.inner still references it: deinit not fired.
test.expect(total.get(), 0)

// Drop holder explicitly by re-binding to a fresh allocation. The
// previous holder's last ref dies → its deinit runs → field release
// drops Tracked's last ref → Tracked's deinit fires.
holder = new Holder(new Tracked(total))
// Old holder gone (released its inner Tracked) — total bumped once.
// Then a NEW Tracked was allocated and is held by the new Holder, so
// total stays at 1 for now.
test.expect(total.get(), 1)

// `weak.get()` upgrades to a strong binding that must keep the target
// alive for the binding's whole lifetime — even if every OTHER strong
// reference drops while the binding is held. `weak_basic.il` covers
// Some/None, and `leak_weak_get_loop.il` covers the per-upgrade reclaim,
// but neither pins the use-after-free guarantee: upgrade, then release
// the last other strong holder, and the object must stay valid (no UAF)
// and deinit exactly once — when the upgraded binding ends, not before.
use std.test as test

class Tally { n: i64; init() { this.n = 0 } }
let tally = new Tally()

class Obj {
    pub v: i64
    init(v: i64) { this.v = v }
    deinit() { tally.n = tally.n + 1 }
}

class Holder {
    pub obj: Obj?
    init(o: Obj) { this.obj = some(o) }
    clear() { this.obj = none }   // releases the strong ref it held
}

fn useAfterClear(w: Obj.weak, h: Holder): i64 {
    let observed = -1
    if let some(strong) = w.get() {
        // `h` holds the only other strong ref — drop it now. The
        // upgraded `strong` binding must keep the object alive.
        h.clear()
        observed = strong.v        // still valid: must read 99, not crash
        test.expect(tally.n, 0)    // not deinited yet — the binding owns it
    }
    // The binding has ended; the object is reclaimed exactly once.
    test.expect(tally.n, 1)
    return observed
}

let h = new Holder(new Obj(99))    // the Obj's sole strong ref lives in h
let w: Obj.weak = h.obj.unwrap()   // weak observer
test.expect(useAfterClear(w, h), 99)
test.expect(tally.n, 1)            // still exactly one deinit, no double-free

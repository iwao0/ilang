// `Pair` owns two `Tracker`s as fields. When `Pair` drops at scope
// exit, its own (empty) deinit fires, then the two field references
// are released — each releasing reference brings the Tracker rc to
// zero, firing each Tracker's deinit. Pre-fix release ordering or a
// missed field-release pass would lose one or both pushes.
use test

class Tracker {
    log: i64[]
    id: i64
    init(l: i64[], i: i64) { this.log = l; this.id = i }
    deinit() { this.log.push(this.id) }
}

class Pair {
    a: Tracker
    b: Tracker
    init(x: Tracker, y: Tracker) {
        this.a = x
        this.b = y
    }
}

let log: i64[] = []
{
    let _p = new Pair(new Tracker(log, 1), new Tracker(log, 2))
}
test.expect(log.length, 2)

// Both ids must have been pushed (order is unspecified).
let saw1 = false
let saw2 = false
for v in log {
    if v == 1 { saw1 = true }
    if v == 2 { saw2 = true }
}
test.expectBool(saw1, true)
test.expectBool(saw2, true)

// Sharing one Tracker across two Pairs: deinit fires only when the
// LAST holder releases it. While the top-level `shared` binding
// outlives the inner blocks, the shared Tracker stays alive across
// block-exits — only the per-Pair fresh Trackers fire.
let log2: i64[] = []
let shared = new Tracker(log2, 99)
{
    let _p1 = new Pair(shared, new Tracker(log2, 1))
    {
        let _p2 = new Pair(shared, new Tracker(log2, 2))
    }
    // _p2 dropped → its tracker (id=2) fires; `shared` still alive
    // (held by _p1 and the top-level binding).
    test.expect(log2.length, 1)
    test.expect(log2[0], 2)
}
// _p1 dropped → its tracker (id=1) fires. `shared` still alive
// (top-level binding holds it).
test.expect(log2.length, 2)
test.expect(log2[1], 1)

// Clean-round pin (no bug fixed — confirms the Map/Set insertion-order
// machinery added in `a4572ede` keeps ARC exact). The sibling
// `map_set_insertion_order.il` pins the ORDER semantics (which slot a
// key lands in); this fixture pins the ARC dimension that order touches
// — every `order: Vec<i64>` handle is non-owning, so a stale or
// double-counted handle would show up as a leak (deinit < N) or an
// over-release / use-after-free (deinit > N) under churn.
//
// Each section hand-computes the exact number of objects that must die
// per round, then churns 100× and asserts the running deinit total
// hits that number on the nose (a loose `>=` would hide over-release)
// while live bytes return to baseline.
//
// Covered, all paths that walk `order`:
//   - string-keyed Map: overwrite (slot kept) + delete + re-insert (tail)
//   - object-keyed Map (@derive Eq/Hash): delete drops the map's key share
//   - object-element Set union / intersection / difference, with two
//     distinct allocations that are Eq-equal (the dup must NOT be
//     retained twice, the survivor must NOT be dropped early)
//   - forEach that deletes the current key mid-iteration: the snapshot's
//     +1 must keep the value alive until the callback returns
//
// Success is silent (test.expect only) + exit 0.

use std.test as test

let deinits: i64[] = [0]

class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

@derive(Eq, Hash)
class Key {
    id: i64
    init(x: i64) { this.id = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

// --- string-keyed Map: overwrite + delete + re-insert ---
// 3 Boxes born per round; old Box(1) dies on overwrite, Box(2) on
// delete, Box(3) on scope exit → 3 deinits/round.
fn strMapRound(): i64 {
    let m = new Map<string, Box>()
    m["a"] = new Box(1)
    m["b"] = new Box(2)
    m["a"] = new Box(3)   // overwrite: keeps slot, old Box(1) dies
    m.delete("b")         // Box(2) dies
    m["a"].n              // borrowed read = 3
}

// --- object-keyed Map ---
// 2 Keys born per round; both die (k1 via map share + local, k2 via
// local after delete drops the map's k2 share) → 2 deinits/round.
fn objKeyRound(): i64 {
    let m = new Map<Key, i64>()
    let k1 = new Key(1)
    let k2 = new Key(2)
    m[k1] = 100
    m[k2] = 200
    m[k1] = 101           // overwrite value; stored key stays k1
    m.delete(k2)          // drops the map's k2 share; k2 local still alive
    m[k1]                 // = 101
}

// --- object-element Set ops with Eq-equal distinct allocations ---
// 4 Keys born per round (o1, o2a, o2b≈o2a, o3); each union/inter/diff
// output takes its own +1 on the survivors, all 4 die → 4 deinits/round.
fn setOpsRound(): i64 {
    let o1 = new Key(1)
    let o2a = new Key(2)
    let o2b = new Key(2)  // Eq-equal to o2a, distinct allocation
    let o3 = new Key(3)
    let a = new Set<Key>()
    let b = new Set<Key>()
    a.add(o1)
    a.add(o2a)
    b.add(o2b)            // dup vs o2a across the union — must not double-retain
    b.add(o3)
    let u = a.union(b)            // {o1, o2a, o3}
    let inter = a.intersection(b) // {o2a}
    let diff = a.difference(b)    // {o1}
    u.size() + inter.size() + diff.size()   // 3 + 1 + 1 = 5
}

// --- forEach deletes the current key mid-iteration ---
// 3 Boxes born per round; the snapshot retains each value before the
// callback runs, so the in-callback delete can't free it early → all 3
// die at scope exit, 3 deinits/round.
fn forEachDeleteRound(): i64 {
    let m = new Map<string, Box>()
    m["a"] = new Box(1)
    m["b"] = new Box(2)
    m["c"] = new Box(3)
    let sum: i64[] = [0]
    m.forEach(fn(k: string, v: Box) {
        m.delete(k)               // drop the map's share first
        sum[0] = sum[0] + v.n     // v must still be live (snapshot +1)
    })
    sum[0] + m.size()             // 6 + 0 = 6
}

let acc: i64[] = [0]

// Warm up each shape once so first-use string interning is out of the
// way before the byte-delta measurement.
test.expect(strMapRound(), 3)
test.expect(objKeyRound(), 101)
test.expect(setOpsRound(), 5)
test.expect(forEachDeleteRound(), 6)

// strMap: 100 × 3 = 300 deinits, bytes back to baseline.
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 { acc[0] = acc[0] + strMapRound(); i = i + 1 }
test.expect(deinits[0] - bd, 300)
test.expect(test.liveAllocBytes() - d0, 0)

// objKey: 100 × 2 = 200 deinits.
bd = deinits[0]
d0 = test.liveAllocBytes()
i = 0
while i < 100 { acc[0] = acc[0] + objKeyRound(); i = i + 1 }
test.expect(deinits[0] - bd, 200)
test.expect(test.liveAllocBytes() - d0, 0)

// setOps: 100 × 4 = 400 deinits.
bd = deinits[0]
d0 = test.liveAllocBytes()
i = 0
while i < 100 { acc[0] = acc[0] + setOpsRound(); i = i + 1 }
test.expect(deinits[0] - bd, 400)
test.expect(test.liveAllocBytes() - d0, 0)

// forEach-delete: 100 × 3 = 300 deinits.
bd = deinits[0]
d0 = test.liveAllocBytes()
i = 0
while i < 100 { acc[0] = acc[0] + forEachDeleteRound(); i = i + 1 }
test.expect(deinits[0] - bd, 300)
test.expect(test.liveAllocBytes() - d0, 0)

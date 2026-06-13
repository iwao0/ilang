// `Map.forEach` snapshots the entry order at the start of the callback
// loop, so mutating the map from inside the callback is safe: a value
// added mid-iteration is freed when the map drops, and a future key
// deleted mid-iteration is kept alive by the snapshot's +1 until its
// callback runs (no use-after-free). Nested forEach over the same map
// doesn't double-count or leak. This pins the ARC of all three.

use std.test as test

let deinits: i64[] = [0]

class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

// Add an entry during iteration. The added Box is created and freed.
// 3 Box/round.
fn addDuring(): i64 {
    let m: Map<string, Box> = new Map<string, Box>()
    m.set("a", new Box(1))
    m.set("b", new Box(2))
    let sum: i64[] = [0]
    let once: i64[] = [0]
    m.forEach(fn(k: string, v: Box) {
        sum[0] = sum[0] + v.n
        if once[0] == 0 {
            once[0] = 1
            m.set("c", new Box(3))
        }
    })
    sum[0]
}

// Delete a FUTURE key during iteration. The snapshot keeps it alive, so
// it is still visited (sum includes it) and no UAF occurs. 3 Box/round.
fn deleteFuture(): i64 {
    let m: Map<string, Box> = new Map<string, Box>()
    m.set("a", new Box(1))
    m.set("b", new Box(2))
    m.set("c", new Box(3))
    let sum: i64[] = [0]
    m.forEach(fn(k: string, v: Box) {
        sum[0] = sum[0] + v.n
        if k == "a" {
            m.delete("c")
        }
    })
    sum[0]
}

// Nested forEach over the same map. 2 Box/round.
fn nested(): i64 {
    let m: Map<string, Box> = new Map<string, Box>()
    m.set("a", new Box(1))
    m.set("b", new Box(2))
    let total: i64[] = [0]
    m.forEach(fn(k1: string, v1: Box) {
        m.forEach(fn(k2: string, v2: Box) {
            total[0] = total[0] + v1.n * v2.n
        })
    })
    total[0]
}

let acc: i64[] = [0]

test.expect(addDuring(), 3)        // a + b (c added but not visited this run)
test.expect(deleteFuture(), 6)     // a + b + c (snapshot kept c alive)
test.expect(nested(), 9)           // 1*1 + 1*2 + 2*1 + 2*2

// Per round: addDuring 3 + deleteFuture 3 + nested 2 = 8 deinits.
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 {
    acc[0] = acc[0] + addDuring() + deleteFuture() + nested()
    i = i + 1
}
test.expect(deinits[0] - bd, 800)
test.expect(test.liveAllocBytes() - d0, 0)

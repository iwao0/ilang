// Confirmation pin (第42弾): a tuple whose element needs a `T -> T?`
// wrap (`(Box?, Box)`) is built correctly at every remaining store site,
// completing the composite-element-wrap sweep started for bare field
// writes (第36弾) and index stores (第41弾). All sound:
//
//   * return position        fn(): (Box?, Box) { (box, b) }
//   * some(...)              let o: (Box?, Box)? = some((box, b))
//   * enum payload           E.pair((box, b))   variant pair: ((Box?, Box))
//   * argument position      consume((box, b))
//   * local reassign         t = (box, b)
//   * explicit field write   this.pair = (box, b)
//   * deeply nested          ((Box?, Box), Box?)
//
// Dispatch through each Optional confirms the real object was stored;
// ARC is exact across all sites.

use std.test as test

let deinits: i64[] = [0]

class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

enum E {
    empty
    pair: ((Box?, Box))
}

class H {
    p: (Box?, Box)
    init() { this.p = (none, new Box(0)) }
    set(a: Box, b: Box): i64 {
        this.p = (a, b)
        slot0(this.p)
    }
}

// Helper: read slot 0 (Optional) + slot 1 of a (Box?, Box).
fn slot0(t: (Box?, Box)): i64 {
    match t[0] {
        some(v) { v.n + t[1].n }
        none { t[1].n }
    }
}

// return + some + enum payload + arg + reassign + field + nested.
fn ret(): (Box?, Box) { (new Box(7), new Box(3)) }     // 2 Box

fn useReturn(): i64 { slot0(ret()) }                   // 10

fn useSome(): i64 {                                    // 2 Box
    let o: (Box?, Box)? = some((new Box(5), new Box(6)))
    match o {
        some(t) { slot0(t) }
        none { -1 }
    }
}

fn useEnum(): i64 {                                    // 2 Box
    let e: E = E.pair((new Box(1), new Box(2)))
    match e {
        pair(t) { slot0(t) }
        empty { -1 }
    }
}

fn useArg(): i64 { slot0((new Box(4), new Box(8))) }   // 2 Box, 12

fn useReassign(): i64 {                                // 3 Box (Box0 + Box7 + Box3)
    let t: (Box?, Box) = (none, new Box(0))
    t = (new Box(7), new Box(3))
    slot0(t)
}

fn useField(): i64 {                                   // 3 Box (Box0 + Box7 + Box3)
    let h = new H()
    h.set(new Box(7), new Box(3))
}

fn useNested(): i64 {                                  // 3 Box
    let t: ((Box?, Box), Box?) = ((new Box(1), new Box(2)), new Box(3))
    let inner = t[0]
    let a = match inner[0] {
        some(v) { v.n }
        none { 0 }
    }
    let c = match t[1] {
        some(v) { v.n }
        none { 0 }
    }
    a + inner[1].n + c
}

let acc: i64[] = [0]

test.expect(useReturn(), 10)
test.expect(useSome(), 11)
test.expect(useEnum(), 3)
test.expect(useArg(), 12)
test.expect(useReassign(), 10)
test.expect(useField(), 10)
test.expect(useNested(), 6)

// Per round: 2 + 2 + 2 + 2 + 3 + 3 + 3 = 17 deinits.
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 {
    acc[0] = acc[0] + useReturn() + useSome() + useEnum() + useArg()
        + useReassign() + useField() + useNested()
    i = i + 1
}
test.expect(deinits[0] - bd, 1700)
test.expect(test.liveAllocBytes() - d0, 0)

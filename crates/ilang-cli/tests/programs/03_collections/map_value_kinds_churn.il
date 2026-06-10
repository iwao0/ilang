// Overwrite churn across the heap value kinds a Map can hold — enum
// (payload), Optional, array, tuple, closure, nested Map — plus a
// class-with-Map-field method-tail read. Each loop overwrites the
// same key 200 times and reads the value back; the ARC balance must
// hold for every kind's release cascade.

use std.test as test

class Box {
    n: i64
    init(x: i64) { this.n = x }
}

enum Msg {
    silent
    pair: (string, i64)
}

class Cache {
    store: Map<string, Box>
    init() { this.store = new Map<string, Box>() }
    put(k: string, b: Box) { store[k] = b }
    grab(k: string): Box { store[k] }
}

let base = test.liveAllocBytes()

// enum payload values
let em = new Map<string, Msg>()
let i = 0
while i < 200 {
    em["k"] = Msg.pair("payload", i)
    let got = match em["k"] {
        silent { -1 }
        pair(_, n) { n }
    }
    test.expect(got, i)
    i = i + 1
}
em.clear()

// Optional values
let om = new Map<string, Box?>()
i = 0
while i < 200 {
    om["k"] = some(new Box(i))
    if let some(b) = om["k"] { test.expect(b.n, i) }
    i = i + 1
}
om.clear()

// array values
let am = new Map<string, Box[]>()
i = 0
while i < 200 {
    am["k"] = [new Box(i), new Box(i + 1)]
    test.expect(am["k"][0].n + am["k"][1].n, i + i + 1)
    i = i + 1
}
am.clear()

// tuple values
let tm = new Map<string, (i64, Box)>()
i = 0
while i < 200 {
    tm["k"] = (i, new Box(i * 2))
    let t = tm["k"]
    test.expect(t[0], i)
    test.expect(t[1].n, i * 2)
    i = i + 1
}
tm.clear()

// closure values
let fm = new Map<string, fn(i64): i64>()
i = 0
while i < 200 {
    let bias = i
    fm["f"] = fn(x: i64): i64 { x + bias }
    let f = fm["f"]
    test.expect(f(10), 10 + i)
    i = i + 1
}
fm.clear()

// nested map values: write through a chained index, then replace the
// inner map so its cascade releases the occupant
let mm = new Map<string, Map<string, Box>>()
mm["a"] = new Map<string, Box>()
i = 0
while i < 200 {
    mm["a"]["b"] = new Box(i)
    test.expect(mm["a"]["b"].n, i)
    i = i + 1
}
mm["a"] = new Map<string, Box>()
mm.clear()

// method-tail map read on a class field
let c = new Cache()
i = 0
while i < 200 {
    c.put("k", new Box(i))
    test.expect(c.grab("k").n, i)
    i = i + 1
}

let after = test.liveAllocBytes()
test.expectTrue(after - base < 2048)

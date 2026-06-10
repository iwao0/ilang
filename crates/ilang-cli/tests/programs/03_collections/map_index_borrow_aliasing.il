// `m[k]` is a borrowed read (same convention as `ArrayLoad`): the
// map's slot keeps its share, and consumers that store the value take
// their own retain. This pins the alias-safety side of that contract
// — a binding / container / closure holding a previously-read value
// must keep it alive across overwrite and delete of the source entry.

use std.test as test

class Box {
    n: i64
    init(x: i64) { this.n = x }
}

// binding aliases the entry, entry overwritten / deleted afterwards
let m = new Map<string, Box>()
m["a"] = new Box(1)
let x = m["a"]
m["a"] = new Box(2)
test.expect(x.n, 1)
test.expect(m["a"].n, 2)
m["b"] = new Box(3)
let y = m["b"]
test.expectBool(m.delete("b"), true)
test.expect(y.n, 3)

// fn tail returning m[k] — caller receives an owned reference
fn pick(mm: Map<string, Box>, flag: bool): Box {
    if flag { mm["a"] } else { mm["x"] }
}
m["x"] = new Box(4)
let p = pick(m, true)
m["a"] = new Box(9)
test.expect(p.n, 2)

// value moved into another container survives source overwrite
let arr: Box[] = [m["x"]]
m["x"] = new Box(5)
test.expect(arr[0].n, 4)

// value moved across maps
let m2 = new Map<string, Box>()
m2["copy"] = m["a"]
m["a"] = new Box(10)
test.expect(m2["copy"].n, 9)

// self-assignment keeps the value alive (retain-new before
// release-old inside host_map_set)
m["a"] = m["a"]
test.expect(m["a"].n, 10)

// closure captures the read value
let captured = m["a"]
let reader = fn(): i64 { captured.n }
m["a"] = new Box(11)
test.expect(reader(), 10)

// fresh receiver index hands ownership of the selected value
fn makeMap(): Map<string, Box> {
    let t = new Map<string, Box>()
    t["k"] = new Box(42)
    t
}
test.expect(makeMap()["k"].n, 42)

// the whole dance leaves no garbage behind
let base = test.liveAllocBytes()
let i = 0
while i < 200 {
    m["churn"] = new Box(i)
    let alias = m["churn"]
    m["churn"] = new Box(i + 1)
    test.expect(alias.n, i)
    test.expect(m["churn"].n, i + 1)
    i = i + 1
}
m.delete("churn")
let after = test.liveAllocBytes()
test.expectTrue(after - base < 1024)

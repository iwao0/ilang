// Set<T> — construct / add / has / delete / size / forEach plus the
// set-algebra methods, over both integer and string element kinds.
use std.test as test

// ---- int set: basics + dedup ----
let a = new Set<i64>()
test.expect(a.size(), 0)
a.add(1)
a.add(2)
a.add(2)
a.add(3)
test.expect(a.size(), 3)
test.expectBool(a.has(2), true)
test.expectBool(a.has(9), false)
test.expectBool(a.delete(2), true)
test.expectBool(a.delete(2), false)
test.expect(a.size(), 2)

// forEach visits every remaining element exactly once.
let total = 0
a.forEach(fn(v: i64) { total = total + v })
test.expect(total, 4)   // 1 + 3

// values() snapshot length matches size.
test.expect(a.values().length, 2)

// ---- int set algebra ----
let x = new Set<i64>()
x.add(1)
x.add(2)
x.add(3)
let y = new Set<i64>()
y.add(2)
y.add(3)
y.add(4)
test.expect(x.union(y).size(), 4)
test.expect(x.intersection(y).size(), 2)
test.expect(x.difference(y).size(), 1)
test.expectBool(x.isSubsetOf(x.union(y)), true)
test.expectBool(x.isSupersetOf(y), false)
test.expectBool(x.isDisjointFrom(y), false)
let z = new Set<i64>()
z.add(10)
test.expectBool(x.isDisjointFrom(z), true)

// ---- string set ----
let s = new Set<string>()
s.add("apple")
s.add("banana")
s.add("apple")
test.expect(s.size(), 2)
test.expectBool(s.has("apple"), true)
test.expectBool(s.has("cherry"), false)
test.expectBool(s.delete("banana"), true)
test.expect(s.size(), 1)

let s2 = new Set<string>()
s2.add("apple")
s2.add("cherry")
test.expect(s.union(s2).size(), 2)         // apple, cherry
test.expect(s.intersection(s2).size(), 1)  // apple
test.expect(s.difference(s2).size(), 0)    // apple removed by intersection set

// dynamically built (heap, non-literal) string keys round-trip.
let d = new Set<string>()
let i = 0
while i < 5 {
    d.add("k" + i.toString())
    i = i + 1
}
test.expect(d.size(), 5)
test.expectBool(d.has("k3"), true)
test.expectBool(d.has("k9"), false)
d.clear()
test.expect(d.size(), 0)

// ---- leak check: build & drop string sets in a loop ----
let baseline = test.liveAllocBytes()
let j = 0
while j < 1000 {
    let tmp = new Set<string>()
    tmp.add("x" + j.toString())
    tmp.add("y")
    let _ = tmp.has("y")
    let _ = tmp.delete("y")
    j = j + 1
}
test.expectTrue(test.liveAllocBytes() - baseline < 4096)

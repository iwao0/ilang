// `@derive(Eq, Hash)` supports enum-typed fields: the synthesised
// `equals` / `hashCode` route an enum field through the enum's own
// structural equality (round 105) and a matching structural hash, so a
// class with an enum field is a first-class Set / Map key. Enum values
// also gain standalone `.equals(other)` / `.hashCode()`.
//
// Before: an enum field made the synthesised `this.f.equals(...)` /
// `this.f.hashCode()` resolve the enum name as a class, surfacing a
// misleading "undefined class" error.
//
// expect: ok
use std.test as test
let deinits: i64[] = [0]
class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

enum Tag {
    none
    num: (i64)
    pair: (i64, string)
}

@derive(Eq, Hash)
class Key {
    label: string
    tag: Tag
    init(l: string, t: Tag) {
        this.label = l
        this.tag = t
    }
}

// --- standalone enum equality / hashing ---
test.expectTrue(Tag.num(5).equals(Tag.num(5)))
test.expectTrue(!Tag.num(5).equals(Tag.num(9)))
test.expectTrue(Tag.pair(1, "a").equals(Tag.pair(1, "a")))
test.expectTrue(!Tag.pair(1, "a").equals(Tag.pair(1, "b")))  // 2nd slot
test.expectTrue(Tag.none.equals(Tag.none))
// equal values hash equal (the equals/hashCode contract)
test.expect(Tag.num(5).hashCode(), Tag.num(5).hashCode())
test.expect(Tag.pair(1, "a").hashCode(), Tag.pair(1, "a").hashCode())

// --- Set dedup over a class with string + enum fields ---
let s = new Set<Key>()
s.add(new Key("a", Tag.num(5)))
s.add(new Key("a", Tag.num(5)))      // structural duplicate
s.add(new Key("a", Tag.num(9)))      // enum payload differs
s.add(new Key("a", Tag.pair(1, "x")))
s.add(new Key("a", Tag.pair(1, "x"))) // duplicate
s.add(new Key("b", Tag.num(5)))      // label differs
s.add(new Key("a", Tag.none))
test.expect(s.size(), 5)
test.expectTrue(s.has(new Key("a", Tag.num(5))))
test.expectTrue(s.has(new Key("a", Tag.pair(1, "x"))))
test.expectTrue(!s.has(new Key("a", Tag.num(7))))
test.expectTrue(!s.has(new Key("a", Tag.pair(1, "y"))))

// --- Map keyed by the class ---
let m = new Map<Key, i64>()
m[new Key("k", Tag.num(3))] = 42
m[new Key("k", Tag.num(3))] = 43     // same key overwrites
test.expect(m[new Key("k", Tag.num(3))], 43)
test.expect(m.size(), 1)

// --- ARC: lookups borrow their fresh key, no leak ---
let acc: i64[] = [0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 {
    if s.has(new Key("a", Tag.num(5))) { acc[0] = acc[0] + 1 }
    i = i + 1
}
test.expect(acc[0], 100)
test.expect(test.liveAllocBytes() - d0, 0)

console.log("ok")

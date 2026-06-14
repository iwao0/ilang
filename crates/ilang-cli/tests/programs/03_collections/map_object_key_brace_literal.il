// `Map<MyClass, V>` built from the `{}` map-literal shorthand must
// key on the class's value-equality protocol (`equals` / `hashCode`),
// exactly like `new Map<MyClass, V>()`. It did not: `new Map<K,V>()`
// lowers to `$map.newObject(equals, hashCode)`, but a map LITERAL
// lowered to `Inst::NewMap`, whose codegen always builds the
// primitive `__map_new` store and keys on the boxed pointer. So a
// `let m: Map<K, V> = {}` map looked up value-equal-but-distinct keys
// by ADDRESS:
//
//     let m: Map<Point, string> = {}
//     m[new Point(1, 2)] = "a"
//     m.has(new Point(1, 2))     // false — different pointer
//     m.get(new Point(1, 2))     // none, and a second insert never
//                                // overwrote — the map silently kept
//                                // two entries for one logical key
//
// while `new Set<Point>()` (Sets have no literal, so they always run
// the object constructor) deduped correctly — the asymmetry is what
// surfaced it. Map literals can't spell a class-instance key (keys
// are literal tokens), so only the empty `{}` shorthand reaches this,
// but that is the idiomatic empty-map form.
//
// Fix: the `{}`-against-`Map<K,V>` literal path now shares the same
// `build_object_keyed_map` helper as `new Map<K,V>()` when the key is
// a value-equality class, so both wire `equals` / `hashCode`.

// expect: {K { a: 1 }: 100}
// expect: map object key brace literal ok

use std.test as test

let deinits: i64[] = [0]

@derive(Eq, Hash)
class K {
    pub a: i64
    init(a: i64) { this.a = a }
    deinit() { deinits[0] = deinits[0] + 1 }
}

// --- value-equality keying via the `{}` shorthand ---
let m: Map<K, string> = {}
m[new K(1)] = "a"
test.expectTrue(m.has(new K(1)))            // value-equal lookup hits
test.expectTrue(!m.has(new K(9)))
m[new K(1)] = "b"                            // same logical key overwrites
test.expect(m.size(), 1)
let got = match m.get(new K(1)) {
    some(v) { v }
    none { "?" }
}
test.expectStr(got, "b")
test.expectTrue(m.delete(new K(1)))          // delete is value-keyed too
test.expect(m.size(), 0)

// console.log over a class-keyed `{}` map renders the key structurally.
let show: Map<K, i64> = {}
show[new K(1)] = 100
console.log(show)

// --- ARC: keys and values cascade-release when the map drops ---
// Each round makes a key K and a value K (both with `deinit`); the
// map owns both and frees them on scope exit.
fn round(): i64 {
    let mm: Map<K, K> = {}
    mm[new K(1)] = new K(2)
    mm.size()
}
let bd = deinits[0]
let r = round()
test.expect(r, 1)
test.expect(deinits[0] - bd, 2)              // key + value freed

let acc: i64[] = [0]
let d0 = test.liveAllocBytes()
let bd1 = deinits[0]
let i = 0
while i < 100 { acc[0] = acc[0] + round(); i = i + 1 }
test.expect(test.liveAllocBytes() - d0, 0)   // no leak
test.expect(deinits[0] - bd1, 200)           // 2 per round, exactly

console.log("map object key brace literal ok")

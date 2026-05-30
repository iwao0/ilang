// `Map<string, V>` must own and release its string keys symmetrically
// with its values. Pre-fix, host_map_set parked the caller's key
// pointer in `str_key_origs` without a retain and only `clear()`
// released it: dropping a string-keyed map (or overwriting / deleting
// keys) leaked every heap-built key string. Now the map retains the
// adopted key and releases it on delete / clear / map-drop, and codegen
// drops the caller's fresh key transient — mirroring the value path.
use std.test as test

// 1) Build and drop string-keyed maps with fresh (heap-built) keys,
//    never calling clear(). The map-drop must release the keys.
let baseline = test.liveStringCount()
let n = 0
while n < 200 {
    let m = new Map<string, i64>()
    let i = 0
    while i < 8 {
        m.set("key" + i.toString(), i)
        i = i + 1
    }
    n = n + 1
}
test.expectTrue(test.liveStringCount() - baseline < 64)

// 2) Overwriting a key must drop the redundant new key pointer; only
//    the distinct keys stay live, and clear() drains them.
let base2 = test.liveStringCount()
let m2 = new Map<string, i64>()
let j = 0
while j < 2000 {
    m2.set("k" + (j % 4).toString(), j)
    j = j + 1
}
m2.clear()
test.expectTrue(test.liveStringCount() - base2 < 64)

// 3) set + delete in a loop must not accumulate key strings.
let base3 = test.liveStringCount()
let m3 = new Map<string, i64>()
let d = 0
while d < 1000 {
    let key = "d" + d.toString()
    m3.set(key, d)
    let _ = m3.delete(key)
    d = d + 1
}
test.expect(m3.size(), 0)
test.expectTrue(test.liveStringCount() - base3 < 64)

// 4) Map literal with heap-built string values must not leak on drop.
let base4 = test.liveStringCount()
let e = 0
while e < 200 {
    let lit = { "a": "v" + e.toString() }
    e = e + 1
}
test.expectTrue(test.liveStringCount() - base4 < 64)

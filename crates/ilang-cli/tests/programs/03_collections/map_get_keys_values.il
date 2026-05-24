// Map.get returns V?, Map.keys / values return arrays.
use std.test

let m = new Map<string, i64>()
m.set("alice", 30)
m.set("bob", 25)
m.set("carol", 40)

// get returns Some(v) / None.
let g_alice = m.get("alice")
test.expectBool(g_alice.isSome, true)
test.expect(g_alice.unwrap(), 30)
let g_dave = m.get("dave")
test.expectBool(g_dave.isNone, true)

// keys() — order is unspecified; assert via membership + length.
let ks = m.keys()
test.expect(ks.length, 3)
test.expectBool(ks.includes("alice"), true)
test.expectBool(ks.includes("bob"), true)
test.expectBool(ks.includes("carol"), true)

// values() — likewise check sum + length.
let vs = m.values()
test.expect(vs.length, 3)
let total = vs[0] + vs[1] + vs[2]
test.expect(total, 30 + 25 + 40)

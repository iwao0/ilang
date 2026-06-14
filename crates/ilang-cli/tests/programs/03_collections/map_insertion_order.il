// Map iteration is in INSERTION ORDER, matching JS `Map`: keys(),
// values() and entries() all walk the same `order` list the runtime
// keeps, so an overwrite keeps the key's original position and a delete
// removes just that slot. `map_get_keys_values.il` deliberately asserts
// only membership ("order is unspecified"), but the runtime documents
// this order as a contract (it was added to kill run-to-run HashMap
// nondeterminism), so pin it here.
//
// expect: ok
use std.test as test

let m: Map<string, i64> = {}
m["c"] = 3
m["a"] = 1
m["b"] = 2
m["a"] = 10        // overwrite: keeps a's original (2nd) slot, not moved to end
m.delete("c")      // remove the first slot
m["d"] = 4         // appended last

// keys() in insertion order: c gone, a stays 2nd, b 3rd, d last.
let ks = ""
for k in m.keys() { ks = ks + k }
test.expectStr(ks, "abd")

// values() walk the same order (a=10, b=2, d=4).
let vs = ""
for v in m.values() { vs = vs + v.toString() }
test.expectStr(vs, "1024")

// entries() too.
let es = ""
for e in m.entries() {
    let (k, v) = e
    es = es + k + v.toString()
}
test.expectStr(es, "a10b2d4")

console.log("ok")

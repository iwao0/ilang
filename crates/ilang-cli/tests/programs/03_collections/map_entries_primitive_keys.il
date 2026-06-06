// `Map<K, V>.entries(): (K, V)[]` — non-string key shapes. Per
// syntax.md §9. `map_of_class.il` exercises entries() for string
// keys; this fixture covers `i64` and `bool` keys.
use std.test as test

// Integer keys.
let mi: Map<i64, string> = {1: "one", 2: "two", 3: "three"}
let ei = mi.entries()
test.expect(ei.length, 3)

// Sum the keys / collect the values to assert content without
// pinning down iteration order.
let key_sum = 0
let saw_one = false
let saw_two = false
let saw_three = false
for e in ei {
    key_sum += e[0]
    if e[1] == "one" { saw_one = true }
    if e[1] == "two" { saw_two = true }
    if e[1] == "three" { saw_three = true }
}
test.expect(key_sum, 6)
test.expectBool(saw_one, true)
test.expectBool(saw_two, true)
test.expectBool(saw_three, true)

// Bool keys — both true / false slots can coexist.
let mb: Map<bool, i64> = {true: 10, false: 20}
let eb = mb.entries()
test.expect(eb.length, 2)
let t_val = 0
let f_val = 0
for e in eb {
    if e[0] { t_val = e[1] }
    else { f_val = e[1] }
}
test.expect(t_val, 10)
test.expect(f_val, 20)

// Empty map (after clearing one entry) has no entries.
let mz: Map<i64, i64> = {1: 1}
mz.delete(1)
test.expect(mz.entries().length, 0)

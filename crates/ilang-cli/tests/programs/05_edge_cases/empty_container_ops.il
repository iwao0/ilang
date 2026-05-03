// Operations on empty containers — should be defined behavior, NOT
// panics, for the "absent" cases.
use test

// Empty array.
let xs: i64[] = []
test.expect(xs.length, 0)
test.expectBool(xs.pop().isNone(), true)
test.expect(xs.indexOf(1), -1)
test.expectBool(xs.includes(1), false)
let sliced = xs.slice(0, 0)
test.expect(sliced.length, 0)

// for-in over empty array — body shouldn't run.
let counter = 0
for _ in xs { counter += 1 }
test.expect(counter, 0)

// Empty map.
let m = new Map<string, i64>()
test.expect(m.size(), 0)
test.expectBool(m.has("x"), false)
test.expectBool(m.get("x").isNone(), true)
test.expectBool(m.delete("x"), false)
test.expect(m.keys().length, 0)
test.expect(m.values().length, 0)

// Empty string.
test.expect("".length, 0)
test.expectStr("".toUpper(), "")
test.expectStr("".trim(), "")
test.expectStr("".charAt(0), "")
test.expectBool("".startsWith(""), true)
test.expectBool("".includes("x"), false)
test.expect("".split(",").length, 1)   // JS-style: single empty element

// `indexOf` / `includes` / `remove` on `string[]` compare by content,
// matching `==`'s structural equality on strings. The runtime used a
// raw cell compare (pointer equality), which worked for interned
// literals but missed needles built at runtime — `arr.indexOf("b" +
// "b")` returned -1 against `["aa", "bb"]`. Objects keep reference
// equality (same as `==` on classes). Fresh needles also drop their
// transient +1 after the call instead of leaking a registry string.

use std.test as test

let arr: string[] = ["aa", "bb", "cc"]

// content equality with runtime-built needles
test.expect(arr.indexOf("b" + "b"), 1)
test.expect(arr.indexOf(`a${"a"}`), 0)
test.expect(arr.indexOf("zz"), -1)
test.expectBool(arr.includes("c" + "c"), true)
test.expectBool(arr.includes("c" + "x"), false)

// remove by content — drops the stored element, keeps the rest
test.expectBool(arr.remove("b" + "b"), true)
test.expect(arr.length, 2)
test.expectStr(arr[0], "aa")
test.expectStr(arr[1], "cc")
test.expectBool(arr.remove("bb"), false)

// objects still compare by reference
class Box {
    n: i64
    init(x: i64) { this.n = x }
}
let b0 = new Box(1)
let b1 = new Box(1)
let boxes = [b0]
test.expect(boxes.indexOf(b0), 0)
test.expect(boxes.indexOf(b1), -1)   // equal contents, different identity

// fresh needles leave no transient behind
let strBase = test.liveStringCount()
let i = 0
while i < 200 {
    test.expect(arr.indexOf("a" + "a"), 0)
    test.expectBool(arr.includes("nope" + `${i}`), false)
    i = i + 1
}
let strAfter = test.liveStringCount()
test.expectTrue(strAfter - strBase < 16)

// remove churn: push a runtime-built string, remove it by a different
// runtime-built pointer with equal content — registry stays balanced
let churn: string[] = []
let churnBase = test.liveStringCount()
i = 0
while i < 200 {
    churn.push("v" + `${i}`)
    test.expectBool(churn.remove(`v${i}`), true)
    i = i + 1
}
test.expect(churn.length, 0)
let churnAfter = test.liveStringCount()
test.expectTrue(churnAfter - churnBase < 16)

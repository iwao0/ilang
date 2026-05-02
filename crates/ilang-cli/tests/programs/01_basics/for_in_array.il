// for-in over an array. Both primitive and string elements.
use test

let xs: i64[] = [10, 20, 30, 40]
let total = 0
for x in xs { total += x }
test.expect(total, 100)

// continue inside for-in.
let evens = 0
for x in xs {
    if x % 20 != 0 { continue }
    evens += 1
}
test.expect(evens, 2)        // 20 and 40

// break inside for-in.
let found = -1
let target = 30
for x in xs {
    if x == target {
        found = x
        break
    }
}
test.expect(found, 30)

// String element array — exercises heap-element for-in.
let words: string[] = ["hi", "hello", "yo"]
let combined = ""
for w in words { combined = combined + w }
test.expectStr(combined, "hihelloyo")

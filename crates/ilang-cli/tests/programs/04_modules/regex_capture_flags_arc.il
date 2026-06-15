// `std.regex` beyond the smoke test (`regex_basic.il` covers test / find
// / findAll / replace / split with the `i` flag). This pins the broad
// surface that was unpinned: capture-group replacement (`$1`...), the
// `m` / `s` / `x` flags, `$$` literal-dollar escaping, split with
// leading/trailing matches, the zero-width (empty pattern) match, and the
// ARC of the heap string arrays / Regex objects under churn.
//
// expect: ok
use std.regex as regex
use std.test as test

// --- capture-group replacement ---
let date = new regex.Regex("(\\d{4})-(\\d{2})-(\\d{2})", "")
test.expectStr(date.replace("2026-06-15", "$3/$2/$1"), "15/06/2026")
let kv = new regex.Regex("(\\w+)=(\\w+)", "")
test.expectStr(kv.replace("a=1 b=2", "$2:$1"), "1:a 2:b")
// literal `$` via `$$`
let price = new regex.Regex("price", "")
test.expectStr(price.replace("price up", "$$100"), "$100 up")

// --- flags ---
// m: ^ / $ match line boundaries
let ml = new regex.Regex("^x", "m")
test.expect(ml.findAll("x1\nx2\ny3").length, 2)
// s: dot matches newline
test.expectTrue(new regex.Regex("a.b", "s").test("a\nb"))
test.expectTrue(!new regex.Regex("a.b", "").test("a\nb"))
// x: extended (whitespace in the pattern ignored)
test.expectTrue(new regex.Regex("a b c", "x").test("abc"))
// i already covered in regex_basic; confirm composition (i + m)
let im = new regex.Regex("^foo", "im")
test.expect(im.findAll("FOO\nbar\nFoo").length, 2)

// --- split edges + zero-width ---
let comma = new regex.Regex(",", "")
let parts = comma.split(",a,b,")
test.expect(parts.length, 4)          // "", "a", "b", ""
test.expectStr(parts[0], "")
test.expectStr(parts[3], "")
let emptyPat = new regex.Regex("", "")
test.expect(emptyPat.findAll("ab").length, 3)   // zero-width: before a, before b, end

// --- unicode word chars ---
test.expect(new regex.Regex("\\w+", "").findAll("café über").length, 2)

// --- ARC: findAll heap arrays + Regex objects under churn, no leak ---
let r = new regex.Regex("\\w+", "")
let acc = 0
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 {
    acc = acc + r.findAll("alpha beta gamma delta").length
    let fresh = new regex.Regex("z+", "")     // fresh Regex per iter
    if fresh.test("zzz") { acc = acc + 1 }
    i = i + 1
}
test.expect(acc, 500)                  // 4 words * 100 + 1 * 100
test.expect(test.liveAllocBytes() - d0, 0)

console.log("ok")

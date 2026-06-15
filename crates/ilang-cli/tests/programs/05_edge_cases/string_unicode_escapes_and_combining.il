// `\u{...}` code-point escapes, combining characters, and the code-point
// (not byte) basis of the string methods that take or return an index.
// `string_edge.il` covers `.length` / `charAt` on multibyte literals;
// this pins the escape syntax, the no-normalization rule (precomposed vs
// decomposed compare unequal), astral-plane escapes, and the
// lastIndexOf-fromIndex / split / replace-all behaviours on multibyte
// text.
//
// expect: ok
use std.test as test

// --- `\u{...}` escapes produce single code points (BMP and astral) ---
test.expect("\u{0301}".length, 1)          // combining acute accent
test.expect("\u{00E9}".length, 1)          // precomposed é
test.expect("\u{1F600}".length, 1)         // 😀 (astral, 4 UTF-8 bytes)
test.expectStr("\u{1F600}", "😀")
test.expectStr("\u{00E9}", "é")

// --- combining: decomposed is 2 code points; NO normalization ---
let dec = "e\u{0301}"                       // e + combining accent
let pre = "\u{00E9}"                        // precomposed é
test.expect(dec.length, 2)
test.expect(pre.length, 1)
test.expectTrue(!(dec == pre))              // byte compare, not NFC
test.expectStr(dec.charAt(0), "e")
test.expectStr(dec.charAt(1), "\u{0301}")

// --- index-taking / returning methods are code-point based ---
let s = "あいあいあ"                          // 5 code points
test.expect(s.lastIndexOf("あ"), 4)
test.expect(s.lastIndexOf("あ", 2), 2)      // fromIndex in code points
test.expect(s.indexOf("いあ"), 1)
test.expectTrue(s.startsWith("あい"))
test.expectTrue(s.endsWith("いあ"))

// --- split: empty delimiter splits into code points; multibyte delim ---
test.expect("あいう".split("").length, 3)
let parts = "aあbあc".split("あ")
test.expect(parts.length, 3)
test.expectStr(parts[1], "b")

// --- replace: all occurrences, code-point length may change ---
test.expectStr("あいあ".replace("あ", "xy"), "xyいxy")
test.expect("あいあ".replace("あ", "xy").length, 5)   // 2 + 1 + 2

// --- astral slice + charAt round-trip ---
let emo = "\u{1F600}\u{1F601}\u{1F602}"
test.expect(emo.length, 3)
test.expectStr(emo.slice(1, 3), "😁😂")
test.expectStr(emo.charAt(2), "😂")

console.log("ok")

// Iterate a string character-by-character via `charAt(i)` for `i` in
// `0..length`. `length` is in code points (not bytes), so multibyte
// strings round-trip cleanly. `string_edge.il` covers `charAt` slicing
// edge cases; this fixture covers the iteration shape.
use std.test

let s = "hello"
let chars: string[] = []
let i = 0
while i < s.length {
    chars.push(s.charAt(i))
    i += 1
}
test.expect(chars.length, 5)
test.expectStr(chars[0], "h")
test.expectStr(chars[4], "o")

// Multibyte — each codepoint is one slot.
let m = "あいう"
test.expect(m.length, 3)
let mc: string[] = []
let j = 0
while j < m.length {
    mc.push(m.charAt(j))
    j += 1
}
test.expect(mc.length, 3)
test.expectStr(mc[0], "あ")
test.expectStr(mc[1], "い")
test.expectStr(mc[2], "う")

// Reverse a string by walking from the end.
fn reverse(s: string): string {
    let acc = ""
    let i = s.length - 1
    while i >= 0 {
        acc = acc + s.charAt(i)
        i -= 1
    }
    acc
}
test.expectStr(reverse("hello"), "olleh")
test.expectStr(reverse(""), "")
test.expectStr(reverse("a"), "a")
test.expectStr(reverse("あいう"), "ういあ")
test.expectStr(reverse("a😀b"), "b😀a")

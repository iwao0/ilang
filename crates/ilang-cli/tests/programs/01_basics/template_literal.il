// Template literals — backtick strings with `${...}` interpolation
// and the full backslash-escape set. Per syntax.md §4.
use std.test as test

// Plain interpolation.
let name = "world"
test.expectStr(`hello, ${name}!`, "hello, world!")
let n = 42
test.expectStr(`n=${n}`, "n=42")

// Integer / bool / float interpolation goes through `.toString()`.
test.expectStr(`${1 + 2}`, "3")
test.expectStr(`flag=${true}`, "flag=true")
test.expectStr(`pi≈${3.14}`, "pi≈3.14")

// No interpolation — backticks behave like plain string literals.
test.expectStr(`plain`, "plain")
test.expectStr(``, "")

// Escape sequences.
test.expectStr(`a\nb`, "a\nb")
test.expectStr(`a\tb`, "a\tb")
test.expectStr(`a\rb`, "a\rb")
test.expectStr(`a\0b`, "a\0b")
test.expectStr(`\\`, "\\")
test.expectStr(`back\`tick`, "back`tick")
test.expectStr(`dollar\${not_interp}`, "dollar${not_interp}")

// Hex / unicode escapes.
test.expectStr(`\x41`, "A")
test.expectStr(`\u{3B1}`, "α")

// Multiple interpolations in one literal.
let a = 1
let b = 2
test.expectStr(`${a} + ${b} = ${a + b}`, "1 + 2 = 3")
